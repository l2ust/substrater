// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::{
	balances, claims, collective, crab_issuing, democracy, dvm, elections_phragmen, ethereum,
	ethereum_backing, ethereum_relay, grandpa, im_online, membership, offences, proxy,
	relay_authorities, scheduler, session, staking, sudo, system, treasury, vesting,
};

pub type BlockNumber = u32;
pub type Hash = [u8; 32];
pub type AccountId = [u8; 32];
pub type Balance = u128;

#[derive(Debug, Decode)]
pub enum Event {
	#[codec(index = "0")]
	System(system::Event<AccountId>),
	#[codec(index = "4")]
	Balances(balances::Event<AccountId, Balance>),
	#[codec(index = "5")]
	Kton(balances::Event<AccountId, Balance>),
	#[codec(index = "8")]
	Staking(staking::Event<BlockNumber, AccountId, Balance>),
	#[codec(index = "9")]
	Offences(offences::Event),
	#[codec(index = "11")]
	Session(session::Event),
	#[codec(index = "13")]
	Grandpa(grandpa::Event<AccountId>),
	#[codec(index = "14")]
	ImOnline(im_online::Event<AccountId, staking::Exposure<AccountId, Balance>>),
	#[codec(index = "16")]
	Democracy(democracy::Event<BlockNumber, Hash, AccountId, Balance>),
	#[codec(index = "17")]
	Council(collective::Event<Hash, AccountId>),
	#[codec(index = "18")]
	TechnicalCommittee(collective::Event<Hash, AccountId>),
	#[codec(index = "19")]
	ElectionsPhragmen(elections_phragmen::Event<AccountId, Balance>),
	#[codec(index = "20")]
	TechnicalMembership(membership::Event),
	#[codec(index = "21")]
	Treasury(treasury::Event<Hash, AccountId, Balance>),
	#[codec(index = "22")]
	Claims(claims::Event<AccountId, Balance>),
	#[codec(index = "23")]
	Vesting(vesting::Event<AccountId, Balance>),
	#[codec(index = "24")]
	Scheduler(scheduler::Event<BlockNumber>),
	#[codec(index = "30")]
	Proxy(proxy::Event<Hash, AccountId, ProxyType>),
	#[codec(index = "31")]
	Sudo(sudo::Event<AccountId>),
	#[codec(index = "32")]
	CrabIssuing(crab_issuing::Event<AccountId, Balance>),
	#[codec(index = "35")]
	EthereumRelay(ethereum_relay::Event<AccountId>),
	#[codec(index = "36")]
	EthereumBacking(ethereum_backing::Event<AccountId, Balance>),
	#[codec(index = "39")]
	EVM(dvm::Event<AccountId>),
	#[codec(index = "40")]
	Ethereum(ethereum::Event),
	#[codec(index = "41")]
	EthereumRelayAuthorities(relay_authorities::Event<BlockNumber, AccountId>),
}

#[derive(Debug, Decode)]
pub enum ProxyType {
	Any,
	NonTransfer,
	Governance,
	Staking,
	EthereumBridge,
}
