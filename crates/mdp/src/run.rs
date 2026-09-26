//! `mdp run`: the headless peer loop.
//!
//! Loads (or creates) the config, listens on the configured port, resolves
//! the peer (manual host first, mDNS otherwise), races accept against dial,
//! pairs on the CLI for unknown keys, pins, exchanges Hello + Arrangement,
//! and drives the Crossing engine against the native platform until the
//! Link drops — then backs off and reconnects. The `confirm` callback is a
//! parameter (not hardcoded CLI) so `mdp ui` can pass `ui::pairing::bridge`
//! instead; both satisfy `Fn(&str) -> bool + Send + Sync`.

use crate::config::{Config, Side as ConfigSide};
use crate::discovery::{select_peer_target, Discovery, PeerCandidate, PeerTarget};
use crate::platform::Native;
use mdp_core::crossing::{
    Arrangement as CrossingArrangement, CrossingEngine, Side as CrossingSide,
};
use mdp_core::link::{Link, LinkError, PeerKey, StaticKeypair};
use mdp_core::peer::{exchange_hello, mirror_wire, side_to_wire, Peer, PeerError};
use mdp_core::platform::{Desktop, Platform};
use mdp_core::proto::ArrangementSide;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use tokio::net::TcpListener;

const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BROWSE_TIME: Duration = Duration::from_secs(3);

/// Config side -> engine side (both cover all four sides).
fn config_side_to_crossing(side: ConfigSide) -> CrossingSide {
    match side {
        ConfigSide::Left => CrossingSide::Left,
        ConfigSide::Right => CrossingSide::Right,
        ConfigSide::Top => CrossingSide::Top,
        ConfigSide::Bottom => CrossingSide::Bottom,
    }
}

/// Config side -> wire side (both cover all four sides).
fn config_side_to_wire(side: ConfigSide) -> ArrangementSide {
    side_to_wire(config_side_to_crossing(side))
}

/// Who dials: the higher static key dials, the lower only listens, so mutual
/// discovery yields exactly one Link. Missing fingerprints fall back to dial.
fn should_dial(own: &PeerKey, peer: &PeerCandidate) -> bool {
    peer.public_key.is_none_or(|key| *own > key)
}

/// Best-effort local IP for mDNS advertisements: a UDP "connect" sends no
/// packets, it just reveals the outbound interface address.
fn outbound_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// CLI Pairing: print the code, ask y/n on stdin. Captures nothing but an
/// owned description, so it fits the Link confirm path.
fn cli_confirm(peer_desc: String) -> impl Fn(&str) -> bool + Send + Sync {
    move |code| {
        println!("Pairing code for {peer_desc}: {code}");
        loop {
            print!("Pair with this peer? [y/n]: ");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_err() {
                return false;
            }
            match line.trim().to_lowercase().as_str() {
                "y" | "yes" => return true,
                "n" | "no" => return false,
                _ => continue,
            }
        }
    }
}

fn target_name(target: &PeerTarget) -> String {
    match target {
        PeerTarget::Manual(host) => host.clone(),
        PeerTarget::Discovered(candidate) => candidate.instance.clone(),
    }
}

/// Resolve who to dial: the manual host wins; otherwise browse mDNS. The
/// description names the peer for the Pairing prompt. Losing the static-key
/// election returns no dial target: this side only listens.
fn resolve_target(
    config: &Config,
    discovery: Option<&Discovery>,
    keypair: &StaticKeypair,
) -> Result<(Option<PeerTarget>, String), String> {
    if let Some(manual) = config.peer.clone() {
        return Ok((Some(PeerTarget::Manual(manual.clone())), manual));
    }
    let discovery = discovery.ok_or("no manual peer configured and no discovery".to_string())?;
    let found = discovery
        .browse(BROWSE_TIME)
        .map_err(|err| format!("browse: {err}"))?;
    match select_peer_target(None, &found) {
        Some(PeerTarget::Discovered(candidate)) => {
            let desc = match candidate.addrs.first() {
                Some(addr) => format!("{} ({addr})", candidate.instance),
                None => candidate.instance.clone(),
            };
            if should_dial(&keypair.public_key(), &candidate) {
                Ok((Some(PeerTarget::Discovered(candidate)), desc))
            } else {
                Ok((None, desc))
            }
        }
        _ => Err("no peer discovered".to_string()),
    }
}

async fn dial_target<F>(
    target: &PeerTarget,
    keypair: &StaticKeypair,
    pinned: &[PeerKey],
    confirm: F,
) -> Result<(Link, PeerKey), PeerError>
where
    F: Fn(&str) -> bool + Send,
{
    match target {
        PeerTarget::Manual(host) => Link::connect(host.as_str(), keypair, pinned, confirm)
            .await
            .map_err(PeerError::from),
        PeerTarget::Discovered(candidate) => {
            let addr = candidate
                .addrs
                .first()
                .map(|ip| SocketAddr::new(*ip, candidate.port))
                .ok_or_else(|| {
                    PeerError::Handshake("discovered peer has no address".to_string())
                })?;
            Link::connect(addr, keypair, pinned, confirm)
                .await
                .map_err(PeerError::from)
        }
    }
}

/// Establish one Link: race inbound accept against the dial target (or only
/// listen after losing the election). Returns the Link, the peer key for
/// pinning, and the dial name when this side dialed.
async fn establish<F>(
    listener: &TcpListener,
    dial: Option<&PeerTarget>,
    keypair: &StaticKeypair,
    pinned: &[PeerKey],
    confirm: &F,
) -> Result<(Link, PeerKey, Option<String>), PeerError>
where
    F: Fn(&str) -> bool + Send + Sync,
{
    let accept = async {
        let (stream, _) = listener.accept().await.map_err(LinkError::from)?;
        Link::accept(stream, keypair, pinned, confirm).await
    };
    match dial {
        Some(target) => {
            let dialing = dial_target(target, keypair, pinned, confirm);
            tokio::select! {
                inbound = accept => {
                    let (link, key) = inbound?;
                    Ok((link, key, None))
                }
                outbound = dialing => {
                    let (link, key) = outbound?;
                    Ok((link, key, Some(target_name(target))))
                }
            }
        }
        None => {
            let (link, key) = accept.await?;
            Ok((link, key, None))
        }
    }
}

/// One connect-drive cycle: establish, pin, shake hands, drive till the Link
/// drops. A fresh platform per round keeps capture reinstallable.
async fn round(
    config: &mut Config,
    config_path: &Path,
    keypair: &StaticKeypair,
    desktop: &Desktop,
    listener: &TcpListener,
    discovery: Option<&Discovery>,
) -> Result<(), String> {
    let (dial, peer_desc) = resolve_target(config, discovery, keypair)?;
    let pinned = config
        .pinned_peer_key()
        .map_err(|err| format!("pinned key: {err}"))?;
    let pinned_list: &[PeerKey] = match &pinned {
        Some(key) => std::slice::from_ref(key),
        None => &[],
    };
    let confirm = cli_confirm(peer_desc);
    let (mut link, peer_key, dial_name) =
        establish(listener, dial.as_ref(), keypair, pinned_list, &confirm)
            .await
            .map_err(|err| format!("link: {err}"))?;
    if Some(peer_key) != pinned {
        let name = dial_name
            .as_deref()
            .or(config.pinned_peer_name.as_deref())
            .unwrap_or("peer")
            .to_string();
        config.set_pinned_peer(&name, &peer_key);
        config
            .save(config_path)
            .map_err(|err| format!("save config: {err}"))?;
        println!("mdp run: pinned peer {name}");
    }
    let my_side = config_side_to_wire(config.arrangement.side);
    let (peer_desktop, (peer_side, peer_offset)) =
        exchange_hello(&mut link, desktop, my_side, config.arrangement.offset)
            .await
            .map_err(|err| format!("hello: {err}"))?;
    let (expect_side, expect_offset) = mirror_wire(my_side, config.arrangement.offset);
    if (peer_side, peer_offset) != (expect_side, expect_offset) {
        eprintln!(
            "mdp run: warning: peer arrangement {peer_side:?} offset {peer_offset} \
             is not the mirror; crossings may misbehave"
        );
    }
    let engine = CrossingEngine::new(
        *desktop,
        (peer_desktop.width, peer_desktop.height),
        CrossingArrangement {
            side: config_side_to_crossing(config.arrangement.side),
            offset: config.arrangement.offset,
        },
        keypair.public_key(),
        peer_key,
    );
    let mut platform = Native::new();
    let capture = platform
        .start_capture()
        .map_err(|err| format!("capture: {err}"))?;
    let mut peer = Peer::new(platform, link, capture, engine, *desktop);
    println!("mdp run: link up; driving");
    Err(format!("link lost: {}", peer.drive().await))
}

/// `mdp run`: real headless peer on the native platform. Loops forever,
/// backing off between rounds; only fatal setup errors exit.
pub fn run() -> ExitCode {
    match run_blocking() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mdp run: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run_blocking() -> Result<(), String> {
    let path: PathBuf = Config::default_path().map_err(|err| format!("config path: {err}"))?;
    let config = Config::load_or_create(&path).map_err(|err| format!("config: {err}"))?;
    println!("mdp run: config {}", path.display());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("runtime: {err}"))?;
    runtime.block_on(run_async(config, path))
}

async fn run_async(mut config: Config, path: PathBuf) -> Result<(), String> {
    let keypair = config
        .static_keypair()
        .map_err(|err| format!("keypair: {err}"))?;
    let desktop = Native::new()
        .desktop_bounds()
        .map_err(|err| format!("desktop: {err}"))?;
    println!(
        "mdp run: local Desktop x={} y={} w={} h={}",
        desktop.x, desktop.y, desktop.width, desktop.height
    );
    let listener = TcpListener::bind(("0.0.0.0", config.port))
        .await
        .map_err(|err| {
            format!(
                "cannot listen on port {} (another peer running?): {err}",
                config.port
            )
        })?;
    let discovery = match Discovery::new() {
        Ok(discovery) => Some(discovery),
        Err(err) => {
            eprintln!("mdp run: warning: discovery unavailable ({err}); manual peer only");
            None
        }
    };
    if let (Some(discovery), Some(ip)) = (discovery.as_ref(), outbound_ip()) {
        let instance = format!("mdp-{}", hex::encode(&keypair.public_key()[..4]));
        let host = format!("{instance}.local.");
        match discovery.advertise(
            &instance,
            &host,
            &ip.to_string(),
            config.port,
            &keypair.public_key(),
        ) {
            Ok(_) => println!("mdp run: advertising {instance} on {ip}"),
            Err(err) => eprintln!("mdp run: warning: advertise failed ({err}); browse-only"),
        }
    }
    let mut backoff = INITIAL_BACKOFF;
    loop {
        match round(
            &mut config,
            &path,
            &keypair,
            &desktop,
            &listener,
            discovery.as_ref(),
        )
        .await
        {
            Ok(()) => backoff = INITIAL_BACKOFF,
            Err(err) => {
                eprintln!("mdp run: {err}; retrying in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Side as ConfigSide;
    use mdp_core::crossing::Side as CrossingSide;
    use mdp_core::proto::ArrangementSide;

    #[test]
    fn config_sides_convert_all_four_ways() {
        for (config, crossing, wire) in [
            (ConfigSide::Left, CrossingSide::Left, ArrangementSide::Left),
            (
                ConfigSide::Right,
                CrossingSide::Right,
                ArrangementSide::Right,
            ),
            (ConfigSide::Top, CrossingSide::Top, ArrangementSide::Top),
            (
                ConfigSide::Bottom,
                CrossingSide::Bottom,
                ArrangementSide::Bottom,
            ),
        ] {
            assert_eq!(config_side_to_crossing(config), crossing);
            assert_eq!(config_side_to_wire(config), wire);
        }
    }

    #[test]
    fn higher_key_dials_lower_listens() {
        let low = PeerCandidate {
            instance: "low".to_string(),
            addrs: vec![],
            port: 24800,
            public_key: Some([0x0A; 32]),
        };
        let high = PeerCandidate {
            instance: "high".to_string(),
            addrs: vec![],
            port: 24800,
            public_key: Some([0x0B; 32]),
        };
        assert!(should_dial(&[0x0B; 32], &low));
        assert!(!should_dial(&[0x0A; 32], &high));
        let unknown = PeerCandidate {
            instance: "unknown".to_string(),
            addrs: vec![],
            port: 24800,
            public_key: None,
        };
        assert!(should_dial(&[0x0A; 32], &unknown));
    }

    #[test]
    fn bridge_confirm_fits_the_link_confirm_path() {
        fn assert_confirm<F: Fn(&str) -> bool + Send + Sync>(_: F) {}
        let (confirm, _requests) = crate::ui::pairing::bridge("peer".to_string());
        assert_confirm(confirm);
    }

    #[test]
    fn outbound_ip_probe_never_panics() {
        let _ = outbound_ip();
    }
}
