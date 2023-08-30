//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod packet;
pub use packet::{Error as PacketError, Packet, WriteError};

mod qos;
pub use qos::QoS;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
