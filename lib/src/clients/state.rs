use super::Message;
use crate::{
	misc::WrappingNonZeroU16,
	packets::{self, Publish, SerializePacket, SubAck, Subscribe, UnsubAck, Unsubscribe},
	FilterBuf, PacketId, PacketType, QoS, Topic, TopicBuf,
};
use bytes::{Bytes, BytesMut};
use core::fmt;
use std::{
	collections::HashMap,
	num::NonZeroU16,
	time::{Duration, Instant},
};

#[derive(Debug)]
pub enum StateError {
	/// Received a packet that we were not expecting.
	Unsolicited(PacketType),
	/// The Client received a packet that the Server should not send.
	InvalidPacket(PacketType),
	ProtocolError(&'static str),
	DeliveryFailure(Message),
	HardDeliveryFailure,
}

#[derive(Debug)]
pub struct ClientState<PubTx, PubResp, SubResp, UnSubResp> {
	/// Active subscriptions. All incoming packets are matched against these
	/// filters.
	active_subscriptions: Vec<Subscription<PubTx>>,

	outgoing: BytesMut,

	/// Incoming Publish packets.
	pub incoming: HashMap<PacketId, Message>,

	publish_state: HashMap<PacketId, PublishState<PubResp>>,
	subscribe_state: HashMap<PacketId, SubscribeState<PubTx, SubResp>>,
	unsubscribe_state: HashMap<PacketId, UnsubscribeState<UnSubResp>>,

	publish_packet_id: WrappingNonZeroU16,
	subscribe_packet_id: WrappingNonZeroU16,
	unsubscribe_packet_id: WrappingNonZeroU16,

	// Serialized Connect packet. We store a copy so we can re-send it on
	// reconnections.
	connect: Bytes,

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
			outgoing: BytesMut::new(),
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
	pub fn new(connect: &packets::Connect) -> Self {
		let mut buffer = BytesMut::new();
		connect.serialize_to_bytes(&mut buffer).unwrap();

		Self {
			connect: buffer.freeze(),
			..Default::default()
		}
	}

	/// Serializes `packet` to the internal packet buffer.
	pub fn queue_packet(&mut self, packet: &(impl SerializePacket + fmt::Debug)) {
		#[cfg(feature = "tokio-client")]
		tracing::trace!(?packet, "queueing packet");

		packet
			.serialize_to_bytes(&mut self.outgoing)
			.expect("serializing to BytesMut should be infallible");
	}

	pub fn has_outgoing(&self) -> bool {
		!self.outgoing.is_empty()
	}

	/// Splits the outgoing packet buffer, returning a [`Bytes`] containing
	/// serialized packets to be sent to the Server, and clearing the internal
	/// buffer.
	///
	/// Returns `None` if the internal buffer is empty.
	pub fn take_buffer(&mut self) -> Option<Bytes> {
		self.has_outgoing().then(|| self.outgoing.split().freeze())
	}

	/// Initiates a new connection state.
	///
	/// This is currently incomplete.
	pub fn connect(&mut self) {
		// For now, just queue the Connect packet.
		self.outgoing.extend_from_slice(&self.connect[..]);
	}

	/// Handles an unsubscribe command.
	pub fn unsubscribe(&mut self, filters: Vec<FilterBuf>, response: UnSubResp) {
		// Generate and serialize an UnSub packet.
		let id = self.generate_unsubscribe_id();
		self.queue_packet(&Unsubscribe {
			id,
			filters: filters.iter().map(|filter| filter.as_ref()).collect(),
		});

		self.unsubscribe_state.insert(
			id,
			UnsubscribeState {
				filters,
				response,
				expires: Instant::now(),
			},
		);
	}

	/// Handles an incoming UnSubAck packet.
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

	/// Generates a new unused [`PacketId`] to use for outgoing [`Publish`]
	/// packets.
	///
	/// NOTE: This will deadlock if there are 65,534 inflight Publish requests.
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

	/// Generates a new unused [`PacketId`] to use for outgoing [`Subscribe`]
	/// packets.
	///
	/// NOTE: This will deadlock if there are 65,534 inflight Subscribe
	/// requests.
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

	/// Generates a new unused [`PacketId`] to use for outgoing [`Unsubscribe`]
	/// packets.
	///
	/// NOTE: This will deadlock if there are 65,534 inflight Unsubscribe
	/// requests.
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

	/// Generates a [`Subscribe`] packet to re-subscribe all active
	/// subscriptions.
	///
	/// Use this to resume the state's active subscriptions when re-connecting
	/// to the Server.
	pub fn generate_resubscribe(&mut self, response: SubResp) -> bool {
		if !self.active_subscriptions.is_empty() {
			let filters: Vec<_> = self.active_subscriptions.drain(..).collect();

			let id = self.generate_subscribe_id();
			let packet = packets::Subscribe {
				id,
				filters: filters
					.iter()
					.map(|Subscription { filter, qos, .. }| (filter.as_ref(), *qos))
					.collect(),
			};

			self.queue_packet(&packet);

			self.subscribe_state.insert(
				id,
				SubscribeState {
					filters,
					response,
					expires: Instant::now(),
				},
			);

			true
		} else {
			false
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
				self.queue_packet(&Publish::AtMostOnce {
					retain,
					topic,
					payload,
				});

				Some(response)
			}
			QoS::AtLeastOnce => {
				let id = self.generate_publish_id();
				self.publish_state
					.insert(id, PublishState::Ack { response });

				// Generate the first attempt.
				self.queue_packet(&Publish::AtLeastOnce {
					id,
					retain,
					duplicate: false,
					topic,
					payload,
				});

				None
			}
			QoS::ExactlyOnce => {
				let id = self.generate_publish_id();
				self.publish_state
					.insert(id, PublishState::Rec { response });

				// Generate the first attempt.
				self.queue_packet(&Publish::ExactlyOnce {
					id,
					retain,
					duplicate: false,
					topic,
					payload,
				});

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
		self.queue_packet(&packets::PubRel { id });
		Ok(())
	}

	/// Handles an incoming PubComp packet.
	pub fn pubcomp(&mut self, id: NonZeroU16) -> Result<PubResp, StateError> {
		let Some(PublishState::Comp { response }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubComp));
		};

		Ok(response)
	}

	/// Handles an incoming PubRel packet.
	pub fn pubrel(&mut self, id: PacketId) -> Result<Message, StateError> {
		let Some(message) = self.incoming.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubRel));
		};

		Ok(message)
	}

	/// Finds a channel to publish messages for `topic` to.
	#[inline]
	pub fn find_publish_channel(&self, topic: &Topic) -> Option<&PubTx> {
		#[cfg(feature = "tokio-client")]
		let start = Instant::now();

		for Subscription {
			filter, channel, ..
		} in &self.active_subscriptions
		{
			if filter.matches_topic(topic) {
				#[cfg(feature = "tokio-client")]
				{
					let time = start.elapsed();
					tracing::info!(topic = ?topic, filter = ?filter, time = ?time, "found channel for");
				}
				return Some(channel);
			}
		}

		None
	}
}

impl<PubTx: Clone + fmt::Debug, PubResp, SubResp, UnSubResp>
	ClientState<PubTx, PubResp, SubResp, UnSubResp>
{
	pub fn subscribe(&mut self, filters: Vec<(FilterBuf, QoS)>, channel: PubTx, response: SubResp) {
		// Generate an ID for the subscribe packet.
		let id = self.generate_subscribe_id();
		self.queue_packet(&Subscribe {
			id,
			filters: filters
				.iter()
				.map(|(filter, qos)| (filter.as_ref(), *qos))
				.collect(),
		});

		self.subscribe_state.insert(
			id,
			SubscribeState {
				filters: filters
					.into_iter()
					.map(|(filter, qos)| Subscription {
						filter,
						qos,
						channel: channel.clone(),
					})
					.collect(),
				response,
				expires: Instant::now(),
			},
		);
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
					#[cfg(feature = "tokio-client")]
					tracing::warn!("replacing existing filter subscription");

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

		// Sort the filters based on how specific they are.
		self.active_subscriptions
			.sort_by_key(|Subscription { filter, .. }| filter.score());

		Ok((
			response,
			successful_filters
				.into_iter()
				.map(|(f, _, q, _)| (f, q))
				.collect(),
		))
	}
}
