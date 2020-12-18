// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	NewTerm(Vec<(AccountId, Balance)>),
	EmptyTerm,
	ElectionError,
	MemberKicked(AccountId),
	MemberRenounced(AccountId),
	VoterReported(AccountId, AccountId, bool),
}
