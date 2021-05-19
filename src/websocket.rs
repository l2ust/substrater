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
use serde_json::Value;
// --- substrater ---
use crate::{
	error::{AsyncError, SubstraterResult, WebsocketError},
	r#type::*,
};

#[derive(Debug)]
pub struct Websocket {
	pub handle: Mutex<Option<JoinHandle<SubstraterResult<()>>>>,
	pub sender: Sender<Bytes>,
	pub rpc_id: RpcId,
	pub rpc_results: Arc<Mutex<HashMap<Id, Value>>>,
	pub subscription_ids: Mutex<HashSet<SubscriptionId>>,
	pub subscriptions: Arc<Mutex<HashMap<SubscriptionId, Value>>>,
}
// TODO: polish function visible
impl Websocket {
	pub async fn connect(uri: impl Display + IntoClientRequest + Unpin) -> SubstraterResult<Self> {
		tracing::info!("`Websocket` starting a new connection to `{}`", uri);

		let (client_sender, node_receiver) = channel::unbounded();
		let rpc_results = Arc::new(Mutex::new(HashMap::new()));
		let subscriptions = Arc::new(Mutex::new(HashMap::new()));
		let (websocket, _) = tungstenite_async_std::connect_async(uri).await?;
		let (mut write, mut read) = websocket.split();
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
						tracing::trace!("{:?}", msg);

						write.send(Message::from(msg)).await?;

						read_future = read_future_continue;
					}
					Either::Right((msg, _)) => {
						match msg {
							Some(msg) => {
								let msg =
									serde_json::from_slice::<Value>(&msg?.into_data()).unwrap();
								tracing::trace!("{:?}", msg);

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
									tracing::error!("{:?}", msg);
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
			rpc_id: RpcId(Mutex::new(1)),
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

	pub async fn rpc_id(&self) -> Id {
		self.rpc_id.get().await
	}

	pub async fn try_take_rpc_result_of(&self, rpc_id: impl Borrow<Id>) -> Option<Value> {
		self.rpc_results.lock().await.remove(rpc_id.borrow())
	}

	pub async fn take_rpc_result_of(&self, rpc_id: impl Borrow<Id>) -> SubstraterResult<Value> {
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
pub struct RpcId(Mutex<Id>);
impl RpcId {
	pub async fn get(&self) -> Id {
		let mut mutex = self.0.lock().await;
		let id = *mutex;

		if id == Id::max_value() {
			*mutex = 1;
		} else {
			*mutex += 1;
		}

		id
	}
}
