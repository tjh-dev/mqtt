#[derive(Debug)]
pub enum PacketType {
	Connect,
	ConnAck,
	Publish,
	PubAck,
	PubRec,
	PubRel,
	PubComp,
	Subscribe,
	SubAck,
	Unsubscribe,
	UnsubAck,
	PingReq,
	PingResp,
	Disconnect,
}
