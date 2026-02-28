#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sculk_cli::run_cli().await
}
