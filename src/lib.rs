mod packet;
mod qos;
pub use qos::QoS;

#[cfg(feature = "tokio")]
pub mod tokio;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
