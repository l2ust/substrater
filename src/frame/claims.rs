// --- crates.io ---
use parity_scale_codec::Decode;

pub type Address = [u8; 20];

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	Claimed(AccountId, Address, Balance),
}
