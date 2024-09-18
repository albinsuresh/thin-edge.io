use bumpalo::Bump;
use jsonata::JsonAta;
use serde::Deserialize;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tedge_mqtt_ext::MqttError;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use thiserror::Error;
use time::format_description;
use time::OffsetDateTime;

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawDynamicMapperRule {
    #[serde(rename = "targetAPI")]
    target_api: TargetApi,
    mapping_topic: String,
    subscription_topic: String,
    qos: RawQoS,
    substitutions: Vec<SubstitutionRule>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TargetApi {
    Inventory,
    Measurement,
    Event,
    Alarm,
}

impl TargetApi {
    fn topic(&self) -> Topic {
        let topic_str = match self {
            TargetApi::Inventory => "c8y/inventory/managedObjects/update",
            TargetApi::Measurement => "c8y/measurement/measurements/create",
            TargetApi::Event => "c8y/event/events/create",
            TargetApi::Alarm => "c8y/alarm/alarms/create",
        };

        Topic::new_unchecked(topic_str)
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawQoS {
    AtMostOnce,
    AtLeastOnce,
    ExactlyOnce,
}

impl From<RawQoS> for QoS {
    fn from(value: RawQoS) -> Self {
        match value {
            RawQoS::AtMostOnce => QoS::AtMostOnce,
            RawQoS::AtLeastOnce => QoS::AtLeastOnce,
            RawQoS::ExactlyOnce => QoS::ExactlyOnce,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionRule {
    path_source: String,
    path_target: String,
}

pub struct DynamicMapper {
    rules: Vec<DynamicMapperRule>,
}

#[derive(Error, Debug)]
pub enum DynamicMapperError {
    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromMqtt(#[from] tedge_mqtt_ext::MqttError),

    #[error(transparent)]
    FromJsonAta(#[from] jsonata::errors::Error),

    #[error("Invalid topic index value {0}")]
    InvalidIndex(String),

    #[error("No value at index {0}")]
    NoValueAtIndex(usize),

    #[error("Mapper rules are out-of-date")]
    RulesOutOfDate,
}

pub fn parse_dynamic_mapping_rules(config_dir: &Path) -> Result<DynamicMapper, DynamicMapperError> {
    let mapper_rules_path = config_dir.join("c8y").join("dynamic_mapper.json");
    if mapper_rules_path.exists() {
        let file = File::open(mapper_rules_path).expect("file not found");

        // Create a buffered reader for efficiency.
        let reader = BufReader::new(file);

        let raw_mapping_rules: Vec<RawDynamicMapperRule> = serde_json::from_reader(reader)?;
        DynamicMapper::try_new(raw_mapping_rules)
    } else {
        Ok(DynamicMapper { rules: vec![] })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DynamicMapperRule {
    target_api: TargetApi,
    mapping_topic: TopicFilter,
    subscription_topic: TopicFilter,
    qos: QoS,
    substitutions: Vec<SubstitutionRule>,
}

impl TryFrom<RawDynamicMapperRule> for DynamicMapperRule {
    type Error = MqttError;

    fn try_from(value: RawDynamicMapperRule) -> Result<Self, Self::Error> {
        Ok(DynamicMapperRule {
            target_api: value.target_api,
            mapping_topic: TopicFilter::new(&value.mapping_topic)?,
            subscription_topic: TopicFilter::new(&value.subscription_topic)?,
            qos: value.qos.into(),
            substitutions: value.substitutions,
        })
    }
}

impl DynamicMapper {
    pub fn try_new(rules: Vec<RawDynamicMapperRule>) -> Result<DynamicMapper, DynamicMapperError> {
        let mut mapping_rules = vec![];

        for raw_mapping_rule in rules {
            mapping_rules.push(raw_mapping_rule.try_into()?);
        }
        Ok(DynamicMapper {
            rules: mapping_rules,
        })
    }

    pub fn subscription_topics(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for rule in self.rules.iter() {
            topics.add_all(rule.subscription_topic.clone());
        }
        topics
    }

    pub fn convert(&self, message: &MqttMessage) -> Result<Vec<MqttMessage>, DynamicMapperError> {
        let mut mapped_messages = vec![];
        let mut out_payload = json!({});
        for mapping_rule in self.rules.iter() {
            if mapping_rule.mapping_topic.accept(message) {
                for substitution in mapping_rule.substitutions.iter() {
                    let source_value =
                        Self::get_value_from_message(message, &substitution.path_source)?;
                    Self::append_to_json(&mut out_payload, &substitution.path_target, source_value);
                }
                let out_message =
                    MqttMessage::new(&mapping_rule.target_api.topic(), out_payload.to_string());
                mapped_messages.push(out_message);
            }
        }

        Ok(mapped_messages)
    }

    fn get_value_from_message(
        message: &MqttMessage,
        expr: &str,
    ) -> Result<Value, DynamicMapperError> {
        let topic_prefix = "_TOPIC_LEVEL_[";
        let topic_suffix = "]";
        if expr.eq("$now()") {
            let now = OffsetDateTime::now_utc()
                .format(&format_description::well_known::Rfc3339)
                .unwrap();
            Ok(json!(now))
        } else if expr.starts_with(topic_prefix) && expr.ends_with(topic_suffix) {
            let topic_index_str = expr
                .strip_prefix(topic_prefix)
                .unwrap()
                .strip_suffix(topic_suffix)
                .unwrap();
            let topic_index: usize = topic_index_str
                .parse()
                .map_err(|_| DynamicMapperError::InvalidIndex(topic_index_str.to_string()))?;
            let topic_levels: Vec<&str> = message.topic.name.split('/').collect();
            let topic_key = topic_levels
                .get(topic_index)
                .ok_or_else(|| DynamicMapperError::NoValueAtIndex(topic_index))?;
            Ok(json!(topic_key))
        } else {
            let in_payload_str = message.payload_str()?;
            let arena = Bump::new();
            let jsonata = JsonAta::new(expr, &arena)?;
            let result = jsonata.evaluate(Some(in_payload_str))?;
            let json_str = result.serialize(false);
            let json: Value = serde_json::from_str(&json_str).unwrap();
            Ok(json)
        }
    }

    fn append_to_json(json: &mut Value, key: &str, value: Value) {
        let keys: Vec<&str> = key.split('.').collect();
        let mut current = json;

        for k in keys[..keys.len() - 1].iter() {
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }

            current = current
                .as_object_mut()
                .unwrap()
                .entry(k.to_string())
                .or_insert(Value::Object(Map::new()));
        }

        if let Some(obj) = current.as_object_mut() {
            obj.insert(keys[keys.len() - 1].to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parsing() {
        let input = json!([
            {
                "snoopStatus": "NONE",
                "mappingTopicSample": "measurement/berlin_01/gazoline",
                "ident": "lj2zfk",
                "tested": false,
                "mapDeviceIdentifier": true,
                "active": false,
                "autoAckOperation": true,
                "targetAPI": "MEASUREMENT",
                "source": "{\"fuel\":65,\"ts\":\"2022-08-05T00:14:49.389+02:00\",\"mea\":\"c8y_FuelMeasurement\"}",
                "target": "{\"c8y_FuelMeasurement\":{\"L\":{\"value\":110,\"unit\":\"L\"}},\"time\":\"2022-10-18T00:14:49.389+02:00\",\"source\":{\"id\":\"909090\"},\"type\":\"c8y_FuelMeasurement\"}",
                "externalIdType": "c8y_Serial",
                "mappingTopic": "measurement/+/gazoline",
                "qos": "AT_LEAST_ONCE",
                "substitutions": [
                {
                    "resolve2ExternalId": false,
                    "pathSource": "_TOPIC_LEVEL_[1]",
                    "pathTarget": "source.id",
                    "repairStrategy": "DEFAULT",
                    "expandArray": false
                },
                {
                    "resolve2ExternalId": false,
                    "pathSource": "fuel",
                    "pathTarget": "c8y_FuelMeasurement.L.value",
                    "repairStrategy": "DEFAULT",
                    "expandArray": false
                },
                {
                    "resolve2ExternalId": false,
                    "pathSource": "$now()",
                    "pathTarget": "time",
                    "repairStrategy": "DEFAULT",
                    "expandArray": false
                }
                ],
                "updateExistingDevice": false,
                "mappingType": "JSON",
                "lastUpdate": 1726566875,
                "debug": false,
                "name": "Mapping - 05",
                "snoopedTemplates": [],
                "createNonExistingDevice": false,
                "id": "8160926674",
                "subscriptionTopic": "measurement/#",
                "direction": "INBOUND"
            }
        ]);

        let mapping_rules: Vec<RawDynamicMapperRule> = serde_json::from_value(input).unwrap();
        let mapper = DynamicMapper::try_new(mapping_rules).unwrap();

        let in_payload = json!({"fuel": 50}).to_string();
        let in_message = MqttMessage::new(
            &Topic::new_unchecked("measurement/berlin_01/gazoline"),
            in_payload,
        );
        let out_message = mapper.convert(&in_message).unwrap();
        dbg!(&out_message);
    }

    #[test]
    fn dummy() {
        let in_payload_str = json!({"fuel": 50}).to_string();
        let arena = Bump::new();
        let jsonata = JsonAta::new("fuel", &arena).unwrap();
        let result = jsonata.evaluate(Some(&in_payload_str)).unwrap();
        let json = result.serialize(false);
        let val: Value = serde_json::from_str(&json).unwrap();
        println!("{}", val.is_number());
        println!("{}", val);
    }
}
