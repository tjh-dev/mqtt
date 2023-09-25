use crate::{FilterBuf, InvalidFilter, QoS};

/// A collection of FilterBuf.
pub struct Filters(pub(crate) Vec<FilterBuf>);

/// A collection of (FilterBuf, QoS).
pub struct FiltersWithQoS(pub(crate) Vec<(FilterBuf, QoS)>);

impl TryFrom<&[&str]> for Filters {
	type Error = InvalidFilter;
	fn try_from(value: &[&str]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push(FilterBuf::new(*s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<&[String]> for Filters {
	type Error = InvalidFilter;
	fn try_from(value: &[String]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<&str>> for Filters {
	type Error = InvalidFilter;
	fn try_from(value: Vec<&str>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.into_iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<String>> for Filters {
	type Error = InvalidFilter;
	fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.into_iter() {
			filters.push(FilterBuf::new(s)?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<&str> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		let filter = FilterBuf::new(value)?;
		Ok(Self(vec![(filter, QoS::default())]))
	}
}

impl TryFrom<String> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: String) -> Result<Self, Self::Error> {
		let filter = FilterBuf::new(value)?;
		Ok(Self(vec![(filter, QoS::default())]))
	}
}

impl TryFrom<&[&str]> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: &[&str]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for raw_filter in value.into_iter() {
			let filter = FilterBuf::new(*raw_filter)?;
			filters.push((filter, QoS::default()));
		}
		Ok(Self(filters))
	}
}

impl<const N: usize> TryFrom<[&str; N]> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: [&str; N]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(N);
		for raw_filter in value.into_iter() {
			let filter = FilterBuf::new(raw_filter)?;
			filters.push((filter, QoS::default()));
		}
		Ok(Self(filters))
	}
}

impl TryFrom<(&str, QoS)> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: (&str, QoS)) -> Result<Self, Self::Error> {
		let (raw_filter, qos) = value;
		let filter = FilterBuf::new(raw_filter)?;
		Ok(Self(vec![(filter, qos)]))
	}
}

impl TryFrom<(String, QoS)> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: (String, QoS)) -> Result<Self, Self::Error> {
		let (raw_filter, qos) = value;
		let filter = FilterBuf::new(raw_filter)?;
		Ok(Self(vec![(filter, qos)]))
	}
}

impl TryFrom<Vec<(&str, QoS)>> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: Vec<(&str, QoS)>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for (raw_filter, qos) in value.into_iter() {
			let filter = FilterBuf::new(raw_filter)?;
			filters.push((filter, qos));
		}
		Ok(Self(filters))
	}
}

impl TryFrom<Vec<(String, QoS)>> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: Vec<(String, QoS)>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for (raw_filter, qos) in value.into_iter() {
			let filter = FilterBuf::new(raw_filter)?;
			filters.push((filter, qos));
		}
		Ok(Self(filters))
	}
}

impl TryFrom<(Vec<&str>, QoS)> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: (Vec<&str>, QoS)) -> Result<Self, Self::Error> {
		let (raw_filters, qos) = value;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(Self(filters))
	}
}

impl TryFrom<(Vec<String>, QoS)> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: (Vec<String>, QoS)) -> Result<Self, Self::Error> {
		let (raw_filters, qos) = value;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(Self(filters))
	}
}
