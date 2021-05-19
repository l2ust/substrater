// --- std ---
use std::mem;
// --- crates.io ---
use parity_scale_codec::{Compact, Encode, FullCodec};
use subcryptor::{MultiSignature, PublicKey};
// --- substrater ---
use crate::{frame::system::Nonce, runtime::pangolin::Balance};

pub struct SignedPayload<Call: Encode, AdditionalSigned: FullCodec>(
	pub (Call, Extra, AdditionalSigned),
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

pub struct Extrinsic<Call>
where
	Call: Encode,
{
	pub signature: Option<(PublicKey, MultiSignature, Extra)>,
	pub call: Call,
}
impl<Call> Extrinsic<Call>
where
	Call: Encode,
{
	pub fn encode(&self) -> String {
		array_bytes::bytes2hex("0x", Encode::encode(self))
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

// Era, Nonce, TransactionPayment
#[derive(Clone, Debug, Encode)]
pub struct Extra(pub Era, pub Compact<Nonce>, pub Compact<Balance>);

#[derive(Clone, Debug, Encode)]
pub enum Era {
	Immortal,
	Mortal(u64, u64),
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
		matches!(self, Self::Ignored)
	}
}
