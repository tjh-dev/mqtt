use mqtt_protocol::{FilterBuf, QoS, TopicBuf};
use bytes::Bytes;

#[derive(Debug)]
pub enum Command<T, PubResp, SubResp, UnSubResp> {
	Publish(PublishCommand<PubResp>),
	Subscribe(SubscribeCommand<T, SubResp>),
	Unsubscribe(UnsubscribeCommand<UnSubResp>),
	Shutdown,
}

#[derive(Debug)]
pub struct PublishCommand<R> {
	pub topic: TopicBuf,
	pub payload: Bytes,
	pub qos: QoS,
	pub retain: bool,
	pub response: R,
}

#[derive(Debug)]
pub struct SubscribeCommand<T, R> {
	pub filters: Vec<(FilterBuf, QoS)>,
	pub channel: T,
	pub response: R,
}

#[derive(Debug)]
pub struct UnsubscribeCommand<R> {
	pub filters: Vec<FilterBuf>,
	pub response: R,
}
