//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
mod packet;
mod qos;
mod serde;

#[cfg(feature = "async-client")]
pub mod async_client;
pub mod filter;
pub mod misc;
pub mod packets;

pub use self::{
	filter::{Filter, FilterBuf, FilterError},
	packet::{Packet, PacketType},
	qos::{InvalidQoS, QoS},
};

pub type PacketId = core::num::NonZeroU16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
