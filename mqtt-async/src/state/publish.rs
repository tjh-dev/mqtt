use super::{subscriptions::SubscriptionsManager, ResponseTx, StateError};
use crate::command::PublishCommand;
use mqtt_core::{Packet, PacketId, PacketType, PubAck, PubComp, PubRec, PubRel, Publish, QoS};
use std::{
	collections::{HashMap, HashSet},
	num::NonZeroU16,
};
use tokio::sync::mpsc::error::TrySendError;

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
			Some(PubComp { id }.into())
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
				(_, _, Err(TrySendError::Closed(publish))) => {
					tracing::error!("failed to deliver Publish packet {publish:?}");
					unimplemented!();
				}
				(QoS::AtMostOnce, Some(_), _)
				| (QoS::AtLeastOnce, None, _)
				| (QoS::ExactlyOnce, None, _) => {
					unreachable!();
				}
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
					Ok(Some(PubAck { id }.into()))
				}
				(QoS::ExactlyOnce, Some(id), Ok(_)) => {
					// We've recevied the Publish packet, found a suitable destination,
					// and successfully delivered it.
					//
					// Send a PubRec.
					self.awaiting_pubrel.insert(id);
					Ok(Some(PubRec { id }.into()))
				}
				(QoS::AtLeastOnce, Some(_), Err(TrySendError::Full(publish)))
				| (QoS::ExactlyOnce, Some(_), Err(TrySendError::Full(publish))) => {
					tracing::error!("failed to deliver Publish packet, channel full, {publish:?}");
					Err(StateError::DeliveryFailure(publish))
				}
			}
		} else {
			tracing::error!("failed to acquire destination for {publish:?}");
			Ok(None)
		}
	}

	pub fn handle_pubrel(&mut self, pubrel: PubRel) -> Result<Option<Packet>, StateError> {
		if !self.awaiting_pubrel.remove(&pubrel.id) {
			return Err(StateError::Unsolicited(PacketType::PubRel));
		}

		// See if we've already have queued PubComp.
		if self.queued_pubcomp.contains(&pubrel.id) {
			self.queued_pubcomp.remove(&pubrel.id);
			Ok(Some(PubComp { id: pubrel.id }.into()))
		} else {
			// Add to the set of awaiting PubRels.
			self.queued_pubrel.insert(pubrel.id);
			Ok(None)
		}
	}
}

impl Default for OutgoingPublishManager {
	fn default() -> Self {
		Self {
			publish_id: NonZeroU16::MAX,
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

	pub fn handle_puback(&mut self, puback: PubAck) -> Result<(), StateError> {
		let tx = self
			.awaiting_puback
			.remove(&puback.id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		let _ = tx.send(());
		Ok(())
	}

	pub fn handle_pubrec(&mut self, pubrec: PubRec) -> Result<Option<Packet>, StateError> {
		let tx = self
			.awaiting_pubrec
			.remove(&pubrec.id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		self.awaiting_pubcomp.insert(pubrec.id, tx);
		Ok(Some(PubRel { id: pubrec.id }.into()))
	}

	/// The outgoing Publish cycle has been concluded.
	///
	pub fn handle_pubcomp(&mut self, pubcomp: PubComp) -> Result<(), StateError> {
		let tx = self
			.awaiting_pubcomp
			.remove(&pubcomp.id)
			.ok_or(StateError::Unsolicited(mqtt_core::PacketType::PubAck))?;

		let _ = tx.send(());
		Ok(())
	}

	/// Generates a new, non-zero packet ID.
	#[inline]
	fn generate_id(&mut self) -> PacketId {
		loop {
			self.publish_id = self.publish_id.checked_add(1).unwrap_or(NonZeroU16::MIN);

			// We don't need to check `awaiting_pubcomp`
			if !(self.awaiting_puback.contains_key(&self.publish_id)
				| self.awaiting_pubrec.contains_key(&self.publish_id))
			{
				break;
			}
		}
		self.publish_id
	}
}
