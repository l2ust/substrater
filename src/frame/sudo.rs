// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::DispatchResult;

#[derive(Debug, Decode)]
pub enum Event<AccountId> {
	Sudid(DispatchResult),
	KeyChanged(AccountId),
	SudoAsDone(DispatchResult),
}
