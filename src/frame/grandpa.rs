// --- crates.io ---
use parity_scale_codec::Decode;

pub type AuthorityList<Public> = Vec<(Public, u64)>;

#[derive(Debug, Decode)]
pub enum Event<Public> {
	NewAuthorities(AuthorityList<Public>),
	Paused,
	Resumed,
}
