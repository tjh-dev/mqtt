mod subscriptions;

use self::subscriptions::SubscriptionsManager;
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
			Command::Subscribe(command) => self.subscriptions.handle_subscribe_command(command),
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
			Packet::Publish(publish) => self.handle_publish(publish),
			Packet::SubAck { id, result } => {
				self.subscriptions.handle_suback(id, result).map(|_| None)
			}
			Packet::Connect(_)
			| Packet::ConnAck { .. }
			| Packet::Subscribe { .. }
			| Packet::Unsubscribe { .. }
			| Packet::PingReq
			| Packet::Disconnect => Err(StateError::InvalidPacket),
			_ => unimplemented!(),
		}
	}

	fn handle_publish(&mut self, publish: Publish) -> Result<Option<Packet>, StateError> {
		Ok(None)
	}
}
