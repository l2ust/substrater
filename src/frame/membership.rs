// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event<AccountId> {
	MemberAdded,
	MemberRemoved,
	MembersSwapped,
	MembersReset,
	KeyChanged,
	Dummy(Vec<(AccountId, Event<AccountId>)>),
}
