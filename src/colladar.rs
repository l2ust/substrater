// --- std ---
use std::{
	convert::{TryFrom, TryInto},
	fmt::Display,
	mem,
};
// --- crates.io ---
use anyhow::Result as AnyResult;
use async_std::{
	channel::{self, Receiver, Sender},
	task::{self, JoinHandle},
};
use async_trait::async_trait;
use parity_scale_codec::{Compact, Decode, Encode, FullCodec};
use serde::Deserialize;
use serde_json::Value;
use subcryptor::{
	schnorrkel::{self, ExpansionMode, Keypair, MiniSecretKey},
	PublicKey, Signature, SIGNING_CTX,
};
use subrpcer::{author, chain, state};
use substorager::{StorageHasher, StorageType};
use tracing::{error, info, trace};
use tungstenite::{client::IntoClientRequest, Message};
// --- github.com ---
use frame_metadata::{
	DecodeDifferent, RuntimeMetadata, RuntimeMetadataPrefixed, StorageEntryType,
	StorageHasher as DeprecatedStorageHasher,
};
// --- colladar ---
use crate::error::{Error, JsonError, SignatureError};

#[macro_export]
macro_rules! call {
	($colladar:expr, $module:expr, $call:expr$(, $data:expr)*) => {{
		($colladar.node.call($module, $call)?$(, $data)*)
		}};
}

pub type Bytes = Vec<u8>;

pub type BlockNumber = u32;
pub type Hash = [u8; 32];
pub type Index = u32;
pub type Version = u32;
pub type RefCount = u32;
pub type Balance = u128;

// SpecVersion, TxVersion, Genesis, Era, Index, Weight, TransactionPayment, EthereumRelayHeaderParcel
// pub type DarwiniaAdditionalSigned = (Version, Version, Hash, Hash, (), (), (), ());

#[async_trait]
pub trait WebSocket: Sized {
	type ClientMsg;
	type NodeMsg;

	fn connect(uri: impl Display + IntoClientRequest) -> AnyResult<Self>;

	async fn disconnect(self);

	async fn send(&self, msg: Self::ClientMsg) -> AnyResult<()>;

	async fn recv(&self) -> AnyResult<Self::NodeMsg>;
}

pub struct Colladar {
	pub signer: Keypair,
	pub node: Node,
}
impl Colladar {
	pub async fn init(uri: impl Into<String>, seed: &str) -> AnyResult<Self> {
		let signer = MiniSecretKey::from_bytes(&array_bytes::bytes(seed).unwrap())
			.map_err(|_| SignatureError::BytesLengthMismatch)?
			.expand_to_keypair(ExpansionMode::Ed25519);

		trace!("Seed: {}", seed);
		trace!(
			"Public key: {:?}",
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

	// TODO: handle null result in rpc
	pub async fn nonce(&self) -> AnyResult<Index> {
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
				.ok_or(JsonError::ExpectedStr)?,
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
	pub async fn init(uri: impl Into<String>) -> AnyResult<Self> {
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
	) -> AnyResult<Bytes> {
		self.metadata.storage_map_key(module, item, key)
	}

	pub fn call(
		&self,
		module_name: impl Into<String>,
		call_name: impl Into<String>,
	) -> AnyResult<[u8; 2]> {
		self.metadata.call(module_name, call_name)
	}

	pub fn extrinsic(
		&self,
		call: impl Clone + Encode,
		signer: &Keypair,
		nonce: Index,
		tip: Balance,
	) -> AnyResult<String> {
		let extra = Extra(Era::Immortal, Compact(nonce), Compact(tip));
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

		Ok(extrinsic.encode())
	}
}

#[derive(Debug)]
pub struct Messenger {
	pub handle: Option<JoinHandle<AnyResult<()>>>,
	pub sender: Sender<Bytes>,
	pub receiver: Receiver<Bytes>,
}
#[async_trait]
impl WebSocket for Messenger {
	type ClientMsg = Bytes;
	type NodeMsg = Bytes;

	fn connect(uri: impl Display + IntoClientRequest) -> AnyResult<Self> {
		trace!("`Messenger` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let (node_sender, client_receiver) = channel::unbounded();
		let (mut messenger, response) = tungstenite::connect(uri)?;

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			loop {
				messenger.write_message(Message::Binary(node_receiver.recv().await?))?;
				node_sender
					.send(messenger.read_message()?.into_data())
					.await?;
			}
		});

		Ok(Self {
			handle: Some(handle),
			sender: client_sender,
			receiver: client_receiver,
		})
	}

	async fn disconnect(self) {
		if let Some(handle) = self.handle {
			handle.cancel().await;
		}
	}

	async fn send(&self, msg: Self::ClientMsg) -> AnyResult<()> {
		Ok(self.sender.send(msg).await?)
	}

	async fn recv(&self) -> AnyResult<Self::NodeMsg> {
		Ok(self.receiver.recv().await?)
	}
}

#[derive(Debug)]
pub struct Excreamer {
	pub handle: Option<JoinHandle<AnyResult<()>>>,
	pub sender: Sender<(Bytes, ExtrinsicState)>,
}
#[async_trait]
impl WebSocket for Excreamer {
	type ClientMsg = (Bytes, ExtrinsicState);
	type NodeMsg = ();

	fn connect(uri: impl Display + IntoClientRequest) -> AnyResult<Self> {
		trace!("`Excreamer` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let (mut excreamer, response) = tungstenite::connect(uri)?;

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			let parse = |v: Value| -> (ExtrinsicState, String, String) {
				let extrinsic_state;
				let subscription;
				let mut block_hash = String::new();

				if let Some(subscription_) = v["result"].as_str() {
					extrinsic_state = ExtrinsicState::Sent;
					subscription = subscription_.into();
				} else {
					let params = &v["params"];
					let result = &params["result"];
					let fallback = || {
						error!("Failed to parse `ExtrinsicState` from `{:?}`", v);

						ExtrinsicState::Ignored
					};

					extrinsic_state = if let Some(extrinsic_state) = result.as_str() {
						match extrinsic_state {
							"ready" => ExtrinsicState::Ready,
							_ => fallback(),
						}
					} else if let Some(block_hash_) = result.get("inBlock") {
						block_hash = block_hash_.as_str().unwrap().into();

						ExtrinsicState::InBlock
					} else if let Some(block_hash_) = result.get("finalized") {
						block_hash = block_hash_.as_str().unwrap().into();

						ExtrinsicState::Finalized
					} else {
						fallback()
					};
					subscription = params["subscription"].as_str().unwrap().into();
				}

				(extrinsic_state, subscription, block_hash)
			};

			loop {
				let (bytes, expect_extrinsic_state): Self::ClientMsg = node_receiver.recv().await?;

				excreamer.write_message(Message::Binary(bytes))?;

				let ignored_state = expect_extrinsic_state.ignored();

				loop {
					let (extrinsic_state, subscription, block_hash) = parse(
						serde_json::from_slice::<Value>(&excreamer.read_message()?.into_data())?,
					);

					info!(
						"`ExtrinsicState({})`: `{:?}({})`",
						subscription, extrinsic_state, block_hash
					);

					if ignored_state || extrinsic_state == expect_extrinsic_state {
						break;
					}
				}
			}
		});

		Ok(Self {
			handle: Some(handle),
			sender: client_sender,
		})
	}

	async fn disconnect(self) {
		if let Some(handle) = self.handle {
			handle.cancel().await;
		}
	}

	async fn send(&self, msg: Self::ClientMsg) -> AnyResult<()> {
		Ok(self.sender.send(msg).await?)
	}

	async fn recv(&self) -> AnyResult<Self::NodeMsg> {
		Ok(())
	}
}

#[derive(Debug, PartialEq)]
pub enum ExtrinsicState {
	Ignored,
	Sent,
	Ready,
	InBlock,
	Finalized,
}
impl ExtrinsicState {
	pub fn ignored(&self) -> bool {
		match self {
			Self::Ignored => true,
			_ => false,
		}
	}
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
	spec_version: Version,
	transaction_version: Version,
}

#[derive(Debug)]
pub struct Metadata {
	pub modules: Vec<Module>,
}
impl Metadata {
	pub fn storage_prefix(&self, module_name: impl Into<String>) -> AnyResult<&str> {
		let module_name = module_name.into();
		let module = self
			.modules
			.iter()
			.find(|module| &module.name == &module_name)
			.ok_or(Error::ModuleNotFound { module_name })?;

		Ok(&module.storages.prefix)
	}

	pub fn storage(
		&self,
		module_name: impl Into<String>,
		item_name: impl Into<String>,
	) -> AnyResult<&Storage> {
		let module_name = module_name.into();
		let item_name = item_name.into();
		let module = self
			.modules
			.iter()
			.find(|module| &module.name == &module_name)
			.ok_or(Error::ModuleNotFound {
				module_name: module_name.clone(),
			})?;
		let item = module
			.storages
			.items
			.iter()
			.find(|item| &item.name == &item_name)
			.ok_or(Error::StorageItemNotFound {
				module_name,
				item_name,
			})?;

		Ok(item)
	}

	pub fn storage_map_key(
		&self,
		module: impl AsRef<str>,
		item: impl AsRef<str>,
		key: impl AsRef<[u8]>,
	) -> AnyResult<Bytes> {
		let module = module.as_ref();
		let item = item.as_ref();
		let prefix = self.storage_prefix(module)?;
		let storage = self.storage(module, item)?;

		match &storage.r#type {
			StorageType::Map(hasher) => {
				Ok(substorager::storage_map_key(prefix, item, (hasher, key)))
			}
			r#type => Err(Error::StorageTypeMismatch {
				expected: "Map".into(),
				found: r#type.to_owned(),
			})?,
		}
	}

	pub fn call(
		&self,
		module_name: impl Into<String>,
		call_name: impl Into<String>,
	) -> AnyResult<[u8; 2]> {
		let module_name = module_name.into();
		let call_name = call_name.into();
		let module_index = self
			.modules
			.iter()
			.position(|module| &module.name == &module_name)
			.ok_or(Error::ModuleNotFound {
				module_name: module_name.clone(),
			})?;
		let call_index = self.modules[module_index]
			.calls
			.iter()
			.position(|call| &call.name == &call_name)
			.ok_or(Error::CallNotFound {
				module_name,
				call_name,
			})?;

		Ok([module_index as _, call_index as _])
	}
}
impl TryFrom<RuntimeMetadata> for Metadata {
	type Error = Error;

	fn try_from(runtime_metadata: RuntimeMetadata) -> Result<Self, Self::Error> {
		// --- github.com ---
		use RuntimeMetadata::*;

		macro_rules! err {
			($found:expr) => {{
				Err(Error::MetadataVersionMismatch {
					expected: "V12".into(),
					found: $found.into(),
					})
				}};
		}
		macro_rules! inner {
			($enum:expr) => {{
				if let DecodeDifferent::Decoded(inner) = $enum {
					inner
				} else {
					unreachable!();
					}
				}};
		}
		macro_rules! hasher {
			($hasher:expr) => {{
				match $hasher {
					DeprecatedStorageHasher::Blake2_128 => StorageHasher::Blake2_128,
					DeprecatedStorageHasher::Blake2_256 => StorageHasher::Blake2_256,
					DeprecatedStorageHasher::Blake2_128Concat => StorageHasher::Blake2_128Concat,
					DeprecatedStorageHasher::Twox128 => StorageHasher::Twox128,
					DeprecatedStorageHasher::Twox256 => StorageHasher::Twox256,
					DeprecatedStorageHasher::Twox64Concat => StorageHasher::Twox64Concat,
					DeprecatedStorageHasher::Identity => StorageHasher::Identity,
					}
				}};
		}

		match runtime_metadata {
			V0(_) => err!("V0"),
			V1(_) => err!("V1"),
			V2(_) => err!("V2"),
			V3(_) => err!("V3"),
			V4(_) => err!("V4"),
			V5(_) => err!("V5"),
			V6(_) => err!("V6"),
			V7(_) => err!("V7"),
			V8(_) => err!("V8"),
			V9(_) => err!("V9"),
			V10(_) => err!("V10"),
			V11(_) => err!("V11"),
			V12(runtime_metadata) => {
				let mut metadata = Self { modules: vec![] };

				for module in inner!(runtime_metadata.modules) {
					let mut storages = Storages {
						prefix: Default::default(),
						items: vec![],
					};
					let mut calls = vec![];

					if let Some(wrap_storages) = module.storage {
						let wrap_storages = inner!(wrap_storages);

						storages.prefix = inner!(wrap_storages.prefix);

						for storage in inner!(wrap_storages.entries) {
							storages.items.push(Storage {
								name: inner!(storage.name),
								r#type: match storage.ty {
									StorageEntryType::Plain(_) => StorageType::Plain,
									StorageEntryType::Map { hasher, .. } => {
										StorageType::Map(hasher!(hasher))
									}
									StorageEntryType::DoubleMap {
										hasher,
										key2_hasher,
										..
									} => StorageType::DoubleMap(
										hasher!(hasher),
										hasher!(key2_hasher),
									),
								},
							});
						}
					}

					if let Some(wrap_calls) = module.calls {
						for call in inner!(wrap_calls) {
							calls.push(Call {
								name: inner!(call.name),
							});
						}
					}

					metadata.modules.push(Module {
						name: inner!(module.name),
						storages,
						calls,
					});
				}

				Ok(metadata)
			}
		}
	}
}

#[derive(Debug)]
pub struct Module {
	pub name: String,
	// pub events: Vec<Event>,
	pub storages: Storages,
	pub calls: Vec<Call>,
}

#[derive(Debug)]
pub struct Storages {
	pub prefix: String,
	pub items: Vec<Storage>,
}

#[derive(Debug)]
pub struct Storage {
	pub name: String,
	pub r#type: StorageType,
}

#[derive(Debug)]
pub struct Call {
	pub name: String,
}

pub struct SignedPayload<Call: Encode, AdditionalSigned: FullCodec>(
	(Call, Extra, AdditionalSigned),
);
impl<Call, AdditionalSigned> SignedPayload<Call, AdditionalSigned>
where
	Call: Encode,
	AdditionalSigned: FullCodec,
{
	pub fn from_raw(call: Call, extra: Extra, additional_signed: AdditionalSigned) -> Self {
		Self((call, extra, additional_signed))
	}

	pub fn using_encoded<R, F>(&self, f: F) -> R
	where
		F: FnOnce(&[u8]) -> R,
	{
		self.0.using_encoded(|payload| {
			if payload.len() > 256 {
				f(&subhasher::blake2_256(payload))
			} else {
				f(payload)
			}
		})
	}
}

// Era, Index, TransactionPayment
#[derive(Clone, Debug, Encode)]
pub struct Extra(Era, Compact<Index>, Compact<Balance>);

#[derive(Clone, Debug, Encode)]
pub enum Era {
	Immortal,
	Mortal(u64, u64),
}

pub struct Extrinsic<Call>
where
	Call: Encode,
{
	signature: Option<(PublicKey, MultiSignature, Extra)>,
	call: Call,
}
impl<Call> Extrinsic<Call>
where
	Call: Encode,
{
	pub fn encode(&self) -> String {
		array_bytes::hex_str("0x", Encode::encode(self))
	}
}
impl<Call> Encode for Extrinsic<Call>
where
	Call: Encode,
{
	fn encode(&self) -> Vec<u8> {
		const V4: u8 = 4;

		fn encode_with_vec_prefix<T: Encode, F: Fn(&mut Vec<u8>)>(encoder: F) -> Vec<u8> {
			let size = mem::size_of::<T>();
			let reserve = match size {
				0..=0b0011_1111 => 1,
				0b0100_0000..=0b0011_1111_1111_1111 => 2,
				_ => 4,
			};

			let mut v = Vec::with_capacity(reserve + size);
			v.resize(reserve, 0);
			encoder(&mut v);

			let mut length: Vec<()> = Vec::new();
			length.resize(v.len() - reserve, ());
			length.using_encoded(|s| {
				v.splice(0..reserve, s.iter().cloned());
			});

			v
		}

		encode_with_vec_prefix::<Self, _>(|v| {
			match self.signature.as_ref() {
				Some(s) => {
					v.push(V4 | 0b1000_0000);
					s.encode_to(v);
				}
				None => {
					v.push(V4 & 0b0111_1111);
				}
			}
			self.call.encode_to(v);
		})
	}
}

#[derive(Encode)]
pub enum MultiSignature {
	_Ed25519,
	Sr25519(Signature),
	_Ecdsa,
}
impl From<Signature> for MultiSignature {
	fn from(signature: Signature) -> Self {
		Self::Sr25519(signature)
	}
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

pub async fn test() -> AnyResult<()> {
	let uri = "ws://127.0.0.1:9944";
	let seed = "0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a";
	let colladar = Colladar::init(uri, seed).await?;
	let call = call!(
		colladar,
		"Balances",
		"transfer",
		array_bytes::hex_str_array_unchecked!(
			"0xe659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",
			32
		),
		Compact(10_000_000_000u128)
	);
	let extrinsic = colladar
		.node
		.extrinsic(call, &colladar.signer, colladar.nonce().await?, 0)?;

	colladar
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
