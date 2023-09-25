use crate::{
	misc::WrappingNonZeroU16,
	packets::{self, Publish, SubAck, Subscribe, UnsubAck, Unsubscribe},
	FilterBuf, Packet, PacketId, PacketType, QoS, Topic, TopicBuf,
};
use bytes::Bytes;
use core::fmt;
use std::{
	collections::{HashMap, VecDeque},
	num::NonZeroU16,
	time::{Duration, Instant},
};

#[derive(Debug)]
pub enum StateError {
	Unsolicited(PacketType),
	/// The Client received a packet that the Server should not send.
	InvalidPacket,
	ProtocolError(&'static str),
	DeliveryFailure(Publish),
	HardDeliveryFailure,
}

#[derive(Debug)]
pub struct ClientState<PubTx, PubResp, SubResp, UnSubResp> {
	/// Active subscriptions. All incoming packets are matched against these
	/// filters.
	active_subscriptions: Vec<Subscription<PubTx>>,

	pub outgoing: VecDeque<Packet>,

	/// Incoming Publish packets.
	pub incoming: HashMap<PacketId, packets::Publish>,

	publish_state: HashMap<PacketId, PublishState<PubResp>>,
	subscribe_state: HashMap<PacketId, SubscribeState<PubTx, SubResp>>,
	unsubscribe_state: HashMap<PacketId, UnsubscribeState<UnSubResp>>,

	publish_packet_id: WrappingNonZeroU16,
	subscribe_packet_id: WrappingNonZeroU16,
	unsubscribe_packet_id: WrappingNonZeroU16,

	pub connect: packets::Connect,
	pub keep_alive: Duration,

	// This is Some if there is a active PingReq request.
	pub pingreq_state: Option<Instant>,
}

#[derive(Debug)]
struct Subscription<T> {
	filter: FilterBuf,
	qos: QoS,
	channel: T,
}

#[derive(Debug)]
enum PublishState<R> {
	Ack { response: R },
	Rec { response: R },
	Comp { response: R },
}

#[derive(Debug)]
struct SubscribeState<T, R> {
	filters: Vec<Subscription<T>>,
	response: R,
	expires: Instant,
}

#[derive(Debug)]
struct UnsubscribeState<T> {
	filters: Vec<FilterBuf>,
	response: T,
	expires: Instant,
}

impl<PubTx, PubResp, SubResp, UnSubResp> Default
	for ClientState<PubTx, PubResp, SubResp, UnSubResp>
{
	fn default() -> Self {
		Self {
			active_subscriptions: Vec::new(),
			outgoing: VecDeque::new(),
			incoming: Default::default(),
			publish_state: Default::default(),
			subscribe_state: Default::default(),
			unsubscribe_state: Default::default(),
			publish_packet_id: WrappingNonZeroU16::MAX,
			subscribe_packet_id: WrappingNonZeroU16::MAX,
			unsubscribe_packet_id: WrappingNonZeroU16::MAX,
			connect: Default::default(),
			keep_alive: Duration::default(),
			pingreq_state: Default::default(),
		}
	}
}

impl<PubTx: fmt::Debug, PubResp, SubResp, UnSubResp>
	ClientState<PubTx, PubResp, SubResp, UnSubResp>
{
	pub fn unsubscribe(&mut self, filters: Vec<FilterBuf>, response: UnSubResp) {
		let id = self.generate_unsubscribe_id();
		self.unsubscribe_state.insert(
			id,
			UnsubscribeState {
				filters: filters.clone(),
				response,
				expires: Instant::now(),
			},
		);

		self.outgoing.push_back(Unsubscribe { id, filters }.into());
	}

	pub fn unsuback(&mut self, unsuback: UnsubAck) -> Result<UnSubResp, StateError> {
		let UnsubAck { id } = unsuback;

		let Some(unsubscribe_state) = self.unsubscribe_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::UnsubAck));
		};

		let UnsubscribeState {
			filters, response, ..
		} = unsubscribe_state;

		// Remove the filters from the active subscriptions.
		self.active_subscriptions
			.retain(|sub| !filters.contains(&sub.filter));

		Ok(response)
	}

	fn generate_publish_id(&mut self) -> PacketId {
		loop {
			self.publish_packet_id += 1;
			if !self
				.publish_state
				.contains_key(&self.publish_packet_id.get())
			{
				break;
			}
		}
		self.publish_packet_id.get()
	}

	fn generate_subscribe_id(&mut self) -> PacketId {
		loop {
			self.subscribe_packet_id += 1;
			if !self
				.subscribe_state
				.contains_key(&self.subscribe_packet_id.get())
			{
				break;
			}
		}
		self.subscribe_packet_id.get()
	}

	fn generate_unsubscribe_id(&mut self) -> PacketId {
		loop {
			self.unsubscribe_packet_id += 1;
			if !self
				.unsubscribe_state
				.contains_key(&self.unsubscribe_packet_id.get())
			{
				break;
			}
		}
		self.unsubscribe_packet_id.get()
	}

	#[inline]
	pub fn has_active_subscriptions(&self) -> bool {
		!self.active_subscriptions.is_empty()
	}

	pub fn generate_resubscribe(&mut self, response: SubResp) -> Option<Packet> {
		if !self.active_subscriptions.is_empty() {
			let filters: Vec<_> = self.active_subscriptions.drain(..).collect();

			let id = self.generate_subscribe_id();
			let packet = crate::packets::Subscribe {
				id,
				filters: filters
					.iter()
					.map(|Subscription { filter, qos, .. }| (filter.clone(), *qos))
					.collect(),
			};

			self.subscribe_state.insert(
				id,
				SubscribeState {
					filters,
					response,
					expires: Instant::now(),
				},
			);

			Some(packet.into())
		} else {
			None
		}
	}

	pub fn expired(&self) -> bool {
		let now = Instant::now();

		let expired_pingreq = self.pingreq_state.map_or(false, |v| v > now);

		let expired_subscribes = self
			.subscribe_state
			.iter()
			.any(|(_, SubscribeState { expires, .. })| expires > &now);

		let expired_unsubscribes = self
			.unsubscribe_state
			.iter()
			.any(|(_, UnsubscribeState { expires, .. })| expires > &now);

		expired_pingreq || expired_subscribes || expired_unsubscribes
	}

	/// Generates an outgoing Publish packet.
	pub fn publish(
		&mut self,
		topic: TopicBuf,
		payload: Bytes,
		qos: QoS,
		retain: bool,
		response: PubResp,
	) -> Option<PubResp> {
		match qos {
			QoS::AtMostOnce => {
				// Just queue the Publish packet.
				self.outgoing.push_back(
					Publish::AtMostOnce {
						retain,
						topic,
						payload,
					}
					.into(),
				);

				Some(response)
			}
			QoS::AtLeastOnce => {
				let id = self.generate_publish_id();
				self.publish_state
					.insert(id, PublishState::Ack { response });

				// Generate the first attempt.
				self.outgoing.push_back(
					Publish::AtLeastOnce {
						id,
						retain,
						duplicate: false,
						topic,
						payload,
					}
					.into(),
				);

				None
			}
			QoS::ExactlyOnce => {
				let id = self.generate_publish_id();
				self.publish_state
					.insert(id, PublishState::Rec { response });

				// Generate the first attempt.
				self.outgoing.push_back(
					Publish::ExactlyOnce {
						id,
						retain,
						duplicate: false,
						topic,
						payload,
					}
					.into(),
				);

				None
			}
		}
	}

	/// Handles an incoming PubAck packet.
	pub fn puback(&mut self, id: NonZeroU16) -> Result<PubResp, StateError> {
		let Some(PublishState::Ack { response, .. }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubAck));
		};

		Ok(response)
	}

	/// Handles an incoming PubRec packet.
	pub fn pubrec(&mut self, id: NonZeroU16) -> Result<(), StateError> {
		let Some(PublishState::Rec { response, .. }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubRec));
		};

		self.publish_state
			.insert(id, PublishState::Comp { response });

		// Queue an incoming PubRel packet.
		self.outgoing.push_back(packets::PubRel { id }.into());
		Ok(())
	}

	/// Handles an incoming PubComp packet.
	pub fn pubcomp(&mut self, id: NonZeroU16) -> Result<PubResp, StateError> {
		let Some(PublishState::Comp { response }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubComp));
		};

		Ok(response)
	}

	pub fn pubrel(&mut self, id: PacketId) -> Result<Publish, StateError> {
		let Some(publish) = self.incoming.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubRel));
		};

		Ok(publish)
	}

	/// Finds a channel to publish messages for `topic` to.
	pub fn find_publish_channel(&self, topic: &Topic) -> Option<&PubTx> {
		let start = Instant::now();

		let Some((filter, score, channel)) = self
			.active_subscriptions
			.iter()
			.filter_map(
				|Subscription {
				     filter, channel, ..
				 }| {
					filter
						.matches_topic(topic)
						.map(|score| (filter, score.score(), channel))
				},
			)
			.max_by_key(|(_, score, _)| *score)
		else {
			#[cfg(feature = "tokio-client")]
			tracing::error!(topic = ?topic, "failed to find channel for");
			return None;
		};

		let time = start.elapsed();
		#[cfg(feature = "tokio-client")]
		tracing::debug!(topic = ?topic, filter = ?filter, score = ?score, time = ?time, "found channel for");

		Some(channel)
	}
}

impl<PubTx: Clone + fmt::Debug, PubResp, SubResp, UnSubResp>
	ClientState<PubTx, PubResp, SubResp, UnSubResp>
{
	pub fn subscribe(&mut self, filters: Vec<(FilterBuf, QoS)>, channel: PubTx, response: SubResp) {
		// Generate an ID for the subscribe packet.
		let id = self.generate_subscribe_id();

		self.subscribe_state.insert(
			id,
			SubscribeState {
				filters: filters
					.iter()
					.map(|(filter, qos)| Subscription {
						filter: filter.clone(),
						qos: *qos,
						channel: channel.clone(),
					})
					.collect(),
				response,
				expires: Instant::now(),
			},
		);

		// Generate the packet to send.
		self.outgoing.push_back(Subscribe { id, filters }.into());
	}

	/// Handles an incoming SubAck packet.
	pub fn suback(&mut self, ack: SubAck) -> Result<(SubResp, Vec<(FilterBuf, QoS)>), StateError> {
		let SubAck { id, result } = ack;

		// Confirm we have an active subscription request for the SubAck packet ID.
		let subscribe_state = self
			.subscribe_state
			.remove(&id)
			.ok_or(StateError::Unsolicited(PacketType::SubAck))?;

		let SubscribeState {
			filters, response, ..
		} = subscribe_state;

		if result.len() != filters.len() {
			return Err(StateError::ProtocolError(
				"SubAck payload length does not correspond to Subscribe payload length",
			));
		}

		let successful_filters: Vec<_> = result
			.into_iter()
			.zip(filters)
			.filter_map(
				|(
					result_qos,
					Subscription {
						filter,
						qos,
						channel,
					},
				)| {
					let result_qos = result_qos.ok()?;
					Some((filter, qos, result_qos, channel))
				},
			)
			.collect();

		'outer: for (filter, _, qos, channel) in &successful_filters {
			// If the filter matches a already subscribed filter, replace it.
			for sub in self.active_subscriptions.iter_mut() {
				if &sub.filter == filter {
					sub.channel = channel.clone();
					sub.qos = *qos;
					continue 'outer;
				}
			}

			// Otherwise, append to the set of active subscriptions.
			self.active_subscriptions.push(Subscription {
				filter: filter.clone(),
				qos: *qos,
				channel: channel.clone(),
			});
		}

		Ok((
			response,
			successful_filters
				.into_iter()
				.map(|(f, _, q, _)| (f, q))
				.collect(),
		))
	}
}
