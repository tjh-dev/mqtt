#![allow(clippy::tabs_in_doc_comments)]
//! # MQTT
//!
//! A library for interacting with the MQTT protocol.
mod bytes_reader;
mod filter;
mod packet;
mod qos;
mod serde;
mod topic;

pub mod clients;
pub mod misc;
pub mod packets;

pub use self::{
	filter::{Filter, FilterBuf, InvalidFilter},
	packet::{Packet, PacketType},
	qos::{InvalidQoS, QoS},
	topic::{InvalidTopic, Topic, TopicBuf},
};

pub type PacketId = core::num::NonZeroU16;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
