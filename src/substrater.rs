// --- std ---
use std::convert::TryInto;
// --- crates.io ---
use parity_scale_codec::{Compact, Decode, Encode};
use serde::Deserialize;
use serde_json::Value;
use subcryptor::{
	schnorrkel::{self, ExpansionMode, Keypair, MiniSecretKey},
	PublicKey, SIGNING_CTX,
};
use submetadatan::{Metadata, RuntimeMetadataPrefixed};
use subrpcer::{author, chain, state};
use tracing::trace;
// --- substrater ---
use crate::{
	error::{CryptoError, SerdeJsonError, SignatureError, SubstraterResult},
	extrinsic::*,
	frame::{
		balances::Balance,
		system::{BlockNumber, Hash, Index, RefCount, Version},
	},
	r#type::*,
	websocket::*,
};

#[macro_export]
macro_rules! call {
	($substrater:expr, $module:expr, $call:expr$(, $data:expr)*) => {{
		($substrater.node.call($module, $call)?$(, $data)*)
		}};
}

// SpecVersion, TxVersion, Genesis, Era, Index, Weight, TransactionPayment, EthereumRelayHeaderParcel
// pub type DarwiniaAdditionalSigned = (Version, Version, Hash, Hash, (), (), (), ());

pub struct Substrater {
	pub signer: Keypair,
	pub node: Node,
}
impl Substrater {
	pub async fn init(uri: impl Into<String>, seed: &str) -> SubstraterResult<Self> {
		let signer = MiniSecretKey::from_bytes(&array_bytes::bytes(seed).unwrap())
			.map_err(|_| CryptoError::from(SignatureError::BytesLengthMismatch))?
			.expand_to_keypair(ExpansionMode::Ed25519);

		trace!("Seed: {}", seed);
		trace!(
			"Public key: {}",
			array_bytes::hex_str("0x", signer.public.to_bytes())
		);

		Ok(Self {
			signer,
			node: Node::init(uri).await?,
		})
	}

	pub fn public_key(&self) -> PublicKey {
		self.signer.public.to_bytes()
	}

	// pub async block_number(&self) ->

	pub async fn nonce(&self) -> SubstraterResult<Index> {
		let key = array_bytes::hex_str(
			"0x",
			self.node
				.storage_map_key("System", "Account", self.public_key())?,
		);

		self.node
			.messenger
			.send(
				serde_json::to_vec(&state::get_storage(key, <Option<BlockNumber>>::None)).unwrap(),
			)
			.await?;

		let account = AccountInfo::decode(&mut &*array_bytes::bytes(
			serde_json::from_slice::<Value>(&self.node.messenger.recv().await?).unwrap()["result"]
				.as_str()
				.ok_or(SerdeJsonError::ExpectedStr)?,
		)?)?;

		Ok(account.nonce)
	}
}

#[derive(Debug)]
pub struct Node {
	pub uri: String,
	pub messenger: Messenger,
	pub excreamer: Excreamer,
	pub genesis_hash: Hash,
	pub versions: Versions,
	pub metadata: Metadata,
}
impl Node {
	pub async fn init(uri: impl Into<String>) -> SubstraterResult<Self> {
		let uri = uri.into();
		let messenger = Messenger::connect(&uri)?;
		let excreamer = Excreamer::connect(&uri)?;

		messenger
			.send(serde_json::to_vec(&chain::get_block_hash(0u8)).unwrap())
			.await?;
		let genesis_hash = array_bytes::hex_str_array_unchecked!(
			serde_json::from_slice::<Value>(&messenger.recv().await?).unwrap()["result"]
				.as_str()
				.unwrap(),
			32
		)
		.into();

		messenger
			.send(serde_json::to_vec(&state::get_runtime_version()).unwrap())
			.await?;
		let versions = serde_json::from_value(
			serde_json::from_slice::<Value>(&messenger.recv().await?).unwrap()["result"].clone(),
		)
		.unwrap();

		messenger
			.send(serde_json::to_vec(&state::get_metadata()).unwrap())
			.await?;
		let metadata = RuntimeMetadataPrefixed::decode(
			&mut &*array_bytes::bytes(
				serde_json::from_slice::<Value>(&messenger.recv().await?).unwrap()["result"]
					.as_str()
					.unwrap(),
			)
			.unwrap(),
		)
		.unwrap()
		.1
		.try_into()
		.unwrap();

		Ok(Self {
			uri,
			messenger,
			excreamer,
			genesis_hash,
			versions,
			metadata,
		})
	}

	pub fn spec_version(&self) -> Version {
		self.versions.spec_version
	}

	pub fn transaction_version(&self) -> Version {
		self.versions.transaction_version
	}

	pub fn genesis_hash(&self) -> Hash {
		self.genesis_hash
	}

	pub fn storage_map_key(
		&self,
		module: impl AsRef<str>,
		item: impl AsRef<str>,
		key: impl AsRef<[u8]>,
	) -> SubstraterResult<Bytes> {
		Ok(self.metadata.storage_map_key(module, item, key)?)
	}

	pub fn call(
		&self,
		module_name: impl Into<String>,
		call_name: impl Into<String>,
	) -> SubstraterResult<[u8; 2]> {
		Ok(self.metadata.call(module_name, call_name)?)
	}

	pub fn extrinsic(
		&self,
		call: impl Clone + Encode,
		signer: &Keypair,
		era: Era,
		nonce: Index,
		tip: Balance,
	) -> String {
		let extra = Extra(era, Compact(nonce), Compact(tip));
		let raw_payload = SignedPayload::from_raw(
			call.clone(),
			extra.clone(),
			(
				self.spec_version(),
				self.transaction_version(),
				self.genesis_hash(),
				self.genesis_hash(),
				(),
				(),
				(),
			),
		);
		let signature = raw_payload.using_encoded(|payload| {
			let context = schnorrkel::signing_context(SIGNING_CTX);
			signer.sign(context.bytes(payload)).to_bytes()
		});
		let extrinsic = Extrinsic {
			signature: Some((signer.public.to_bytes(), signature.into(), extra)),
			call,
		};

		extrinsic.encode()
	}
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
	spec_version: Version,
	transaction_version: Version,
}

#[derive(Debug, Decode)]
pub struct AccountInfo {
	pub nonce: Index,
	pub ref_count: RefCount,
	pub data: AccountData,
}

#[derive(Debug, Decode)]
pub struct AccountData {
	pub free: Balance,
	pub reserved: Balance,
	pub free_kton: Balance,
	pub reserved_kton: Balance,
}

pub async fn test() -> SubstraterResult<()> {
	let uri = "ws://127.0.0.1:9944";
	let seed = "0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a";
	let substrater = Substrater::init(uri, seed).await?;
	let call = call!(
		substrater,
		"Balances",
		"transfer",
		array_bytes::hex_str_array_unchecked!(
			"0xe659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",
			32
		),
		Compact(10_000_000_000u128)
	);
	let extrinsic = substrater.node.extrinsic(
		call,
		&substrater.signer,
		Era::Immortal,
		substrater.nonce().await?,
		0,
	);

	substrater
		.node
		.excreamer
		.send((
			serde_json::to_vec(&author::submit_and_watch_extrinsic(&extrinsic)).unwrap(),
			ExtrinsicState::Finalized,
		))
		.await?;

	run().await;

	Ok(())
}

pub async fn run() {
	loop {}
}
