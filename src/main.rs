use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use clap::Parser;
use qq_repo_guardian::{bot, config::AppConfig, http, notifier::Notifier};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
#[command(author, version, about = "QQ bot for GitHub repository notifications")]
struct Args {
    #[arg(short, long, env = "QRG_CONFIG", default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let config =
        AppConfig::load(&args.config).with_context(|| format!("failed to load {}", args.config))?;
    let address: SocketAddr = config.server.bind.parse().context("invalid server.bind")?;
    let bot = bot::from_config(&config.bot).context("failed to initialize bot client")?;
    let state = http::AppState::new(Arc::new(config.github), Arc::new(Notifier::new(bot)));

    tracing::info!(%address, "starting qq-repo-guardian");
    http::serve(address, state).await
}
