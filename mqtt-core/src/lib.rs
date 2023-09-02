//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod filter;
mod packet;
mod packet_type;
mod qos;

pub use self::{
	filter::{Filter, FilterBuf, FilterError},
	packet::{
		ConnAck, Connect, Disconnect, Error as PacketError, Packet, PingReq, PingResp, PubAck,
		PubComp, PubRec, PubRel, Publish, Subscribe, UnsubAck, WriteError,
	},
	packet_type::PacketType,
	qos::QoS,
};

pub type PacketId = u16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
