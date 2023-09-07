use crate::{Filter, FilterBuf, Packet, QoS};
use std::{
	cell::Cell,
	collections::{BTreeMap, VecDeque},
};

type InternalPacketId = u16;

#[derive(Debug, Default)]
pub struct ClientState<T> {
	outgoing_packets: VecDeque<PacketState>,
	incoming_packets: VecDeque<PacketState>,
	next_packet_id: Cell<InternalPacketId>,

	/// Active subscriptions.
	subscriptions: BTreeMap<FilterBuf, T>,
}

pub struct ClientError;

#[derive(Debug)]
pub struct PacketState(InternalPacketId, Packet);

impl<T> ClientState<T> {
	/// Handles an incoming packet.
	pub fn incoming_packet(&mut self, packet: Packet) -> Result<(), ClientError> {
		self.incoming_packets
			.push_back(PacketState(self.packet_id(), packet));
		unimplemented!()
	}

	/// Retrieves the next outgoing packet.
	pub fn next_packet(&mut self) -> Option<Packet> {
		None
	}

	pub fn add_filter(&mut self, filter: FilterBuf, value: T) -> Option<T> {
		self.subscriptions.insert(filter, value)
	}

	pub fn subscribe(&mut self, filter: &Filter, qos: QoS) -> Result<(), ClientError> {
		let packet = crate::packets::Subscribe {
			id: std::num::NonZeroU16::MIN,
			filters: vec![(filter.to_owned(), qos)],
		};

		self.queue_packet(packet.into());

		unimplemented!()
	}

	fn queue_packet(&mut self, packet: Packet) {
		self.outgoing_packets
			.push_back(PacketState(self.packet_id(), packet));
	}

	fn packet_id(&self) -> InternalPacketId {
		let id = self.next_packet_id.get();
		self.next_packet_id.set(id.wrapping_add(1));
		id
	}
}
