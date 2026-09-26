//! Windows implementation of [`Platform`](mdp_core::Platform).
//!
//! Capture uses low-level hooks (`WH_KEYBOARD_LL` / `WH_MOUSE_LL`) installed
//! on a dedicated message-loop thread. The hook callbacks never block: they
//! forward decoded events to an unbounded channel and return. Events flagged
//! `LLMHF_INJECTED` / `LLKHF_INJECTED` are never reported, so injected input
//! never loops back. While suppressed (Focus remote) the hooks swallow input
//! by returning 1. Injection uses `SendInput` with keys by scan code (via
//! `mdp_core::keymap`); warp uses `SetCursorPos`; Desktop bounds union the
//! per-monitor logical rects under per-monitor-DPI-awareness v2; clipboard
//! goes through `arboard`.

use mdp_core::keymap::{hid_to_windows_scancode, windows_scancode_to_hid};
use mdp_core::{
    Desktop, InputEvent, MouseButton, PermissionStatus, Platform, PlatformError,
    CLIPBOARD_MAX_BYTES,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use windows::core::BOOL;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, GetSystemMetrics, PostThreadMessageW, SetCursorPos,
    SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, LLKHF_EXTENDED,
    LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
};

/// Warn once per process when injection drops a key with no Windows mapping.
static UNKNOWN_KEY_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_unknown_key(usage_id: u16) {
    if !UNKNOWN_KEY_WARNED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "mdp: no Windows mapping for HID {usage_id:#04X}; dropping (further drops silent)"
        );
    }
}

struct HookSink {
    tx: mpsc::Sender<InputEvent>,
    suppressed: Arc<AtomicBool>,
}

/// The live capture hook endpoint, if any. The hook callbacks grab this
/// without blocking (a short uncontended lock, then an unbounded send).
static HOOK_SINK: Mutex<Option<Arc<HookSink>>> = Mutex::new(None);

/// Decode a low-level keyboard event. Pure: OS numbers in, [`InputEvent`] out.
fn translate_keyboard(msg: u32, scancode: u16, extended: bool) -> Option<InputEvent> {
    let usage_id = windows_scancode_to_hid(scancode, extended)?;
    let pressed = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    Some(InputEvent::Key { usage_id, pressed })
}

/// Wheel delta from the high word of mouse data, in 120-unit notches.
fn wheel_notches(mouse_data: u32) -> f64 {
    (((mouse_data >> 16) & 0xFFFF) as u16) as i16 as f64 / 120.0
}

/// Decode a low-level mouse event. Pure: OS numbers in, [`InputEvent`] out.
/// Fourth/fifth (X) buttons have no [`InputEvent`] slot and map to [`None`].
fn translate_mouse(msg: u32, mouse_data: u32, x: i32, y: i32) -> Option<InputEvent> {
    match msg {
        WM_MOUSEMOVE => Some(InputEvent::MouseMove {
            x: x as f64,
            y: y as f64,
        }),
        WM_LBUTTONDOWN => Some(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        }),
        WM_LBUTTONUP => Some(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: false,
        }),
        WM_RBUTTONDOWN => Some(InputEvent::MouseButton {
            button: MouseButton::Right,
            pressed: true,
        }),
        WM_RBUTTONUP => Some(InputEvent::MouseButton {
            button: MouseButton::Right,
            pressed: false,
        }),
        WM_MBUTTONDOWN => Some(InputEvent::MouseButton {
            button: MouseButton::Middle,
            pressed: true,
        }),
        WM_MBUTTONUP => Some(InputEvent::MouseButton {
            button: MouseButton::Middle,
            pressed: false,
        }),
        WM_MOUSEWHEEL => Some(InputEvent::Scroll {
            dx: 0.0,
            dy: wheel_notches(mouse_data),
        }),
        WM_MOUSEHWHEEL => Some(InputEvent::Scroll {
            dx: wheel_notches(mouse_data),
            dy: 0.0,
        }),
        _ => None,
    }
}

/// Physical monitor rect + DPI → logical rect `(x, y, w, h)` in points.
fn logical_rect(x: i32, y: i32, w: i32, h: i32, dpi_x: u32, dpi_y: u32) -> (f64, f64, f64, f64) {
    let scale_x = dpi_x as f64 / 96.0;
    let scale_y = dpi_y as f64 / 96.0;
    (
        x as f64 / scale_x,
        y as f64 / scale_y,
        w as f64 / scale_x,
        h as f64 / scale_y,
    )
}

/// Union of logical monitor rects into one [`Desktop`].
fn union_desktop(rects: &[(f64, f64, f64, f64)]) -> Desktop {
    let mut iter = rects.iter();
    let first = iter.next().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
    let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0 + first.2, first.1 + first.3);
    for &(x, y, w, h) in iter {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    Desktop::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Logical point → absolute `SendInput` coordinates on the virtual screen.
fn absolute_coords(x: f64, y: f64, screen: (i32, i32, i32, i32)) -> (i32, i32) {
    let (ox, oy, w, h) = screen;
    let dx = ((x - ox as f64) * 65535.0 / w as f64)
        .round()
        .clamp(0.0, 65535.0) as i32;
    let dy = ((y - oy as f64) * 65535.0 / h as f64)
        .round()
        .clamp(0.0, 65535.0) as i32;
    (dx, dy)
}

/// Wheel delta in notches → `SendInput` mouse data units.
fn wheel_data(notches: f64) -> u32 {
    (notches * 120.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32 as u32
}

fn keybd_input(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Virtual-screen metrics `(x, y, w, h)` for absolute mouse injection.
fn virtual_screen_metrics() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn monitor_dpi(monitor: HMONITOR) -> (u32, u32) {
    unsafe {
        let mut dpi_x = 0u32;
        let mut dpi_y = 0u32;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok()
            && dpi_x > 0
            && dpi_y > 0
        {
            (dpi_x, dpi_y)
        } else {
            (96, 96)
        }
    }
}

unsafe extern "system" fn enum_monitor_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let out = &mut *(data.0 as *mut Vec<(f64, f64, f64, f64)>);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut MONITORINFO) != BOOL(0) {
        let rect = info.monitorInfo.rcMonitor;
        let (dpi_x, dpi_y) = monitor_dpi(monitor);
        out.push(logical_rect(
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            dpi_x,
            dpi_y,
        ));
    }
    BOOL(1)
}

fn enumerate_monitors() -> Option<Vec<(f64, f64, f64, f64)>> {
    let mut rects = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_proc),
            LPARAM(&mut rects as *mut Vec<(f64, f64, f64, f64)> as isize),
        )
    };
    if ok != BOOL(0) {
        Some(rects)
    } else {
        None
    }
}

/// What a hook does with one event: `(capture, swallow)`. Physical input is
/// always captured, even while suppressed: Focus is remote then, and the
/// captured events are what the Source forwards (and where the escape chord
/// is seen). Injected input is never captured and never swallowed.
fn hook_decision(injected: bool, suppressed: bool) -> (bool, bool) {
    (!injected, suppressed && !injected)
}

unsafe extern "system" fn keyboard_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 {
        let sink = HOOK_SINK.lock().ok().and_then(|guard| guard.clone());
        if let Some(sink) = sink {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let injected = info.flags & LLKHF_INJECTED != KBDLLHOOKSTRUCT_FLAGS(0);
            let (capture, swallow) =
                hook_decision(injected, sink.suppressed.load(Ordering::SeqCst));
            if capture {
                let extended = info.flags & LLKHF_EXTENDED != KBDLLHOOKSTRUCT_FLAGS(0);
                if let Some(event) =
                    translate_keyboard(wparam.0 as u32, info.scanCode as u16, extended)
                {
                    let _ = sink.tx.send(event);
                }
            }
            if swallow {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 {
        let sink = HOOK_SINK.lock().ok().and_then(|guard| guard.clone());
        if let Some(sink) = sink {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let injected = info.flags & LLMHF_INJECTED != 0;
            let (capture, swallow) =
                hook_decision(injected, sink.suppressed.load(Ordering::SeqCst));
            if capture {
                if let Some(event) =
                    translate_mouse(wparam.0 as u32, info.mouseData, info.pt.x, info.pt.y)
                {
                    let _ = sink.tx.send(event);
                }
            }
            if swallow {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

fn hook_thread_main(sink: Arc<HookSink>, ready: mpsc::Sender<Result<u32, String>>) {
    unsafe {
        if let Ok(mut guard) = HOOK_SINK.lock() {
            *guard = Some(sink);
        } else {
            let _ = ready.send(Err("hook sink busy".to_string()));
            return;
        }
        let module = match GetModuleHandleW(None) {
            Ok(module) => module,
            Err(err) => {
                let _ = ready.send(Err(format!("module handle: {err}")));
                return;
            }
        };
        let instance = HINSTANCE(module.0);
        let keyboard =
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), Some(instance), 0) {
                Ok(hook) => hook,
                Err(err) => {
                    let _ = ready.send(Err(format!("keyboard hook: {err}")));
                    return;
                }
            };
        let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), Some(instance), 0) {
            Ok(hook) => hook,
            Err(err) => {
                let _ = UnhookWindowsHookEx(keyboard);
                let _ = ready.send(Err(format!("mouse hook: {err}")));
                return;
            }
        };
        let thread_id = GetCurrentThreadId();
        if ready.send(Ok(thread_id)).is_err() {
            let _ = UnhookWindowsHookEx(keyboard);
            let _ = UnhookWindowsHookEx(mouse);
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
    }
    if let Ok(mut guard) = HOOK_SINK.lock() {
        *guard = None;
    }
}

struct InstalledHooks {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

/// Windows Peer platform.
pub struct WindowsPlatform {
    suppressed: Arc<AtomicBool>,
    installed: Mutex<Option<InstalledHooks>>,
    virtual_screen: (i32, i32, i32, i32),
}

impl WindowsPlatform {
    /// Create the Windows platform handle. Enables per-monitor-DPI-awareness
    /// v2 for the process (best effort) so bounds are logical points.
    pub fn new() -> Self {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        Self {
            suppressed: Arc::new(AtomicBool::new(false)),
            installed: Mutex::new(None),
            virtual_screen: virtual_screen_metrics(),
        }
    }

    /// Current cursor position in logical points.
    pub fn cursor_position() -> (f64, f64) {
        unsafe {
            let mut point = POINT::default();
            if GetCursorPos(&mut point).is_ok() {
                (point.x as f64, point.y as f64)
            } else {
                (0.0, 0.0)
            }
        }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WindowsPlatform {
    fn drop(&mut self) {
        let installed = self
            .installed
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        if let Some(mut installed) = installed {
            unsafe {
                let _ = PostThreadMessageW(installed.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(thread) = installed.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Platform for WindowsPlatform {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        match enumerate_monitors() {
            Some(rects) if !rects.is_empty() => Ok(union_desktop(&rects)),
            _ => {
                let (x, y, w, h) = virtual_screen_metrics();
                Ok(Desktop::new(x as f64, y as f64, w as f64, h as f64))
            }
        }
    }

    fn start_capture(&mut self) -> Result<mpsc::Receiver<InputEvent>, PlatformError> {
        if self
            .installed
            .lock()
            .map_err(|_| PlatformError::Capture("hook state poisoned".to_string()))?
            .is_some()
        {
            return Err(PlatformError::Capture("already capturing".to_string()));
        }
        let (tx, rx) = mpsc::channel();
        let sink = Arc::new(HookSink {
            tx,
            suppressed: Arc::clone(&self.suppressed),
        });
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mdp-hooks".to_string())
            .spawn(move || hook_thread_main(sink, ready_tx))
            .map_err(|err| PlatformError::Capture(format!("hook thread: {err}")))?;
        let thread_id = ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| PlatformError::Capture("hook install timed out".to_string()))?
            .map_err(PlatformError::Capture)?;
        *self
            .installed
            .lock()
            .map_err(|_| PlatformError::Capture("hook state poisoned".to_string()))? =
            Some(InstalledHooks {
                thread_id,
                thread: Some(thread),
            });
        Ok(rx)
    }

    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        self.suppressed.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn inject(&mut self, event: InputEvent) -> Result<(), PlatformError> {
        unsafe {
            match event {
                InputEvent::Key { usage_id, pressed } => {
                    let Some((scancode, extended)) = hid_to_windows_scancode(usage_id) else {
                        warn_unknown_key(usage_id);
                        return Ok(());
                    };
                    let mut flags = KEYEVENTF_SCANCODE;
                    if !pressed {
                        flags |= KEYEVENTF_KEYUP;
                    }
                    if extended {
                        flags |= KEYEVENTF_EXTENDEDKEY;
                    }
                    send_one(&keybd_input(scancode, flags), "key")
                }
                InputEvent::MouseMove { x, y } => {
                    let (dx, dy) = absolute_coords(x, y, self.virtual_screen);
                    send_one(
                        &mouse_input(
                            dx,
                            dy,
                            0,
                            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                        ),
                        "mouse move",
                    )
                }
                InputEvent::MouseButton { button, pressed } => {
                    let flag = match (button, pressed) {
                        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                    };
                    send_one(&mouse_input(0, 0, 0, flag), "mouse button")
                }
                InputEvent::Scroll { dx, dy } => {
                    if dy != 0.0 {
                        send_one(
                            &mouse_input(0, 0, wheel_data(dy), MOUSEEVENTF_WHEEL),
                            "wheel",
                        )?;
                    }
                    if dx != 0.0 {
                        send_one(
                            &mouse_input(0, 0, wheel_data(dx), MOUSEEVENTF_HWHEEL),
                            "h-wheel",
                        )?;
                    }
                    Ok(())
                }
            }
        }
    }

    fn warp_cursor(&mut self, x: f64, y: f64) -> Result<(), PlatformError> {
        unsafe { SetCursorPos(x as i32, y as i32) }
            .map_err(|err| PlatformError::Warp(format!("SetCursorPos: {err}")))?;
        Ok(())
    }

    fn clipboard_get(&self) -> Result<String, PlatformError> {
        arboard::Clipboard::new()
            .map_err(|err| PlatformError::Clipboard(err.to_string()))?
            .get_text()
            .map_err(|err| PlatformError::Clipboard(err.to_string()))
    }

    fn clipboard_set(&mut self, text: &str) -> Result<(), PlatformError> {
        if text.len() > CLIPBOARD_MAX_BYTES {
            return Err(PlatformError::Clipboard(format!(
                "text exceeds {CLIPBOARD_MAX_BYTES} bytes"
            )));
        }
        arboard::Clipboard::new()
            .map_err(|err| PlatformError::Clipboard(err.to_string()))?
            .set_text(text.to_owned())
            .map_err(|err| PlatformError::Clipboard(err.to_string()))?;
        Ok(())
    }

    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        // Windows desktop apps need no Accessibility-style grant for hooks,
        // SendInput, or the clipboard. (The secure desktop stays out of
        // scope per CONTEXT/architecture.md.)
        Ok(PermissionStatus::Granted)
    }
}

unsafe fn send_one(input: &INPUT, what: &str) -> Result<(), PlatformError> {
    if SendInput(
        std::slice::from_ref(input),
        std::mem::size_of::<INPUT>() as i32,
    ) == 1
    {
        Ok(())
    } else {
        Err(PlatformError::Inject(format!("SendInput {what} failed")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppressed_physical_input_is_still_captured() {
        assert_eq!(hook_decision(false, false), (true, false));
        assert_eq!(hook_decision(false, true), (true, true));
        assert_eq!(hook_decision(true, false), (false, false));
        assert_eq!(hook_decision(true, true), (false, false));
    }
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_KEYUP, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    #[test]
    fn keyboard_translation() {
        assert_eq!(
            translate_keyboard(WM_KEYDOWN, 0x1E, false),
            Some(InputEvent::Key {
                usage_id: 0x04,
                pressed: true
            })
        );
        assert_eq!(
            translate_keyboard(WM_KEYUP, 0x1E, false),
            Some(InputEvent::Key {
                usage_id: 0x04,
                pressed: false
            })
        );
        assert_eq!(
            translate_keyboard(WM_SYSKEYDOWN, 0x38, false),
            Some(InputEvent::Key {
                usage_id: 0xE2,
                pressed: true
            })
        );
        assert_eq!(
            translate_keyboard(WM_SYSKEYUP, 0x38, false),
            Some(InputEvent::Key {
                usage_id: 0xE2,
                pressed: false
            })
        );
        assert_eq!(
            translate_keyboard(WM_KEYDOWN, 0x1D, true),
            Some(InputEvent::Key {
                usage_id: 0xE4,
                pressed: true
            })
        );
        assert_eq!(translate_keyboard(WM_KEYDOWN, 0x00, false), None);
        assert_eq!(translate_keyboard(WM_KEYDOWN, 0x2A, true), None);
    }

    #[test]
    fn mouse_translation() {
        assert_eq!(
            translate_mouse(WM_MOUSEMOVE, 0, 100, 200),
            Some(InputEvent::MouseMove { x: 100.0, y: 200.0 })
        );
        assert_eq!(
            translate_mouse(WM_LBUTTONDOWN, 0, 0, 0),
            Some(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true
            })
        );
        assert_eq!(
            translate_mouse(WM_LBUTTONUP, 0, 0, 0),
            Some(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: false
            })
        );
        assert_eq!(
            translate_mouse(WM_RBUTTONDOWN, 0, 0, 0),
            Some(InputEvent::MouseButton {
                button: MouseButton::Right,
                pressed: true
            })
        );
        assert_eq!(
            translate_mouse(WM_MBUTTONUP, 0, 0, 0),
            Some(InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: false
            })
        );
        assert_eq!(
            translate_mouse(WM_MOUSEWHEEL, 120u32 << 16, 0, 0),
            Some(InputEvent::Scroll { dx: 0.0, dy: 1.0 })
        );
        assert_eq!(
            translate_mouse(WM_MOUSEWHEEL, ((-120i32) as u32) << 16, 0, 0),
            Some(InputEvent::Scroll { dx: 0.0, dy: -1.0 })
        );
        assert_eq!(
            translate_mouse(WM_MOUSEHWHEEL, 120u32 << 16, 0, 0),
            Some(InputEvent::Scroll { dx: 1.0, dy: 0.0 })
        );
        assert_eq!(translate_mouse(WM_XBUTTONDOWN, 0, 0, 0), None);
        assert_eq!(translate_mouse(WM_XBUTTONUP, 0, 0, 0), None);
    }

    #[test]
    fn logical_rect_scales_by_dpi() {
        assert_eq!(
            logical_rect(0, 0, 1920, 1080, 96, 96),
            (0.0, 0.0, 1920.0, 1080.0)
        );
        assert_eq!(
            logical_rect(0, 0, 1920, 1080, 144, 144),
            (0.0, 0.0, 1280.0, 720.0)
        );
        assert_eq!(
            logical_rect(-1920, 0, 1920, 1080, 96, 96),
            (-1920.0, 0.0, 1920.0, 1080.0)
        );
    }

    #[test]
    fn union_desktop_spans_monitors() {
        let desktop = union_desktop(&[(0.0, 0.0, 1920.0, 1080.0), (-1920.0, 0.0, 1920.0, 1080.0)]);
        assert_eq!(desktop, Desktop::new(-1920.0, 0.0, 3840.0, 1080.0));
        let single = union_desktop(&[(0.0, 0.0, 1280.0, 720.0)]);
        assert_eq!(single, Desktop::new(0.0, 0.0, 1280.0, 720.0));
    }

    #[test]
    fn absolute_coords_map_and_clamp() {
        let screen = (0, 0, 1920, 1080);
        assert_eq!(absolute_coords(0.0, 0.0, screen), (0, 0));
        assert_eq!(absolute_coords(1920.0, 1080.0, screen), (65535, 65535));
        assert_eq!(absolute_coords(960.0, 540.0, screen), (32768, 32768));
        assert_eq!(absolute_coords(5000.0, -50.0, screen), (65535, 0));
        let offset = (-1920, 0, 3840, 1080);
        assert_eq!(absolute_coords(-1920.0, 0.0, offset), (0, 0));
    }
}
