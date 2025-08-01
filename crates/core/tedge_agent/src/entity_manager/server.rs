use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt as _;
use serde_json::Map;
use serde_json::Value;
use std::fs::File;
use std::path::PathBuf;
use tedge_actors::LoggingSender;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityTwinMessage;
use tedge_api::entity_store::EntityUpdateMessage;
use tedge_api::entity_store::ListFilters;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::TopicFilter;
use tracing::error;

const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";

#[derive(Debug)]
pub enum EntityStoreRequest {
    Get(EntityTopicId),
    Create(EntityRegistrationMessage),
    Update(EntityTopicId, EntityUpdateMessage),
    Delete(EntityTopicId),
    List(ListFilters),
    MqttMessage(MqttMessage),
    InitComplete,
    GetTwinFragment(EntityTopicId, String),
    SetTwinFragment(EntityTwinMessage),
    GetTwinFragments(EntityTopicId),
    SetTwinFragments(EntityTopicId, Map<String, Value>),
}

#[derive(Debug)]
pub enum EntityStoreResponse {
    Get(Option<EntityMetadata>),
    Create(Result<Vec<RegisteredEntityData>, entity_store::Error>),
    Update(Result<EntityMetadata, entity_store::Error>),
    Delete(Vec<EntityMetadata>),
    List(Vec<EntityMetadata>),
    Ok,
    GetTwinFragment(Option<Value>),
    SetTwinFragment(Result<bool, entity_store::Error>),
    GetTwinFragments(Result<Map<String, Value>, entity_store::Error>),
    SetTwinFragments(Result<(), entity_store::Error>),
}

pub struct EntityStoreServer {
    config: EntityStoreServerConfig,
    entity_store: EntityStore,
    mqtt_publisher: LoggingSender<MqttMessage>,
    retain_requests: LoggingSender<(mpsc::UnboundedSender<MqttMessage>, TopicFilter)>,
}

pub struct EntityStoreServerConfig {
    pub config_dir: PathBuf,
    pub mqtt_schema: MqttSchema,
    pub entity_auto_register: bool,
}

impl EntityStoreServerConfig {
    pub fn new(config_dir: PathBuf, mqtt_schema: MqttSchema, entity_auto_register: bool) -> Self {
        Self {
            config_dir,
            mqtt_schema,
            entity_auto_register,
        }
    }
}

impl EntityStoreServer {
    pub fn new<M>(
        config: EntityStoreServerConfig,
        entity_store: EntityStore,
        mqtt_actor: &mut M,
    ) -> Self
    where
        M: MessageSink<MqttRequest>,
    {
        let mqtt_publisher =
            LoggingSender::new("MqttPublisher".into(), mqtt_actor.get_sender().get_sender());
        let retain_requests = LoggingSender::new(
            "DeregistrationClient".into(),
            Box::new(MappingSender::new(
                mqtt_actor.get_sender(),
                move |(tx, topics)| [MqttRequest::RetrieveRetain(tx, topics)],
            )),
        );

        Self {
            config,
            entity_store,
            mqtt_publisher,
            retain_requests,
        }
    }

    #[cfg(test)]
    pub fn entity_topic_ids(&self) -> impl Iterator<Item = &EntityTopicId> {
        self.entity_store.entity_topic_ids()
    }

    #[cfg(test)]
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entity_store.get(entity_topic_id)
    }
}

#[async_trait]
impl Server for EntityStoreServer {
    type Request = EntityStoreRequest;
    type Response = EntityStoreResponse;

    fn name(&self) -> &str {
        "EntityStoreServer"
    }

    async fn handle(&mut self, request: EntityStoreRequest) -> EntityStoreResponse {
        match request {
            EntityStoreRequest::Get(topic_id) => {
                let entity = self.entity_store.get(&topic_id);
                EntityStoreResponse::Get(entity.cloned())
            }
            EntityStoreRequest::Create(entity) => {
                let res = self.register_entity(entity).await;
                EntityStoreResponse::Create(res)
            }
            EntityStoreRequest::Update(topic_id, update_message) => {
                let res = self.update_entity(&topic_id, update_message).await;
                EntityStoreResponse::Update(res.cloned())
            }
            EntityStoreRequest::Delete(topic_id) => {
                let deleted_entities = self.deregister_entity(&topic_id).await;
                EntityStoreResponse::Delete(deleted_entities)
            }
            EntityStoreRequest::List(filters) => {
                let entities = self.entity_store.list_entity_tree(filters);
                EntityStoreResponse::List(entities.into_iter().cloned().collect())
            }
            EntityStoreRequest::GetTwinFragment(topic_id, fragment_key) => {
                let twin = self
                    .entity_store
                    .get_twin_fragment(&topic_id, &fragment_key);
                EntityStoreResponse::GetTwinFragment(twin.cloned())
            }
            EntityStoreRequest::SetTwinFragment(twin_data) => {
                let res = self.set_twin_fragment(twin_data).await;
                EntityStoreResponse::SetTwinFragment(res)
            }
            EntityStoreRequest::GetTwinFragments(topic_id) => {
                let res = self.entity_store.get_twin_fragments(&topic_id);
                EntityStoreResponse::GetTwinFragments(res.cloned())
            }
            EntityStoreRequest::SetTwinFragments(topic_id, fragments) => {
                let res = self.set_entity_twin_fragments(&topic_id, fragments).await;
                EntityStoreResponse::SetTwinFragments(res)
            }
            EntityStoreRequest::MqttMessage(mqtt_message) => {
                self.process_mqtt_message(mqtt_message).await;
                EntityStoreResponse::Ok
            }
            EntityStoreRequest::InitComplete => {
                if let Err(err) = self.init_complete().await {
                    error!("Failed to process inventory.json file: {err}");
                }
                EntityStoreResponse::Ok
            }
        }
    }
}

impl EntityStoreServer {
    async fn init_complete(&mut self) -> Result<(), entity_store::Error> {
        let inventory_file_path = self
            .config
            .config_dir
            .join(INVENTORY_FRAGMENTS_FILE_LOCATION);
        let file = File::open(inventory_file_path)?;
        let inventory_json: Value = serde_json::from_reader(file)?;
        let main_device = self.entity_store.main_device().clone();
        if let Value::Object(map) = inventory_json {
            for (key, value) in map {
                if self
                    .entity_store
                    .get_twin_fragment(&main_device, &key)
                    .is_none()
                {
                    self.publish_twin_data(&main_device, key.clone(), value.clone())
                        .await;
                }
            }
        } else {
            error!(
                "Invalid inventory.json format: expected a JSON object, found {:?}",
                inventory_json
            );
        }

        Ok(())
    }

    pub(crate) async fn process_mqtt_message(&mut self, message: MqttMessage) {
        if let Ok((topic_id, channel)) = self.config.mqtt_schema.entity_channel_of(&message.topic) {
            if let Channel::EntityMetadata = channel {
                self.process_entity_registration(topic_id, message.payload_bytes())
                    .await;
            } else {
                let res = self
                    .process_entity_data(topic_id, channel, message.clone())
                    .await;
                if let Err(err) = res {
                    error!("Failed to process entity data message: {message} due to : {err}");
                }
            }
        } else {
            error!("Ignoring the message: {message} received on unsupported topic",);
        }
    }

    async fn process_entity_registration(&mut self, topic_id: EntityTopicId, payload: &[u8]) {
        if payload.is_empty() {
            let _ = self.deregister_entity(&topic_id).await;
            return;
        }

        match EntityRegistrationMessage::try_from(topic_id.clone(), payload) {
            Ok(entity) => match self.entity_store.update(entity.clone()) {
                Ok(registered) => {
                    for entity in registered {
                        for (fragment_key, fragment_value) in entity.reg_message.twin_data {
                            self.publish_twin_data(
                                &entity.reg_message.topic_id,
                                fragment_key,
                                fragment_value,
                            )
                            .await;
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to register entity registration message: {entity:?} due to {err}"
                    );
                }
            },
            Err(err) => {
                error!("Failed to parse message on {topic_id} as an entity registration message: {err}")
            }
        }
    }

    async fn process_entity_data(
        &mut self,
        topic_id: EntityTopicId,
        channel: Channel,
        message: MqttMessage,
    ) -> Result<(), entity_store::Error> {
        // if the target entity is unregistered, try to register it first using auto-registration
        if self.entity_store.get(&topic_id).is_none()
            && self.config.entity_auto_register
            && topic_id.matches_default_topic_scheme()
            && !message.payload().is_empty()
        {
            let entities = self.entity_store.auto_register_entity(&topic_id)?;
            for entity in entities {
                let message = entity
                    .to_mqtt_message(&self.config.mqtt_schema)
                    .with_retain();
                self.publish_message(message).await;
            }
        }

        if let Channel::EntityTwinData { fragment_key } = channel {
            let fragment_value = if message.payload().is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(message.payload_bytes())?
            };
            let twin_message = EntityTwinMessage::new(topic_id, fragment_key, fragment_value);
            self.entity_store.update_twin_fragment(twin_message)?;
        }

        Ok(())
    }

    async fn set_twin_fragment(
        &mut self,
        twin_message: EntityTwinMessage,
    ) -> Result<bool, entity_store::Error> {
        let updated = self
            .entity_store
            .update_twin_fragment(twin_message.clone())?;
        if updated {
            self.publish_twin_data(
                &twin_message.topic_id,
                twin_message.fragment_key,
                twin_message.fragment_value,
            )
            .await;
        }

        Ok(updated)
    }

    async fn publish_twin_data(
        &mut self,
        topic_id: &EntityTopicId,
        fragment_key: String,
        fragment_value: Value,
    ) {
        let twin_channel = Channel::EntityTwinData { fragment_key };
        let topic = self.config.mqtt_schema.topic_for(topic_id, &twin_channel);
        let payload = if fragment_value.is_null() {
            "".to_string()
        } else {
            fragment_value.to_string()
        };
        let message = MqttMessage::new(&topic, payload).with_retain();
        self.publish_message(message).await;
    }

    async fn publish_message(&mut self, message: MqttMessage) {
        let topic = message.topic.clone();
        if let Err(err) = self.mqtt_publisher.send(message).await {
            error!("Failed to publish the message on topic: {topic:?} due to {err}");
        }
    }

    async fn register_entity(
        &mut self,
        entity: EntityRegistrationMessage,
    ) -> Result<Vec<RegisteredEntityData>, entity_store::Error> {
        if self.entity_store.get(&entity.topic_id).is_some() {
            return Err(entity_store::Error::EntityAlreadyRegistered(
                entity.topic_id,
            ));
        }

        if let Some(parent) = entity.parent.as_ref() {
            if self.entity_store.get(parent).is_none() {
                return Err(entity_store::Error::NoParent(
                    parent.to_string().into_boxed_str(),
                ));
            }
        }

        let registered = self.entity_store.update(entity.clone())?;

        if !registered.is_empty() {
            let message = entity.to_mqtt_message(&self.config.mqtt_schema);
            self.publish_message(message).await;
        }
        Ok(registered)
    }

    async fn update_entity(
        &mut self,
        topic_id: &EntityTopicId,
        update_message: EntityUpdateMessage,
    ) -> Result<&EntityMetadata, entity_store::Error> {
        let entity = self.entity_store.update_entity(topic_id, update_message)?;
        let entity_reg_msg: EntityRegistrationMessage = entity.into();
        let entity_msg = entity_reg_msg.to_mqtt_message(&self.config.mqtt_schema);

        self.publish_message(entity_msg).await;

        self.entity_store.try_get(topic_id)
    }

    async fn deregister_entity(&mut self, topic_id: &EntityTopicId) -> Vec<EntityMetadata> {
        let deleted = self.entity_store.deregister_entity(topic_id);
        if deleted.is_empty() {
            return deleted;
        }

        let mut topics = TopicFilter::empty();
        for entity in deleted.iter() {
            for channel_filter in [
                ChannelFilter::MeasurementMetadata,
                ChannelFilter::EventMetadata,
                ChannelFilter::AlarmMetadata,
                ChannelFilter::Alarm,
                ChannelFilter::EntityTwinData,
                ChannelFilter::AnyCommand,
                ChannelFilter::AnyCommandMetadata,
                ChannelFilter::Health,
            ] {
                let topic = self
                    .config
                    .mqtt_schema
                    .topics(EntityFilter::Entity(&entity.topic_id), channel_filter);
                topics.add_all(topic);
            }
        }

        let (tx, mut rx) = mpsc::unbounded();
        self.retain_requests.send((tx, topics)).await.unwrap();

        while let Some(retain_message) = rx.next().await {
            if !retain_message.payload.as_bytes().is_empty() {
                let clear_msg = MqttMessage::new(&retain_message.topic, "").with_retain();
                self.mqtt_publisher.send(clear_msg).await.unwrap();
            }
        }

        // Clear the entity metadata of all deleted entities bottom up
        for entity in deleted.iter().rev() {
            let topic = self
                .config
                .mqtt_schema
                .topic_for(&entity.topic_id, &Channel::EntityMetadata);
            let clear_entity_msg = MqttMessage::new(&topic, "").with_retain();

            self.publish_message(clear_entity_msg).await;
        }

        deleted
    }

    async fn set_entity_twin_fragments(
        &mut self,
        topic_id: &EntityTopicId,
        fragments: Map<String, Value>,
    ) -> Result<(), entity_store::Error> {
        let mut old_fragments = self
            .entity_store
            .set_twin_fragments(topic_id, fragments.clone())?;

        let fragments_to_clear = old_fragments
            .keys()
            .filter(|key| !fragments.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();

        // Clear all old twin messages
        for fragment_key in fragments_to_clear.into_iter() {
            let twin_message = EntityTwinMessage::new(topic_id.clone(), fragment_key, Value::Null);
            let message = twin_message.to_mqtt_message(&self.config.mqtt_schema);
            self.publish_message(message).await;
        }

        // Publish new twin messages
        for (fragment_key, fragment_value) in fragments.into_iter() {
            let old_value = old_fragments.remove(&fragment_key);
            if old_value == Some(fragment_value.clone()) {
                continue;
            }
            let twin_message =
                EntityTwinMessage::new(topic_id.clone(), fragment_key, fragment_value);

            let message = twin_message.to_mqtt_message(&self.config.mqtt_schema);
            self.publish_message(message).await;
        }

        Ok(())
    }
}

pub fn subscriptions(topic_root: &str) -> TopicFilter {
    let topic = format!("{}/+/+/+/+/#", topic_root);
    vec![topic].try_into().unwrap()
}
