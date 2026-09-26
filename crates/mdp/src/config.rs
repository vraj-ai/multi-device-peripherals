//! `mdp` config: one TOML file in the OS config dir.
//!
//! Holds the static keypair, the pinned peer key + name, the Arrangement,
//! the manual `peer = "host:port"`, the listen `port`, and the feature
//! toggles. Writes are atomic (temp file + rename) and the config is created
//! on first run. A corrupt file yields a clear error and is never
//! overwritten: loading only reads.

// Unused until T9 wires config into `mdp run`/`pair`; drop this allow then.
#![allow(dead_code)]

use mdp_core::link::{PeerKey, StaticKeypair};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default TCP port the peer listens on and discovers.
pub const DEFAULT_PORT: u16 = 24800;
/// Config file name inside the OS config dir.
pub const CONFIG_FILE_NAME: &str = "config.toml";

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn enabled() -> bool {
    true
}

/// Which side of this Peer's Desktop the other Peer's Desktop sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Side {
    Left,
    #[default]
    Right,
    Top,
    Bottom,
}

/// Where the other Peer's Desktop sits relative to this one: side plus the
/// offset along the shared edge, in logical points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Arrangement {
    #[serde(default)]
    pub side: Side,
    #[serde(default)]
    pub offset: f64,
}

/// The whole `config.toml`. Key material is hex so the file stays readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// This Peer's static keypair: hex of the 64 serialized bytes.
    pub static_keypair_hex: String,
    /// Pinned peer static key: hex of 32 bytes. Unknown keys are refused.
    #[serde(default)]
    pub pinned_peer_key_hex: Option<String>,
    /// Human name of the pinned peer, shown by the UI and tray.
    #[serde(default)]
    pub pinned_peer_name: Option<String>,
    /// Manual `host:port` of the peer (plain hostnames work, e.g. Tailscale
    /// MagicDNS names). Tried before mDNS results.
    #[serde(default)]
    pub peer: Option<String>,
    /// TCP port this peer listens on.
    #[serde(default = "default_port")]
    pub port: u16,
    /// The peer Arrangement: side + offset of the other Desktop.
    #[serde(default)]
    pub arrangement: Arrangement,
    #[serde(default = "enabled")]
    pub share_input: bool,
    #[serde(default = "enabled")]
    pub share_clipboard: bool,
    #[serde(default)]
    pub start_at_login: bool,
}

/// Config failures. Corrupt files surface as [`ConfigError::Parse`] with the
/// path and the reason; loading never writes, so the file stays intact.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse {
        path: Option<PathBuf>,
        message: String,
    },
    BadHex {
        field: &'static str,
        message: String,
    },
    Key(mdp_core::link::LinkError),
    NoConfigDir,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "config I/O failed: {err}"),
            Self::Parse { path, message } => match path {
                Some(path) => write!(f, "cannot parse {}: {message}", path.display()),
                None => write!(f, "cannot parse config: {message}"),
            },
            Self::BadHex { field, message } => {
                write!(f, "config field `{field}` is not valid hex: {message}")
            }
            Self::Key(err) => write!(f, "config keypair is unusable: {err}"),
            Self::NoConfigDir => write!(f, "no OS config dir is available"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Key(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl Config {
    /// Fresh config with a new static keypair and default toggles.
    pub fn generate() -> Result<Self, ConfigError> {
        let keypair = StaticKeypair::generate().map_err(ConfigError::Key)?;
        Ok(Self {
            static_keypair_hex: hex::encode(keypair.to_bytes()),
            pinned_peer_key_hex: None,
            pinned_peer_name: None,
            peer: None,
            port: DEFAULT_PORT,
            arrangement: Arrangement::default(),
            share_input: true,
            share_clipboard: true,
            start_at_login: false,
        })
    }

    /// This Peer's static keypair, decoded from hex.
    pub fn static_keypair(&self) -> Result<StaticKeypair, ConfigError> {
        let bytes = hex::decode(&self.static_keypair_hex).map_err(|err| ConfigError::BadHex {
            field: "static_keypair_hex",
            message: err.to_string(),
        })?;
        StaticKeypair::from_bytes(&bytes).map_err(ConfigError::Key)
    }

    /// Replace the stored static keypair.
    pub fn set_static_keypair(&mut self, keypair: &StaticKeypair) {
        self.static_keypair_hex = hex::encode(keypair.to_bytes());
    }

    /// The pinned peer key, if Pairing completed.
    pub fn pinned_peer_key(&self) -> Result<Option<PeerKey>, ConfigError> {
        self.pinned_peer_key_hex
            .as_deref()
            .map(|hex_str| {
                let bytes = hex::decode(hex_str).map_err(|err| ConfigError::BadHex {
                    field: "pinned_peer_key_hex",
                    message: err.to_string(),
                })?;
                let len = bytes.len();
                bytes.try_into().map_err(|_| ConfigError::BadHex {
                    field: "pinned_peer_key_hex",
                    message: format!("expected 32 bytes, got {len}"),
                })
            })
            .transpose()
    }

    /// Pin the peer after confirming the Pairing code.
    pub fn set_pinned_peer(&mut self, name: &str, key: &PeerKey) {
        self.pinned_peer_name = Some(name.to_string());
        self.pinned_peer_key_hex = Some(hex::encode(key));
    }

    /// Forget the pinned peer (next connect pairs again).
    pub fn clear_pinned_peer(&mut self) {
        self.pinned_peer_name = None;
        self.pinned_peer_key_hex = None;
    }

    /// Render as TOML text.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string(self).map_err(|err| ConfigError::Parse {
            path: None,
            message: err.to_string(),
        })
    }

    /// Parse TOML text. Never touches the filesystem.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|err| ConfigError::Parse {
            path: None,
            message: err.to_string(),
        })
    }

    /// Load the config file. A corrupt file errors and is left untouched.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text).map_err(|err| match err {
            ConfigError::Parse { message, .. } => ConfigError::Parse {
                path: Some(path.to_path_buf()),
                message,
            },
            other => other,
        })
    }

    /// Save atomically: write a temp file in the same dir, then rename.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = self.to_toml()?;
        let mut tmp = path.as_os_str().to_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load the config, creating and saving a fresh one on first run.
    pub fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        if path.exists() {
            Self::load(path)
        } else {
            let config = Self::generate()?;
            config.save(path)?;
            Ok(config)
        }
    }

    /// `config.toml` inside the OS config dir (`%APPDATA%\mdp` on Windows,
    /// `~/Library/Application Support/mdp` on macOS).
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let dirs = directories::ProjectDirs::from("", "", "mdp").ok_or(ConfigError::NoConfigDir)?;
        Ok(dirs.config_dir().join(CONFIG_FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("mdp-t7-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(CONFIG_FILE_NAME)
    }

    fn scrub(path: &Path) {
        let _ = std::fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    #[test]
    fn config_round_trip() {
        let path = temp_path("round-trip");
        let mut config = Config::generate().expect("generate");
        let peer_keypair = StaticKeypair::generate().expect("peer keypair");
        config.set_pinned_peer("macbook", &peer_keypair.public_key());
        config.peer = Some("macbook-tail:24800".to_string());
        config.port = 24900;
        config.arrangement = Arrangement {
            side: Side::Left,
            offset: 120.0,
        };
        config.share_clipboard = false;
        config.start_at_login = true;
        let rotated = StaticKeypair::generate().expect("rotated keypair");
        config.set_static_keypair(&rotated);
        config.save(&path).expect("save");

        let loaded = Config::load(&path).expect("load");
        assert_eq!(loaded, config);
        assert_eq!(
            loaded
                .static_keypair()
                .expect("keypair decodes")
                .public_key(),
            rotated.public_key()
        );
        assert_eq!(
            loaded.pinned_peer_key().expect("pinned key"),
            Some(peer_keypair.public_key())
        );
        scrub(&path);
    }

    #[test]
    fn corrupt_file_errors_and_is_never_overwritten() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "port = [unclosed\0binary{gunk").expect("write junk");
        let before = std::fs::read(&path).expect("read back");
        let err = Config::load(&path).expect_err("corrupt must fail");
        assert!(
            matches!(err, ConfigError::Parse { .. }),
            "clear parse error, got: {err}"
        );
        assert!(
            err.to_string().contains("config.toml"),
            "error names the file"
        );
        assert_eq!(std::fs::read(&path).expect("read back"), before);
        scrub(&path);
    }

    #[test]
    fn wrong_typed_values_error() {
        let err = Config::from_toml("port = \"not-a-port\"").expect_err("bad port");
        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn missing_file_errors_on_load() {
        let path = temp_path("missing");
        scrub(&path);
        assert!(matches!(
            Config::load(&path.join("nope.toml")),
            Err(ConfigError::Io(_))
        ));
    }

    #[test]
    fn load_or_create_generates_once() {
        let path = temp_path("first-run");
        scrub(&path);
        let first = Config::load_or_create(&path).expect("create");
        assert!(path.exists(), "first run creates the file");
        let second = Config::load_or_create(&path).expect("load");
        assert_eq!(first, second, "second run loads the same config");
        scrub(&path);
    }

    #[test]
    fn minimal_toml_gets_sane_defaults() {
        let config = Config::from_toml("static_keypair_hex = \"ab\"").expect("minimal parses");
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.share_input);
        assert!(config.share_clipboard);
        assert!(!config.start_at_login);
        assert_eq!(config.peer, None);
        assert_eq!(config.pinned_peer_key_hex, None);
        assert_eq!(
            config.arrangement,
            Arrangement {
                side: Side::Right,
                offset: 0.0
            }
        );
    }

    #[test]
    fn bad_hex_key_material_is_rejected() {
        let mut config = Config::generate().expect("generate");
        config.static_keypair_hex = "not-hex!!".to_string();
        assert!(matches!(
            config.static_keypair(),
            Err(ConfigError::BadHex { .. })
        ));
        config.static_keypair_hex = hex::encode([0u8; 16]);
        assert!(matches!(config.static_keypair(), Err(ConfigError::Key(_))));
        config.pinned_peer_key_hex = Some("abc".to_string());
        assert!(matches!(
            config.pinned_peer_key(),
            Err(ConfigError::BadHex { .. })
        ));
        config.clear_pinned_peer();
        assert_eq!(config.pinned_peer_key().expect("cleared"), None);
    }

    #[test]
    fn default_path_points_at_config_file() {
        let path = Config::default_path().expect("config dir");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(CONFIG_FILE_NAME)
        );
    }
}
