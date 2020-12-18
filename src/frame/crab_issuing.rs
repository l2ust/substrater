// --- crates.io ---
use parity_scale_codec::Decode;

pub type MappedRing = u128;

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	DummyEvent(AccountId, Balance, MappedRing),
}
