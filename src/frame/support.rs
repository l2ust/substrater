// --- crates.io ---
use parity_scale_codec::Decode;

pub type Weight = u64;
pub type DispatchResult = Result<(), DispatchError>;

#[derive(Debug, Decode)]
pub struct DispatchInfo {
	pub weight: Weight,
	pub class: DispatchClass,
	pub pays_fee: Pays,
}
#[derive(Debug, Decode)]
pub enum DispatchClass {
	Normal,
	Operational,
	Mandatory,
}
#[derive(Debug, Decode)]
pub enum Pays {
	Yes,
	No,
}

#[derive(Debug, Decode)]
pub enum DispatchError {
	Other(#[codec(skip)] &'static str),
	CannotLookup,
	BadOrigin,
	Module {
		index: u8,
		error: u8,
		#[codec(skip)]
		message: Option<&'static str>,
	},
}

#[derive(Debug, Decode)]
pub enum BalanceStatus {
	Free,
	Reserved,
}
