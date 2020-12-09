// --- crates.io ---
use substorager::StorageType;
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum Error {
	#[error("Module `{}` not found", module_name)]
	ModuleNotFound { module_name: String },
	#[error(
		"Storage item `{}` not found under module `{}`",
		module_name,
		item_name
	)]
	StorageItemNotFound {
		module_name: String,
		item_name: String,
	},
	#[error("Storage type expected `{}` but found `{:?}`", expected, found)]
	StorageTypeMismatch {
		expected: String,
		found: StorageType,
	},
	#[error("Call `{}` not found under module `{}`", module_name, call_name)]
	CallNotFound {
		module_name: String,
		call_name: String,
	},
	#[error("Metadata version expected `{}` but found `{}`", expected, found)]
	MetadataVersionMismatch { expected: String, found: String },
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
