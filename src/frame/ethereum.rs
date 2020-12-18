// --- std ---
use std::borrow::Cow;
// --- crates.io ---
use parity_scale_codec::Decode;

pub type H160 = [u8; 20];
pub type H256 = [u8; 32];

#[derive(Debug, Decode)]
pub enum Event {
	Executed(H160, H256, ExitReason),
}
#[derive(Debug, Decode)]
pub enum ExitReason {
	Succeed(ExitSucceed),
	Error(ExitError),
	Revert(ExitRevert),
	Fatal(ExitFatal),
}
#[derive(Debug, Decode)]
pub enum ExitSucceed {
	Stopped,
	Returned,
	Suicided,
}
#[derive(Debug, Decode)]
pub enum ExitError {
	StackUnderflow,
	StackOverflow,
	InvalidJump,
	InvalidRange,
	DesignatedInvalid,
	CallTooDeep,
	CreateCollision,
	CreateContractLimit,
	OutOfOffset,
	OutOfGas,
	OutOfFund,
	PCUnderflow,
	CreateEmpty,
	Other(Cow<'static, str>),
}
#[derive(Debug, Decode)]
pub enum ExitRevert {
	Reverted,
}
#[derive(Debug, Decode)]
pub enum ExitFatal {
	NotSupported,
	UnhandledInterrupt,
	CallErrorAsFatal(ExitError),
	Other(Cow<'static, str>),
}
