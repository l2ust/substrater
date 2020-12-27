// --- std ---
use std::convert::TryInto;
// --- crates.io ---
use futures::future;
use parity_scale_codec::{Compact, Decode, Encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use subcryptor::{
	schnorrkel::{self, ExpansionMode, Keypair, MiniSecretKey},
	PublicKey, SIGNING_CTX,
};
use submetadatan::{Metadata, RuntimeMetadataPrefixed};
use subrpcer::{author, chain, state};
use tracing::{error, info, trace};
// --- substrater ---
use crate::{
	error::{ArrayBytesError, CryptoError, SerdeJsonError, SignatureError, SubstraterResult},
	extrinsic::*,
	frame::system::{BlockNumber, EventRecord, Nonce, RefCount, Version},
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

// SpecVersion, TxVersion, Genesis, Era, Nonce, Weight, TransactionPayment, EthereumRelayHeaderParcel
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

	pub async fn nonce(&self) -> SubstraterResult<Nonce> {
		self.node.nonce_of(self.public_key()).await
	}

	pub async fn submit_and_watch_extrinsic(
		&self,
		extrinsic: impl Serialize,
		expect_extrinsic_state: ExtrinsicState,
	) -> SubstraterResult<()> {
		self.node
			.submit_and_watch_extrinsic(extrinsic, expect_extrinsic_state)
			.await
	}

	pub async fn subscribe_storage(
		&self,
		storage_keys: impl AsRef<str>,
	) -> SubstraterResult<SubscriptionId> {
		self.node.subscribe_storage(storage_keys).await
	}

	pub async fn subscribe_events(&self) -> SubstraterResult<SubscriptionId> {
		self.node.subscribe_events().await
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
		nonce: Nonce,
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

	pub async fn nonce_of(&self, public_key: impl AsRef<[u8]>) -> SubstraterResult<Nonce> {
		let bytes_key = self.storage_map_key("System", "Account", public_key)?;
		let hex_str_key = array_bytes::hex_str("0x", bytes_key);
		let rpc_id = self.websocket.rpc_id().await;

		self.websocket
			.send(
				serde_json::to_vec(&state::get_storage_with_id(
					hex_str_key,
					<Option<BlockNumber>>::None,
					rpc_id,
				))
				.unwrap(),
			)
			.await?;

		let result = &self.websocket.take_rpc_result_of(&rpc_id).await?["result"];
		let hex_str_result = result.as_str().ok_or(SerdeJsonError::ExpectedStr)?;
		let raw_account_info = array_bytes::bytes(hex_str_result).map_err(ArrayBytesError::from)?;
		let account_info = AccountInfo::decode(&mut &*raw_account_info)?;

		Ok(account_info.nonce)
	}

	pub async fn submit_and_watch_extrinsic(
		&self,
		extrinsic: impl Serialize,
		expect_extrinsic_state: ExtrinsicState,
	) -> SubstraterResult<()> {
		let rpc_id = self.websocket.rpc_id().await;

		self.websocket
			.send(
				serde_json::to_vec(&author::submit_and_watch_extrinsic_with_id(
					&extrinsic, rpc_id,
				))
				.unwrap(),
			)
			.await?;

		let subscription_id = self.websocket.take_rpc_result_of(&rpc_id).await?["result"]
			.as_str()
			.unwrap()
			.to_owned();

		self.websocket.add_subscription_id(&subscription_id).await;

		let unwatch_extrinsic_future = self.unwatch_extrinsic(&subscription_id);

		if expect_extrinsic_state.ignored() {
			unwatch_extrinsic_future.await?;

			return Ok(());
		}

		loop {
			let subscription = self
				.websocket
				.take_subscription_of(&subscription_id)
				.await?;
			let result = &subscription["params"]["result"];
			let (extrinsic_state, block_hash) = if result.is_string() {
				(ExtrinsicState::Ready, "")
			} else if let Some(block_hash) = result.get("inBlock") {
				(ExtrinsicState::InBlock, block_hash.as_str().unwrap())
			} else if let Some(block_hash) = result.get("finalized") {
				(ExtrinsicState::Finalized, block_hash.as_str().unwrap())
			} else {
				// TODO
				error!("{:?}", subscription);

				(ExtrinsicState::Ignored, "")
			};

			info!(
				"`ExtrinsicState({})`: `{:?}({})`",
				subscription_id, extrinsic_state, block_hash
			);

			if extrinsic_state.ignored() || extrinsic_state == expect_extrinsic_state {
				unwatch_extrinsic_future.await?;

				break;
			}
		}

		Ok(())
	}

	pub async fn unwatch_extrinsic(
		&self,
		subscription_id: impl AsRef<str>,
	) -> SubstraterResult<()> {
		let rpc_id = self.websocket.rpc_id().await;
		let subscription_id = subscription_id.as_ref();

		self.websocket
			.send(
				serde_json::to_vec(&author::unwatch_extrinsic_with_id(subscription_id, rpc_id))
					.unwrap(),
			)
			.await?;

		// TODO: deadlock if unwatch failed
		self.websocket.take_rpc_result_of(rpc_id).await?;

		self.websocket.remove_subscription_id(subscription_id).await;

		Ok(())
	}

	pub async fn subscribe_storage(
		&self,
		storage_keys: impl AsRef<str>,
	) -> SubstraterResult<SubscriptionId> {
		let rpc_id = self.websocket.rpc_id().await;

		self.websocket
			.send(
				serde_json::to_vec(&state::subscribe_storage_with_id(
					[storage_keys.as_ref()],
					rpc_id,
				))
				.unwrap(),
			)
			.await?;

		let subscription_id = self.websocket.take_rpc_result_of(&rpc_id).await?["result"]
			.as_str()
			.unwrap()
			.into();

		self.websocket.add_subscription_id(&subscription_id).await;

		Ok(subscription_id)
	}

	pub async fn unsubscribe_storage(
		&self,
		subscription_id: impl AsRef<str>,
	) -> SubstraterResult<()> {
		let rpc_id = self.websocket.rpc_id().await;
		let subscription_id = subscription_id.as_ref();

		self.websocket
			.send(
				serde_json::to_vec(&state::unsubscribe_storage_with_id(subscription_id, rpc_id))
					.unwrap(),
			)
			.await?;

		// TODO: deadlock if unsubscribe failed
		self.websocket.take_rpc_result_of(rpc_id).await?;
		self.websocket.take_subscription_of(subscription_id).await?;

		self.websocket.remove_subscription_id(subscription_id).await;

		Ok(())
	}

	// pub async fn unsubscribe_all(&self) {}

	pub async fn subscribe_events(&self) -> SubstraterResult<SubscriptionId> {
		self.subscribe_storage(substorager::hex_storage_key_with_prefix(
			"0x", b"System", b"Events",
		))
		.await
	}

	pub fn parse_events<Event, Hash>(result: &Value) -> Vec<EventRecord<Event, Hash>>
	where
		Event: Decode,
		Hash: Decode,
	{
		let raw_events = result["params"]["result"]["changes"][0][1]
			.as_str()
			.unwrap()
			.to_owned();

		Decode::decode(&mut &*array_bytes::bytes_unchecked(raw_events)).unwrap()
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
	pub nonce: Nonce,
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
	let subscription_id = substrater.subscribe_events().await?;

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
		.submit_and_watch_extrinsic(extrinsic.as_str(), ExtrinsicState::Ignored)
		.await?;

	loop {
		let result = substrater
			.node
			.websocket
			.take_subscription_of(&subscription_id)
			.await?;
		let events = Node::parse_events::<Event, Hash>(&result);

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
