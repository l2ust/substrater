// --- std ---
use std::fmt::Debug;
// --- crates.io ---
use async_std::channel::{RecvError, SendError};
use submetadatan::Error as MetadataError;
use thiserror::Error as ThisError;
use tungstenite::Error as WebsocketError;

#[derive(Debug, ThisError)]
pub enum Error {
	#[error("Async error")]
	Async(#[from] AsyncError),
	#[error("Websocket error")]
	Websocket(#[from] WebsocketError),
	#[error("Metadata error")]
	Metadata(#[from] MetadataError),
	#[error("Crypto error")]
	Crypto(#[from] CryptoError),
	#[error("Json error")]
	Json(#[from] JsonError),
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
pub enum JsonError {
	#[error("Expected `str`")]
	ExpectedStr,
}
