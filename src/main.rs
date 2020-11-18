// --- std ---
use std::sync::mpsc;
// --- crates.io ---
use frame_system::EventRecord;
use isahc::HttpClient;
use parity_scale_codec::{Compact, Decode};
use sp_core::{crypto, sr25519, Pair, H256};
// --- github ---
use pangolin_runtime::Event;
use substrate_api_client::{
	compose_call, compose_extrinsic, extrinsic::xt_primitives::UncheckedExtrinsicV4, Api, XtStatus,
};

type Error = Box<dyn std::error::Error>;
type Result<T> = ::std::result::Result<T, Error>;

struct Colladar {}

struct RpcClient {
	http_client: HttpClient,
}

#[async_std::main]
async fn main() -> Result<()> {
	pretty_env_logger::init_timed();

	let url = "ws://127.0.0.1:9944";
	let root = sr25519::Pair::from(
		<sr25519::Pair as crypto::Pair>::from_seed_slice(&bytes(
			"0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a",
		))
		.unwrap(),
	);
	let api = Api::new(url.into()).set_signer(root);
	let (events_in, events_out) = mpsc::channel();

	api.subscribe_events(events_in);

	loop {
		let call = compose_call!(
			api.metadata.clone(),
			"Balances",
			"set_balance",
			sr25519::Public::from_raw(array!(
				bytes("0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"),
				32
			)),
			Compact(1_000_000_000_000u128),
			Compact(0u128)
		);
		let xt: UncheckedExtrinsicV4<_> = compose_extrinsic!(api.clone(), "Sudo", "sudo", call);

		if let Ok(tx_hash) = api.send_extrinsic(xt.hex_encode(), XtStatus::InBlock) {
			println!("[+] Transaction got included. Hash: {:?}", tx_hash);
		}

		if let Ok(event_str) = events_out.recv() {
			let bytes = bytes(&event_str);
			let events = <Vec<EventRecord<Event, H256>>>::decode(&mut &*bytes);

			println!("{:#?}", events);
		}

		std::thread::sleep_ms(1000);
	}

	Ok(())
}

pub fn bytes(s: &str) -> Vec<u8> {
	let s = s.trim_start_matches("0x");

	(0..s.len())
		.step_by(2)
		.map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
		.collect()
}

#[macro_export]
macro_rules! array {
	($vec:expr, $len:expr) => {{
		unsafe { *($vec.as_ptr() as *const [u8; $len]) }
		}};
}
