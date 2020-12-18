// --- crates.io ---
use parity_scale_codec::Decode;

pub type Kind = [u8; 16];

#[derive(Debug, Decode)]
pub enum Event {
	Offence(Kind, Vec<u8>, bool),
}
