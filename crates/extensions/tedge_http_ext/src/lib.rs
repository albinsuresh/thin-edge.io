mod actor;
mod messages;

#[cfg(test)]
mod tests;

pub use messages::*;

use actor::*;
use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::ConcurrentServiceActor;
use tedge_actors::MessageBoxConnector;
use tedge_actors::MessageBoxPort;
use tedge_actors::RequestResponseHandler;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::ServiceMessageBoxBuilder;

pub type HttpHandle = RequestResponseHandler<HttpRequest, HttpResult>;
pub trait HttpConnectionBuilder: MessageBoxConnector<HttpRequest, HttpResult, ()> {}
impl HttpConnectionBuilder for HttpActorBuilder {}

pub struct HttpActorBuilder {
    actor: ConcurrentServiceActor<HttpService>,
    pub box_builder: ServiceMessageBoxBuilder<HttpRequest, HttpResult>,
}

impl HttpActorBuilder {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let service = HttpService::new(config)?;
        let actor = ConcurrentServiceActor::new(service);
        let box_builder = ServiceMessageBoxBuilder::new("HTTP", 16).with_max_concurrency(4);

        Ok(HttpActorBuilder { actor, box_builder })
    }

    pub async fn run(self) -> Result<(), ChannelError> {
        let actor = self.actor;
        let messages = self.box_builder.build();

        actor.run(messages).await
    }
}

#[async_trait]
impl ActorBuilder for HttpActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let messages = self.box_builder.build();
        runtime.run(actor, messages).await
    }
}

impl MessageBoxConnector<HttpRequest, HttpResult, ()> for HttpActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPort<HttpRequest, HttpResult>,
        config: (),
    ) {
        self.box_builder.connect_with(peer, config)
    }
}
