// --- crates.io ---
use substorager::StorageType;
use thiserror::Error as ThisError;
// --- github.com ---
use sp_core::crypto;

#[derive(Debug, ThisError)]
pub enum Error {
	#[error("Fail to convert `{}` to `sr25519::Pair`", seed)]
	InvalidSeed {
		seed: String,
		err: crypto::SecretStringError,
	},
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
	// #[error("Public length expected `32` but found `{}`", length)]
	// InvalidPublicLength { length: usize },
}
