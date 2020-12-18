// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event<AuthorityId, IdentificationTuple> {
	HeartbeatReceived(AuthorityId),
	AllGood,
	SomeOffline(Vec<IdentificationTuple>),
}
