//! Platform-neutral `mdp selftest` core, shared by the OS arms.
//!
//! The OS arm fetches the current cursor position as `home`, then this core
//! injects a tagged key plus a mouse move, proves capture never reports the
//! injected input, and warps the cursor back home. It finishes in well under
//! a second and leaves the cursor where it found it.

use mdp_core::{Desktop, InputEvent, Platform};
use std::time::{Duration, Instant};

/// HID usage injected as the tagged key: F24, identifiable and rarely bound.
pub const SELFTEST_KEY_HID: u16 = 0x73;
/// How far right the selftest nudges the cursor, in logical points.
pub const SELFTEST_MOVE_DX: f64 = 12.0;
/// Grace time for the hook thread to install before injecting.
const INSTALL_WAIT: Duration = Duration::from_millis(300);
/// Settle time after each injection before checking capture.
const SETTLE: Duration = Duration::from_millis(150);

/// What the selftest proved.
pub struct SelftestReport {
    /// Desktop bounds, for the acceptance printout.
    pub desktop: Desktop,
    /// Wall time of the whole selftest.
    pub elapsed: Duration,
}

fn drain(capture: &std::sync::mpsc::Receiver<InputEvent>) -> Vec<InputEvent> {
    let mut events = Vec::new();
    while let Ok(event) = capture.try_recv() {
        events.push(event);
    }
    events
}

/// Run the silence proof against any [`Platform`]: inject a tagged key and a
/// mouse move, fail if capture reports either, and warp back to `home`.
pub fn run_selftest(
    platform: &mut impl Platform,
    home: (f64, f64),
) -> Result<SelftestReport, String> {
    let started = Instant::now();
    let desktop = platform
        .desktop_bounds()
        .map_err(|err| format!("desktop bounds: {err}"))?;
    let capture = platform
        .start_capture()
        .map_err(|err| format!("capture: {err}"))?;
    std::thread::sleep(INSTALL_WAIT);
    drain(&capture);

    // Tagged key down + up must never surface in capture.
    for pressed in [true, false] {
        platform
            .inject(InputEvent::Key {
                usage_id: SELFTEST_KEY_HID,
                pressed,
            })
            .map_err(|err| format!("inject key: {err}"))?;
    }
    std::thread::sleep(SETTLE);
    let leaked = drain(&capture);
    if !leaked.is_empty() {
        return Err(format!(
            "capture reported {} injected event(s): {leaked:?}",
            leaked.len()
        ));
    }

    // A real cursor move must not surface either.
    platform
        .inject(InputEvent::MouseMove {
            x: home.0 + SELFTEST_MOVE_DX,
            y: home.1,
        })
        .map_err(|err| format!("inject move: {err}"))?;
    std::thread::sleep(SETTLE);
    let leaked = drain(&capture);
    if !leaked.is_empty() {
        return Err(format!(
            "capture reported {} injected event(s): {leaked:?}",
            leaked.len()
        ));
    }

    // Restore the cursor where we found it.
    platform
        .warp_cursor(home.0, home.1)
        .map_err(|err| format!("restore cursor: {err}"))?;
    Ok(SelftestReport {
        desktop,
        elapsed: started.elapsed(),
    })
}
