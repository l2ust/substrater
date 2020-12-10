pub mod error;
pub mod ethereum;
pub mod extrinsic;
pub mod substrater;
pub mod r#type;
pub mod websocket;

#[async_std::main]
async fn main() {
	std::env::set_var("RUST_LOG", "substrater");
	pretty_env_logger::init_timed();

	substrater::test().await.unwrap();
}
