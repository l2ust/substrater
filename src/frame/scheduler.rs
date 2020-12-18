// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::support::DispatchResult;

pub type TaskAddress<BlockNumber> = (BlockNumber, u32);

#[derive(Debug, Decode)]
pub enum Event<BlockNumber> {
	Scheduled(BlockNumber, u32),
	Canceled(BlockNumber, u32),
	Dispatched(TaskAddress<BlockNumber>, Option<Vec<u8>>, DispatchResult),
}
