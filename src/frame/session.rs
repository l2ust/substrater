// --- crates.io ---
use parity_scale_codec::Decode;
// --- substrater ---
use crate::frame::staking::SessionIndex;

#[derive(Debug, Decode)]
pub enum Event {
	NewSession(SessionIndex),
}
