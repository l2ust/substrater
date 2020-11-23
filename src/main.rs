mod colladar;
mod error;
mod ethereum;

// --- crates.io ---
use anyhow::Result as AnyResult;

#[async_std::main]
async fn main() -> AnyResult<()> {
	std::env::set_var("RUST_LOG", "colladar");
	pretty_env_logger::init_timed();

	colladar::test().await?;

	Ok(())
}
