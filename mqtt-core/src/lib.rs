//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod filter;
pub use filter::{Filter, FilterBuf, FilterError};

mod packet;
pub use packet::{Connect, Error as PacketError, Packet, Publish, WriteError};

mod packet_type;
pub use packet_type::PacketType;

mod qos;
pub use qos::QoS;

pub type PacketId = u16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
