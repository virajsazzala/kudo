mod protocol;
mod framing;

use clap::Parser;
use framing::{read_frame, write_frame};
use futures::StreamExt;
use protocol::{Message, Role, VERSION};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Child;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{error, info, warn};

const SERVICE_UUID: &str = "7f5c1e29-4a6b-4c9e-9b3a-2d8f0e6a1c55";

enum Ended {
    Completed(Option<Child>),
    Cancelled,
}

/// Receive a video over Bluetooth from a phone running the kudo app.
#[derive(Parser, Debug)]
#[command(name = "kudo", version, about)]
struct Cli {
    /// Phone Bluetooth MAC address, e.g. 3C:B0:ED:80:01:5B
    #[arg(short, long, value_name = "MAC", conflicts_with = "name")]
    mac: Option<String>,

    /// Phone Bluetooth name to discover, e.g. "Void"
    #[arg(short, long, value_name = "NAME")]
    name: Option<String>,

    /// Save the received video to this path (in addition to or instead of playing)
    #[arg(short, long, value_name = "PATH")]
    save: Option<PathBuf>,

    /// Do not launch a player; just receive (and save, if --save given)
    #[arg(long)]
    no_play: bool,

    /// Media player binary to stream into
    #[arg(long, default_value = "mpv")]
    player: String,

    /// Discovery/connection timeout in seconds
    #[arg(short, long, default_value_t = 60)]
    timeout: u64,

    /// Increase log verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

type E = Box<dyn std::error::Error + Send + Sync>;

// Removes the FIFO on drop, covering normal return, error, and unwinding.
struct FifoGuard(PathBuf);
impl Drop for FifoGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct RxConfig {
    play: bool,
    player: String,
    save: Option<PathBuf>,
    fifo_path: PathBuf,
}

async fn close_after_bye<S>(stream: &mut S) -> Result<(), E>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_frame(stream, &Message::Bye).await?;
    loop {
        match read_frame(stream).await? {
            Some(Message::Bye) | None => return Ok(()),
            Some(Message::Credit { .. }) | Some(Message::Complete { .. }) => continue,
            Some(Message::Error { code, msg }) => return Err(format!("peer ERROR {code}: {msg}").into()),
            Some(_) => return Ok(()),
        }
    }
}

async fn run_receiver<S>(stream: &mut S, cfg: &RxConfig) -> Result<Ended, E>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_frame(stream, &Message::Hello { version: VERSION, role: Role::Receiver, name: "thinkpad".into() }).await?;
    match read_frame(stream).await? {
        Some(Message::Hello { role, name, .. }) if role == Role::Sender => info!("connected to sender {name:?}"),
        Some(Message::Hello { role, .. }) => {
            write_frame(stream, &Message::Error { code: 3, msg: format!("peer role {role:?}") }).await?;
            return Err("role conflict: peer is not a sender".into());
        }
        Some(Message::Cancel { .. }) => return Ok(Ended::Cancelled),
        Some(Message::Error { code, msg }) => return Err(format!("peer ERROR {code}: {msg}").into()),
        None => return Err("peer closed before HELLO".into()),
        Some(other) => {
            write_frame(stream, &Message::Error { code: 4, msg: format!("expected HELLO, got {other:?}") }).await?;
            return Err(format!("protocol violation: expected HELLO, got {other:?}").into());
        }
    }

    let (file_id, total_size, offer_sha, name) = match read_frame(stream).await? {
        Some(Message::Offer { file_id, total_size, sha256, name, .. }) => (file_id, total_size, sha256, name),
        Some(Message::Cancel { .. }) => return Ok(Ended::Cancelled),
        Some(Message::Error { code, msg }) => return Err(format!("peer ERROR {code}: {msg}").into()),
        None => return Err("peer closed before OFFER".into()),
        Some(other) => {
            write_frame(stream, &Message::Error { code: 4, msg: format!("expected OFFER, got {other:?}") }).await?;
            return Err(format!("protocol violation: expected OFFER, got {other:?}").into());
        }
    };
    info!("offer: {name} ({total_size} bytes)");

    write_frame(stream, &Message::Accept { file_id, quality_id: 0, credit: 64 }).await?;

    // Build sinks after ACCEPT. The player must be reading before we open the
    // FIFO write end (the open blocks until a reader attaches).
    let mut mpv: Option<Child> = None;
    let mut fifo_sink: Option<tokio::fs::File> = None;
    if cfg.play {
        let _ = std::fs::remove_file(&cfg.fifo_path);
        std::process::Command::new("mkfifo").arg(&cfg.fifo_path).status()
            .map_err(|e| format!("mkfifo failed: {e}"))?;
        mpv = Some(
            std::process::Command::new(&cfg.player).arg(&cfg.fifo_path).spawn()
                .map_err(|e| format!("could not launch player '{}': {e}", cfg.player))?,
        );
        fifo_sink = Some(tokio::fs::OpenOptions::new().write(true).open(&cfg.fifo_path).await?);
        info!("streaming into {}", cfg.player);
    }
    let mut file_sink = match &cfg.save {
        Some(p) => {
            info!("saving to {}", p.display());
            Some(tokio::fs::File::create(p).await.map_err(|e| format!("cannot create {}: {e}", p.display()))?)
        }
        None => None,
    };

    let transfer: Result<Ended, E> = async {
        let mut hasher = Sha256::new();
        let mut received: u64 = 0;
        let mut since_grant: u32 = 0;
        const GRANT_BATCH: u32 = 32;

        while received < total_size {
            match read_frame(stream).await? {
                Some(Message::Chunk { data, .. }) => {
                    hasher.update(&data);
                    if let Some(f) = fifo_sink.as_mut() { f.write_all(&data).await?; }
                    if let Some(f) = file_sink.as_mut() { f.write_all(&data).await?; }
                    received += data.len() as u64;
                    since_grant += 1;
                    if since_grant >= GRANT_BATCH && received < total_size {
                        write_frame(stream, &Message::Credit { file_id, credit: since_grant }).await?;
                        since_grant = 0;
                    }
                }
                Some(Message::Cancel { .. }) => return Ok(Ended::Cancelled),
                Some(Message::Error { code, msg }) => return Err(format!("peer ERROR {code}: {msg}").into()),
                None => return Err(format!("peer gone mid-transfer at {received}/{total_size}").into()),
                Some(other) => {
                    write_frame(stream, &Message::Error { code: 4, msg: format!("expected CHUNK, got {other:?}") }).await?;
                    return Err(format!("protocol violation: expected CHUNK, got {other:?}").into());
                }
            }
        }
        if let Some(f) = fifo_sink.as_mut() { f.flush().await?; }
        if let Some(f) = file_sink.as_mut() { f.flush().await?; }
        drop(fifo_sink); // close FIFO write end so the player sees the true EOF

        let status: u8 = if hasher.finalize().as_slice() == offer_sha.as_slice() { 0 } else { 1 };
        if status == 0 { info!("hash verified"); } else { warn!("hash MISMATCH"); }
        write_frame(stream, &Message::Complete { file_id, status }).await?;
        close_after_bye(stream).await?;
        Ok(Ended::Completed(None))
    }
    .await;

    match transfer {
        Ok(Ended::Completed(_)) => Ok(Ended::Completed(mpv)),
        Ok(Ended::Cancelled) => {
            if let Some(m) = mpv.as_mut() { let _ = m.kill(); }
            Ok(Ended::Cancelled)
        }
        Err(e) => {
            if let Some(m) = mpv.as_mut() { let _ = m.kill(); }
            Err(e)
        }
    }
}

async fn resolve_device(adapter: &bluer::Adapter, cli: &Cli) -> Result<bluer::Device, E> {
    // By MAC: discover until BlueZ has the object, then return it.
    if let Some(mac) = &cli.mac {
        let addr: bluer::Address = mac.parse().map_err(|_| format!("invalid MAC: {mac}"))?;
        info!("discovering {mac}...");
        let mut events = adapter.discover_devices().await?;
        loop {
            if let Ok(dev) = adapter.device(addr) {
                if dev.rssi().await.is_ok() {
                    return Ok(dev);
                }
            }
            match events.next().await {
                Some(bluer::AdapterEvent::DeviceAdded(a)) if a == addr => return Ok(adapter.device(addr)?),
                Some(_) => continue,
                None => return Err("discovery ended unexpectedly".into()),
            }
        }
    }

    // By name: scan and match the advertised name.
    let want = cli.name.as_ref().expect("clap guarantees mac or name");
    info!("discovering device named {want:?}...");
    let mut events = adapter.discover_devices().await?;
    while let Some(ev) = events.next().await {
        if let bluer::AdapterEvent::DeviceAdded(a) = ev {
            if let Ok(dev) = adapter.device(a) {
                if let Ok(Some(n)) = dev.name().await {
                    if n == *want {
                        info!("found {want:?} at {a}");
                        return Ok(dev);
                    }
                }
            }
        }
    }
    Err(format!("no device named {want:?} found").into())
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let level = match cli.verbose {
        0 => "kudo=info",
        1 => "kudo=debug",
        _ => "kudo=trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(level))
        .with_target(false)
        .without_time()
        .init();

    if cli.mac.is_none() && cli.name.is_none() {
        error!("specify the phone with --mac <MAC> or --name <NAME>");
        return std::process::ExitCode::from(2);
    }
    if cli.no_play && cli.save.is_none() {
        warn!("--no-play with no --save: the video will be received and verified but not kept");
    }

    match run(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), E> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let uuid: bluer::Uuid = SERVICE_UUID.parse().unwrap();
    let profile = bluer::rfcomm::Profile {
        uuid,
        name: Some("kudo-client".into()),
        role: Some(bluer::rfcomm::Role::Client),
        require_authentication: Some(false),
        require_authorization: Some(false),
        auto_connect: Some(true),
        ..Default::default()
    };
    let mut handle = session.register_profile(profile).await?;

    // Bound discovery + connection by the timeout.
    let device = tokio::time::timeout(Duration::from_secs(cli.timeout), resolve_device(&adapter, &cli))
        .await
        .map_err(|_| format!("timed out after {}s finding the phone", cli.timeout))??;

    info!("connecting...");
    let connect = async {
        loop {
            match device.connect_profile(&uuid).await {
                Ok(()) => break,
                Err(e) => {
                    tracing::debug!("connect retry: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    };
    let mut stream = tokio::time::timeout(Duration::from_secs(cli.timeout), async {
        tokio::select! {
            req = handle.next() => req.ok_or_else(|| E::from("profile handle closed"))?.accept().map_err(E::from),
            _ = connect => unreachable!(),
        }
    })
    .await
    .map_err(|_| format!("timed out after {}s connecting", cli.timeout))??;
    info!("connected");

    let mut fifo = std::env::temp_dir();
    fifo.push(format!("kudo-{}.fifo", std::process::id()));
    let _guard = FifoGuard(fifo.clone()); // removed on any exit from this scope

    let cfg = RxConfig {
        play: !cli.no_play,
        player: cli.player.clone(),
        save: cli.save.clone(),
        fifo_path: fifo,
    };

    // Race the transfer against Ctrl-C so an interrupt cleans up instead of
    // leaving mpv and the FIFO behind.
    let result = tokio::select! {
        r = run_receiver(&mut stream, &cfg) => r,
        _ = tokio::signal::ctrl_c() => {
            warn!("interrupted");
            Err("interrupted by user".into())
        }
    };

    match result {
        Ok(Ended::Completed(Some(mut mpv))) => {
            // Wait for the player to finish reading the streamed video.
            tokio::task::spawn_blocking(move || mpv.wait()).await??;
            info!("done");
            Ok(())
        }
        Ok(Ended::Completed(None)) => {
            info!("done");
            Ok(())
        }
        Ok(Ended::Cancelled) => {
            info!("sender cancelled the transfer");
            Ok(())
        }
        Err(e) => Err(e),
    }
    // _guard drops here, removing the FIFO, on both the Ok and Err paths.
}
