// --- std ---
use std::convert::{TryFrom, TryInto};
// --- crates.io ---
use anyhow::Result as AnyResult;
use array_bytes::*;
use async_std::{
	sync::{self, Receiver, Sender},
	task::{self, JoinHandle},
};
use parity_scale_codec::{Compact, Decode, Encode};
use serde::Deserialize;
use serde_json::Value;
use subrpcer::{chain, state};
use tracing::trace;
use tungstenite::{client::IntoClientRequest, Message};
// --- github.com ---
use frame_metadata::{DecodeDifferent, RuntimeMetadata, RuntimeMetadataPrefixed};
use sp_core::{crypto, sr25519, Pair, H256 as Hash};
// --- colladar ---
use crate::error::Error;

// #[macro_export]
// macro_rules! call {
// 	() => {{

// 	}}
// }

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

// SpecVersion, TxVersion, Genesis, Era, Nonce, Weight, TransactionPayment, EthereumRelayHeaderParcel
pub type AdditionalSigned = (u32, u32, Hash, Hash, (), (), (), ());

pub struct Colladar {
	pub uri: String,
	pub signer: sr25519::Pair,
	pub websocket: Websocket,
	pub genesis_hash: Hash,
	pub versions: Versions,
	pub metadata: Metadata,
}
impl Colladar {
	pub async fn init(uri: impl Into<String>, seed: &str) -> AnyResult<Self> {
		let uri = uri.into();
		let websocket = Websocket::connect(&uri)?;

		websocket
			.send(serde_json::to_vec(&chain::get_block_hash(0u8)).unwrap())
			.await;
		let genesis_hash = hex_str_array_unchecked!(
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
			&mut &*bytes(
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
			signer: <sr25519::Pair as crypto::Pair>::from_seed_slice(&bytes(seed).unwrap())
				.map_err(|err| Error::InvalidSeed {
					seed: seed.into(),
					err,
				})
				.unwrap()
				.into(),
			websocket,
			genesis_hash,
			versions,
			metadata,
		})
	}

	fn public_key(&self) -> PublicKey {
		self.signer.public().0
	}

	async fn nonce(&self) {}
}

pub struct Node {
	pub uri: String,
	pub websocket: Websocket,
	pub genesis_hash: Hash,
	pub versions: Versions,
	pub metadata: Metadata,
}
impl Node {}

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
	spec_version: u32,
	transaction_version: u32,
}

#[derive(Debug)]
pub struct Metadata {
	pub modules: Vec<Module>,
}
impl Metadata {
	pub fn call(
		&self,
		module_name: impl Into<String>,
		call_name: impl Into<String>,
	) -> AnyResult<[u32; 2]> {
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

		match runtime_metadata {
			RuntimeMetadata::V0(_) => err!("V0"),
			RuntimeMetadata::V1(_) => err!("V1"),
			RuntimeMetadata::V2(_) => err!("V2"),
			RuntimeMetadata::V3(_) => err!("V3"),
			RuntimeMetadata::V4(_) => err!("V4"),
			RuntimeMetadata::V5(_) => err!("V5"),
			RuntimeMetadata::V6(_) => err!("V6"),
			RuntimeMetadata::V7(_) => err!("V7"),
			RuntimeMetadata::V8(_) => err!("V8"),
			RuntimeMetadata::V9(_) => err!("V9"),
			RuntimeMetadata::V10(_) => err!("V10"),
			RuntimeMetadata::V11(_) => err!("V11"),
			RuntimeMetadata::V12(runtime_metadata) => {
				let mut metadata = Self { modules: vec![] };

				for module in inner!(runtime_metadata.modules) {
					let mut calls = vec![];

					if let Some(wrap_calls) = module.calls {
						for call in inner!(wrap_calls) {
							calls.push(Call {
								name: inner!(call.name),
							});
						}
					}

					metadata.modules.push(Module {
						name: inner!(module.name),
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
	pub calls: Vec<Call>,
}

#[derive(Debug)]
pub struct Call {
	pub name: String,
}

#[derive(Encode)]
pub struct SignedPayload<Call: Encode>((Call, Extra, AdditionalSigned));
impl<Call> SignedPayload<Call>
where
	Call: Encode,
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

// Era, Nonce, TransactionPayment
#[derive(Clone, Encode)]
pub struct Extra(Era, Compact<u32>, Compact<u128>);

#[derive(Clone, Encode)]
pub enum Era {
	Immortal,
	Mortal(u64, u64),
}

pub struct Extrinsic<Call> {
	call: Call,
	public_key: PublicKey,
	signature: MultiSignature,
	extra: Extra,
}

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

	let call = colladar.metadata.call("Balances", "transfer")?;
	let extra = Extra(Era::Immortal, Compact(0), Compact(0));
	let raw_payload = SignedPayload::from_raw(
		call.clone(),
		extra.clone(),
		(
			colladar.versions.spec_version,
			colladar.versions.transaction_version,
			colladar.genesis_hash,
			colladar.genesis_hash,
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
	};

	colladar.websocket.disconnect().await;

	Ok(())
}
