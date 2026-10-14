//! Peer host that is not Windows or macOS.
//!
//! Linux is a non-goal. This type exists so the binary and `cargo test` still
//! build on agent machines: every call fails closed. It never captures or
//! injects input.

use mdp_core::platform::{Desktop, InputEvent, PermissionStatus, Platform, PlatformError};

/// Platform stand-in for an unsupported host.
pub struct UnsupportedPlatform;

impl UnsupportedPlatform {
    /// Construct the stand-in. It has no OS resources to acquire.
    pub fn new() -> Self {
        Self
    }
}

fn unsupported(what: &'static str) -> PlatformError {
    PlatformError::Unsupported(what)
}

impl Platform for UnsupportedPlatform {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        Err(unsupported("desktop bounds"))
    }

    fn start_capture(&mut self) -> Result<std::sync::mpsc::Receiver<InputEvent>, PlatformError> {
        Err(unsupported("input capture"))
    }

    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        Err(unsupported("suppress input"))
    }

    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        Err(unsupported("unsuppress input"))
    }

    fn inject(&mut self, _event: InputEvent) -> Result<(), PlatformError> {
        Err(unsupported("inject input"))
    }

    fn warp_cursor(&mut self, _x: f64, _y: f64) -> Result<(), PlatformError> {
        Err(unsupported("warp cursor"))
    }

    fn clipboard_get(&self) -> Result<String, PlatformError> {
        Err(unsupported("clipboard read"))
    }

    fn clipboard_set(&mut self, _text: &str) -> Result<(), PlatformError> {
        Err(unsupported("clipboard write"))
    }

    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        Err(unsupported("permissions"))
    }
}
