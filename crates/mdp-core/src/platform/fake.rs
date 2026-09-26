//! [`FakePlatform`]: the [`Platform`] test double.
//!
//! Records every injected event and lets tests feed physical events into the
//! capture channel, so platform-free logic is unit-testable on any host.

use super::{Desktop, InputEvent, PermissionStatus, Platform, PlatformError, CLIPBOARD_MAX_BYTES};
use std::sync::mpsc::{self, Receiver, Sender};

/// Test double for [`Platform`]: records injections, replays physical input.
#[derive(Debug)]
pub struct FakePlatform {
    desktop: Desktop,
    capture_tx: Option<Sender<InputEvent>>,
    injected: Vec<InputEvent>,
    suppressed: bool,
    clipboard: String,
    permissions: PermissionStatus,
    warps: Vec<(f64, f64)>,
}

impl FakePlatform {
    /// Fake Peer with the given Desktop bounds.
    pub fn new(desktop: Desktop) -> Self {
        Self {
            desktop,
            capture_tx: None,
            injected: Vec::new(),
            suppressed: false,
            clipboard: String::new(),
            permissions: PermissionStatus::Granted,
            warps: Vec::new(),
        }
    }

    /// Pretend the OS reports this permission state.
    pub fn with_permissions(mut self, status: PermissionStatus) -> Self {
        self.permissions = status;
        self
    }

    /// Feed one physical event into the capture channel.
    pub fn feed_physical_event(&self, event: InputEvent) -> Result<(), PlatformError> {
        self.capture_tx
            .as_ref()
            .ok_or_else(|| PlatformError::Capture("capture not started".to_string()))?
            .send(event)
            .map_err(|err| PlatformError::Capture(err.to_string()))
    }

    /// Every event passed to [`Platform::inject`], in order.
    pub fn recorded_injections(&self) -> &[InputEvent] {
        &self.injected
    }

    /// Whether local input is currently suppressed (this Peer is the Sink).
    pub fn is_suppressed(&self) -> bool {
        self.suppressed
    }

    /// Every cursor position passed to [`Platform::warp_cursor`], in order.
    pub fn warp_log(&self) -> &[(f64, f64)] {
        &self.warps
    }
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self::new(Desktop::default())
    }
}

impl Platform for FakePlatform {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        Ok(self.desktop)
    }

    fn start_capture(&mut self) -> Result<Receiver<InputEvent>, PlatformError> {
        let (tx, rx) = mpsc::channel();
        self.capture_tx = Some(tx);
        Ok(rx)
    }

    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed = true;
        Ok(())
    }

    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed = false;
        Ok(())
    }

    fn inject(&mut self, event: InputEvent) -> Result<(), PlatformError> {
        self.injected.push(event);
        Ok(())
    }

    fn warp_cursor(&mut self, x: f64, y: f64) -> Result<(), PlatformError> {
        self.warps.push((x, y));
        Ok(())
    }

    fn clipboard_get(&self) -> Result<String, PlatformError> {
        Ok(self.clipboard.clone())
    }

    fn clipboard_set(&mut self, text: &str) -> Result<(), PlatformError> {
        if text.len() > CLIPBOARD_MAX_BYTES {
            return Err(PlatformError::Clipboard(format!(
                "text exceeds {CLIPBOARD_MAX_BYTES} bytes"
            )));
        }
        self.clipboard = text.to_string();
        Ok(())
    }

    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        Ok(self.permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fake_platform_feeds_physical_and_records_injected() {
        let mut platform = FakePlatform::new(Desktop::new(0.0, 0.0, 1920.0, 1080.0));
        let capture = platform.start_capture().expect("capture starts");

        let physical = InputEvent::MouseMove { x: 100.0, y: 200.0 };
        platform
            .feed_physical_event(physical.clone())
            .expect("feed physical event");
        assert_eq!(
            capture
                .recv_timeout(Duration::from_secs(1))
                .expect("observe physical event"),
            physical
        );

        let injected = InputEvent::Key {
            usage_id: 0x04,
            pressed: true,
        };
        platform.inject(injected.clone()).expect("inject");
        assert_eq!(platform.recorded_injections(), &[injected]);
    }

    #[test]
    fn fake_platform_rejects_oversize_clipboard() {
        let mut platform = FakePlatform::default();
        let big = "x".repeat(CLIPBOARD_MAX_BYTES + 1);
        assert!(platform.clipboard_set(&big).is_err());

        platform.clipboard_set("hello").expect("set clipboard");
        assert_eq!(platform.clipboard_get().expect("get clipboard"), "hello");
    }
}
