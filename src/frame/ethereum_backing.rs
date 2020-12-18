// --- crates.io ---
use parity_scale_codec::Decode;

pub type EcdsaAddress = [u8; 20];
pub type DepositId = U256;
pub type U256 = [u64; 4];
pub type EthereumTransactionIndex = (H256, u64);
pub type H256 = [u8; 32];

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	RedeemRing(AccountId, Balance, EthereumTransactionIndex),
	RedeemKton(AccountId, Balance, EthereumTransactionIndex),
	RedeemDeposit(AccountId, DepositId, Balance, EthereumTransactionIndex),
	LockRing(AccountId, EcdsaAddress, u8, Balance),
	LockKton(AccountId, EcdsaAddress, u8, Balance),
}
