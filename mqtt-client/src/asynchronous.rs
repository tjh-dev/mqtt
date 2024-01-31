use crate::{PublishRequest, SubscribeRequest};
use mqtt_protocol::Message;

pub trait AsyncClient {
	type Error;

	#[allow(async_fn_in_trait)]
	async fn subscribe(
		&self,
		request: impl Into<SubscribeRequest>,
	) -> Result<impl AsyncSubscription, Self::Error>;

	#[allow(async_fn_in_trait)]
	async fn publish(&self, request: impl Into<PublishRequest>) -> Result<(), Self::Error>;
}

pub trait AsyncSubscription {
	type Error;

	#[allow(async_fn_in_trait)]
	async fn recv(&mut self) -> Option<Message>;

	#[allow(async_fn_in_trait)]
	async fn unsubscribe(self) -> Result<(), Self::Error>;
}
