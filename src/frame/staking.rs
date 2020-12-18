// --- crates.io ---
use parity_scale_codec::Decode;

pub type SessionIndex = u32;
pub type EraIndex = u32;
pub type TsInMs = u64;
pub type Power = u32;

#[derive(Debug, Decode)]
pub enum Event<BlockNumber, AccountId, Balance> {
	EraPayout(EraIndex, Balance, Balance),
	Reward(AccountId, Balance),
	Slash(AccountId, Balance, Balance),
	OldSlashingReportDiscarded(SessionIndex),
	StakingElection(ElectionCompute),
	SolutionStored(ElectionCompute),
	BondRing(Balance, TsInMs, TsInMs),
	BondKton(Balance),
	UnbondRing(Balance, BlockNumber),
	UnbondKton(Balance, BlockNumber),
	DepositsClaimed(AccountId),
	DepositsClaimedWithPunish(AccountId, Balance),
}
#[derive(Debug, Decode)]
pub enum ElectionCompute {
	OnChain,
	Signed,
	Unsigned,
}

#[derive(Debug, Decode)]
pub struct Exposure<AccountId, Balance> {
	#[codec(compact)]
	pub own_ring_balance: Balance,
	#[codec(compact)]
	pub own_kton_balance: Balance,
	pub own_power: Power,
	pub total_power: Power,
	pub others: Vec<IndividualExposure<AccountId, Balance>>,
}
#[derive(Debug, Decode)]
pub struct IndividualExposure<AccountId, Balance> {
	who: AccountId,
	#[codec(compact)]
	ring_balance: Balance,
	#[codec(compact)]
	kton_balance: Balance,
	power: Power,
}
