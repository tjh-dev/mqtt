//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod packet;
mod serde;

#[cfg(feature = "async-client")]
pub mod async_client;
pub mod filter;
pub mod misc;
pub mod packets;
pub mod state;

pub use self::{
	filter::{Filter, FilterBuf, FilterError},
	misc::QoS,
	packet::{Packet, PacketType},
};

pub type PacketId = core::num::NonZeroU16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
