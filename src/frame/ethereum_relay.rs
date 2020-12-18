// --- crates.io ---
use parity_scale_codec::Decode;

pub type EthereumBlockNumber = u64;
pub type U256 = [u64; 4];
pub type H256 = [u8; 32];
pub type Bytes = Vec<u8>;
pub type Bloom = [u8; 256];
pub type EthereumAddress = [u8; 20];

#[derive(Debug, Decode)]
pub enum Event<AccountId> {
	Affirmed(AccountId, RelayAffirmationId),
	DisputedAndAffirmed(AccountId, RelayAffirmationId),
	Extended(AccountId, RelayAffirmationId),
	NewRound(EthereumBlockNumber, Vec<EthereumBlockNumber>),
	GameOver(EthereumBlockNumber),
	RemoveConfirmedParcel(EthereumBlockNumber),
	VerifyReceipt(AccountId, EthereumReceipt, EthereumHeader),
	Pended(EthereumBlockNumber),
	GuardVoted(EthereumBlockNumber, bool),
	PendingRelayHeaderParcelConfirmed(EthereumBlockNumber, Vec<u8>),
	PendingRelayHeaderParcelRejected(EthereumBlockNumber),
}
#[derive(Debug, Decode)]
pub struct RelayAffirmationId {
	game_id: EthereumBlockNumber,
	round: u32,
	index: u32,
}
#[derive(Debug, Decode)]
pub struct EthereumReceipt {
	pub gas_used: U256,
	pub log_bloom: Bloom,
	pub logs: Vec<LogEntry>,
	pub outcome: TransactionOutcome,
}
#[derive(Debug, Decode)]
pub struct LogEntry {
	pub address: EthereumAddress,
	pub topics: Vec<H256>,
	pub data: Bytes,
}
#[derive(Debug, Decode)]
pub enum TransactionOutcome {
	Unknown,
	StateRoot(H256),
	StatusCode(u8),
}
#[derive(Debug, Decode)]
pub struct EthereumHeader {
	pub parent_hash: H256,
	pub timestamp: u64,
	pub number: EthereumBlockNumber,
	pub author: EthereumAddress,
	pub transactions_root: H256,
	pub uncles_hash: H256,
	pub extra_data: Bytes,
	pub state_root: H256,
	pub receipts_root: H256,
	pub log_bloom: Bloom,
	pub gas_used: U256,
	pub gas_limit: U256,
	pub difficulty: U256,
	pub seal: Vec<Bytes>,
	pub hash: Option<H256>,
}
