//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
#[cfg(feature = "async-client")]
pub mod async_client;

mod filter;
mod packet;
mod packet_type;
mod qos;

pub use self::{
	filter::{Filter, FilterBuf, FilterError},
	packet::{
		ConnAck, Connect, Credentials, Disconnect, Error as PacketError, Packet, PingReq, PingResp,
		PubAck, PubComp, PubRec, PubRel, Publish, SubAck, Subscribe, UnsubAck, Unsubscribe, Will,
		WriteError,
	},
	packet_type::PacketType,
	qos::QoS,
};

pub type PacketId = std::num::NonZeroU16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
