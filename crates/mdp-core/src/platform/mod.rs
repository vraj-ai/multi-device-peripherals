//! The [`Platform`] trait: every OS touchpoint for one Peer.
//!
//! The Source captures physical input, the Sink injects it; capture already
//! filters out events the app injected so injected input never loops back.
//! [`FakePlatform`](fake::FakePlatform) is the test double used on any host.

pub mod fake;

pub use fake::FakePlatform;

/// Maximum clipboard payload in bytes (UTF-8 text only, capped at 1 MiB).
pub const CLIPBOARD_MAX_BYTES: usize = 1024 * 1024;

/// Bounding box of one Peer's monitors, in logical points.
///
/// A [`Desktop`] may span several screens. The cursor leaves the Source's
/// Desktop through the shared edge at each Crossing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Desktop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Desktop {
    /// Bounding box with origin `(x, y)` and the given size in logical points.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the point `(x, y)` lies inside this Desktop.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new(0.0, 0.0, 1920.0, 1080.0)
    }
}

/// Which mouse button an event refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// One input event. Keys travel as physical USB HID usage codes so the Sink
/// injects them under its own OS layout, as if the keyboard were plugged in
/// there.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key { usage_id: u16, pressed: bool },
    MouseMove { x: f64, y: f64 },
    MouseButton { button: MouseButton, pressed: bool },
    Scroll { dx: f64, dy: f64 },
}

/// OS permission state for capture/injection.
///
/// macOS requires the user to grant Accessibility + Input Monitoring once;
/// the app shows a guided screen instead of failing silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionStatus {
    Granted,
    Denied,
    #[default]
    NotDetermined,
}

/// Every failure mode of the [`Platform`] trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// Stubbed or unavailable on this Peer (T1 platform impls return this).
    Unsupported(&'static str),
    PermissionDenied(&'static str),
    Capture(String),
    Inject(String),
    Warp(String),
    Clipboard(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported on this peer: {what}"),
            Self::PermissionDenied(what) => write!(f, "permission denied: {what}"),
            Self::Capture(detail) => write!(f, "capture failed: {detail}"),
            Self::Inject(detail) => write!(f, "inject failed: {detail}"),
            Self::Warp(detail) => write!(f, "cursor warp failed: {detail}"),
            Self::Clipboard(detail) => write!(f, "clipboard failed: {detail}"),
        }
    }
}

impl std::error::Error for PlatformError {}

/// OS touchpoints for one Peer. OS impls live in the `mdp` crate behind
/// `cfg(target_os)` so `mdp-core` stays platform-free.
///
/// Capturing sides only ever see physical (non-injected) input: injected
/// events are filtered before they reach the capture channel (Windows
/// `LLMHF_INJECTED` / `LLKHF_INJECTED`; macOS event-source user-data tag).
pub trait Platform {
    /// Bounding box of this Peer's Desktop in logical points.
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError>;

    /// Start capturing physical input; injected events are already filtered.
    fn start_capture(&mut self) -> Result<std::sync::mpsc::Receiver<InputEvent>, PlatformError>;

    /// Suppress local input while this Peer is the Sink (Focus is remote).
    fn suppress_input(&mut self) -> Result<(), PlatformError>;

    /// Re-enable local input when Focus returns to this Peer.
    fn unsuppress_input(&mut self) -> Result<(), PlatformError>;

    /// Inject one event from the Source, as if its hardware were local.
    fn inject(&mut self, event: InputEvent) -> Result<(), PlatformError>;

    /// Move the local cursor to `(x, y)` in logical points.
    fn warp_cursor(&mut self, x: f64, y: f64) -> Result<(), PlatformError>;

    /// Read the clipboard (UTF-8 text only).
    fn clipboard_get(&self) -> Result<String, PlatformError>;

    /// Write the clipboard (UTF-8 text only, capped at [`CLIPBOARD_MAX_BYTES`]).
    fn clipboard_set(&mut self, text: &str) -> Result<(), PlatformError>;

    /// Check OS permissions for capture/injection.
    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError>;
}
