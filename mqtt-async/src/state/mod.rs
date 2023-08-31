mod publish;
mod subscriptions;

use self::{
	publish::{IncomingPublishManager, OutgoingPublishManager},
	subscriptions::SubscriptionsManager,
};
use crate::command::Command;
use mqtt_core::{Packet, PacketType, Publish};
use tokio::sync::{mpsc, oneshot};

type PublishTx = mpsc::Sender<Publish>;
type ResponseTx<T> = oneshot::Sender<T>;

/// Mantains Client state after ConnAck has been recevied.
///
#[derive(Debug, Default)]
pub struct State {
	subscriptions: SubscriptionsManager,
	incoming_publish: IncomingPublishManager,
	outgoing_publish: OutgoingPublishManager,
}

#[derive(Debug)]
pub enum StateError {
	Unsolicited(PacketType),

	/// The Client recevied a packet that the Server should not send.
	InvalidPacket,

	ProtocolError(&'static str),
}

impl State {
	pub fn process_client_command(&mut self, command: Command) -> Option<Packet> {
		match command {
			Command::Publish(command) => self.outgoing_publish.handle_publish_command(command),
			Command::PublishComplete { id } => self.incoming_publish.handle_pubcomp_command(id),
			Command::Subscribe(command) => self.subscriptions.handle_subscribe_command(command),
			Command::Shutdown => Some(Packet::Disconnect),
			_ => None,
		}
	}

	/// Process an incoming Packet from the broker.
	///
	pub fn process_incoming_packet(
		&mut self,
		packet: Packet,
	) -> Result<Option<Packet>, StateError> {
		match packet {
			Packet::Publish(publish) => self
				.incoming_publish
				.handle_publish(&self.subscriptions, publish),
			Packet::PubAck { id } => self.outgoing_publish.handle_puback(id).map(|_| None),
			Packet::PubRec { id } => self.outgoing_publish.handle_pubrec(id),
			Packet::PubRel { id } => self.incoming_publish.handle_pubrel(id),
			Packet::PubComp { id } => self.outgoing_publish.handle_pubcomp(id).map(|_| None),
			Packet::SubAck { id, result } => {
				self.subscriptions.handle_suback(id, result).map(|_| None)
			}
			Packet::PingResp => Ok(None),
			Packet::Connect(_)
			| Packet::ConnAck { .. }
			| Packet::Subscribe { .. }
			| Packet::Unsubscribe { .. }
			| Packet::PingReq
			| Packet::Disconnect => Err(StateError::InvalidPacket),
			_ => unimplemented!(),
		}
	}
}
