// --- crates.io ---
use parity_scale_codec::Decode;

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	VestingUpdated(AccountId, Balance),
	VestingCompleted(AccountId),
}
