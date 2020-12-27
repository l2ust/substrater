// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::{DispatchError, DispatchInfo};

pub type BlockNumber = u32;
pub type Nonce = u32;
pub type Version = u32;
pub type RefCount = u32;

#[derive(Debug, Decode)]
pub struct EventRecord<Event, Hash> {
	pub phase: Phase,
	pub event: Event,
	pub topics: Vec<Hash>,
}
#[derive(Debug, Decode)]
pub enum Phase {
	ApplyExtrinsic(u32),
	Finalization,
	Initialization,
}

#[derive(Debug, Decode)]
pub enum Event<AccountId> {
	ExtrinsicSuccess(DispatchInfo),
	ExtrinsicFailed(DispatchError, DispatchInfo),
	CodeUpdated,
	NewAccount(AccountId),
	KilledAccount(AccountId),
}
