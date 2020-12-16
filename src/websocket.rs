// --- std ---
use std::fmt::Display;
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
use serde_json::Value;
use subrpcer::author;
use tracing::{debug, error, info, trace};
// --- substrater ---
use crate::{
	error::{AsyncError, SubstraterResult},
	extrinsic::ExtrinsicState,
	r#type::*,
};

#[derive(Debug)]
pub struct Websocket {
	pub handle: Option<JoinHandle<SubstraterResult<()>>>,
	pub sender: Sender<Bytes>,
	pub rpc_id: CurrentRpcId,
	pub rpc_results: Arc<Mutex<Vec<(RpcId, Value)>>>,
	pub subscriptions: Arc<Mutex<Vec<(SubscriptionId, Value)>>>,
}
impl Websocket {
	pub async fn connect(uri: impl Display + IntoClientRequest + Unpin) -> SubstraterResult<Self> {
		trace!("`Websocket` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let rpc_results = Arc::new(Mutex::new(vec![]));
		let subscriptions = Arc::new(Mutex::new(vec![]));
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
								let msg = msg?;
								let msg =
									serde_json::from_slice::<Value>(&msg.into_data()).unwrap();

								debug!("{:?}", msg);

								if let Some(rpc_id) = msg["id"].as_u64() {
									rpc_results_cloned.lock().await.push((rpc_id as _, msg));
								} else if let Some(subscription_id) =
									msg["params"]["subscription"].as_str()
								{
									subscriptions_cloned
										.lock()
										.await
										.push((subscription_id.into(), msg));
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
			handle: Some(handle),
			sender: client_sender,
			rpc_id: CurrentRpcId(Mutex::new(1)),
			rpc_results,
			subscriptions,
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

	pub async fn send_and_watch_extrinsic(
		&self,
		rpc_id: RpcId,
		msg: Bytes,
		expect_extrinsic_state: ExtrinsicState,
	) -> SubstraterResult<()> {
		self.send(msg).await?;

		let subscription_id = self.take_rpc_result_of(&rpc_id).await["result"]
			.as_str()
			.unwrap()
			.into();
		let unwatch_extrinsic_future =
			self.send(serde_json::to_vec(&author::unwatch_extrinsic(&subscription_id)).unwrap());

		if expect_extrinsic_state.ignored() {
			unwatch_extrinsic_future.await?;

			return Ok(());
		}

		loop {
			let subscription = self.take_subscription_of(&subscription_id).await;
			let result = &subscription["params"]["result"];
			let (extrinsic_state, block_hash) = if result.is_string() {
				(ExtrinsicState::Ready, "")
			} else {
				if let Some(block_hash) = result.get("inBlock") {
					(ExtrinsicState::InBlock, block_hash.as_str().unwrap())
				} else if let Some(block_hash) = result.get("finalized") {
					(ExtrinsicState::Finalized, block_hash.as_str().unwrap())
				} else {
					// TODO
					error!("{:?}", subscription);

					(ExtrinsicState::Ignored, "")
				}
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

	pub async fn rpc_id(&self) -> RpcId {
		self.rpc_id.get().await
	}

	pub async fn try_take_rpc_result_of(&self, rpc_id: &RpcId) -> Option<Value> {
		let mut ref_rpc_results = self.rpc_results.lock().await;

		if let Some(position) = ref_rpc_results
			.iter()
			.position(|(rpc_id_, _)| rpc_id_ == rpc_id)
		{
			Some(ref_rpc_results.remove(position).1)
		} else {
			None
		}
	}

	pub async fn take_rpc_result_of(&self, rpc_id: &RpcId) -> Value {
		loop {
			if let Some(rpc_result) = self.try_take_rpc_result_of(rpc_id).await {
				return rpc_result;
			}
		}
	}

	pub async fn try_take_subscription_of(
		&self,
		subscription_id: &SubscriptionId,
	) -> Option<Value> {
		let mut ref_subscriptions = self.subscriptions.lock().await;

		if let Some(position) = ref_subscriptions
			.iter()
			.position(|(subscription_id_, _)| subscription_id_ == subscription_id)
		{
			Some(ref_subscriptions.remove(position).1)
		} else {
			None
		}
	}

	pub async fn take_subscription_of(&self, subscription_id: &SubscriptionId) -> Value {
		loop {
			if let Some(subscription) = self.try_take_subscription_of(subscription_id).await {
				return subscription;
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
