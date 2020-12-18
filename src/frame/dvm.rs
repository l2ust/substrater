// --- crates.io ---
use parity_scale_codec::Decode;

pub type H160 = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = [u64; 4];

#[derive(Debug, Decode)]
pub enum Event<AccountId> {
	Log(Log),
	Created(H160),
	CreatedFailed(H160),
	Executed(H160),
	ExecutedFailed(H160),
	BalanceDeposit(AccountId, H160, U256),
	BalanceWithdraw(AccountId, H160, U256),
}
#[derive(Debug, Decode)]
pub struct Log {
	pub address: H160,
	pub topics: Vec<H256>,
	pub data: Vec<u8>,
}
