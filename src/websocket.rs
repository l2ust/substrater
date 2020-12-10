// --- std ---
use std::fmt::Display;
// --- crates.io ---
use async_std::{
	channel::{self, Receiver, Sender},
	task::{self, JoinHandle},
};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{error, info, trace};
use tungstenite::{client::IntoClientRequest, Message};
// --- substrater ---
use crate::{
	error::{AsyncError, SerdeJsonError, SubstraterResult},
	extrinsic::ExtrinsicState,
	r#type::Bytes,
};

#[async_trait]
pub trait Websocket: Sized {
	type ClientMsg;
	type NodeMsg;

	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self>;

	async fn disconnect(self);

	async fn send(&self, msg: Self::ClientMsg) -> SubstraterResult<()>;

	async fn recv(&self) -> SubstraterResult<Self::NodeMsg>;
}

#[derive(Debug)]
pub struct Messenger {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<Bytes>,
	pub receiver: Receiver<Bytes>,
}
#[async_trait]
impl Websocket for Messenger {
	type ClientMsg = Bytes;
	type NodeMsg = Bytes;

	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self> {
		trace!("`Messenger` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let (node_sender, client_receiver) = channel::unbounded();
		let (mut messenger, response) = tungstenite::connect(uri)?;

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			loop {
				messenger.write_message(Message::Binary(
					node_receiver.recv().await.map_err(AsyncError::from)?,
				))?;
				node_sender
					.send(messenger.read_message()?.into_data())
					.await
					.map_err(AsyncError::from)?;
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

	async fn send(&self, msg: Self::ClientMsg) -> SubstraterResult<()> {
		Ok(self.sender.send(msg).await.map_err(AsyncError::from)?)
	}

	async fn recv(&self) -> SubstraterResult<Self::NodeMsg> {
		Ok(self.receiver.recv().await.map_err(AsyncError::from)?)
	}
}

#[derive(Debug)]
pub struct Excreamer {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<(Bytes, ExtrinsicState)>,
}
#[async_trait]
impl Websocket for Excreamer {
	type ClientMsg = (Bytes, ExtrinsicState);
	type NodeMsg = ();

	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self> {
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
				let (bytes, expect_extrinsic_state): Self::ClientMsg =
					node_receiver.recv().await.map_err(AsyncError::from)?;

				excreamer.write_message(Message::Binary(bytes))?;

				let ignored_state = expect_extrinsic_state.ignored();

				loop {
					let (extrinsic_state, subscription, block_hash) = parse(
						serde_json::from_slice::<Value>(&excreamer.read_message()?.into_data()).map_err(SerdeJsonError::from)?,
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

	async fn send(&self, msg: Self::ClientMsg) -> SubstraterResult<()> {
		Ok(self.sender.send(msg).await.map_err(AsyncError::from)?)
	}

	async fn recv(&self) -> SubstraterResult<Self::NodeMsg> {
		Ok(())
	}
}
