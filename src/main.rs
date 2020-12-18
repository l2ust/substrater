pub mod error;
pub mod extrinsic;
pub mod frame;
pub mod runtime;
pub mod substrater;
pub mod r#type;
pub mod websocket;

#[async_std::main]
async fn main() {
	std::env::set_var("RUST_LOG", "substrater");
	pretty_env_logger::init_timed();

	substrater::test().await.unwrap();
}
