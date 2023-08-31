//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod filter;
pub use filter::{Filter, FilterBuf, FilterError};

mod packet;
pub use packet::{Connect, Error as PacketError, Packet, Publish, WriteError};

mod qos;
pub use qos::QoS;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
