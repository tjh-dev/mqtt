use crate::{FilterBuf, FilterError, InvalidTopic, QoS, TopicBuf};

pub trait IntoFilters {
	/// Convert `Self` into a [`Vec`] of [`FilterBuf`].
	fn into_filters(self) -> Result<Vec<FilterBuf>, FilterError>;
}

pub trait IntoFiltersWithQoS {
	/// Convert `Self` into a [`Vec`] of ([`FilterBuf`], [`QoS`]).
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError>;
}

pub trait IntoTopicBuf {
	/// Convert `Self` in to [`TopicBuf`].
	fn into_topic_buf(self) -> Result<TopicBuf, InvalidTopic>;
}

impl IntoFilters for &str {
	#[inline]
	fn into_filters(self) -> Result<Vec<FilterBuf>, FilterError> {
		Ok(vec![FilterBuf::new(self)?])
	}
}

impl IntoFilters for String {
	#[inline]
	fn into_filters(self) -> Result<Vec<FilterBuf>, FilterError> {
		Ok(vec![FilterBuf::new(self)?])
	}
}

impl IntoFilters for Vec<&str> {
	fn into_filters(self) -> Result<Vec<FilterBuf>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for filter in self.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push(filter)
		}
		Ok(filters)
	}
}

impl IntoFilters for Vec<String> {
	fn into_filters(self) -> Result<Vec<FilterBuf>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for filter in self.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push(filter)
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for &str {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let filter = FilterBuf::new(self)?;
		Ok(vec![(filter, QoS::default())])
	}
}

impl IntoFiltersWithQoS for String {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let filter = FilterBuf::new(self)?;
		Ok(vec![(filter, QoS::default())])
	}
}

impl IntoFiltersWithQoS for (&str, QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (filter, qos) = self;
		let filter = FilterBuf::new(filter)?;
		Ok(vec![(filter, qos)])
	}
}

impl IntoFiltersWithQoS for (String, QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (filter, qos) = self;
		let filter = FilterBuf::new(filter)?;
		Ok(vec![(filter, qos)])
	}
}

impl IntoFiltersWithQoS for Vec<(&str, QoS)> {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for (filter, qos) in self.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for Vec<(String, QoS)> {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for (filter, qos) in self.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for &[(&str, QoS)] {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for (filter, qos) in self.iter() {
			let filter = FilterBuf::new(*filter)?;
			filters.push((filter, *qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for &[(String, QoS)] {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let mut filters = Vec::with_capacity(self.len());
		for (filter, qos) in self.iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, *qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for (&[&str], QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (raw_filters, qos) = self;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.iter() {
			let filter = FilterBuf::new(*filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for (&[String], QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (raw_filters, qos) = self;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for (Vec<&str>, QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (raw_filters, qos) = self;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}

impl IntoFiltersWithQoS for (Vec<String>, QoS) {
	fn into_filters_with_qos(self) -> Result<Vec<(FilterBuf, QoS)>, FilterError> {
		let (raw_filters, qos) = self;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			let filter = FilterBuf::new(filter)?;
			filters.push((filter, qos))
		}
		Ok(filters)
	}
}
