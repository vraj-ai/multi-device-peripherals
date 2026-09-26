//! Windows implementation of [`Platform`](mdp_core::Platform) (T1 stub).
//!
//! The full implementation will capture with low-level hooks
//! (`WH_KEYBOARD_LL` / `WH_MOUSE_LL`), ignoring events flagged
//! `LLMHF_INJECTED` / `LLKHF_INJECTED`, and inject with `SendInput`.
//! Every method currently reports [`PlatformError::Unsupported`](mdp_core::PlatformError::Unsupported).

use mdp_core::{Desktop, InputEvent, PermissionStatus, Platform, PlatformError};
use std::sync::mpsc::Receiver;

/// Windows Peer platform.
pub struct WindowsPlatform;

impl WindowsPlatform {
    /// Create the Windows platform handle.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WindowsPlatform {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        Err(PlatformError::Unsupported("desktop_bounds"))
    }

    fn start_capture(&mut self) -> Result<Receiver<InputEvent>, PlatformError> {
        Err(PlatformError::Unsupported("start_capture"))
    }

    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("suppress_input"))
    }

    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("unsuppress_input"))
    }

    fn inject(&mut self, _event: InputEvent) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("inject"))
    }

    fn warp_cursor(&mut self, _x: f64, _y: f64) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("warp_cursor"))
    }

    fn clipboard_get(&self) -> Result<String, PlatformError> {
        Err(PlatformError::Unsupported("clipboard_get"))
    }

    fn clipboard_set(&mut self, _text: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("clipboard_set"))
    }

    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        Err(PlatformError::Unsupported("check_permissions"))
    }
}
