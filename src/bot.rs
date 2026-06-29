#[cfg(feature = "proc-qq")]
use std::net::{Ipv4Addr, SocketAddr};
#[cfg(feature = "proc-qq")]
use std::pin::Pin;
use std::sync::{Arc, Mutex};
#[cfg(feature = "proc-qq")]
use std::task::{Context as TaskContext, Poll};
#[cfg(feature = "proc-qq")]
use std::time::Duration;

use async_trait::async_trait;
#[cfg(feature = "proc-qq")]
use proc_qq::features::connect_handler::{Connection, ConnectionHandler};
#[cfg(feature = "proc-qq")]
use proc_qq::re_exports::ricq::qsign::QSignClient;
#[cfg(feature = "proc-qq")]
use proc_qq::re_exports::ricq::version::ANDROID_WATCH;
#[cfg(feature = "proc-qq")]
use proc_qq::re_exports::ricq_core::msg::MessageChain;
#[cfg(feature = "proc-qq")]
use proc_qq::re_exports::ricq_core::msg::elem::Text;
#[cfg(feature = "proc-qq")]
use proc_qq::{Authentication, ClientBuilder, DeviceSource, FileSessionStore, ShowQR, run_client};

use crate::config::{BotConfig, NotifyTarget};

#[async_trait]
pub trait BotClient: Send + Sync {
    async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()>;
}

pub fn from_config(config: &BotConfig) -> anyhow::Result<Arc<dyn BotClient>> {
    match config {
        BotConfig::ProcQq {
            device_path,
            session_path,
            qsign_endpoint,
            qsign_key,
            qsign_command,
            qsign_timeout_secs,
        } => proc_qq_client::start(
            device_path,
            session_path,
            qsign_endpoint,
            qsign_key,
            qsign_command.as_deref(),
            *qsign_timeout_secs,
        ),
    }
}

#[derive(Default)]
pub struct MockBot {
    messages: Mutex<Vec<(NotifyTarget, String)>>,
}

impl MockBot {
    pub fn messages(&self) -> Vec<(NotifyTarget, String)> {
        self.messages
            .lock()
            .expect("mock bot mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl BotClient for MockBot {
    async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
        self.messages
            .lock()
            .expect("mock bot mutex poisoned")
            .push((target.clone(), message.to_string()));
        tracing::info!(?target, %message, "mock bot message");
        Ok(())
    }
}

#[cfg(feature = "proc-qq")]
mod proc_qq_client {
    use super::*;
    use anyhow::Context;
    use std::str::FromStr;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::net::TcpStream;
    use tokio::task::JoinSet;
    use url::Url;

    pub struct ProcQqClient {
        client: Arc<proc_qq::Client>,
    }

    struct TimedConnectionHandler {
        timeout: Duration,
    }

    struct TimedConnection(TcpStream);

    impl AsyncRead for TimedConnection {
        fn poll_read(
            mut self: Pin<&mut Self>,
            context: &mut TaskContext<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_read(context, buffer)
        }
    }

    impl AsyncWrite for TimedConnection {
        fn poll_write(
            mut self: Pin<&mut Self>,
            context: &mut TaskContext<'_>,
            buffer: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.0).poll_write(context, buffer)
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            context: &mut TaskContext<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_flush(context)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            context: &mut TaskContext<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_shutdown(context)
        }
    }

    impl Connection for TimedConnection {}

    #[async_trait]
    impl ConnectionHandler for TimedConnectionHandler {
        async fn connect(&self, address: SocketAddr) -> anyhow::Result<Box<dyn Connection>> {
            let addresses = qq_login_addresses(address).await;
            tracing::info!(
                address_count = addresses.len(),
                upstream_address = %address,
                timeout_secs = self.timeout.as_secs(),
                "connecting to QQ login servers"
            );
            let stream = connect_fastest(addresses, self.timeout)
                .await
                .with_context(|| {
                    format!(
                        "failed to connect to any QQ login server within {} seconds",
                        self.timeout.as_secs()
                    )
                })?;
            Ok(Box::new(TimedConnection(stream)))
        }
    }

    async fn qq_login_addresses(upstream_address: SocketAddr) -> Vec<SocketAddr> {
        let mut addresses = vec![upstream_address];
        addresses.extend([
            SocketAddr::new(Ipv4Addr::new(42, 81, 172, 81).into(), 80),
            SocketAddr::new(Ipv4Addr::new(114, 221, 148, 59).into(), 14000),
            SocketAddr::new(Ipv4Addr::new(42, 81, 172, 147).into(), 443),
            SocketAddr::new(Ipv4Addr::new(125, 94, 60, 146).into(), 80),
            SocketAddr::new(Ipv4Addr::new(114, 221, 144, 215).into(), 80),
            SocketAddr::new(Ipv4Addr::new(42, 81, 172, 22).into(), 80),
        ]);
        if let Ok(resolved) = tokio::net::lookup_host(("msfwifi.3g.qq.com", 8080)).await {
            addresses.extend(resolved);
        }
        addresses.sort_unstable();
        addresses.dedup();
        addresses
    }

    async fn connect_fastest(
        addresses: Vec<SocketAddr>,
        timeout: Duration,
    ) -> std::io::Result<TcpStream> {
        let mut attempts = JoinSet::new();
        for address in addresses {
            attempts.spawn(async move {
                let result = tokio::time::timeout(timeout, TcpStream::connect(address)).await;
                match result {
                    Ok(Ok(stream)) => Ok((address, stream)),
                    Ok(Err(error)) => Err((address, error)),
                    Err(error) => Err((address, std::io::Error::from(error))),
                }
            });
        }

        let mut last_error = None;
        while let Some(result) = attempts.join_next().await {
            match result {
                Ok(Ok((address, stream))) => {
                    tracing::info!(%address, "connected to QQ login server");
                    return Ok(stream);
                }
                Ok(Err((address, error))) => {
                    tracing::debug!(%address, ?error, "QQ login server connection failed");
                    last_error = Some(error);
                }
                Err(error) => {
                    tracing::debug!(?error, "QQ login server connection task failed");
                    last_error = Some(std::io::Error::other(error));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "no QQ login server connected",
            )
        }))
    }

    pub fn start(
        device_path: &str,
        session_path: &str,
        qsign_endpoint: &str,
        qsign_key: &str,
        qsign_command: Option<&str>,
        qsign_timeout_secs: u64,
    ) -> anyhow::Result<Arc<dyn BotClient>> {
        let device_path = device_path.to_string();
        let session_path = session_path.to_string();
        let qsign_endpoint = configured_qsign_endpoint(qsign_endpoint);
        let qsign_key = configured_qsign_key(qsign_key);
        let qsign_command = configured_qsign_command(qsign_command);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        tokio::spawn(async move {
            let startup_tx = tx.clone();
            let result = async move {
                tracing::info!(
                    qsign_endpoint = %qsign_endpoint,
                    device_path = %device_path,
                    session_path = %session_path,
                    "starting proc_qq client"
                );
                ensure_qsign_ready(
                    &qsign_endpoint,
                    qsign_command.as_deref(),
                    Duration::from_secs(qsign_timeout_secs),
                )
                    .await
                    .with_context(|| format!("qsign service is not reachable at {qsign_endpoint}"))?;
                let diagnostic_qsign_endpoint = qsign_endpoint.clone();
                let qsign = QSignClient::new(
                    qsign_endpoint,
                    qsign_key,
                    Duration::from_secs(qsign_timeout_secs),
                )?;
                match tokio::fs::remove_file("qrcode.png").await {
                    Ok(()) => tracing::info!(qrcode_path = "qrcode.png", "removed stale qrcode file"),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => tracing::warn!(?error, qrcode_path = "qrcode.png", "failed to remove stale qrcode file"),
                }
                if tokio::fs::metadata(&session_path).await.is_err() {
                    tracing::info!(session_path = %session_path, "no saved QQ session found; QR code will be printed in this terminal after connecting to QQ login server");
                }
                let client = ClientBuilder::new()
                    .authentication(Authentication::QRCode)
                    .show_rq(ShowQR::PrintToConsole)
                    .device(DeviceSource::JsonFile(device_path))
                    .version(&ANDROID_WATCH)
                    .connect_handler(Box::new(TimedConnectionHandler {
                        timeout: Duration::from_secs(8),
                    }))
                    .qsign(Some(Arc::new(qsign)))
                    .session_store(FileSessionStore::boxed(session_path))
                    .build()
                    .await?;
                let client = Arc::new(client);
                tracing::info!("proc_qq client built; waiting for QR login in background");
                startup_tx
                    .send(Ok(Arc::new(ProcQqClient {
                        client: client.clone(),
                    }) as Arc<dyn BotClient>))
                    .ok();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    tracing::warn!(
                        qsign_endpoint = %diagnostic_qsign_endpoint,
                        "proc_qq login is still pending after 20 seconds; if no QR code is shown, check connectivity to QQ login servers, otherwise ensure qsign is running and scan confirmation can complete"
                    );
                });
                run_client(client).await
            }
            .await;

            if let Err(error) = result {
                tracing::error!(?error, "proc_qq client stopped");
                tx.send(Err(error)).ok();
            }
        });

        rx.recv()?
    }

    fn configured_qsign_endpoint(configured: &str) -> String {
        std::env::var("QRG_QSIGN_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| configured.trim().to_string())
    }

    fn configured_qsign_key(configured: &str) -> String {
        std::env::var("QRG_QSIGN_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| configured.trim().to_string())
    }

    fn configured_qsign_command(configured: Option<&str>) -> Option<String> {
        std::env::var("QRG_QSIGN_COMMAND")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                configured
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
    }

    async fn ensure_qsign_ready(
        endpoint: &str,
        command: Option<&str>,
        startup_timeout: Duration,
    ) -> anyhow::Result<()> {
        if ensure_qsign_reachable(endpoint).await.is_ok() {
            return Ok(());
        }

        let Some(command) = command else {
            anyhow::bail!(
                "请先启动 qsign 服务监听 {endpoint}，或设置 QRG_QSIGN_COMMAND / [bot].qsign_command 让程序自动启动；也可以用 QRG_QSIGN_ENDPOINT 和 QRG_QSIGN_KEY 指向已运行的 qsign"
            );
        };

        tracing::info!(%command, qsign_endpoint = %endpoint, "starting qsign service");
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .spawn()
            .context("failed to start qsign command")?;

        let deadline = tokio::time::Instant::now() + startup_timeout;
        loop {
            if ensure_qsign_reachable(endpoint).await.is_ok() {
                tracing::info!(qsign_endpoint = %endpoint, "qsign service is reachable");
                return Ok(());
            }
            if let Some(status) = child
                .try_wait()
                .context("failed to inspect qsign command status")?
            {
                anyhow::bail!("qsign command exited before {endpoint} became reachable: {status}");
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "qsign command started, but {endpoint} did not become reachable within {} seconds",
                    startup_timeout.as_secs()
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn ensure_qsign_reachable(endpoint: &str) -> anyhow::Result<()> {
        let url = Url::from_str(endpoint).context("invalid qsign endpoint")?;
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("qsign endpoint must include a host"))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("qsign endpoint must include a port"))?;
        let address = format!("{host}:{port}");
        tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(&address))
            .await
            .with_context(|| format!("timed out connecting to qsign service at {address}"))?
            .with_context(|| format!("could not connect to qsign service at {endpoint}"))?;
        Ok(())
    }

    #[async_trait]
    impl BotClient for ProcQqClient {
        async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
            let chain = MessageChain::new(Text::new(message.to_string()));
            match target {
                NotifyTarget::Group { id } => {
                    self.client.rq_client.send_group_message(*id, chain).await?;
                }
                NotifyTarget::Private { id } => {
                    self.client
                        .rq_client
                        .send_friend_message(*id, chain)
                        .await?;
                }
            }
            Ok(())
        }
    }
}

#[cfg(not(feature = "proc-qq"))]
mod proc_qq_client {
    use super::*;

    pub fn start(
        _device_path: &str,
        _session_path: &str,
        _qsign_endpoint: &str,
        _qsign_key: &str,
        _qsign_command: Option<&str>,
        _qsign_timeout_secs: u64,
    ) -> anyhow::Result<Arc<dyn BotClient>> {
        anyhow::bail!(
            "bot.type = proc_qq requires the `proc-qq` feature; run `cargo run` with default features enabled"
        )
    }
}
