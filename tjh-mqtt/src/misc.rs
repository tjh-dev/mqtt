use crate::{QoS, Topic};
use bytes::Bytes;
use std::{num::NonZeroU16, ops};

/// Client credentials
///
/// Note that is not possible to set a password without also setting a username.
#[derive(Clone, Debug)]
pub struct Credentials<'a> {
	pub username: &'a str,
	pub password: Option<&'a str>,
}

impl<'a> From<&'a str> for Credentials<'a> {
	#[inline]
	fn from(username: &'a str) -> Self {
		Self {
			username,
			password: None,
		}
	}
}

impl<'a> From<(&'a str, &'a str)> for Credentials<'a> {
	#[inline]
	fn from((username, password): (&'a str, &'a str)) -> Self {
		Self {
			username,
			password: Some(password),
		}
	}
}

/// Will Message
///
/// The will message is set by the Client when it connects to the Server. If the
/// Client disconnects abnormally, the Server publishes the will message to the
/// topic on behalf of the Client. The will message MUST be published with the
/// Will QoS and Retain flags as specified.
#[derive(Clone, Debug)]
pub struct Will<'a> {
	/// The topic to publish the will message to.
	pub topic: &'a Topic,

	/// The message to publish as the will.
	pub payload: &'a [u8],

	/// The quality of service to publish the will message at.
	pub qos: QoS,

	/// Whether or not the will message should be retained.
	pub retain: bool,
}

impl<'a> Will<'a> {
	pub fn new(topic: &'a Topic, payload: &'a [u8], qos: QoS, retain: bool) -> Self {
		Self {
			topic,
			payload,
			qos,
			retain,
		}
	}
}

#[derive(Debug)]
pub(crate) struct WrappingNonZeroU16(NonZeroU16);

impl Default for WrappingNonZeroU16 {
	#[inline]
	fn default() -> Self {
		Self(NonZeroU16::MIN)
	}
}

impl ops::AddAssign<u16> for WrappingNonZeroU16 {
	#[inline]
	fn add_assign(&mut self, rhs: u16) {
		let Self(inner) = self;
		*inner = inner.checked_add(rhs).unwrap_or(NonZeroU16::MIN);
	}
}

impl WrappingNonZeroU16 {
	// pub const MIN: Self = Self(NonZeroU16::MIN);
	pub const MAX: Self = Self(NonZeroU16::MAX);

	#[inline]
	pub fn get(&self) -> NonZeroU16 {
		let Self(inner) = self;
		*inner
	}
}
