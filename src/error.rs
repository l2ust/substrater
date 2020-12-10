// --- std ---
use std::fmt::Debug;
// --- crates.io ---
use array_bytes::Error as ArrayBytesError;
use async_std::channel::{RecvError, SendError};
use parity_scale_codec::Error as CodecError;
use serde_json::Error as RawSerdeJsonError;
use submetadatan::Error as MetadataError;
use thiserror::Error as ThisError;
use tungstenite::Error as WebsocketError;

pub type SubstraterResult<T> = Result<T, Error>;

#[derive(Debug, ThisError)]
pub enum Error {
	#[error("Array bytes error")]
	ArrayBytes(#[from] ArrayBytesError),
	#[error("Async error")]
	Async(#[from] AsyncError),
	#[error("Crypto error")]
	Crypto(#[from] CryptoError),
	#[error("Codec Error")]
	Codec(#[from] CodecError),
	#[error("Serde json error")]
	SerdeJson(#[from] SerdeJsonError),
	#[error("Metadata error")]
	Metadata(#[from] MetadataError),
	#[error("Websocket error")]
	Websocket(#[from] WebsocketError),
}

#[derive(Debug, ThisError)]
pub enum AsyncError {
	#[error("Send error")]
	Send,
	#[error("Recv error")]
	Recv(#[from] RecvError),
}
impl<T> From<SendError<T>> for AsyncError {
	fn from(_: SendError<T>) -> Self {
		Self::Send
	}
}

#[derive(Debug, ThisError)]
pub enum CryptoError {
	#[error("Invalid seed")]
	Signature(#[from] SignatureError),
}
#[derive(Debug, ThisError)]
pub enum SignatureError {
	#[error(
		"`MiniSecret` expected `32` length seed due to \
		`Analogous to ed25519 secret key as 32 bytes, see RFC8032.`"
	)]
	BytesLengthMismatch,
}

#[derive(Debug, ThisError)]
pub enum SerdeJsonError {
	#[error("Raw serde json error")]
	Raw(#[from] RawSerdeJsonError),
	#[error("Expected `str`")]
	ExpectedStr,
}
