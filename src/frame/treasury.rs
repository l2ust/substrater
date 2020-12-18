// --- crates.io ---
use parity_scale_codec::Decode;

pub type ProposalIndex = u32;
pub type BountyIndex = u32;

#[derive(Debug, Decode)]
pub enum Event<Hash, AccountId, Balance> {
	Proposed(ProposalIndex),
	Spending(Balance, Balance),
	Awarded(ProposalIndex, Balance, Balance, AccountId),
	Rejected(ProposalIndex, Balance, Balance),
	Burnt(Balance, Balance),
	Rollover(Balance, Balance),
	DepositRing(Balance),
	DepositKton(Balance),
	NewTip(Hash),
	TipClosing(Hash),
	TipClosed(Hash, AccountId, Balance),
	TipRetracted(Hash),
	BountyProposed(BountyIndex),
	BountyRejected(BountyIndex, Balance),
	BountyBecameActive(BountyIndex),
	BountyAwarded(BountyIndex, AccountId),
	BountyClaimed(BountyIndex, Balance, AccountId),
	BountyCanceled(BountyIndex),
	BountyExtended(BountyIndex),
}
