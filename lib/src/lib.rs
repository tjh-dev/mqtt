//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
//!
mod packet;
mod qos;
pub use qos::QoS;

#[cfg(feature = "async-client")]
pub mod async_client;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
