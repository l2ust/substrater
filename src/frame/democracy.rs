// --- crates.io ---
use parity_scale_codec::Decode;

pub type PropIndex = u32;
pub type ReferendumIndex = u32;

#[derive(Debug, Decode)]
pub enum Event<BlockNumber, Hash, AccountId, Balance> {
	Proposed(PropIndex, Balance),
	Tabled(PropIndex, Balance, Vec<AccountId>),
	ExternalTabled,
	Started(ReferendumIndex, VoteThreshold),
	Passed(ReferendumIndex),
	NotPassed(ReferendumIndex),
	Cancelled(ReferendumIndex),
	Executed(ReferendumIndex, bool),
	Delegated(AccountId, AccountId),
	Undelegated(AccountId),
	Vetoed(AccountId, Hash, BlockNumber),
	PreimageNoted(Hash, AccountId, Balance),
	PreimageUsed(Hash, AccountId, Balance),
	PreimageInvalid(Hash, ReferendumIndex),
	PreimageMissing(Hash, ReferendumIndex),
	PreimageReaped(Hash, AccountId, Balance, AccountId),
	Unlocked(AccountId),
	Blacklisted(Hash),
}
#[derive(Debug, Decode)]
pub enum VoteThreshold {
	SuperMajorityApprove,
	SuperMajorityAgainst,
	SimpleMajority,
}
