// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event {
	MemberAdded,
	MemberRemoved,
	MembersSwapped,
	MembersReset,
	KeyChanged,
}
