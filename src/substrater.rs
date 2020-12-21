// --- std ---
use std::convert::TryInto;
// --- crates.io ---
use async_std::prelude::FutureExt;
use futures::future;
use parity_scale_codec::{Compact, Decode, Encode};
use serde::Deserialize;
use subcryptor::{
	schnorrkel::{self, ExpansionMode, Keypair, MiniSecretKey},
	PublicKey, SIGNING_CTX,
};
use submetadatan::{Metadata, RuntimeMetadataPrefixed};
use subrpcer::{chain, state};
use tracing::trace;
// --- substrater ---
use crate::{
	error::{CryptoError, SerdeJsonError, SignatureError, SubstraterResult},
	extrinsic::*,
	frame::system::{BlockNumber, EventRecord, Index, RefCount, Version},
	r#type::*,
	runtime::pangolin::{Balance, Event, Hash},
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
		let rpc_id = self.node.websocket.rpc_id().await;

		self.node
			.websocket
			.send(
				serde_json::to_vec(&state::get_storage_with_id(
					key,
					<Option<BlockNumber>>::None,
					rpc_id,
				))
				.unwrap(),
			)
			.await?;

		let account = AccountInfo::decode(&mut &*array_bytes::bytes(
			self.node.websocket.take_rpc_result_of(&rpc_id).await?["result"]
				.as_str()
				.ok_or(SerdeJsonError::ExpectedStr)?,
		)?)?;

		Ok(account.nonce)
	}
}

#[derive(Debug)]
pub struct Node {
	pub uri: String,
	pub websocket: Websocket,
	pub genesis_hash: Hash,
	pub versions: Versions,
	pub metadata: Metadata,
}
impl Node {
	pub async fn init(uri: impl Into<String>) -> SubstraterResult<Self> {
		let uri = uri.into();
		let websocket = Websocket::connect(&uri).await?;
		let get_block_hash_rpc_id = websocket.rpc_id().await;
		let get_runtime_version_rpc_id = websocket.rpc_id().await;
		let get_metadata_rpc_id = websocket.rpc_id().await;

		future::join_all(vec![
			websocket.send(
				serde_json::to_vec(&chain::get_block_hash_with_id(0u8, get_block_hash_rpc_id))
					.unwrap(),
			),
			websocket.send(
				serde_json::to_vec(&state::get_runtime_version_with_id(
					get_runtime_version_rpc_id,
				))
				.unwrap(),
			),
			websocket.send(
				serde_json::to_vec(&state::get_metadata_with_id(get_metadata_rpc_id)).unwrap(),
			),
		])
		.await;

		let genesis_hash = array_bytes::hex_str_array_unchecked!(
			websocket.take_rpc_result_of(&get_block_hash_rpc_id).await?["result"]
				.as_str()
				.unwrap(),
			32
		);
		let versions = serde_json::from_value(
			websocket
				.take_rpc_result_of(&get_runtime_version_rpc_id)
				.await?["result"]
				.take(),
		)
		.unwrap();
		let metadata = RuntimeMetadataPrefixed::decode(
			&mut &*array_bytes::bytes(
				websocket.take_rpc_result_of(&get_metadata_rpc_id).await?["result"]
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
			websocket,
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

	let submit_and_watch_extrinsic_future = substrater
		.node
		.websocket
		.submit_and_watch_extrinsic(extrinsic.as_str(), ExtrinsicState::Ignored);
	let subscribe_storage_future =
		substrater
			.node
			.websocket
			.subscribe_storage(substorager::hex_storage_key_with_prefix(
				"0x", b"System", b"Events",
			));
	let (_, subscription_id) = submit_and_watch_extrinsic_future
		.join(subscribe_storage_future)
		.await;
	let subscription_id = subscription_id?;

	loop {
		let raw_events = substrater
			.node
			.websocket
			.take_subscription_of(&subscription_id)
			.await?["params"]["result"]["changes"][0][1]
			.as_str()
			.unwrap()
			.to_owned();
		let events = <Vec<EventRecord<Event, Hash>>>::decode(&mut &*array_bytes::bytes_unchecked(
			raw_events,
		))
		.unwrap();

		tracing::info!("{:?}", events);

		// substrater
		// 	.node
		// 	.websocket
		// 	.unsubscribe_storage(&subscription_id)
		// 	.await?;
		tracing::error!(
			"{}, {}, {}",
			substrater.node.websocket.rpc_results.lock().await.len(),
			substrater
				.node
				.websocket
				.subscription_ids
				.lock()
				.await
				.len(),
			substrater.node.websocket.subscriptions.lock().await.len(),
		);
	}

	// run().await;

	// Ok(())
}

// pub async fn run() {
// 	#[allow(clippy::empty_loop)]
// 	loop {}
// }
