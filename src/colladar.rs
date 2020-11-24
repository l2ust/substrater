// --- std ---
use std::convert::{TryFrom, TryInto};
// --- crates.io ---
use anyhow::Result as AnyResult;
use async_std::{
	sync::{self, Receiver, Sender},
	task::{self, JoinHandle},
};
use parity_scale_codec::{Compact, Decode, Encode, FullCodec};
use serde::Deserialize;
use serde_json::Value;
use subrpcer::{author, chain, state};
use substorager::{StorageHasher, StorageType};
use tracing::trace;
use tungstenite::{client::IntoClientRequest, Message};
// --- github.com ---
use frame_metadata::{
	DecodeDifferent, RuntimeMetadata, RuntimeMetadataPrefixed, StorageEntryType,
	StorageHasher as DeprecatedStorageHasher,
};
use sp_core::{crypto, sr25519, Pair, H256 as Hash};
// --- colladar ---
use crate::error::Error;

#[macro_export]
macro_rules! call {
	($call:expr$(, $data:expr)*) => {{
		($call, $($data)*)
		}};
}

// type SignedExtra = (CheckSpecVersion<Runtime>, CheckTxVersion<Runtime>, CheckGenesis<Runtime>, CheckEra<Runtime>, CheckNonce<Runtime>, CheckWeight<Runtime>, ChargeTransactionPayment<Runtime>);
// #[macro_export]
// macro_rules! compose_extrinsic_offline {
//     ($signer: expr,
//     $call: expr,
//     $nonce: expr,
//     $era: expr,
//     $genesis_hash: expr,
//     $genesis_or_current_hash: expr,
//     $runtime_spec_version: expr,
//     $transaction_version: expr) => {{
//         use $crate::extrinsic::xt_primitives::*;
//         use $crate::sp_runtime::generic::Era;
//         let extra = GenericExtra::new($era, $nonce);
//         let raw_payload = SignedPayload::from_raw(
//             $call.clone(),
//             extra.clone(),
//             (
//                 $runtime_spec_version,
//                 $transaction_version,
//                 $genesis_hash,
//                 $genesis_or_current_hash,
//                 (),
//                 (),
//                 (),
//             ),
//         );

//         let signature = raw_payload.using_encoded(|payload| $signer.sign(payload));

//         let mut arr: [u8; 32] = Default::default();
//         arr.clone_from_slice($signer.public().as_ref());

//         UncheckedExtrinsicV4::new_signed(
//             $call,
//             GenericAddress::from(PublicKey::from(arr)),
//             signature.into(),
//             extra,
//         )
//     }};
// }

pub type Bytes = Vec<u8>;

pub type PublicKey = [u8; 32];
pub type Version = u32;

pub type BlockNumber = u32;
pub type Index = u32;
pub type RefCount = u32;
pub type Balance = u128;

// SpecVersion, TxVersion, Genesis, Era, Index, Weight, TransactionPayment, EthereumRelayHeaderParcel
// pub type DarwiniaAdditionalSigned = (Version, Version, Hash, Hash, (), (), (), ());

pub struct Colladar {
	pub signer: sr25519::Pair,
	pub node: Node,
}
impl Colladar {
	pub async fn init(uri: impl Into<String>, seed: &str) -> AnyResult<Self> {
		Ok(Self {
			signer: <sr25519::Pair as crypto::Pair>::from_seed_slice(
				&array_bytes::bytes(seed).unwrap(),
			)
			.map_err(|err| Error::InvalidSeed {
				seed: seed.into(),
				err,
			})
			.unwrap()
			.into(),
			node: Node::init(uri).await?,
		})
	}

	pub fn public_key(&self) -> PublicKey {
		self.signer.public().0
	}

	pub async fn nonce(&self) -> AnyResult<Index> {
		let key = array_bytes::hex_str(
			"0x",
			self.node
				.storage_map_key("System", "Account", self.public_key())?,
		);

		self.node
			.send(
				serde_json::to_vec(&state::get_storage(key, <Option<BlockNumber>>::None)).unwrap(),
			)
			.await;
		let account = AccountInfo::decode(&mut &*array_bytes::bytes(
			serde_json::from_slice::<Value>(&self.node.recv().await?).unwrap()["result"]
				.as_str()
				.unwrap(),
		)?)?;

		Ok(account.nonce)
	}
}

pub struct Node {
	pub uri: String,
	pub websocket: Websocket,
	pub genesis_hash: Hash,
	pub versions: Versions,
	pub metadata: Metadata,
}
impl Node {
	pub async fn init(uri: impl Into<String>) -> AnyResult<Self> {
		let uri = uri.into();
		let websocket = Websocket::connect(&uri)?;

		websocket
			.send(serde_json::to_vec(&chain::get_block_hash(0u8)).unwrap())
			.await;
		let genesis_hash = array_bytes::hex_str_array_unchecked!(
			serde_json::from_slice::<Value>(&websocket.recv().await?).unwrap()["result"]
				.as_str()
				.unwrap(),
			32
		)
		.into();

		websocket
			.send(serde_json::to_vec(&state::get_runtime_version()).unwrap())
			.await;
		let versions = serde_json::from_value(
			serde_json::from_slice::<Value>(&websocket.recv().await?).unwrap()["result"].clone(),
		)
		.unwrap();

		websocket
			.send(serde_json::to_vec(&state::get_metadata()).unwrap())
			.await;
		let metadata = RuntimeMetadataPrefixed::decode(
			&mut &*array_bytes::bytes(
				serde_json::from_slice::<Value>(&websocket.recv().await?).unwrap()["result"]
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

	pub async fn disconnect(self) {
		self.websocket.disconnect().await;
	}

	pub async fn send(&self, bytes: Bytes) {
		self.websocket.send(bytes).await;
	}

	pub async fn recv(&self) -> AnyResult<Bytes> {
		Ok(self.websocket.recv().await?)
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
		module: impl AsRef<str>,
		call: impl AsRef<str>,
		data: impl Clone + Encode,
		signer: (&sr25519::Pair, Index),
	) -> AnyResult<String> {
		let call = call!(self.call(module.as_ref(), call.as_ref())?, data);
		let extra = Extra(Era::Immortal, Compact(signer.1), Compact(0));
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
				(),
			),
		);
		let signature = raw_payload
			.using_encoded(|payload| signer.0.sign(payload))
			.into();
		let extrinsic = Extrinsic {
			call,
			public_key: signer.0.public().0,
			signature,
			extra,
		};

		Ok(extrinsic.encode())
	}
}

pub struct Websocket {
	pub handle: Option<JoinHandle<AnyResult<()>>>,
	pub sender: Sender<Bytes>,
	pub receiver: Receiver<Bytes>,
}
impl Websocket {
	pub fn connect(uri: impl IntoClientRequest) -> AnyResult<Self> {
		trace!("Starting a new connection to the node");

		let (client_sender, node_receiver) = sync::channel(1);
		let (node_sender, client_receiver) = sync::channel(1);
		let (mut websocket, response) = tungstenite::connect(uri)?;

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			loop {
				websocket.write_message(Message::Binary(node_receiver.recv().await?))?;
				node_sender
					.send(websocket.read_message()?.into_data())
					.await;
			}
		});

		Ok(Self {
			handle: Some(handle),
			sender: client_sender,
			receiver: client_receiver,
		})
	}

	pub async fn disconnect(self) {
		if let Some(handle) = self.handle {
			handle.cancel().await;
		}
	}

	pub async fn send(&self, bytes: Bytes) {
		self.sender.send(bytes).await;
	}

	pub async fn recv(&self) -> AnyResult<Bytes> {
		Ok(self.receiver.recv().await?)
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

#[derive(Encode)]
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
				f(&sp_core::blake2_256(payload))
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

#[derive(Encode)]
pub struct Extrinsic<Call>
where
	Call: Encode,
{
	call: Call,
	public_key: PublicKey,
	signature: MultiSignature,
	extra: Extra,
}
impl<Call> Extrinsic<Call>
where
	Call: Encode,
{
	pub fn encode(&self) -> String {
		array_bytes::hex_str("0x", Encode::encode(self))
	}
}

#[derive(Encode)]
pub enum MultiSignature {
	_Ed25519,
	Sr25519(sr25519::Signature),
	_Ecdsa,
}
impl From<sr25519::Signature> for MultiSignature {
	fn from(signature: sr25519::Signature) -> Self {
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

// pub struct PublicKey(pub [u8; 32]);
// impl From<[u8; 32]> for PublicKey {
// 	fn from(bytes: [u8; 32]) -> Self {
// 		Self(bytes)
// 	}
// }
// impl TryFrom<&[u8]> for PublicKey {
// 	type Error = Error;

// 	fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
// 		let length = bytes.len();
// 		let mut fixed_bytes = [0; 32];

// 		fixed_bytes.copy_from_slice(bytes);

// 		if length == 32 {
// 			Ok(Self(fixed_bytes))
// 		} else {
// 			Err(Self::Error::InvalidPublicLength { length })
// 		}
// 	}
// }
// impl TryFrom<Vec<u8>> for PublicKey {
// 	type Error = Error;

// 	fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
// 		TryFrom::try_from(bytes.as_slice())
// 	}
// }

pub async fn test() -> AnyResult<()> {
	let uri = "ws://127.0.0.1:9944";
	let seed = "0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a";
	let colladar = Colladar::init(uri, seed).await?;
	let call = call!(colladar.node.call("Balances", "transfer")?);
	let extra = Extra(Era::Immortal, Compact(colladar.nonce().await?), Compact(0));
	let raw_payload = SignedPayload::from_raw(
		call.clone(),
		extra.clone(),
		(
			colladar.node.spec_version(),
			colladar.node.transaction_version(),
			colladar.node.genesis_hash(),
			colladar.node.genesis_hash(),
			(),
			(),
			(),
			(),
		),
	);
	let signature = raw_payload
		.using_encoded(|payload| colladar.signer.sign(payload))
		.into();
	let extrinsic = Extrinsic {
		call,
		public_key: colladar.public_key(),
		signature,
		extra,
	}
	.encode();

	colladar
		.node
		.send(serde_json::to_vec(&author::submit_and_watch_extrinsic(&extrinsic)).unwrap())
		.await;
	println!(
		"{}",
		serde_json::from_slice::<Value>(&colladar.node.recv().await?).unwrap()
	);

	let extrinsic_1 = colladar.node.extrinsic(
		"Balances",
		"transfer",
		(),
		(&colladar.signer, colladar.nonce().await?),
	)?;

	colladar
		.node
		.send(serde_json::to_vec(&author::submit_and_watch_extrinsic(&extrinsic)).unwrap())
		.await;
	println!(
		"{}",
		serde_json::from_slice::<Value>(&colladar.node.recv().await?).unwrap()
	);

	colladar.node.disconnect().await;

	Ok(())
}
