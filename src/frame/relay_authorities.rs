// --- crates.io ---
use parity_scale_codec::Decode;

pub type MMRRoot = [u8; 32];
pub type RelayAuthoritySigner = [u8; 20];
pub type RelayAuthorityMessage = [u8; 32];
pub type RelayAuthoritySignature = [u8; 65];

#[derive(Debug, Decode)]
pub enum Event<BlockNumber, AccountId> {
	NewMMRRoot(BlockNumber),
	MMRRootSigned(
		BlockNumber,
		MMRRoot,
		Vec<(AccountId, RelayAuthoritySignature)>,
	),
	NewAuthorities(RelayAuthorityMessage),
	AuthoritiesSetSigned(
		u32,
		Vec<RelayAuthoritySigner>,
		Vec<(AccountId, RelayAuthoritySignature)>,
	),
}
