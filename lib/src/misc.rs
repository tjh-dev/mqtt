use crate::{QoS, Topic};
use std::{num::NonZeroU16, ops};

/// Username and password used to authenticate the Client with the Server.
#[derive(Clone, Debug)]
pub struct Credentials<'a> {
	pub username: &'a str,
	pub password: Option<&'a str>,
}

impl<'a> Credentials<'a> {
	/// Creates a new Credentials instance with the specified username.
	///
	/// # Examples
	/// ```
	/// # use tjh_mqtt::misc::Credentials;
	/// let credentials = Credentials::new("tjh");
	/// assert_eq!(credentials.username, "tjh");
	/// ```
	pub fn new(username: &'a str) -> Self {
		Self {
			username,
			password: None,
		}
	}

	/// Creates a new Credentials instance with the specified username
	/// and password.
	///
	/// # Examples
	/// ```
	/// # use tjh_mqtt::misc::Credentials;
	/// let credentials = Credentials::new_with("tjh", "password");
	///
	/// assert_eq!(credentials.username, "tjh");
	/// assert_eq!(credentials.password, Some("password"));
	/// ```
	pub fn new_with(username: &'a str, password: &'a str) -> Self {
		Self {
			username,
			password: Some(password),
		}
	}

	/// Replaces the password field with the specified password.
	///
	/// # Examples
	/// ```
	/// # use tjh_mqtt::misc::Credentials;
	/// let credentials = Credentials::new("tjh").set_password("password");
	///
	/// assert_eq!(credentials.username, "tjh");
	/// assert_eq!(credentials.password, Some("password"));
	/// ```
	pub fn set_password(mut self, password: &'a str) -> Self {
		self.password.replace(password);
		self
	}
}

impl<'a> From<&'a str> for Credentials<'a> {
	#[inline]
	fn from(username: &'a str) -> Self {
		Self::new(username)
	}
}

impl<'a> From<(&'a str, &'a str)> for Credentials<'a> {
	#[inline]
	fn from((username, password): (&'a str, &'a str)) -> Self {
		Self::new_with(username, password)
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

#[allow(unused)]
impl WrappingNonZeroU16 {
	// pub const MIN: Self = Self(NonZeroU16::MIN);
	pub const MAX: Self = Self(NonZeroU16::MAX);

	#[inline]
	pub fn get(&self) -> NonZeroU16 {
		let Self(inner) = self;
		*inner
	}
}

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
