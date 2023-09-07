mod publish;
mod subscriptions;

use self::{
	publish::{IncomingPublishManager, OutgoingPublishManager},
	subscriptions::SubscriptionsManager,
};
use super::command::{Command, ResponseRx};
use crate::{
	packets::{Disconnect, Publish},
	Packet, PacketType,
};
use tokio::sync::{mpsc, oneshot};

pub type PublishTx = mpsc::Sender<Publish>;
pub type PublishRx = mpsc::Receiver<Publish>;

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

	DeliveryFailure(Publish),
}

impl State {
	pub fn process_client_command(&mut self, command: Command) -> Option<Packet> {
		match command {
			Command::Publish(command) => self.outgoing_publish.handle_publish_command(command),
			Command::PublishComplete { id } => self.incoming_publish.handle_pubcomp_command(id),
			Command::Subscribe(command) => self.subscriptions.handle_subscribe_command(command),
			Command::Unsubscribe(command) => self.subscriptions.handle_unsubscribe_command(command),
			Command::Shutdown => Some(Disconnect.into()),
		}
	}

	/// Process an incoming Packet from the broker.
	///
	pub async fn process_incoming_packet(
		&mut self,
		packet: Packet,
	) -> Result<Option<Packet>, StateError> {
		match packet {
			Packet::Publish(publish) => {
				self.incoming_publish
					.handle_publish(&self.subscriptions, publish)
					.await
			}
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
		}
	}

	#[must_use]
	pub fn connected(&mut self, session_present: bool) -> Option<(Packet, ResponseRx<()>)> {
		if !session_present {
			let (response_tx, response_rx) = oneshot::channel();
			let packet = self.subscriptions.generate_resubscribe(response_tx)?;
			Some((packet, response_rx))
		} else {
			None
		}
	}
}
