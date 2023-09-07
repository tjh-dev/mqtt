mod publish;
mod subscriptions;

use std::collections::VecDeque;

use self::{
	publish::{IncomingPublishManager, OutgoingPublishManager},
	subscriptions::SubscriptionsManager,
};
use super::command::Command;
use crate::{
	packets::{Disconnect, Publish},
	Packet, PacketType,
};
use tokio::{sync::mpsc, time::Instant};

pub type PublishTx = mpsc::Sender<Publish>;
pub type PublishRx = mpsc::Receiver<Publish>;

type InternalPacketId = u16;

/// Mantains Client state after ConnAck has been recevied.
///
#[derive(Debug, Default)]
pub struct State {
	/// Outgoing packets.
	packets: VecDeque<PacketState>,

	packet_id: InternalPacketId,

	subscriptions: SubscriptionsManager,
	incoming_publish: IncomingPublishManager,
	outgoing_publish: OutgoingPublishManager,
}

#[derive(Debug)]
pub struct PacketState {
	pub internal_id: InternalPacketId,
	pub packet: Packet,
	pub sent_at: Option<Instant>,
}

#[derive(Debug)]
pub enum StateError {
	Unsolicited(PacketType),

	/// The Client recevied a packet that the Server should not send.
	InvalidPacket,

	ProtocolError(&'static str),

	DeliveryFailure(Publish),
}

impl State {
	pub fn process_client_command(&mut self, command: Command) {
		let packet = match command {
			Command::Publish(command) => self.outgoing_publish.handle_publish_command(command),
			Command::PublishComplete { id } => self.incoming_publish.handle_pubcomp_command(id),
			Command::Subscribe(command) => self.subscriptions.handle_subscribe_command(command),
			Command::Unsubscribe(command) => self.subscriptions.handle_unsubscribe_command(command),
			Command::Shutdown => Some(Disconnect.into()),
		};

		if let Some(packet) = packet {
			// Add the packet to the outgoing queue.
			let internal_id = self.generate_id();
			self.packets.push_back(PacketState {
				internal_id,
				packet,
				sent_at: None,
			});
		}
	}

	/// Process an incoming Packet from the broker.
	///
	pub fn process_incoming_packet(&mut self, packet: Packet) -> Result<(), StateError> {
		let outgoing_packet = match packet {
			Packet::Publish(publish) => self
				.incoming_publish
				.handle_publish(&self.subscriptions, publish),
			Packet::PubAck(pkt) => self.outgoing_publish.handle_puback(pkt).map(|_| None),
			Packet::PubRec(pkt) => self.outgoing_publish.handle_pubrec(pkt),
			Packet::PubRel(pkt) => self.incoming_publish.handle_pubrel(pkt),
			Packet::PubComp(pkt) => self.outgoing_publish.handle_pubcomp(pkt).map(|_| None),
			Packet::SubAck(pkt) => self.subscriptions.handle_suback(pkt).map(|_| None),
			Packet::UnsubAck(pkt) => self.subscriptions.handle_unsuback(pkt).map(|_| None),
			Packet::PingResp => Ok(None),
			Packet::Connect(_)
			| Packet::ConnAck { .. }
			| Packet::Subscribe { .. }
			| Packet::Unsubscribe { .. }
			| Packet::PingReq
			| Packet::Disconnect => Err(StateError::InvalidPacket),
		}?;

		if let Some(packet) = outgoing_packet {
			// Add the packet to the outgoing queue.
			let internal_id = self.generate_id();
			self.packets.push_back(PacketState {
				internal_id,
				packet,
				sent_at: None,
			});
		}

		Ok(())
	}

	pub fn next_packet<T, E, F: FnOnce(&Packet) -> crate::Result<T>>(
		&mut self,
		f: F,
	) -> crate::Result<()> {
		let Some(mut next_packet) = self.packets.pop_front() else {
			// There is no packet to send.
			return Ok(());
		};

		let packet = &next_packet.packet;
		match f(&packet) {
			Ok(_) => Ok(()),
			Err(e) => {
				next_packet.sent_at = Some(Instant::now());
				self.packets.push_back(next_packet);
				Err(e)
			}
		}
	}

	#[inline]
	fn generate_id(&mut self) -> InternalPacketId {
		self.packet_id = self.packet_id.wrapping_add(1);
		self.packet_id
	}
}
