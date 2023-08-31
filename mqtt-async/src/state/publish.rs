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
			let result = channel.try_send(publish);

			match (qos, result, id) {
				(QoS::AtMostOnce, _, None) => Ok(None),
				(QoS::AtLeastOnce, Ok(_), Some(id)) => Ok(Some(Packet::PubAck { id })),
				(QoS::ExactlyOnce, Ok(_), Some(id)) => {
					self.awaiting_pubrel.insert(id);
					Ok(Some(Packet::PubRec { id }))
				}
				_ => panic!(),
			}
		} else {
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
			tx,
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

		let tx = match qos {
			QoS::AtMostOnce => Some(tx),
			QoS::AtLeastOnce => {
				self.awaiting_puback.insert(id, tx);
				None
			}
			QoS::ExactlyOnce => {
				self.awaiting_pubrec.insert(id, tx);
				None
			}
		};

		// TODO: Send this AFTER the Publish packet has been successfully written
		// to the stream.
		//
		if let Some(tx) = tx {
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
