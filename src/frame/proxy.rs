// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::DispatchResult;

#[derive(Debug, Decode)]
pub enum Event<Hash, AccountId, ProxyType> {
	ProxyExecuted(DispatchResult),
	AnonymousCreated(AccountId, AccountId, ProxyType, u16),
	Announced(AccountId, AccountId, Hash),
}
