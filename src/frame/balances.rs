// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::BalanceStatus;

#[derive(Debug, Decode)]
pub enum Event<AccountId, Balance> {
	Endowed(AccountId, Balance),
	DustLost(AccountId, Balance),
	Transfer(AccountId, AccountId, Balance),
	BalanceSet(AccountId, Balance, Balance),
	Deposit(AccountId, Balance),
	Reserved(AccountId, Balance),
	Unreserved(AccountId, Balance),
	ReserveRepatriated(AccountId, AccountId, Balance, BalanceStatus),
}
