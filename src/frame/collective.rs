// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::DispatchResult;

pub type ProposalIndex = u32;
pub type MemberCount = u32;

#[derive(Debug, Decode)]
pub enum Event<Hash, AccountId> {
	Proposed(AccountId, ProposalIndex, Hash, MemberCount),
	Voted(AccountId, Hash, bool, MemberCount, MemberCount),
	Approved(Hash),
	Disapproved(Hash),
	Executed(Hash, DispatchResult),
	MemberExecuted(Hash, DispatchResult),
	Closed(Hash, MemberCount, MemberCount),
}
