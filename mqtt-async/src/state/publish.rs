use super::{subscriptions::SubscriptionsManager, ResponseTx, StateError};
use crate::command::PublishCommand;
use mqtt_core::{Packet, PacketId, PacketType, Publish, QoS};
use std::{
	collections::{HashMap, HashSet},
	num::NonZeroU16,
};

#[derive(Debug, Default)]
pub struct IncomingPublishManager {
	awaiting_pubrel: HashSet<PacketId>,
	queued_pubrel: HashSet<PacketId>,
	queued_pubcomp: HashSet<PacketId>,
}

#[derive(Debug)]
pub struct OutgoingPublishManager {
	/// The next packet ID to use for Publish requests.
	publish_id: NonZeroU16,
	awaiting_puback: HashMap<PacketId, ResponseTx<()>>,
	awaiting_pubrec: HashMap<PacketId, ResponseTx<()>>,
	awaiting_pubcomp: HashMap<PacketId, ResponseTx<()>>,
}

impl IncomingPublishManager {
	pub fn handle_pubcomp_command(&mut self, id: PacketId) -> Option<Packet> {
		// See if we've already received a matching PubRel.
		if self.queued_pubrel.contains(&id) {
			self.queued_pubrel.remove(&id);
			Some(Packet::PubComp { id })
		} else {
			// Add to the set of acknowledged PubComps.
			tracing::info!(
				"queueing PubComp {{ id: {id} }} to await incoming PubRel {{ id: {id} }}"
			);
			self.queued_pubcomp.insert(id);
			None
		}
	}

	pub fn handle_publish(
		&mut self,
		subscriptions: &SubscriptionsManager,
		publish: Publish,
	) -> Result<Option<Packet>, StateError> {
		let channel = subscriptions.find_publish_channel(publish.topic());

		if let Some(channel) = channel {
			tracing::info!("found channel for {}", publish.topic());

			let qos = publish.qos();
			let id = publish.id();

			// Attempt to deliver the Publish packet.
			let result = channel.try_send(publish);

			match (qos, id, result) {
				(QoS::AtMostOnce, None, _) => {
					// We've received the Publish packet, found a suitable destination,
					// and *tried* to deliver it.
					Ok(None)
				}
				(QoS::AtLeastOnce, Some(id), Ok(_)) => {
					// We've recevied the Publish packet, found a suitable destination,
					// and successfully delivered it.
					//
					// Send a PubAck.
					Ok(Some(Packet::PubAck { id }))
				}
				(QoS::AtLeastOnce, Some(_), Err(e)) => {
					tracing::error!("failed to deliver Publish packet, {e:?}");
					Ok(None)
				}
				(QoS::ExactlyOnce, Some(id), Ok(_)) => {
					// We've recevied the Publish packet, found a suitable destination,
					// and successfully delivered it.
					//
					// Send a PubRec.
					self.awaiting_pubrel.insert(id);
					Ok(Some(Packet::PubRec { id }))
				}
				(QoS::ExactlyOnce, Some(_), Err(e)) => {
					tracing::error!("failed to deliver Publish packet, {e:?}");
					Ok(None)
				}
				_ => unreachable!(),
			}
		} else {
			tracing::error!("failed to acquire destination for {publish:?}");
			Ok(None)
		}
	}

	pub fn handle_pubrel(&mut self, id: u16) -> Result<Option<Packet>, StateError> {
		if !self.awaiting_pubrel.remove(&id) {
			return Err(StateError::Unsolicited(PacketType::PubRel));
		}

		// See if we've already have queued PubComp.
		if self.queued_pubcomp.contains(&id) {
			self.queued_pubcomp.remove(&id);
			Ok(Some(Packet::PubComp { id }))
		} else {
			// Add to the set of awaiting PubRels.
			self.queued_pubrel.insert(id);
			Ok(None)
		}
	}
}

impl Default for OutgoingPublishManager {
	fn default() -> Self {
		Self {
			publish_id: unsafe { NonZeroU16::new_unchecked(1) },
			awaiting_puback: Default::default(),
			awaiting_pubrec: Default::default(),
			awaiting_pubcomp: Default::default(),
		}
	}
}

impl OutgoingPublishManager {
	pub fn handle_publish_command(&mut self, command: PublishCommand) -> Option<Packet> {
		let id = self.generate_id();
		let PublishCommand {
			topic,
			payload,
			qos,
			retain,
			response_tx,
		} = command;
		let packet = match qos {
			QoS::AtMostOnce => Packet::Publish(Publish::AtMostOnce {
				topic,
				payload,
				retain,
			}),
			QoS::AtLeastOnce => Packet::Publish(Publish::AtLeastOnce {
				id,
				topic,
				payload,
				retain,
				duplicate: false,
			}),
			QoS::ExactlyOnce => Packet::Publish(Publish::ExactlyOnce {
				id,
				topic,
				payload,
				retain,
				duplicate: false,
			}),
		};

		let response_tx = match qos {
			QoS::AtMostOnce => Some(response_tx),
			QoS::AtLeastOnce => {
				self.awaiting_puback.insert(id, response_tx);
				None
			}
			QoS::ExactlyOnce => {
				self.awaiting_pubrec.insert(id, response_tx);
				None
			}
		};

		// TODO: Send this AFTER the Publish packet has been successfully written
		// to the stream.
		//
		if let Some(tx) = response_tx {
			let _ = tx.send(());
		}

		Some(packet)
	}

	pub fn handle_puback(&mut self, id: u16) -> Result<(), StateError> {
		let tx = self
			.awaiting_puback
			.remove(&id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		let _ = tx.send(());
		Ok(())
	}

	pub fn handle_pubrec(&mut self, id: u16) -> Result<Option<Packet>, StateError> {
		let tx = self
			.awaiting_pubrec
			.remove(&id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		self.awaiting_pubcomp.insert(id, tx);
		Ok(Some(Packet::PubRel { id }))
	}

	/// The outgoing Publish cycle has been concluded.
	///
	pub fn handle_pubcomp(&mut self, id: u16) -> Result<(), StateError> {
		let tx = self
			.awaiting_pubcomp
			.remove(&id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		let _ = tx.send(());
		Ok(())
	}

	/// Generates a new, non-zero packet ID.
	///
	/// TODO: This does not currently verify that the packet ID isn't already in
	/// use.
	#[inline(always)]
	fn generate_id(&mut self) -> PacketId {
		let current = self.publish_id.get();
		self.publish_id = self
			.publish_id
			.checked_add(1)
			.unwrap_or(unsafe { NonZeroU16::new_unchecked(1) });
		current
	}
}
