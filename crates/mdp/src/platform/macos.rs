//! macOS implementation of [`Platform`](mdp_core::Platform).
//!
//! Capture is a session `CGEventTap` on a dedicated CFRunLoop thread. Every
//! event we inject carries [`INJECT_TAG`] in `kCGEventSourceUserData`, so the
//! tap passes it through untouched and never reports it (no input loops).
//! While Focus is remote the tap swallows local input and the cursor is
//! detached from the mouse, so motion is reported from the raw deltas.
//! Injection posts `CGEvent`s at the HID tap point; keys go through the
//! HID-usage keymap and carry the modifier flags we hold.
//! Accessibility + Input Monitoring are checked, never assumed.

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CGMouseButton, CallbackResult, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mdp_core::keymap::{hid_to_mac_keycode, mac_keycode_to_hid};
use mdp_core::platform::CLIPBOARD_MAX_BYTES;
use mdp_core::{Desktop, InputEvent, MouseButton, PermissionStatus, Platform, PlatformError};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `kCGEventSourceUserData` value stamped on every event we inject ("mdp!").
const INJECT_TAG: i64 = 0x6d64_7021;

/// `kCGMouseEventClickState`: 1 = single click, 2 = double click, ...
const MOUSE_EVENT_CLICK_STATE: u32 = 1;
/// `kCGScrollWheelEventFixedPtDeltaAxis1/2`: fractional line deltas.
const SCROLL_FIXED_PT_DELTA_AXIS_1: u32 = 93;
const SCROLL_FIXED_PT_DELTA_AXIS_2: u32 = 94;

// ponytail: fixed double-click window; read NSEvent.doubleClickInterval if users tune it.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// HID usage for Caps Lock.
const HID_CAPS_LOCK: u16 = 0x39;
/// Mac virtual keycode for Caps Lock.
const MAC_CAPS_LOCK: u16 = 0x39;

/// One modifier key: HID usage, Mac keycode, device-dependent flag bit
/// (left/right specific, `NX_DEVICE*KEYMASK`), device-independent flag.
struct Modifier {
    hid: u16,
    mac: u16,
    device_bit: u64,
    flag: u64,
}

const CONTROL: u64 = 0x0004_0000;
const SHIFT: u64 = 0x0002_0000;
const ALTERNATE: u64 = 0x0008_0000;
const COMMAND: u64 = 0x0010_0000;

const MODIFIERS: [Modifier; 8] = [
    Modifier {
        hid: 0xE0,
        mac: 0x3B,
        device_bit: 0x0001,
        flag: CONTROL,
    },
    Modifier {
        hid: 0xE1,
        mac: 0x38,
        device_bit: 0x0002,
        flag: SHIFT,
    },
    Modifier {
        hid: 0xE2,
        mac: 0x3A,
        device_bit: 0x0020,
        flag: ALTERNATE,
    },
    Modifier {
        hid: 0xE3,
        mac: 0x37,
        device_bit: 0x0008,
        flag: COMMAND,
    },
    Modifier {
        hid: 0xE4,
        mac: 0x3E,
        device_bit: 0x2000,
        flag: CONTROL,
    },
    Modifier {
        hid: 0xE5,
        mac: 0x3C,
        device_bit: 0x0004,
        flag: SHIFT,
    },
    Modifier {
        hid: 0xE6,
        mac: 0x3D,
        device_bit: 0x0040,
        flag: ALTERNATE,
    },
    Modifier {
        hid: 0xE7,
        mac: 0x36,
        device_bit: 0x0010,
        flag: COMMAND,
    },
];

const DEVICE_BITS: u64 = 0x0001 | 0x0002 | 0x0004 | 0x0008 | 0x0010 | 0x0020 | 0x0040 | 0x2000;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
}

/// `(accessibility, input_monitoring)` grants for this process.
pub fn permission_flags() -> (bool, bool) {
    // SAFETY: argument-free status queries.
    unsafe { (AXIsProcessTrusted(), CGPreflightListenEventAccess()) }
}

/// A `flagsChanged` event names the modifier key that moved and the flags
/// after the change; turn that into HID key transitions. Caps Lock only
/// reports toggles, so it becomes a full press + release.
fn flags_changed(keycode: u16, flags: u64) -> Vec<(u16, bool)> {
    if keycode == MAC_CAPS_LOCK {
        return vec![(HID_CAPS_LOCK, true), (HID_CAPS_LOCK, false)];
    }
    let Some(m) = MODIFIERS.iter().find(|m| m.mac == keycode) else {
        return Vec::new();
    };
    // Some virtual keyboards omit the left/right bits; fall back to the
    // device-independent flag then.
    let pressed = if flags & DEVICE_BITS != 0 {
        flags & m.device_bit != 0
    } else {
        flags & m.flag != 0
    };
    vec![(m.hid, pressed)]
}

/// Flags for an injected event given the held HID modifier usages.
fn modifier_flags(held: &[u16]) -> u64 {
    MODIFIERS
        .iter()
        .filter(|m| held.contains(&m.hid))
        .fold(0, |acc, m| acc | m.flag | m.device_bit)
}

/// Cursor position to report. While suppressed the cursor is detached
/// from the mouse, so accumulate raw deltas onto the last position.
fn next_cursor(
    last: (f64, f64),
    location: (f64, f64),
    delta: (f64, f64),
    suppressed: bool,
) -> (f64, f64) {
    if suppressed {
        (last.0 + delta.0, last.1 + delta.1)
    } else {
        location
    }
}

/// Bounding box of display rects `(x, y, w, h)`.
fn union_bounds(rects: impl IntoIterator<Item = (f64, f64, f64, f64)>) -> Option<Desktop> {
    rects.into_iter().fold(None, |acc, (x, y, w, h)| {
        let (x0, y0, x1, y1) = match acc {
            None => (x, y, x + w, y + h),
            Some(d) => (
                d.x.min(x),
                d.y.min(y),
                (d.x + d.width).max(x + w),
                (d.y + d.height).max(y + h),
            ),
        };
        Some(Desktop::new(x0, y0, x1 - x0, y1 - y0))
    })
}

/// Click count for a press: consecutive presses of the same button within
/// [`DOUBLE_CLICK`] count up (double/triple click), anything else resets.
fn click_count(
    prev: Option<(MouseButton, Instant, i64)>,
    button: MouseButton,
    now: Instant,
) -> i64 {
    match prev {
        Some((b, at, n)) if b == button && now.duration_since(at) <= DOUBLE_CLICK => n + 1,
        _ => 1,
    }
}

/// State owned by the tap callback (only ever touched on the tap thread).
struct TapState {
    tx: Sender<InputEvent>,
    suppressed: Arc<AtomicBool>,
    port: Arc<AtomicPtr<c_void>>,
    last: Mutex<(f64, f64)>,
}

impl TapState {
    fn handle(&self, etype: CGEventType, event: &CGEvent) -> CallbackResult {
        if matches!(
            etype,
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
        ) {
            // SAFETY: the port is our live tap; set before the run loop starts.
            unsafe { CGEventTapEnable(self.port.load(Ordering::Acquire), true) };
            return CallbackResult::Keep;
        }
        if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == INJECT_TAG {
            return CallbackResult::Keep;
        }
        let suppressed = self.suppressed.load(Ordering::Relaxed);
        for input in self.translate(etype, event, suppressed) {
            // Never block: an unbounded send; a dropped receiver is ignored.
            let _ = self.tx.send(input);
        }
        if suppressed {
            CallbackResult::Drop
        } else {
            CallbackResult::Keep
        }
    }

    fn translate(&self, etype: CGEventType, event: &CGEvent, suppressed: bool) -> Vec<InputEvent> {
        let button = |button, pressed| vec![InputEvent::MouseButton { button, pressed }];
        let keycode = || event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
        match etype {
            CGEventType::KeyDown | CGEventType::KeyUp => mac_keycode_to_hid(keycode())
                .map(|usage_id| InputEvent::Key {
                    usage_id,
                    pressed: matches!(etype, CGEventType::KeyDown),
                })
                .into_iter()
                .collect(),
            CGEventType::FlagsChanged => flags_changed(keycode(), event.get_flags().bits())
                .into_iter()
                .map(|(usage_id, pressed)| InputEvent::Key { usage_id, pressed })
                .collect(),
            CGEventType::MouseMoved
            | CGEventType::LeftMouseDragged
            | CGEventType::RightMouseDragged
            | CGEventType::OtherMouseDragged => {
                let loc = event.location();
                let delta = (
                    event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X) as f64,
                    event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y) as f64,
                );
                let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
                *last = next_cursor(*last, (loc.x, loc.y), delta, suppressed);
                vec![InputEvent::MouseMove {
                    x: last.0,
                    y: last.1,
                }]
            }
            CGEventType::LeftMouseDown => button(MouseButton::Left, true),
            CGEventType::LeftMouseUp => button(MouseButton::Left, false),
            CGEventType::RightMouseDown => button(MouseButton::Right, true),
            CGEventType::RightMouseUp => button(MouseButton::Right, false),
            CGEventType::OtherMouseDown | CGEventType::OtherMouseUp
                if event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER) == 2 =>
            {
                button(
                    MouseButton::Middle,
                    matches!(etype, CGEventType::OtherMouseDown),
                )
            }
            // Lines; +dy = up, +dx = right (macOS axis 2 is +left).
            CGEventType::ScrollWheel => vec![InputEvent::Scroll {
                dx: -event.get_double_value_field(SCROLL_FIXED_PT_DELTA_AXIS_2),
                dy: event.get_double_value_field(SCROLL_FIXED_PT_DELTA_AXIS_1),
            }],
            _ => Vec::new(),
        }
    }
}

/// Body of the capture thread: install the tap, report readiness, run.
fn run_tap(state: TapState, ready: mpsc::SyncSender<Result<(), PlatformError>>) {
    let port = state.port.clone();
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            CGEventType::MouseMoved,
            CGEventType::LeftMouseDragged,
            CGEventType::RightMouseDragged,
            CGEventType::OtherMouseDragged,
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGEventType::ScrollWheel,
        ],
        move |_proxy, etype, event| state.handle(etype, event),
    );
    let Ok(tap) = tap else {
        // SAFETY: plain permission request; shows the system prompt once.
        unsafe { CGRequestListenEventAccess() };
        let _ = ready.send(Err(PlatformError::PermissionDenied(
            "Input Monitoring / Accessibility (event tap refused)",
        )));
        return;
    };
    port.store(
        tap.mach_port().as_concrete_TypeRef() as *mut c_void,
        Ordering::Release,
    );
    let Ok(source) = tap.mach_port().create_runloop_source(0) else {
        let _ = ready.send(Err(PlatformError::Capture("run loop source".into())));
        return;
    };
    // SAFETY: kCFRunLoopCommonModes is an immutable CF constant.
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    let _ = ready.send(Ok(()));
    // ponytail: the tap lives for the process; add CFRunLoop::stop on drop if capture restarts.
    CFRunLoop::run_current();
}

fn source() -> Result<CGEventSource, PlatformError> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| PlatformError::Inject("CGEventSourceCreate failed".into()))
}

fn cg_button(button: MouseButton) -> CGMouseButton {
    match button {
        MouseButton::Left => CGMouseButton::Left,
        MouseButton::Right => CGMouseButton::Right,
        MouseButton::Middle => CGMouseButton::Center,
    }
}

/// macOS Peer platform.
#[derive(Default)]
pub struct MacosPlatform {
    suppressed: Arc<AtomicBool>,
    /// HID modifier usages we hold down through injection.
    held_mods: Vec<u16>,
    /// Buttons we hold down through injection (moves become drags).
    held_buttons: Vec<MouseButton>,
    last_click: Option<(MouseButton, Instant, i64)>,
}

impl MacosPlatform {
    /// Create the macOS platform handle.
    pub fn new() -> Self {
        Self::default()
    }

    fn build(&mut self, event: InputEvent) -> Result<CGEvent, PlatformError> {
        let fail = |what: &str| PlatformError::Inject(format!("{what} event creation failed"));
        match event {
            InputEvent::Key { usage_id, pressed } => {
                let keycode = hid_to_mac_keycode(usage_id).ok_or_else(|| {
                    PlatformError::Inject(format!("no macOS keycode for HID usage {usage_id:#04x}"))
                })?;
                if MODIFIERS.iter().any(|m| m.hid == usage_id) {
                    self.held_mods.retain(|&h| h != usage_id);
                    if pressed {
                        self.held_mods.push(usage_id);
                    }
                }
                let cg = CGEvent::new_keyboard_event(source()?, keycode, pressed)
                    .map_err(|()| fail("keyboard"))?;
                cg.set_flags(CGEventFlags::from_bits_retain(modifier_flags(
                    &self.held_mods,
                )));
                Ok(cg)
            }
            InputEvent::MouseMove { x, y } => {
                let (etype, button) = match self.held_buttons.first() {
                    Some(MouseButton::Left) => (CGEventType::LeftMouseDragged, CGMouseButton::Left),
                    Some(MouseButton::Right) => {
                        (CGEventType::RightMouseDragged, CGMouseButton::Right)
                    }
                    Some(MouseButton::Middle) => {
                        (CGEventType::OtherMouseDragged, CGMouseButton::Center)
                    }
                    None => (CGEventType::MouseMoved, CGMouseButton::Left),
                };
                CGEvent::new_mouse_event(source()?, etype, CGPoint::new(x, y), button)
                    .map_err(|()| fail("mouse move"))
            }
            InputEvent::MouseButton { button, pressed } => {
                let etype = match (button, pressed) {
                    (MouseButton::Left, true) => CGEventType::LeftMouseDown,
                    (MouseButton::Left, false) => CGEventType::LeftMouseUp,
                    (MouseButton::Right, true) => CGEventType::RightMouseDown,
                    (MouseButton::Right, false) => CGEventType::RightMouseUp,
                    (MouseButton::Middle, true) => CGEventType::OtherMouseDown,
                    (MouseButton::Middle, false) => CGEventType::OtherMouseUp,
                };
                self.held_buttons.retain(|&b| b != button);
                if pressed {
                    let count = click_count(self.last_click, button, Instant::now());
                    self.last_click = Some((button, Instant::now(), count));
                    self.held_buttons.push(button);
                }
                let clicks = self.last_click.map_or(1, |(_, _, n)| n);
                let here = CGEvent::new(source()?)
                    .map_err(|()| fail("null"))?
                    .location();
                let cg = CGEvent::new_mouse_event(source()?, etype, here, cg_button(button))
                    .map_err(|()| fail("mouse button"))?;
                cg.set_integer_value_field(MOUSE_EVENT_CLICK_STATE, clicks);
                Ok(cg)
            }
            // ponytail: whole lines only; accumulate fractional remainders if trackpad scrolling feels coarse.
            InputEvent::Scroll { dx, dy } => CGEvent::new_scroll_event(
                source()?,
                ScrollEventUnit::LINE,
                2,
                dy.round() as i32,
                (-dx).round() as i32,
                0,
            )
            .map_err(|()| fail("scroll")),
        }
    }
}

impl Platform for MacosPlatform {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        let ids = CGDisplay::active_displays()
            .map_err(|e| PlatformError::Capture(format!("CGGetActiveDisplayList: {e}")))?;
        union_bounds(ids.into_iter().map(|id| {
            let r = CGDisplay::new(id).bounds();
            (r.origin.x, r.origin.y, r.size.width, r.size.height)
        }))
        .ok_or_else(|| PlatformError::Capture("no active displays".into()))
    }

    fn start_capture(&mut self) -> Result<Receiver<InputEvent>, PlatformError> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let start = CGEvent::new(source()?)
            .map(|e| e.location())
            .map(|p| (p.x, p.y))
            .unwrap_or_default();
        let state = TapState {
            tx,
            suppressed: self.suppressed.clone(),
            port: Arc::new(AtomicPtr::new(std::ptr::null_mut())),
            last: Mutex::new(start),
        };
        std::thread::Builder::new()
            .name("mdp-capture".into())
            .spawn(move || run_tap(state, ready_tx))
            .map_err(|e| PlatformError::Capture(format!("spawn capture thread: {e}")))?;
        ready_rx
            .recv()
            .map_err(|_| PlatformError::Capture("capture thread exited".into()))??;
        Ok(rx)
    }

    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed.store(true, Ordering::Relaxed);
        // Freeze the local cursor; the tap still sees raw mouse deltas.
        CGDisplay::associate_mouse_and_mouse_cursor_position(false)
            .map_err(|e| PlatformError::Capture(format!("detach cursor: {e}")))
    }

    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed.store(false, Ordering::Relaxed);
        CGDisplay::associate_mouse_and_mouse_cursor_position(true)
            .map_err(|e| PlatformError::Capture(format!("reattach cursor: {e}")))
    }

    fn inject(&mut self, event: InputEvent) -> Result<(), PlatformError> {
        let cg = self.build(event)?;
        cg.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, INJECT_TAG);
        cg.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn warp_cursor(&mut self, x: f64, y: f64) -> Result<(), PlatformError> {
        CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y))
            .map_err(|e| PlatformError::Warp(format!("CGWarpMouseCursorPosition: {e}")))?;
        CGDisplay::associate_mouse_and_mouse_cursor_position(true).map_err(|e| {
            PlatformError::Warp(format!("CGAssociateMouseAndMouseCursorPosition: {e}"))
        })
    }

    fn clipboard_get(&self) -> Result<String, PlatformError> {
        arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .map_err(|e| PlatformError::Clipboard(e.to_string()))
    }

    fn clipboard_set(&mut self, text: &str) -> Result<(), PlatformError> {
        if text.len() > CLIPBOARD_MAX_BYTES {
            return Err(PlatformError::Clipboard(format!(
                "{} bytes exceeds the {CLIPBOARD_MAX_BYTES}-byte cap",
                text.len()
            )));
        }
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text))
            .map_err(|e| PlatformError::Clipboard(e.to_string()))
    }

    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        match permission_flags() {
            (true, true) => Ok(PermissionStatus::Granted),
            (false, true) => Err(PlatformError::PermissionDenied(
                "Accessibility (System Settings > Privacy & Security > Accessibility)",
            )),
            (true, false) => Err(PlatformError::PermissionDenied(
                "Input Monitoring (System Settings > Privacy & Security > Input Monitoring)",
            )),
            (false, false) => Err(PlatformError::PermissionDenied(
                "Accessibility and Input Monitoring (System Settings > Privacy & Security)",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_changed_left_and_right_shift() {
        // Left shift down: device bit 0x2 plus Shift.
        assert_eq!(flags_changed(0x38, SHIFT | 0x2), vec![(0xE1, true)]);
        // Right shift down while left still held.
        assert_eq!(flags_changed(0x3C, SHIFT | 0x2 | 0x4), vec![(0xE5, true)]);
        // Left shift up; right still held keeps Shift set.
        assert_eq!(flags_changed(0x38, SHIFT | 0x4), vec![(0xE1, false)]);
        assert_eq!(flags_changed(0x3C, 0), vec![(0xE5, false)]);
    }

    #[test]
    fn flags_changed_without_device_bits_uses_class_flag() {
        assert_eq!(flags_changed(0x37, COMMAND), vec![(0xE3, true)]);
        assert_eq!(flags_changed(0x37, 0), vec![(0xE3, false)]);
    }

    #[test]
    fn flags_changed_caps_lock_is_a_tap_and_unknown_is_ignored() {
        assert_eq!(
            flags_changed(0x39, 0x0001_0000),
            vec![(0x39, true), (0x39, false)]
        );
        assert_eq!(flags_changed(0x3F, 0x0080_0000), vec![]);
    }

    #[test]
    fn every_modifier_round_trips_through_flags() {
        for m in &MODIFIERS {
            let flags = modifier_flags(&[m.hid]);
            assert_eq!(flags_changed(m.mac, flags), vec![(m.hid, true)]);
            assert_eq!(flags_changed(m.mac, 0), vec![(m.hid, false)]);
        }
        assert_eq!(modifier_flags(&[]), 0);
        assert_eq!(
            modifier_flags(&[0xE0, 0xE7]),
            CONTROL | 0x1 | COMMAND | 0x10
        );
    }

    #[test]
    fn cursor_follows_location_unless_suppressed() {
        assert_eq!(
            next_cursor((10.0, 10.0), (50.0, 60.0), (3.0, 4.0), false),
            (50.0, 60.0)
        );
        assert_eq!(
            next_cursor((10.0, 10.0), (50.0, 60.0), (3.0, -4.0), true),
            (13.0, 6.0)
        );
    }

    #[test]
    fn desktop_is_union_of_displays() {
        assert_eq!(union_bounds([]), None);
        let d = union_bounds([(0.0, 0.0, 1440.0, 900.0), (-1920.0, -180.0, 1920.0, 1080.0)]);
        assert_eq!(d, Some(Desktop::new(-1920.0, -180.0, 3360.0, 1080.0)));
    }

    #[test]
    fn clicks_count_up_within_window_on_same_button() {
        let t = Instant::now();
        assert_eq!(click_count(None, MouseButton::Left, t), 1);
        let prev = Some((MouseButton::Left, t, 1));
        assert_eq!(
            click_count(prev, MouseButton::Left, t + Duration::from_millis(200)),
            2
        );
        assert_eq!(
            click_count(prev, MouseButton::Right, t + Duration::from_millis(200)),
            1
        );
        assert_eq!(
            click_count(prev, MouseButton::Left, t + Duration::from_secs(1)),
            1
        );
    }
}
