//! Tray / menu-bar icon (`CONTEXT/ui-design.md` §Tray): the two-rectangle
//! glyph in three states plus the menu.

use super::theme;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Which glyph the tray shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// Dashed grey: no Link.
    NoPeer,
    /// Left rectangle filled amber: Focus on this Peer.
    FocusHere,
    /// Right rectangle filled amber: Focus on the other Peer.
    FocusPeer,
}

pub fn tray_state(linked: bool, focus_here: bool) -> TrayState {
    match (linked, focus_here) {
        (false, _) => TrayState::NoPeer,
        (true, true) => TrayState::FocusHere,
        (true, false) => TrayState::FocusPeer,
    }
}

/// Side length of the rendered icon in pixels.
pub const ICON_PX: u32 = 32;

/// Glyph rectangles in the mockup's 28×20 viewBox: (x, y, w, h).
const LEFT_RECT: (f32, f32, f32, f32) = (1.0, 3.0, 14.0, 10.0);
const RIGHT_RECT: (f32, f32, f32, f32) = (13.0, 7.0, 14.0, 10.0);

/// RGBA pixels (`ICON_PX`²) of the glyph for `state`.
pub fn icon_rgba(state: TrayState) -> Vec<u8> {
    let color = match state {
        TrayState::NoPeer => [0x7C, 0x84, 0x8E, 0xFF],
        _ => theme::ACCENT.to_array(),
    };
    let filled = match state {
        TrayState::NoPeer => None,
        TrayState::FocusHere => Some(LEFT_RECT),
        TrayState::FocusPeer => Some(RIGHT_RECT),
    };
    // Fit the 28-wide viewBox into the square, centred vertically.
    let scale = ICON_PX as f32 / 28.0;
    let y_pad = (ICON_PX as f32 - 20.0 * scale) / 2.0;
    let stroke = 2.0;
    let mut px = vec![0u8; (ICON_PX * ICON_PX * 4) as usize];
    for y in 0..ICON_PX {
        for x in 0..ICON_PX {
            let (vx, vy) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5 - y_pad) / scale);
            let inside = |(rx, ry, rw, rh): (f32, f32, f32, f32)| {
                vx >= rx && vx <= rx + rw && vy >= ry && vy <= ry + rh
            };
            let on_border = |r: (f32, f32, f32, f32)| {
                let (rx, ry, rw, rh) = r;
                inside(r)
                    && (vx - rx < stroke / 2.0 + 0.5
                        || rx + rw - vx < stroke / 2.0 + 0.5
                        || vy - ry < stroke / 2.0 + 0.5
                        || ry + rh - vy < stroke / 2.0 + 0.5)
            };
            let dashed_gap = state == TrayState::NoPeer && ((vx + vy) as i32 / 3) % 2 == 1;
            let lit = filled.is_some_and(inside)
                || ((on_border(LEFT_RECT) || on_border(RIGHT_RECT)) && !dashed_gap);
            if lit {
                let i = ((y * ICON_PX + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&color);
            }
        }
    }
    px
}

fn icon(state: TrayState) -> Icon {
    Icon::from_rgba(icon_rgba(state), ICON_PX, ICON_PX).expect("icon size matches buffer")
}

/// What the user picked from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShareInput(bool),
    ShareClipboard(bool),
    OpenArrange,
    Pair,
    Unpair,
    Quit,
}

/// The live tray icon and its menu. Build it on the main thread after the
/// event loop starts (inside the eframe app creator).
pub struct Tray {
    icon: TrayIcon,
    events: std::sync::mpsc::Receiver<MenuEvent>,
    status: MenuItem,
    share_input: CheckMenuItem,
    share_clipboard: CheckMenuItem,
    arrange: MenuItem,
    pair: MenuItem,
    unpair: MenuItem,
    quit: MenuItem,
}

impl Tray {
    /// `wake` runs on every menu pick so a hidden app still reacts
    /// (eframe calls `App::logic` after `request_repaint`).
    pub fn new(
        peer_name: &str,
        share_input: bool,
        share_clipboard: bool,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (events_tx, events) = std::sync::mpsc::channel();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = events_tx.send(event);
            wake();
        }));
        let status = MenuItem::new("Not linked", false, None);
        let share_input = CheckMenuItem::new("Share mouse & keyboard", true, share_input, None);
        let share_clipboard =
            CheckMenuItem::new("Sync clipboard text", true, share_clipboard, None);
        let arrange = MenuItem::new("Arrange displays…", true, None);
        let pair = MenuItem::new("Pair a new device…", true, None);
        let unpair = MenuItem::new(format!("Unpair {peer_name}"), true, None);
        let quit = MenuItem::new("Quit mdp", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &status,
            &PredefinedMenuItem::separator(),
            &share_input,
            &share_clipboard,
            &PredefinedMenuItem::separator(),
            &arrange,
            &pair,
            &unpair,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("mdp")
            .with_icon(icon(TrayState::NoPeer))
            .build()?;
        Ok(Self {
            icon,
            events,
            status,
            share_input,
            share_clipboard,
            arrange,
            pair,
            unpair,
            quit,
        })
    }

    /// Show the Link/Focus state: glyph plus the status line, e.g.
    /// `Linked to Mac · 3 ms · focus: this PC`.
    pub fn set_status(&self, state: TrayState, text: &str) {
        let _ = self.icon.set_icon(Some(icon(state)));
        let _ = self.icon.set_tooltip(Some(format!("mdp — {text}")));
        self.status.set_text(text);
    }

    /// Menu picks since the last call.
    pub fn poll(&self) -> Vec<TrayCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            let id = event.id();
            commands.push(if id == self.share_input.id() {
                TrayCommand::ShareInput(self.share_input.is_checked())
            } else if id == self.share_clipboard.id() {
                TrayCommand::ShareClipboard(self.share_clipboard.is_checked())
            } else if id == self.arrange.id() {
                TrayCommand::OpenArrange
            } else if id == self.pair.id() {
                TrayCommand::Pair
            } else if id == self.unpair.id() {
                TrayCommand::Unpair
            } else if id == self.quit.id() {
                TrayCommand::Quit
            } else {
                continue;
            });
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_follows_link_and_focus() {
        assert_eq!(tray_state(false, true), TrayState::NoPeer);
        assert_eq!(tray_state(false, false), TrayState::NoPeer);
        assert_eq!(tray_state(true, true), TrayState::FocusHere);
        assert_eq!(tray_state(true, false), TrayState::FocusPeer);
    }

    fn pixel(px: &[u8], vx: f32, vy: f32) -> [u8; 4] {
        let scale = ICON_PX as f32 / 28.0;
        let y_pad = (ICON_PX as f32 - 20.0 * scale) / 2.0;
        let (x, y) = ((vx * scale) as u32, (vy * scale + y_pad) as u32);
        let i = ((y * ICON_PX + x) * 4) as usize;
        px[i..i + 4].try_into().expect("4 bytes")
    }

    #[test]
    fn glyph_fills_the_focused_rectangle() {
        let amber = theme::ACCENT.to_array();
        let clear = [0, 0, 0, 0];
        // Centre of the left-only area and of the right-only area.
        let (left, right) = ((6.0, 6.0), (22.0, 13.0));

        let here = icon_rgba(TrayState::FocusHere);
        assert_eq!(pixel(&here, left.0, left.1), amber);
        assert_eq!(pixel(&here, right.0, right.1), clear);

        let peer = icon_rgba(TrayState::FocusPeer);
        assert_eq!(pixel(&peer, left.0, left.1), clear);
        assert_eq!(pixel(&peer, right.0, right.1), amber);

        let none = icon_rgba(TrayState::NoPeer);
        assert_eq!(pixel(&none, left.0, left.1), clear);
        assert_eq!(pixel(&none, right.0, right.1), clear);
        assert!(none.chunks(4).any(|p| p == [0x7C, 0x84, 0x8E, 0xFF]));
        assert!(!none.chunks(4).any(|p| p == amber));
    }
}
