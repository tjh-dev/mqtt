/// Quality of Service
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum QoS {
	#[default]
	AtMostOnce = 0,
	AtLeastOnce,
	ExactlyOnce,
}

#[derive(Debug)]
pub struct InvalidQoS;

impl TryFrom<u8> for QoS {
	type Error = InvalidQoS;
	#[inline]
	fn try_from(value: u8) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Self::AtMostOnce),
			1 => Ok(Self::AtLeastOnce),
			2 => Ok(Self::ExactlyOnce),
			_ => Err(InvalidQoS),
		}
	}
}
