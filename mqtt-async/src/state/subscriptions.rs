use super::{PacketType, PublishTx, ResponseTx, StateError};
use crate::command::SubscribeCommand;
use mqtt_core::{FilterBuf, Packet, PacketId, QoS, Subscribe};
use std::{
	collections::{BTreeMap, HashMap},
	num::NonZeroU16,
};

#[derive(Debug)]
pub struct SubscriptionsManager {
	/// The next packet ID to use for Subscribe requests.
	subscribe_id: NonZeroU16,

	/// State for subcriptions requests awaiting a SubAck from the broker.
	subscribe_state: HashMap<PacketId, SubscribeState>,

	/// Active subcriptions.
	subscriptions: BTreeMap<FilterBuf, PublishTx>,
}

#[derive(Debug)]
struct SubscribeState {
	requested_filters: Vec<(FilterBuf, QoS)>,
	publish_tx: PublishTx,
	response_tx: ResponseTx<Vec<(FilterBuf, QoS)>>,
}

impl Default for SubscriptionsManager {
	fn default() -> Self {
		Self {
			subscribe_id: unsafe { NonZeroU16::new_unchecked(1) },
			subscribe_state: Default::default(),
			subscriptions: Default::default(),
		}
	}
}

impl SubscriptionsManager {
	pub fn handle_subscribe_command(&mut self, command: SubscribeCommand) -> Option<Packet> {
		let SubscribeCommand {
			filters,
			publish_tx,
			response_tx,
		} = command;
		let id = self.generate_id();

		self.subscribe_state.insert(
			id,
			SubscribeState {
				requested_filters: filters.clone(),
				publish_tx,
				response_tx,
			},
		);

		// Build the Subscribe packet
		Some(Subscribe { id, filters }.into())
	}

	pub fn handle_suback(
		&mut self,
		id: PacketId,
		result: Vec<Option<QoS>>,
	) -> Result<(), StateError> {
		// Ascertain that we have an active subscription request for the SubAck
		// packet ID.
		//
		let Some(subscribe_state) = self.subscribe_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::SubAck));
		};

		let SubscribeState {
			requested_filters,
			publish_tx,
			response_tx,
			..
		} = subscribe_state;

		if result.len() != requested_filters.len() {
			return Err(StateError::ProtocolError(
				"SubAck payload length does not correspond to Subscribe payload length",
			));
		}

		let successful_filters: Vec<_> = result
			.into_iter()
			.zip(requested_filters)
			.filter_map(|(result_qos, (requested_filter, _))| {
				let qos = result_qos?;
				Some((requested_filter, qos))
			})
			.collect();

		for (filter, _) in &successful_filters {
			self.subscriptions
				.insert(filter.clone(), publish_tx.clone());
		}

		if response_tx.send(successful_filters).is_err() {
			tracing::warn!("response channel for SubAck {{ id: {id} }} closed");
		}

		// We don't generate any packets in response to SubAck.
		Ok(())
	}

	/// Finds a channel to publish messages for `topic` to.
	pub fn find_publish_channel(&self, topic: &str) -> Option<&PublishTx> {
		self.subscriptions
			.iter()
			.filter_map(|(filter, channel)| {
				let mat = filter.matches_topic(topic)?;
				Some((mat.score(), channel))
			})
			.max_by_key(|(score, _)| *score)
			.map(|(_, channel)| channel)
	}

	/// Generates a new, non-zero packet ID.
	///
	/// TODO: This does not currently verify that the packet ID isn't already in
	/// use.
	#[inline(always)]
	fn generate_id(&mut self) -> PacketId {
		let current = self.subscribe_id.get();
		self.subscribe_id = self
			.subscribe_id
			.checked_add(1)
			.unwrap_or(unsafe { NonZeroU16::new_unchecked(1) });
		current
	}
}
