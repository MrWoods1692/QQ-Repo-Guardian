use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use clap::Parser;
use qq_repo_guardian::{
    bot, config::AppConfig, http, notifier::Notifier, poller::GithubPagePoller,
};
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
    let github = Arc::new(config.github);
    let notifier = Arc::new(Notifier::new(bot));
    if config.poller.enabled {
        let poller = Arc::new(GithubPagePoller::new(github.clone(), notifier.clone())?);
        tokio::spawn(poller.run(Duration::from_secs(config.poller.interval_secs.max(30))));
    }
    let public_base_url = std::env::var("QRG_SERVER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("http://{}", address));
    let state = http::AppState::new(github, notifier, public_base_url);

    tracing::info!(%address, "starting qq-repo-guardian");
    http::serve(address, state).await
}
