use crate::{Filter, FilterBuf, InvalidFilter, QoS};

/// A collection of FilterBuf.
pub struct Filters(pub(crate) Vec<FilterBuf>);

/// A collection of (FilterBuf, QoS).
pub struct FiltersWithQoS(pub(crate) Vec<(FilterBuf, QoS)>);

impl<'a, T: AsRef<str>> TryFrom<&[T]> for Filters {
	type Error = InvalidFilter;
	fn try_from(value: &[T]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push(s.as_ref().try_into()?);
		}
		Ok(Self(filters))
	}
}

impl<E, T: TryInto<FilterBuf, Error = E>> TryFrom<Vec<T>> for Filters
where
	InvalidFilter: From<E>,
{
	type Error = InvalidFilter;
	fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.into_iter() {
			filters.push(s.try_into()?);
		}
		Ok(Self(filters))
	}
}

impl TryFrom<&str> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: &str) -> Result<Self, Self::Error> {
		Self::try_from(value.to_owned())
	}
}

impl TryFrom<String> for FiltersWithQoS {
	type Error = InvalidFilter;
	#[inline]
	fn try_from(value: String) -> Result<Self, Self::Error> {
		let filter = FilterBuf::new(value)?;
		Ok(Self(vec![(filter, QoS::default())]))
	}
}

impl TryFrom<&Filter> for FiltersWithQoS {
	type Error = InvalidFilter;
	#[inline]
	fn try_from(value: &Filter) -> Result<Self, Self::Error> {
		Self::try_from(value.to_owned())
	}
}

impl TryFrom<FilterBuf> for FiltersWithQoS {
	type Error = InvalidFilter;
	#[inline]
	fn try_from(value: FilterBuf) -> Result<Self, Self::Error> {
		Ok(Self(vec![(value, QoS::default())]))
	}
}

impl<'a, T: AsRef<str>, const N: usize> TryFrom<[T; N]> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: [T; N]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(N);
		for s in value.iter() {
			filters.push((s.as_ref().try_into()?, QoS::default()));
		}
		Ok(Self(filters))
	}
}

impl<'a, T: AsRef<str>> TryFrom<&[T]> for FiltersWithQoS {
	type Error = InvalidFilter;
	fn try_from(value: &[T]) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for s in value.iter() {
			filters.push((s.as_ref().try_into()?, QoS::default()));
		}
		Ok(Self(filters))
	}
}

impl<E, T: TryInto<FilterBuf, Error = E>> TryFrom<(T, QoS)> for FiltersWithQoS
where
	InvalidFilter: From<E>,
{
	type Error = InvalidFilter;
	fn try_from(value: (T, QoS)) -> Result<Self, Self::Error> {
		let (filter, qos) = value;
		Ok(Self(vec![(filter.try_into()?, qos)]))
	}
}

impl<E, T: TryInto<FilterBuf, Error = E>> TryFrom<Vec<(T, QoS)>> for FiltersWithQoS
where
	InvalidFilter: From<E>,
{
	type Error = InvalidFilter;
	fn try_from(value: Vec<(T, QoS)>) -> Result<Self, Self::Error> {
		let mut filters = Vec::with_capacity(value.len());
		for (filter, qos) in value.into_iter() {
			filters.push((filter.try_into()?, qos));
		}
		Ok(Self(filters))
	}
}

impl<E, T: TryInto<FilterBuf, Error = E>> TryFrom<(Vec<T>, QoS)> for FiltersWithQoS
where
	InvalidFilter: From<E>,
{
	type Error = InvalidFilter;
	fn try_from(value: (Vec<T>, QoS)) -> Result<Self, Self::Error> {
		let (raw_filters, qos) = value;
		let mut filters = Vec::with_capacity(raw_filters.len());
		for filter in raw_filters.into_iter() {
			filters.push((filter.try_into()?, qos))
		}
		Ok(Self(filters))
	}
}
