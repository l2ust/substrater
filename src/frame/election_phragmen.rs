// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	NewTerm(Vec<(AccountId, Balance)>),
	EmptyTerm,
	ElectionError,
	MemberKicked(AccountId),
	CandidateSlashed(AccountId, Balance),
	SeatHolderSlashed(AccountId, Balance),
	MemberRenounced(AccountId),
	VoterReported(AccountId, AccountId, bool),
}
