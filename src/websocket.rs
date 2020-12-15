// --- std ---
use std::fmt::Display;
// --- crates.io ---
use async_std::{
	channel::{self, Receiver, Sender},
	sync::Mutex,
	task::{self, JoinHandle},
};
use async_trait::async_trait;
use async_tungstenite::async_std as tungstenite_async_std;
use futures::{
	future::{self, Either},
	pin_mut, SinkExt, StreamExt,
};
use serde_json::Value;
use tracing::{debug, error, info, trace};
use tungstenite::{client::IntoClientRequest, Message};
// --- substrater ---
use crate::{
	error::{AsyncError, SerdeJsonError, SubstraterResult},
	extrinsic::ExtrinsicState,
	r#type::Bytes,
};

pub struct Websocket2 {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<Bytes>,
	pub rpc_id: RpcId,
	pub rpc_results: Arc<Mutex<Vec<(u32, Value)>>>,
	// pub subscriptions: Arc<Mutex<Vec<(String, Option<Bytes>)>>>,
}
impl Websocket2 {
	pub async fn connect(uri: impl Display + IntoClientRequest + Unpin) -> SubstraterResult<Self> {
		trace!("`Websocket2` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let rpc_results = Arc::new(Mutex::new(vec![]));
		let (websocket2, response) = tungstenite_async_std::connect_async(uri).await?;
		let (mut write, mut read) = websocket2.split();

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			let mut read_future = read.next();

			loop {
				let recv_future = node_receiver.recv();

				pin_mut!(recv_future);

				match future::select(recv_future, read_future).await {
					Either::Left((msg, read_future_continue)) => {
						let msg = msg.map_err(AsyncError::from)?;

						debug!("{:?}", msg);

						write.send(Message::from(msg)).await?;

						read_future = read_future_continue;
					}
					Either::Right((msg, _)) => {
						match msg {
							Some(msg) => {
								let msg = msg?;
								let msg =
									serde_json::from_slice::<Value>(&msg.into_data()).unwrap();

								debug!("{:?}", msg);

								read_future = read.next();
							}
							None => break,
						};
					}
				}
			}

			Ok(())
		});

		Ok(Self {
			handle: Some(handle),
			sender: client_sender,
			rpc_id: RpcId(Mutex::new(1)),
			rpc_results,
			// subscriptions,
		})
	}

	pub async fn disconnect(self) {
		if let Some(handle) = self.handle {
			handle.cancel().await;
		}
	}

	pub async fn send(&self, msg: Bytes) -> SubstraterResult<()> {
		Ok(self.sender.send(msg).await.map_err(AsyncError::from)?)
	}

	// pub async fn recv_rpc_result(&self, id: Bytes) -> SubstraterResult<()> {
	// Ok(self.sender.send(msg).await.map_err(AsyncError::from)?)
	// }

	// pub async fn recv_subscriptions(&self)

	pub async fn rpc_id(&self) -> u32 {
		self.rpc_id.get().await
	}
}

pub struct RpcId(Mutex<u32>);
impl RpcId {
	pub async fn get(&self) -> u32 {
		let mut mutex = self.0.lock().await;
		let id = *mutex;

		if id == u32::max_value() {
			*mutex = 1;
		} else {
			*mutex += 1;
		}

		id
	}
}

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
pub struct Postman {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<Bytes>,
	pub receiver: Receiver<Bytes>,
}
#[async_trait]
impl Websocket for Postman {
	type ClientMsg = Bytes;
	type NodeMsg = Bytes;

	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self> {
		trace!("`Postman` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let (node_sender, client_receiver) = channel::unbounded();
		let (mut postman, response) = tungstenite::connect(uri)?;

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let handle = task::spawn(async move {
			loop {
				postman.write_message(Message::Binary(
					node_receiver.recv().await.map_err(AsyncError::from)?,
				))?;
				node_sender
					.send(postman.read_message()?.into_data())
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
pub struct Salesman {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<(Bytes, ExtrinsicState)>,
}
#[async_trait]
impl Websocket for Salesman {
	type ClientMsg = (Bytes, ExtrinsicState);
	type NodeMsg = ();

	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self> {
		trace!("`Salesman` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let (mut salesman, response) = tungstenite::connect(uri)?;

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

				salesman.write_message(Message::Binary(bytes))?;

				let ignored_state = expect_extrinsic_state.ignored();

				loop {
					let (extrinsic_state, subscription, block_hash) = parse(
						serde_json::from_slice::<Value>(&salesman.read_message()?.into_data())
							.map_err(SerdeJsonError::from)?,
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

// #[derive(Debug)]
// pub struct Fisherman {
// 	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
// 	pub receiver: Receiver<Bytes>,
// }
// #[async_trait]
// impl Websocket for Fisherman {
// 	type ClientMsg = Bytes;
// 	type NodeMsg = Bytes;

// 	fn connect(uri: impl Display + IntoClientRequest) -> SubstraterResult<Self> {
// 		trace!("`Fisherman` starting a new connection to `{}`", uri);

// 		let (client_sender, node_receiver) = channel::unbounded();
// 		let (mut salesman, response) = tungstenite::connect(uri)?;

// 		for (header, _) in response.headers() {
// 			trace!("* {}", header);
// 		}

// 		let handle = task::spawn(async move {});

// 		unimplemented!()
// 	}

// 	async fn disconnect(self) {
// 		if let Some(handle) = self.handle {
// 			handle.cancel().await;
// 		}
// 	}

// 	async fn send(&self, msg: Self::ClientMsg) -> SubstraterResult<()> {
// 		Ok(self.sender.send(msg).await.map_err(AsyncError::from)?)
// 	}

// 	async fn recv(&self) -> SubstraterResult<Self::NodeMsg> {
// 		Ok(self.receiver.recv().await.map_err(AsyncError::from)?)
// 	}
// }
