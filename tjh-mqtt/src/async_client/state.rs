use super::command::ResponseTx;
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
	time::Duration,
};
use tokio::time::Instant;

pub trait Sender<T> {
	fn send(self, t: T) -> Result<(), T>;
}

impl<T> Sender<T> for ResponseTx<T> {
	#[inline]
	fn send(self, t: T) -> Result<(), T> {
		self.send(t)
	}
}

#[derive(Debug)]
pub enum StateError {
	Unsolicited(PacketType),
	/// The Client recevied a packet that the Server should not send.
	InvalidPacket,
	ProtocolError(&'static str),
	DeliveryFailure(Publish),
	HardDeliveryFailure,
}

#[derive(Debug)]
pub struct ClientState<T, PublishResponse, SubscribeResponse> {
	/// Active subscriptions. All incoming packets are matched against these
	/// filters.
	active_subscriptions: Vec<Subscription<T>>,

	pub outgoing: VecDeque<Packet>,

	/// Incoming Publish packets.
	pub incoming: HashMap<PacketId, packets::Publish>,

	publish_state: HashMap<PacketId, OutgoingPublish<PublishResponse>>,
	subscribe_state: HashMap<PacketId, SubscribeState<T, SubscribeResponse>>,
	unsubscribe_state: HashMap<PacketId, UnsubscribeState>,
	resubscribe_state: Option<(PacketId, ResponseTx<()>)>,

	publish_packet_id: WrappingNonZeroU16,
	subscribe_packet_id: WrappingNonZeroU16,
	unsubscribe_packet_id: WrappingNonZeroU16,

	pub connect: packets::Connect,
	pub keep_alive: Duration,
	pub pingreq_state: Option<Instant>,
}

#[derive(Debug)]
pub struct Subscription<T> {
	filter: FilterBuf,
	qos: QoS,
	channel: T,
}

#[derive(Debug)]
enum OutgoingPublish<R> {
	Ack {
		topic: TopicBuf,
		payload: Bytes,
		retain: bool,
		qos: QoS,
		response: R,
		attempts: u16,
		created_at: Instant,
	},
	Rec {
		topic: TopicBuf,
		payload: Bytes,
		retain: bool,
		qos: QoS,
		response: R,
		attempts: u16,
		created_at: Instant,
	},
	Comp {
		response: R,
	},
}

#[derive(Debug)]
struct SubscribeState<T, R> {
	filters: Vec<(FilterBuf, QoS)>,
	channel: T,
	response: R,
	expires: Instant,
}

#[derive(Debug)]
struct UnsubscribeState {
	filters: Vec<FilterBuf>,
	response: ResponseTx<()>,
	expires: Instant,
}

impl<T, PR, SR> Default for ClientState<T, PR, SR> {
	fn default() -> Self {
		Self {
			active_subscriptions: Vec::new(),
			outgoing: VecDeque::new(),
			incoming: Default::default(),
			publish_state: Default::default(),
			subscribe_state: Default::default(),
			unsubscribe_state: Default::default(),
			resubscribe_state: None,
			publish_packet_id: WrappingNonZeroU16::MAX,
			subscribe_packet_id: WrappingNonZeroU16::MAX,
			unsubscribe_packet_id: WrappingNonZeroU16::MAX,
			connect: Default::default(),
			keep_alive: Duration::default(),
			pingreq_state: Default::default(),
		}
	}
}

impl<T, PublishResponse, SubscribeResponse> ClientState<T, PublishResponse, SubscribeResponse> {
	pub fn subscribe(
		&mut self,
		filters: Vec<(FilterBuf, QoS)>,
		channel: T,
		response: SubscribeResponse,
	) {
		// Generate an ID for the subscribe packet.
		let id = self.generate_subscribe_id();
		self.subscribe_state.insert(
			id,
			SubscribeState {
				filters: filters.clone(),
				channel,
				response,
				expires: Instant::now(),
			},
		);

		// Generate the packet to send.
		self.outgoing.push_back(Subscribe { id, filters }.into());
	}

	pub fn unsubscribe(&mut self, filters: Vec<FilterBuf>, response: ResponseTx<()>) {
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

	pub fn unsuback(&mut self, unsuback: UnsubAck) -> Result<(), StateError> {
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

		let _ = response.send(());
		Ok(())
	}

	#[inline]
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

	#[inline]
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

	#[inline]
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

	pub fn generate_resubscribe(&mut self, response_tx: ResponseTx<()>) -> Option<Packet> {
		if !self.active_subscriptions.is_empty() {
			let mut filters = Vec::new();
			for Subscription { filter, qos, .. } in self.active_subscriptions.iter() {
				filters.push((filter.clone(), *qos));
			}

			let id = self.generate_subscribe_id();
			let packet = crate::packets::Subscribe { id, filters };
			self.resubscribe_state = Some((id, response_tx));

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
}

impl<T: fmt::Debug, R, SubscribeResponse: Sender<Vec<(FilterBuf, QoS)>>>
	ClientState<T, R, SubscribeResponse>
{
	pub fn pubrel(&mut self, id: PacketId) -> Result<Publish, StateError> {
		let Some(publish) = self.incoming.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubRel));
		};

		Ok(publish)
	}

	/// Finds a channel to publish messages for `topic` to.
	pub fn find_publish_channel(&self, topic: &Topic) -> Option<&T> {
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
			tracing::error!(topic = ?topic, subscriptions = ?self.active_subscriptions, "failed to find channel for");
			return None;
		};

		let time = start.elapsed();
		tracing::debug!(topic = ?topic, filter = ?filter, score = ?score, time = ?time, "found channel for");

		Some(channel)
	}
}

impl<T: Clone + fmt::Debug, R, SubscribeResponse: Sender<Vec<(FilterBuf, QoS)>>>
	ClientState<T, R, SubscribeResponse>
{
	pub fn suback(&mut self, ack: SubAck) -> Result<(), StateError> {
		let SubAck { id, result } = ack;
		// Check that this isn't a resubscribe.
		if let Some((resub_id, channel)) = self.resubscribe_state.take() {
			tracing::error!("Found re-subscribe packet, expected SubAck {{ id: {id} }}");
			if resub_id == id {
				if channel.send(()).is_err() {
					tracing::warn!("response channel for resubscribe closed");
				}
				self.resubscribe_state = None;
				tracing::info!("resubscribe successful");
				return Ok(());
			}
		}
		// Ascertain that we have an active subscription request for the SubAck
		// packet ID.
		//
		let Some(subscribe_state) = self.subscribe_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::SubAck));
		};

		let SubscribeState {
			filters,
			channel,
			response,
			..
		} = subscribe_state;

		if result.len() != filters.len() {
			return Err(StateError::ProtocolError(
				"SubAck payload length does not correspond to Subscribe payload length",
			));
		}

		let successful_filters: Vec<_> = result
			.into_iter()
			.zip(filters)
			.filter_map(|(result_qos, (requested_filter, requested_qos))| {
				let qos = result_qos.ok()?;
				Some((requested_filter, requested_qos, qos))
			})
			.collect();

		'outer: for (filter, _, qos) in &successful_filters {
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

			tracing::debug!(filters = ?self.active_subscriptions);
		}

		let _ = response
			.send(
				successful_filters
					.into_iter()
					.map(|(f, _, q)| (f, q))
					.collect(),
			)
			.is_err();

		// We don't generate any packets in response to SubAck.
		Ok(())
	}
}

impl<T, R: Sender<()>, SubscribeResponse> ClientState<T, R, SubscribeResponse> {
	pub fn publish(
		&mut self,
		topic: TopicBuf,
		payload: Bytes,
		qos: QoS,
		retain: bool,
		response: R,
	) {
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
				let _ = response.send(());
			}
			QoS::AtLeastOnce => {
				let id = self.generate_publish_id();
				self.publish_state.insert(
					id,
					OutgoingPublish::Ack {
						topic: topic.clone(),
						payload: payload.clone(),
						retain,
						qos,
						response,
						attempts: 1,
						created_at: Instant::now(),
					},
				);

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
			}
			QoS::ExactlyOnce => {
				let id = self.generate_publish_id();
				self.publish_state.insert(
					id,
					OutgoingPublish::Rec {
						topic: topic.clone(),
						payload: payload.clone(),
						retain,
						qos,
						response,
						attempts: 1,
						created_at: Instant::now(),
					},
				);

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
			}
		}
	}

	/// Handles a PubAck packet.
	pub fn puback(&mut self, id: NonZeroU16) -> Result<(), StateError> {
		let Some(OutgoingPublish::Ack { response, .. }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubAck));
		};

		let _ = response.send(());
		Ok(())
	}

	/// Handles a PubRec packet.
	pub fn pubrec(&mut self, id: NonZeroU16) -> Result<(), StateError> {
		let Some(OutgoingPublish::Rec { response, .. }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubRec));
		};

		self.publish_state
			.insert(id, OutgoingPublish::Comp { response });
		self.outgoing.push_back(packets::PubRel { id }.into());
		Ok(())
	}

	pub fn pubcomp(&mut self, id: NonZeroU16) -> Result<(), StateError> {
		let Some(OutgoingPublish::Comp { response }) = self.publish_state.remove(&id) else {
			return Err(StateError::Unsolicited(PacketType::PubComp));
		};

		let _ = response.send(());
		Ok(())
	}
}
