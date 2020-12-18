// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::{
	balances, claims, collective, crab_issuing, democracy, election_phragmen, ethereum_backing,
	ethereum_relay, grandpa, im_online, membership, offences, proxy, scheduler, session, staking,
	sudo, system, treasury, vesting,
};

pub type BlockNumber = u32;
pub type Hash = [u8; 32];
pub type AccountId = [u8; 32];
pub type Balance = u128;

#[derive(Debug, Decode)]
pub enum Event {
	System(system::Event<AccountId>),
	Balances(balances::Event<AccountId, Balance>),
	Kton(balances::Event<AccountId, Balance>),
	Staking(staking::Event<BlockNumber, AccountId, Balance>),
	Offences(offences::Event),
	Session(session::Event),
	Grandpa(grandpa::Event<AccountId>),
	ImOnline(im_online::Event<AccountId, staking::Exposure<AccountId, Balance>>),
	Democracy(democracy::Event<BlockNumber, Hash, AccountId, Balance>),
	Council(collective::Event<Hash, AccountId>),
	TechnicalCommittee(collective::Event<Hash, AccountId>),
	ElectionsPhragmen(election_phragmen::Event<AccountId, Balance>),
	TechnicalMembership(membership::Event<AccountId>),
	Treasury(treasury::Event<Hash, AccountId, Balance>),
	Claims(claims::Event<AccountId, Balance>),
	Vesting(vesting::Event<AccountId, Balance>),
	Scheduler(scheduler::Event<BlockNumber>),
	Proxy(proxy::Event<Hash, AccountId, ProxyType>),
	Sudo(sudo::Event<AccountId>),
	CrabIssuing(crab_issuing::Event<AccountId, Balance>),
	EthereumRelay(ethereum_relay::Event<AccountId>),
	EthereumBacking(ethereum_backing::Event<AccountId, Balance>),
	EVM,
	Ethereum,
	EthereumRelayAuthorities,
}

#[derive(Debug, Decode)]
pub enum ProxyType {
	Any,
	NonTransfer,
	Governance,
	Staking,
	EthereumBridge,
}
