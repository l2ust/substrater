// --- std ---
use core::task::Poll;
use std::{
	borrow::Borrow,
	collections::{HashMap, HashSet},
	fmt::Display,
};
// --- crates.io ---
use async_std::{
	channel::{self, Sender},
	sync::{Arc, Mutex},
	task::{self, JoinHandle},
};
use async_tungstenite::{
	async_std as tungstenite_async_std,
	tungstenite::{client::IntoClientRequest, Message},
};
use futures::{
	future::{self, Either},
	pin_mut, SinkExt, StreamExt,
};
use futures_lite::FutureExt;
use serde::Serialize;
use serde_json::Value;
use subrpcer::{author, state};
use tracing::{debug, error, info, trace};
// --- substrater ---
use crate::{
	error::{AsyncError, SubstraterResult, WebsocketError},
	extrinsic::ExtrinsicState,
	r#type::*,
};

#[derive(Debug)]
pub struct Websocket {
	pub handle: Mutex<Option<JoinHandle<SubstraterResult<()>>>>,
	pub sender: Sender<Bytes>,
	pub rpc_id: CurrentRpcId,
	pub rpc_results: Arc<Mutex<HashMap<RpcId, Value>>>,
	pub subscription_ids: Mutex<HashSet<SubscriptionId>>,
	pub subscriptions: Arc<Mutex<HashMap<SubscriptionId, Value>>>,
}
// TODO: polish function visible
impl Websocket {
	pub async fn connect(uri: impl Display + IntoClientRequest + Unpin) -> SubstraterResult<Self> {
		trace!("`Websocket` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let rpc_results = Arc::new(Mutex::new(HashMap::new()));
		let subscriptions = Arc::new(Mutex::new(HashMap::new()));
		let (websocket, response) = tungstenite_async_std::connect_async(uri).await?;
		let (mut write, mut read) = websocket.split();

		for (header, _) in response.headers() {
			trace!("* {}", header);
		}

		let rpc_results_cloned = rpc_results.clone();
		let subscriptions_cloned = subscriptions.clone();
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
								let msg =
									serde_json::from_slice::<Value>(&msg?.into_data()).unwrap();

								debug!("{:?}", msg);

								if let Some(rpc_id) = msg["id"].as_u64() {
									rpc_results_cloned.lock().await.insert(rpc_id as _, msg);
								} else if let Some(subscription_id) =
									msg["params"]["subscription"].as_str()
								{
									subscriptions_cloned
										.lock()
										.await
										.insert(subscription_id.into(), msg);
								} else {
									// TODO
									error!("{:?}", msg);
								}

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
			handle: Mutex::new(Some(handle)),
			sender: client_sender,
			rpc_id: CurrentRpcId(Mutex::new(1)),
			rpc_results,
			subscription_ids: Mutex::new(HashSet::new()),
			subscriptions,
		})
	}

	pub async fn disconnect(self) {
		if let Some(handle) = self.handle.into_inner() {
			handle.cancel().await;
		}
	}

	pub async fn check_connection(&self) -> SubstraterResult<()> {
		if let Some(ref mut handle) = self.handle.lock().await.as_mut() {
			return future::poll_fn(|cx| match handle.poll(cx) {
				Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
				_ => Poll::Ready(Ok(())),
			})
			.await;
		}

		Err(WebsocketError::AlreadyClosed.into())
	}

	pub async fn send(&self, msg: impl Into<Bytes>) -> SubstraterResult<()> {
		Ok(self
			.sender
			.send(msg.into())
			.await
			.map_err(AsyncError::from)?)
	}

	pub async fn subscribe_storage(
		&self,
		storage_keys: impl AsRef<str>,
	) -> SubstraterResult<SubscriptionId> {
		let rpc_id = self.rpc_id().await;

		self.send(
			serde_json::to_vec(&state::subscribe_storage_with_id(
				[storage_keys.as_ref()],
				rpc_id,
			))
			.unwrap(),
		)
		.await?;

		let subscription_id = self.take_rpc_result_of(&rpc_id).await?["result"]
			.as_str()
			.unwrap()
			.into();

		self.add_subscription_id(&subscription_id).await;

		Ok(subscription_id)
	}

	pub async fn submit_and_watch_extrinsic(
		&self,
		extrinsic: impl Serialize,
		expect_extrinsic_state: ExtrinsicState,
	) -> SubstraterResult<()> {
		let rpc_id = self.rpc_id().await;

		self.send(
			serde_json::to_vec(&author::submit_and_watch_extrinsic_with_id(
				&extrinsic, rpc_id,
			))
			.unwrap(),
		)
		.await?;

		let subscription_id = self.take_rpc_result_of(&rpc_id).await?["result"]
			.as_str()
			.unwrap()
			.to_owned();

		self.add_subscription_id(&subscription_id).await;

		let unwatch_extrinsic_future = self.unwatch_extrinsic(&subscription_id);

		if expect_extrinsic_state.ignored() {
			unwatch_extrinsic_future.await?;

			return Ok(());
		}

		loop {
			let subscription = self.take_subscription_of(&subscription_id).await?;
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
		let rpc_id = self.rpc_id().await;
		let subscription_id = subscription_id.as_ref();

		self.send(
			serde_json::to_vec(&author::unwatch_extrinsic_with_id(subscription_id, rpc_id))
				.unwrap(),
		)
		.await?;

		// TODO: deadlock if unwatch failed
		self.take_rpc_result_of(rpc_id).await?;

		self.remove_subscription_id(subscription_id).await;

		Ok(())
	}

	pub async fn unsubscribe_storage(
		&self,
		subscription_id: impl AsRef<str>,
	) -> SubstraterResult<()> {
		let rpc_id = self.rpc_id().await;
		let subscription_id = subscription_id.as_ref();

		self.send(
			serde_json::to_vec(&state::unsubscribe_storage_with_id(subscription_id, rpc_id))
				.unwrap(),
		)
		.await?;

		// TODO: deadlock if unsubscribe failed
		self.take_rpc_result_of(rpc_id).await?;
		self.take_subscription_of(subscription_id).await?;

		self.remove_subscription_id(subscription_id).await;

		Ok(())
	}

	// pub async fn unsubscribe_all(&self) {}

	pub async fn rpc_id(&self) -> RpcId {
		self.rpc_id.get().await
	}

	pub async fn try_take_rpc_result_of(&self, rpc_id: impl Borrow<RpcId>) -> Option<Value> {
		self.rpc_results.lock().await.remove(rpc_id.borrow())
	}

	pub async fn take_rpc_result_of(&self, rpc_id: impl Borrow<RpcId>) -> SubstraterResult<Value> {
		let rpc_id = rpc_id.borrow();

		loop {
			self.check_connection().await?;

			if let Some(rpc_result) = self.try_take_rpc_result_of(rpc_id).await {
				return Ok(rpc_result);
			}
		}
	}

	pub async fn add_subscription_id(&self, subscription_id: impl Into<SubscriptionId>) {
		self.subscription_ids
			.lock()
			.await
			.insert(subscription_id.into());
	}

	pub async fn remove_subscription_id(&self, subscription_id: impl AsRef<str>) {
		self.subscription_ids
			.lock()
			.await
			.remove(subscription_id.as_ref());
	}

	pub async fn try_take_subscription_of(
		&self,
		subscription_id: impl AsRef<str>,
	) -> Option<Value> {
		self.subscriptions
			.lock()
			.await
			.remove(subscription_id.as_ref())
	}

	pub async fn take_subscription_of(
		&self,
		subscription_id: impl AsRef<str>,
	) -> SubstraterResult<Value> {
		let subscription_id = subscription_id.as_ref();

		loop {
			self.check_connection().await?;

			if let Some(subscription) = self.try_take_subscription_of(subscription_id).await {
				return Ok(subscription);
			}
		}
	}
}

#[derive(Debug)]
pub struct CurrentRpcId(Mutex<RpcId>);
impl CurrentRpcId {
	pub async fn get(&self) -> RpcId {
		let mut mutex = self.0.lock().await;
		let id = *mutex;

		if id == RpcId::max_value() {
			*mutex = 1;
		} else {
			*mutex += 1;
		}

		id
	}
}
