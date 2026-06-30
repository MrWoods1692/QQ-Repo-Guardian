use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use clap::Parser;
use qq_repo_guardian::{
    bot, config::AppConfig, http, notifier::Notifier, poller::GithubPagePoller,
    scheduler::ScheduleRuntime,
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
    let schedule_config = config.schedule.clone();
    let github = Arc::new(config.github);
    let public_base_url = std::env::var("QRG_SERVER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("http://{}", address));
    let notifier = Arc::new(Notifier::with_public_base_url(bot, public_base_url.clone()));
    let github_client = qq_repo_guardian::github::build_github_client(
        config.poller.proxy.as_deref(),
        Duration::from_secs(config.poller.timeout_secs.max(3)),
    )?;
    if config.poller.enabled {
        let poll_interval = Duration::from_secs(config.poller.interval_secs.max(30));
        tracing::info!(
            interval_secs = poll_interval.as_secs(),
            timeout_secs = config.poller.timeout_secs.max(3),
            "starting GitHub page poller"
        );
        let poller = Arc::new(GithubPagePoller::new(
            github.clone(),
            notifier.clone(),
            &config.poller,
        )?);
        tokio::spawn(poller.run(poll_interval));
    }
    let schedule = if schedule_config.enabled {
        let targets = github
            .repositories
            .iter()
            .flat_map(|repository| repository.targets.iter().cloned())
            .collect::<Vec<_>>();
        let schedule = Arc::new(ScheduleRuntime::new(
            schedule_config,
            notifier.clone(),
            &targets,
        ));
        if schedule.has_groups() {
            tokio::spawn(schedule.clone().run());
            Some(schedule)
        } else {
            tracing::warn!("schedule is enabled but no QQ groups are configured");
            None
        }
    } else {
        None
    };
    let state = http::AppState::new(github, notifier, github_client, schedule, public_base_url);

    tracing::info!(%address, "starting qq-repo-guardian");
    http::serve(address, state).await
}
