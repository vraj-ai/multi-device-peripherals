//! Peer discovery: mDNS `_mdp._tcp` plus the manual peer host.
//!
//! Each peer advertises its instance with the static-key fingerprint in TXT;
//! browsing returns the candidates found on the LAN. A configured manual
//! host is always tried first (it also covers plain hostnames, e.g.
//! Tailscale MagicDNS names); mDNS results are the fallback.

// Unused until T9 wires discovery into `mdp run`/`pair`; drop this allow then.
#![allow(dead_code)]

use mdp_core::link::PeerKey;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// The mDNS service type peers advertise and browse.
pub const SERVICE_TYPE: &str = "_mdp._tcp.local.";
/// Suffix stripped from a fullname to recover the instance name.
const SERVICE_SUFFIX: &str = "._mdp._tcp.local.";
/// TXT key carrying the hex static-key fingerprint.
pub const TXT_PUBLIC_KEY: &str = "pk";

/// One peer found on the LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCandidate {
    /// mDNS instance name.
    pub instance: String,
    /// Resolved addresses (v4 and v6).
    pub addrs: Vec<IpAddr>,
    /// Advertised TCP port.
    pub port: u16,
    /// Static key from TXT, when present and well-formed.
    pub public_key: Option<PeerKey>,
}

/// Where to dial: the manual host wins over anything discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerTarget {
    Manual(String),
    Discovered(PeerCandidate),
}

/// Pick the peer to dial: a configured manual host first, then the first
/// mDNS candidate, else nothing.
pub fn select_peer_target(
    manual: Option<&str>,
    discovered: &[PeerCandidate],
) -> Option<PeerTarget> {
    if let Some(host) = manual {
        return Some(PeerTarget::Manual(host.to_string()));
    }
    discovered.first().cloned().map(PeerTarget::Discovered)
}

/// Discovery failures.
#[derive(Debug)]
pub enum DiscoveryError {
    Mdns(mdns_sd::Error),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mdns(err) => write!(f, "mDNS failed: {err}"),
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mdns(err) => Some(err),
        }
    }
}

impl From<mdns_sd::Error> for DiscoveryError {
    fn from(err: mdns_sd::Error) -> Self {
        Self::Mdns(err)
    }
}

/// Recover the instance name from a fullname like `peer._mdp._tcp.local.`.
fn instance_name(fullname: &str) -> Option<&str> {
    fullname.strip_suffix(SERVICE_SUFFIX)
}

fn parse_peer_key(hex_str: &str) -> Option<PeerKey> {
    hex::decode(hex_str).ok()?.try_into().ok()
}

fn upsert_candidate(candidates: &mut Vec<PeerCandidate>, info: &mdns_sd::ResolvedService) {
    let Some(instance) = instance_name(info.get_fullname()) else {
        return;
    };
    let addrs: Vec<IpAddr> = info
        .get_addresses()
        .iter()
        .map(mdns_sd::ScopedIp::to_ip_addr)
        .collect();
    if addrs.is_empty() {
        return;
    }
    let public_key = info
        .get_property_val_str(TXT_PUBLIC_KEY)
        .and_then(parse_peer_key);
    match candidates
        .iter_mut()
        .find(|known| known.instance == instance)
    {
        Some(known) => {
            for addr in addrs {
                if !known.addrs.contains(&addr) {
                    known.addrs.push(addr);
                }
            }
            if known.public_key.is_none() {
                known.public_key = public_key;
            }
        }
        None => candidates.push(PeerCandidate {
            instance: instance.to_string(),
            addrs,
            port: info.get_port(),
            public_key,
        }),
    }
}

/// mDNS discovery for `_mdp._tcp`. Owns one daemon per instance.
pub struct Discovery {
    daemon: mdns_sd::ServiceDaemon,
}

impl Discovery {
    /// Start a discovery daemon.
    pub fn new() -> Result<Self, DiscoveryError> {
        Ok(Self {
            daemon: mdns_sd::ServiceDaemon::new()?,
        })
    }

    /// Advertise this peer: `instance` (unique per peer), mDNS `host`,
    /// `ip`, TCP `port`, and the static-key fingerprint in TXT. Returns the
    /// fullname for [`Discovery::unadvertise`].
    pub fn advertise(
        &self,
        instance: &str,
        host: &str,
        ip: &str,
        port: u16,
        public_key: &PeerKey,
    ) -> Result<String, DiscoveryError> {
        let fingerprint = hex::encode(public_key);
        let properties = vec![mdns_sd::TxtProperty::from(&(
            TXT_PUBLIC_KEY,
            fingerprint.as_str(),
        ))];
        let info = mdns_sd::ServiceInfo::new(SERVICE_TYPE, instance, host, ip, port, properties)?;
        let fullname = info.get_fullname().to_string();
        self.daemon.register(info)?;
        Ok(fullname)
    }

    /// Stop advertising `fullname` (best effort: the daemon dies with us).
    pub fn unadvertise(&self, fullname: &str) -> Result<(), DiscoveryError> {
        let receiver = self.daemon.unregister(fullname)?;
        let _ = receiver.recv_timeout(Duration::from_secs(2));
        Ok(())
    }

    /// Browse for peers until `timeout`, returning every candidate seen.
    pub fn browse(&self, timeout: Duration) -> Result<Vec<PeerCandidate>, DiscoveryError> {
        let receiver = self.daemon.browse(SERVICE_TYPE)?;
        let deadline = Instant::now() + timeout;
        let mut candidates = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                    upsert_candidate(&mut candidates, &info);
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        self.daemon.stop_browse(SERVICE_TYPE)?;
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdp_core::link::StaticKeypair;

    fn candidate(instance: &str) -> PeerCandidate {
        PeerCandidate {
            instance: instance.to_string(),
            addrs: vec![IpAddr::from([127, 0, 0, 1])],
            port: crate::config::DEFAULT_PORT,
            public_key: None,
        }
    }

    #[test]
    fn manual_host_wins_over_mdns() {
        let found = vec![candidate("macbook"), candidate("thinkpad")];
        assert_eq!(
            select_peer_target(Some("macbook-tail:24800"), &found),
            Some(PeerTarget::Manual("macbook-tail:24800".to_string()))
        );
        assert_eq!(
            select_peer_target(Some("macbook-tail:24800"), &[]),
            Some(PeerTarget::Manual("macbook-tail:24800".to_string()))
        );
        assert_eq!(
            select_peer_target(None, &found),
            Some(PeerTarget::Discovered(found[0].clone()))
        );
        assert_eq!(select_peer_target(None, &[]), None);
    }

    #[test]
    fn instance_name_strips_service_suffix() {
        assert_eq!(instance_name("peer._mdp._tcp.local."), Some("peer"));
        assert_eq!(instance_name("something-else._tcp.local."), None);
    }

    #[test]
    fn advertise_and_browse_finds_self() {
        let discovery = Discovery::new().expect("daemon starts");
        let keypair = StaticKeypair::generate().expect("keypair");
        let public_key = keypair.public_key();
        let instance = format!("mdp-t7-test-{}", std::process::id());
        let fullname = discovery
            .advertise(
                &instance,
                "mdp-test.local.",
                "127.0.0.1",
                24800,
                &public_key,
            )
            .expect("advertise");
        let found = discovery.browse(Duration::from_secs(5)).expect("browse");
        discovery.unadvertise(&fullname).ok();

        let Some(me) = found.iter().find(|known| known.instance == instance) else {
            println!("SKIPPED: mDNS multicast is unavailable in this environment");
            return;
        };
        assert_eq!(me.port, 24800);
        assert!(
            me.addrs.contains(&IpAddr::from([127, 0, 0, 1])),
            "self resolves, got: {:?}",
            me.addrs
        );
        assert_eq!(me.public_key, Some(public_key));
    }
}
