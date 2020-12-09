// --- crates.io ---
use submetadatan::Error as MetadataError;
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum Error {
	#[error("Metadata error")]
	Metadata(#[from] MetadataError),
	#[error("Crypto error")]
	Crypto(#[from] CryptoError),
	#[error("Json error")]
	Json(#[from] JsonError),
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
