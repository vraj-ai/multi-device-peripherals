//! The Crossing engine: pure Focus/Source state machine for one Peer.
//!
//! Given the local [`Desktop`](crate::platform::Desktop), the peer Desktop
//! size, and the [`Arrangement`], the engine consumes local physical input
//! plus frames received from the peer, and emits [`CrossingFrame`]s to send
//! plus [`CrossingAction`]s for the [`Platform`](crate::platform::Platform).
//! It holds no I/O, no threads, and no Link: the wiring ticket maps
//! [`CrossingFrame`] 1:1 onto the wire frames and executes the actions.
//!
//! Rules (per the v1 spec):
//! - The Source tracks the absolute cursor. Hitting the shared edge inside
//!   the overlap span emits `Enter` with the proportional position, then the
//!   engine suppresses local input and streams relative motion. Motion
//!   outside the overlap span never crosses.
//! - The Sink maps `Enter` onto its Desktop and warps; relative motion is
//!   injected and tracked, and exiting back through the shared edge emits
//!   `Leave`, returning Focus.
//! - Every Focus change sends `ReleaseAll` and releases locally held
//!   keys/buttons, so no Crossing, disconnect, or Source change leaves stuck
//!   keys behind.
//! - Source arbitration: physical input on a Peer that is not Source sends a
//!   claim with a Lamport sequence number. The highest sequence wins; ties
//!   break toward the lower static key, so simultaneous claims resolve
//!   identically on both sides. The loser releases held input. Claims never
//!   move Focus; Focus follows the cursor only.
//! - Disconnect while remote warps back to the pre-cross cursor and
//!   un-suppresses. No frames are emitted: the Link is down.
//! - The escape chord (Ctrl+Alt+Shift+Esc, either side's modifiers) on the
//!   Source always returns cursor and keyboard locally.

use crate::platform::{Desktop, MouseButton};

/// USB HID usage for Esc.
const HID_ESC: u16 = 0x29;
/// USB HID usages for left/right Ctrl.
const HID_CTRLS: [u16; 2] = [0xE0, 0xE4];
/// USB HID usages for left/right Shift.
const HID_SHIFTS: [u16; 2] = [0xE1, 0xE5];
/// USB HID usages for left/right Alt.
const HID_ALTS: [u16; 2] = [0xE2, 0xE6];

/// How far inside the shared edge an entering cursor is placed, in logical
/// points, so the first relative motion does not instantly exit again.
const ENTRY_NUDGE: f64 = 1.0;

/// Which Peer currently receives keyboard and mouse input. It follows the
/// cursor: local while the cursor is on this Peer's Desktop, remote while
/// the cursor is on the other Peer's Desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Local,
    Remote,
}

/// Which side of this Peer's Desktop the other Peer's Desktop sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Where the other Peer's Desktop sits relative to this one: side plus the
/// offset of the peer Desktop along the shared edge, in logical points,
/// relative to edge-aligned placement. Both Peers hold the same Arrangement,
/// mirrored; the last saved one wins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrangement {
    pub side: Side,
    pub offset: f64,
}

impl Arrangement {
    /// Peer on `side`, edges aligned (`offset` zero).
    pub fn new(side: Side) -> Self {
        Self { side, offset: 0.0 }
    }

    /// Peer on `side`, shifted `offset` logical points along the shared edge.
    pub fn with_offset(side: Side, offset: f64) -> Self {
        Self { side, offset }
    }
}

/// Frames the Crossing engine exchanges with the other Peer. The wiring
/// ticket maps these 1:1 onto the Link wire frames.
#[derive(Debug, Clone, PartialEq)]
pub enum CrossingFrame {
    /// Crossing out through the shared edge; proportional entry position.
    Enter {
        edge_pos: f64,
    },
    /// Cursor left back through the shared edge; Focus returns.
    Leave {
        edge_pos: f64,
    },
    /// Relative cursor motion while the cursor is remote.
    MouseMove {
        dx: f64,
        dy: f64,
    },
    MouseButton {
        button: MouseButton,
        down: bool,
    },
    Wheel {
        dx: f64,
        dy: f64,
    },
    /// Key by physical USB HID usage code.
    Key {
        hid_usage: u16,
        down: bool,
    },
    /// Release every held key/button (sent on every Focus change).
    ReleaseAll,
    /// Claim Source with a Lamport sequence number.
    ClaimSource {
        seq: u64,
    },
}

/// Actions the Crossing engine asks the [`Platform`](crate::platform::Platform)
/// to perform.
#[derive(Debug, Clone, PartialEq)]
pub enum CrossingAction {
    /// Suppress local input: this Peer is no longer focused.
    SuppressLocalInput,
    /// Re-enable local input: Focus is back home.
    UnsuppressLocalInput,
    /// Move the local cursor, in logical points.
    WarpCursor { x: f64, y: f64 },
    /// Inject a key from the Source while Sink.
    InjectKey { hid_usage: u16, down: bool },
    /// Inject relative motion from the Source while Sink.
    InjectMouseMove { dx: f64, dy: f64 },
    /// Inject a mouse button from the Source while Sink.
    InjectMouseButton { button: MouseButton, down: bool },
    /// Inject a wheel event from the Source while Sink.
    InjectWheel { dx: f64, dy: f64 },
    /// Release locally held keys/buttons (stuck-key safety).
    ReleaseLocal {
        keys: Vec<u16>,
        buttons: Vec<MouseButton>,
    },
}

/// Everything one engine step emits: frames for the peer, actions for the
/// local [`Platform`](crate::platform::Platform).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EngineOutput {
    pub frames: Vec<CrossingFrame>,
    pub actions: Vec<CrossingAction>,
}

impl EngineOutput {
    /// No frames and no actions.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty() && self.actions.is_empty()
    }
}

fn track<T: PartialEq>(held: &mut Vec<T>, value: T, down: bool) {
    if down {
        if !held.contains(&value) {
            held.push(value);
        }
    } else {
        held.retain(|held| *held != value);
    }
}

/// The Crossing engine. See the module docs for the rules.
pub struct CrossingEngine {
    local: Desktop,
    peer_width: f64,
    peer_height: f64,
    arrangement: Arrangement,
    focus: Focus,
    is_source: bool,
    seq: u64,
    own_key: [u8; 32],
    peer_key: [u8; 32],
    cursor: (f64, f64),
    exit_point: (f64, f64),
    remote_active: bool,
    remote_cursor: (f64, f64),
    held_keys: Vec<u16>,
    held_buttons: Vec<MouseButton>,
}

impl CrossingEngine {
    /// One Peer with `local` Desktop bounds, a peer Desktop of
    /// `peer_size`, and this Peer's [`Arrangement`]. `own_key`/`peer_key`
    /// are the static keys for Source-arbitration tie-breaks. Both Peers
    /// start as non-Source; the first physical input claims it.
    pub fn new(
        local: Desktop,
        peer_size: (f64, f64),
        arrangement: Arrangement,
        own_key: [u8; 32],
        peer_key: [u8; 32],
    ) -> Self {
        let home = (local.x + local.width / 2.0, local.y + local.height / 2.0);
        Self {
            local,
            peer_width: peer_size.0,
            peer_height: peer_size.1,
            arrangement,
            focus: Focus::Local,
            is_source: false,
            seq: 0,
            own_key,
            peer_key,
            cursor: home,
            exit_point: home,
            remote_active: false,
            remote_cursor: home,
            held_keys: Vec::new(),
            held_buttons: Vec::new(),
        }
    }

    /// Which Peer currently receives input.
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// Whether this Peer is the Source (its hardware drives input).
    pub fn is_source(&self) -> bool {
        self.is_source
    }

    /// Peer Desktop placement in local coordinates: `(x, y, width, height)`.
    fn peer_rect(&self) -> (f64, f64, f64, f64) {
        match self.arrangement.side {
            Side::Right => (
                self.local.x + self.local.width,
                self.local.y + self.arrangement.offset,
                self.peer_width,
                self.peer_height,
            ),
            Side::Left => (
                self.local.x - self.peer_width,
                self.local.y + self.arrangement.offset,
                self.peer_width,
                self.peer_height,
            ),
            Side::Bottom => (
                self.local.x + self.arrangement.offset,
                self.local.y + self.local.height,
                self.peer_width,
                self.peer_height,
            ),
            Side::Top => (
                self.local.x + self.arrangement.offset,
                self.local.y - self.peer_height,
                self.peer_width,
                self.peer_height,
            ),
        }
    }

    /// Overlap of the shared edge along the edge axis, in local coordinates.
    /// `None` when the Desktops do not overlap along the shared edge.
    fn shared_span(&self) -> Option<(f64, f64)> {
        let (peer_x, peer_y, peer_w, peer_h) = self.peer_rect();
        let (start, end) = match self.arrangement.side {
            Side::Right | Side::Left => (
                self.local.y.max(peer_y),
                (self.local.y + self.local.height).min(peer_y + peer_h),
            ),
            Side::Top | Side::Bottom => (
                self.local.x.max(peer_x),
                (self.local.x + self.local.width).min(peer_x + peer_w),
            ),
        };
        if end > start {
            Some((start, end))
        } else {
            None
        }
    }

    /// Exact point on the shared edge at proportional `pos` (`0..=1`,
    /// clamped), in local coordinates. `None` without an overlap span.
    fn edge_point(&self, pos: f64) -> Option<(f64, f64)> {
        let (start, end) = self.shared_span()?;
        let axis = start + pos.clamp(0.0, 1.0) * (end - start);
        Some(match self.arrangement.side {
            Side::Right => (self.local.x + self.local.width, axis),
            Side::Left => (self.local.x, axis),
            Side::Bottom => (axis, self.local.y + self.local.height),
            Side::Top => (axis, self.local.y),
        })
    }

    /// Point just inside the shared edge at proportional `pos`: where an
    /// entering cursor lands so the next relative motion is tracked, not
    /// instantly exited.
    fn entry_point(&self, pos: f64) -> Option<(f64, f64)> {
        let (x, y) = self.edge_point(pos)?;
        Some(match self.arrangement.side {
            Side::Right => (x - ENTRY_NUDGE, y),
            Side::Left => (x + ENTRY_NUDGE, y),
            Side::Bottom => (x, y - ENTRY_NUDGE),
            Side::Top => (x, y + ENTRY_NUDGE),
        })
    }

    /// If `(x, y)` crosses the shared edge inside the overlap span, the
    /// proportional position plus the exact edge point.
    fn edge_crossing(&self, x: f64, y: f64) -> Option<(f64, (f64, f64))> {
        let (start, end) = self.shared_span()?;
        let span_len = end - start;
        if span_len <= 0.0 {
            return None;
        }
        let (beyond, axis) = match self.arrangement.side {
            Side::Right => (x >= self.local.x + self.local.width, y),
            Side::Left => (x <= self.local.x, y),
            Side::Bottom => (y >= self.local.y + self.local.height, x),
            Side::Top => (y <= self.local.y, x),
        };
        if !beyond || axis < start || axis > end {
            return None;
        }
        let edge_pos = (axis - start) / span_len;
        Some((edge_pos, self.edge_point(edge_pos)?))
    }

    fn clamp_cursor(&self, x: f64, y: f64) -> (f64, f64) {
        (
            x.clamp(self.local.x, self.local.x + self.local.width),
            y.clamp(self.local.y, self.local.y + self.local.height),
        )
    }

    fn take_held(&mut self) -> (Vec<u16>, Vec<MouseButton>) {
        (
            std::mem::take(&mut self.held_keys),
            std::mem::take(&mut self.held_buttons),
        )
    }

    /// Change Focus, releasing everything held on both sides first.
    fn set_focus(&mut self, focus: Focus, out: &mut EngineOutput) {
        if self.focus == focus {
            return;
        }
        self.focus = focus;
        out.frames.push(CrossingFrame::ReleaseAll);
        let (keys, buttons) = self.take_held();
        out.actions
            .push(CrossingAction::ReleaseLocal { keys, buttons });
    }

    /// Physical input on a Peer that is not Source claims Source.
    fn claim_source(&mut self, out: &mut EngineOutput) {
        if self.is_source {
            return;
        }
        self.seq += 1;
        self.is_source = true;
        out.frames
            .push(CrossingFrame::ClaimSource { seq: self.seq });
    }

    /// Come home: warp to the pre-cross cursor, un-suppress, Focus local.
    fn come_home(&mut self, out: &mut EngineOutput) {
        let (x, y) = self.exit_point;
        out.actions.push(CrossingAction::WarpCursor { x, y });
        out.actions.push(CrossingAction::UnsuppressLocalInput);
        self.remote_active = false;
        self.set_focus(Focus::Local, out);
    }

    fn escape_chord_held(&self) -> bool {
        self.held_keys.contains(&HID_ESC)
            && HID_CTRLS.iter().any(|key| self.held_keys.contains(key))
            && HID_SHIFTS.iter().any(|key| self.held_keys.contains(key))
            && HID_ALTS.iter().any(|key| self.held_keys.contains(key))
    }

    /// Absolute local cursor movement: cross out, or stream relative motion.
    pub fn note_local_cursor(&mut self, x: f64, y: f64) -> EngineOutput {
        let mut out = EngineOutput::default();
        self.claim_source(&mut out);
        match self.focus {
            Focus::Local => {
                if let Some((edge_pos, point)) = self.edge_crossing(x, y) {
                    self.exit_point = point;
                    self.cursor = point;
                    self.set_focus(Focus::Remote, &mut out);
                    out.actions.push(CrossingAction::SuppressLocalInput);
                    out.frames.push(CrossingFrame::Enter { edge_pos });
                } else {
                    self.cursor = self.clamp_cursor(x, y);
                }
            }
            Focus::Remote => {
                let dx = x - self.cursor.0;
                let dy = y - self.cursor.1;
                self.cursor = (x, y);
                if dx != 0.0 || dy != 0.0 {
                    out.frames.push(CrossingFrame::MouseMove { dx, dy });
                }
            }
        }
        out
    }

    /// Local physical key event. Completing the escape chord while remote
    /// consumes the key and comes home instead of forwarding it.
    pub fn note_local_key(&mut self, hid_usage: u16, down: bool) -> EngineOutput {
        let mut out = EngineOutput::default();
        self.claim_source(&mut out);
        track(&mut self.held_keys, hid_usage, down);
        if self.focus == Focus::Remote {
            if down && self.escape_chord_held() {
                self.come_home(&mut out);
            } else {
                out.frames.push(CrossingFrame::Key { hid_usage, down });
            }
        }
        out
    }

    /// Local physical mouse button event.
    pub fn note_local_button(&mut self, button: MouseButton, down: bool) -> EngineOutput {
        let mut out = EngineOutput::default();
        self.claim_source(&mut out);
        track(&mut self.held_buttons, button, down);
        if self.focus == Focus::Remote {
            out.frames.push(CrossingFrame::MouseButton { button, down });
        }
        out
    }

    /// Local physical wheel event.
    pub fn note_local_wheel(&mut self, dx: f64, dy: f64) -> EngineOutput {
        let mut out = EngineOutput::default();
        self.claim_source(&mut out);
        if self.focus == Focus::Remote {
            out.frames.push(CrossingFrame::Wheel { dx, dy });
        }
        out
    }

    /// One frame received from the other Peer.
    pub fn receive(&mut self, frame: CrossingFrame) -> EngineOutput {
        let mut out = EngineOutput::default();
        match frame {
            CrossingFrame::Enter { edge_pos } => {
                // The peer's cursor arrives: warp onto the shared edge,
                // release anything held locally, become the Sink.
                if let Some(point) = self.entry_point(edge_pos) {
                    self.remote_cursor = point;
                    self.remote_active = true;
                    if self.focus == Focus::Remote {
                        out.actions.push(CrossingAction::UnsuppressLocalInput);
                    }
                    self.focus = Focus::Local;
                    let (keys, buttons) = self.take_held();
                    if !keys.is_empty() || !buttons.is_empty() {
                        out.actions
                            .push(CrossingAction::ReleaseLocal { keys, buttons });
                    }
                    out.actions.push(CrossingAction::WarpCursor {
                        x: point.0,
                        y: point.1,
                    });
                }
            }
            CrossingFrame::Leave { edge_pos } => {
                // The cursor comes back through the shared edge.
                if let Some(point) = self.entry_point(edge_pos) {
                    self.cursor = point;
                    self.exit_point = point;
                }
                self.come_home(&mut out);
            }
            CrossingFrame::MouseMove { dx, dy } => {
                if self.remote_active {
                    let x = self.remote_cursor.0 + dx;
                    let y = self.remote_cursor.1 + dy;
                    out.actions.push(CrossingAction::InjectMouseMove { dx, dy });
                    if let Some((edge_pos, _)) = self.edge_crossing(x, y) {
                        // Out back through the shared edge: Focus returns.
                        self.remote_active = false;
                        self.set_focus(Focus::Remote, &mut out);
                        out.frames.push(CrossingFrame::Leave { edge_pos });
                    } else {
                        self.remote_cursor = self.clamp_cursor(x, y);
                    }
                }
            }
            CrossingFrame::Key { hid_usage, down } => {
                if self.remote_active {
                    track(&mut self.held_keys, hid_usage, down);
                    out.actions
                        .push(CrossingAction::InjectKey { hid_usage, down });
                }
            }
            CrossingFrame::MouseButton { button, down } => {
                if self.remote_active {
                    track(&mut self.held_buttons, button, down);
                    out.actions
                        .push(CrossingAction::InjectMouseButton { button, down });
                }
            }
            CrossingFrame::Wheel { dx, dy } => {
                if self.remote_active {
                    out.actions.push(CrossingAction::InjectWheel { dx, dy });
                }
            }
            CrossingFrame::ReleaseAll => {
                let (keys, buttons) = self.take_held();
                if !keys.is_empty() || !buttons.is_empty() {
                    out.actions
                        .push(CrossingAction::ReleaseLocal { keys, buttons });
                }
            }
            CrossingFrame::ClaimSource { seq } => {
                let beats = seq > self.seq || (seq == self.seq && self.peer_key < self.own_key);
                self.seq = self.seq.max(seq) + 1;
                if beats && self.is_source {
                    self.is_source = false;
                    out.frames.push(CrossingFrame::ReleaseAll);
                    let (keys, buttons) = self.take_held();
                    out.actions
                        .push(CrossingAction::ReleaseLocal { keys, buttons });
                }
            }
        }
        out
    }

    /// The Link dropped. While remote, warp back and un-suppress; no frames
    /// are emitted because the Link is down.
    pub fn peer_disconnected(&mut self) -> EngineOutput {
        let mut out = EngineOutput::default();
        self.remote_active = false;
        if self.focus == Focus::Remote {
            let (x, y) = self.exit_point;
            out.actions.push(CrossingAction::WarpCursor { x, y });
            out.actions.push(CrossingAction::UnsuppressLocalInput);
            self.focus = Focus::Local;
        }
        let (keys, buttons) = self.take_held();
        if !keys.is_empty() || !buttons.is_empty() {
            out.actions
                .push(CrossingAction::ReleaseLocal { keys, buttons });
        }
        out
    }

    /// The escape hatch: return cursor and keyboard locally now.
    pub fn escape(&mut self) -> EngineOutput {
        let mut out = EngineOutput::default();
        if self.focus == Focus::Remote {
            self.come_home(&mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: u16 = 0xE1;

    fn engine(side: Side, offset: f64) -> CrossingEngine {
        CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 1080.0),
            Arrangement::with_offset(side, offset),
            [0x0A; 32],
            [0x0B; 32],
        )
    }

    fn approx(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn cross_out(engine: &mut CrossingEngine, x: f64, y: f64) -> EngineOutput {
        engine.note_local_cursor(x, y)
    }

    fn enter_pos(out: &EngineOutput) -> f64 {
        out.frames
            .iter()
            .find_map(|frame| match frame {
                CrossingFrame::Enter { edge_pos } => Some(*edge_pos),
                _ => None,
            })
            .expect("output must contain Enter")
    }

    fn leave_pos(out: &EngineOutput) -> f64 {
        out.frames
            .iter()
            .find_map(|frame| match frame {
                CrossingFrame::Leave { edge_pos } => Some(*edge_pos),
                _ => None,
            })
            .expect("output must contain Leave")
    }

    #[test]
    fn crosses_all_four_sides_with_offsets() {
        // Right, aligned: full-height span.
        let mut right = engine(Side::Right, 0.0);
        let out = cross_out(&mut right, 1925.0, 500.0);
        assert_eq!(right.focus(), Focus::Remote);
        assert!(out.actions.contains(&CrossingAction::SuppressLocalInput));
        approx(enter_pos(&out), 500.0 / 1080.0);

        // Left, aligned.
        let mut left = engine(Side::Left, 0.0);
        let out = cross_out(&mut left, -5.0, 540.0);
        assert_eq!(left.focus(), Focus::Remote);
        approx(enter_pos(&out), 0.5);

        // Top with a +100 offset and a smaller peer: span x in [100, 900].
        let mut top = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (800.0, 600.0),
            Arrangement::with_offset(Side::Top, 100.0),
            [0x0A; 32],
            [0x0B; 32],
        );
        let out = cross_out(&mut top, 500.0, -5.0);
        assert_eq!(top.focus(), Focus::Remote);
        approx(enter_pos(&out), (500.0 - 100.0) / 800.0);

        // Bottom with a -200 offset: span x in [0, 800].
        let mut bottom = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1000.0, 800.0),
            Arrangement::with_offset(Side::Bottom, -200.0),
            [0x0A; 32],
            [0x0B; 32],
        );
        let out = cross_out(&mut bottom, 400.0, 1085.0);
        assert_eq!(bottom.focus(), Focus::Remote);
        approx(enter_pos(&out), 0.5);
    }

    #[test]
    fn no_crossing_outside_overlap_span() {
        // Peer only 400 tall: span y in [0, 400]; y=900 is outside it.
        let mut engine = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 400.0),
            Arrangement::new(Side::Right),
            [0x0A; 32],
            [0x0B; 32],
        );
        let out = cross_out(&mut engine, 1925.0, 900.0);
        assert!(
            !out.frames.iter().any(|frame| matches!(
                frame,
                CrossingFrame::Enter { .. } | CrossingFrame::Leave { .. }
            )),
            "outside the span: no crossing"
        );
        assert_eq!(engine.focus(), Focus::Local);
        assert_eq!(engine.cursor, (1920.0, 900.0));
    }

    #[test]
    fn negative_origin_multimonitor_desktop_crosses() {
        // Bounding box spanning two monitors, with a negative origin.
        let mut engine = CrossingEngine::new(
            Desktop::new(-1920.0, -200.0, 3840.0, 1280.0),
            (1920.0, 1080.0),
            Arrangement::new(Side::Right),
            [0x0A; 32],
            [0x0B; 32],
        );
        let out = cross_out(&mut engine, 1925.0, 300.0);
        assert_eq!(engine.focus(), Focus::Remote);
        // Span y in [-200, 880]: (300 + 200) / 1080.
        approx(enter_pos(&out), 500.0 / 1080.0);
    }

    #[test]
    fn stuck_keys_released_on_crossing() {
        let mut engine = engine(Side::Right, 0.0);
        // First physical input claims Source ...
        let claim = engine.note_local_key(SHIFT, true);
        assert_eq!(claim.frames, vec![CrossingFrame::ClaimSource { seq: 1 }]);
        let out = cross_out(&mut engine, 1925.0, 540.0);
        // The first physical input claims Source; then the peer gets
        // ReleaseAll before Enter ...
        assert_eq!(
            out.frames,
            vec![
                CrossingFrame::ReleaseAll,
                CrossingFrame::Enter { edge_pos: 0.5 }
            ]
        );
        // ... and local suppress releases Shift too.
        assert!(out.actions.contains(&CrossingAction::SuppressLocalInput));
        assert!(out.actions.contains(&CrossingAction::ReleaseLocal {
            keys: vec![SHIFT],
            buttons: vec![],
        }));
    }

    #[test]
    fn disconnect_while_remote_warps_back_and_unsuppresses() {
        let mut engine = engine(Side::Right, 0.0);
        cross_out(&mut engine, 1925.0, 540.0);
        assert_eq!(engine.focus(), Focus::Remote);
        let out = engine.peer_disconnected();
        assert!(out.frames.is_empty(), "the Link is down: no frames");
        assert_eq!(
            out.actions,
            vec![
                CrossingAction::WarpCursor {
                    x: 1920.0,
                    y: 540.0
                },
                CrossingAction::UnsuppressLocalInput,
            ]
        );
        assert_eq!(engine.focus(), Focus::Local);

        // Disconnect while local and empty-handed is a no-op.
        assert!(engine.peer_disconnected().is_empty());
    }

    #[test]
    fn simultaneous_claims_resolve_identically() {
        // Lower static key wins ties: A holds 0x0A, B holds 0x0B.
        let mut a = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 1080.0),
            Arrangement::new(Side::Right),
            [0x0A; 32],
            [0x0B; 32],
        );
        let mut b = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 1080.0),
            Arrangement::new(Side::Left),
            [0x0B; 32],
            [0x0A; 32],
        );
        // Simultaneous physical input: both claim seq 1.
        let out_a = a.note_local_cursor(100.0, 100.0);
        let out_b = b.note_local_cursor(200.0, 200.0);
        assert!(out_a
            .frames
            .contains(&CrossingFrame::ClaimSource { seq: 1 }));
        assert!(out_b
            .frames
            .contains(&CrossingFrame::ClaimSource { seq: 1 }));
        // Exchange the claims: both sides agree A is Source.
        let _ = a.receive(CrossingFrame::ClaimSource { seq: 1 });
        let out_b = b.receive(CrossingFrame::ClaimSource { seq: 1 });
        assert!(a.is_source(), "lower key keeps Source");
        assert!(!b.is_source(), "higher key yields");
        assert!(out_b.frames.contains(&CrossingFrame::ReleaseAll));

        // A strictly higher sequence always wins, whatever the keys.
        let mut c = engine(Side::Right, 0.0);
        c.note_local_key(SHIFT, true);
        let out = c.receive(CrossingFrame::ClaimSource { seq: 5 });
        assert!(!c.is_source(), "higher sequence yields");
        assert!(out.frames.contains(&CrossingFrame::ReleaseAll));
        assert!(out.actions.contains(&CrossingAction::ReleaseLocal {
            keys: vec![SHIFT],
            buttons: vec![],
        }));
    }

    #[test]
    fn escape_chord_restores_local_focus() {
        let mut engine = engine(Side::Right, 0.0);
        cross_out(&mut engine, 1925.0, 540.0);
        assert_eq!(engine.focus(), Focus::Remote);
        // Ctrl, Shift, Alt forward normally while remote ...
        assert!(engine
            .note_local_key(0xE0, true)
            .frames
            .contains(&CrossingFrame::Key {
                hid_usage: 0xE0,
                down: true
            }));
        assert!(engine
            .note_local_key(SHIFT, true)
            .frames
            .contains(&CrossingFrame::Key {
                hid_usage: SHIFT,
                down: true
            }));
        assert!(engine
            .note_local_key(0xE2, true)
            .frames
            .contains(&CrossingFrame::Key {
                hid_usage: 0xE2,
                down: true
            }));
        // ... but Esc completing Ctrl+Alt+Shift+Esc comes home instead.
        let out = engine.note_local_key(HID_ESC, true);
        assert!(
            !out.frames.iter().any(|frame| matches!(
                frame,
                CrossingFrame::Key {
                    hid_usage: HID_ESC,
                    ..
                }
            )),
            "the chord Esc is consumed, not forwarded"
        );
        assert!(out.frames.contains(&CrossingFrame::ReleaseAll));
        assert_eq!(
            out.actions,
            vec![
                CrossingAction::WarpCursor {
                    x: 1920.0,
                    y: 540.0
                },
                CrossingAction::UnsuppressLocalInput,
                CrossingAction::ReleaseLocal {
                    keys: vec![0xE0, SHIFT, 0xE2, HID_ESC],
                    buttons: vec![],
                },
            ]
        );
        assert_eq!(engine.focus(), Focus::Local);

        // Already home: the hatch is a no-op.
        assert!(engine.escape().is_empty());
    }

    #[test]
    fn sink_tracks_remote_cursor_and_leaves_through_shared_edge() {
        let mut engine = engine(Side::Right, 0.0);
        // Enter lands just inside the shared edge.
        let out = engine.receive(CrossingFrame::Enter { edge_pos: 0.5 });
        assert_eq!(
            out.actions,
            vec![CrossingAction::WarpCursor {
                x: 1919.0,
                y: 540.0
            }]
        );
        // Relative motion injects and tracks ...
        let out = engine.receive(CrossingFrame::MouseMove {
            dx: -100.0,
            dy: 0.0,
        });
        assert_eq!(
            out.actions,
            vec![CrossingAction::InjectMouseMove {
                dx: -100.0,
                dy: 0.0
            }]
        );
        assert!(out.frames.is_empty());
        // ... until the cursor exits back through the shared edge.
        let out = engine.receive(CrossingFrame::MouseMove { dx: 200.0, dy: 0.0 });
        assert_eq!(engine.focus(), Focus::Remote);
        assert!(out.frames.contains(&CrossingFrame::ReleaseAll));
        approx(leave_pos(&out), 0.5);
    }

    #[test]
    fn sink_exit_outside_span_is_clamped_not_left() {
        let mut engine = CrossingEngine::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 400.0),
            Arrangement::new(Side::Right),
            [0x0A; 32],
            [0x0B; 32],
        );
        let _ = engine.receive(CrossingFrame::Enter { edge_pos: 0.5 });
        // Past the edge but outside span y [0, 400]: clamped, no Leave.
        let out = engine.receive(CrossingFrame::MouseMove {
            dx: 100.0,
            dy: 300.0,
        });
        assert!(
            !out.frames
                .iter()
                .any(|frame| matches!(frame, CrossingFrame::Leave { .. })),
            "outside the span: no Leave"
        );
        assert_eq!(engine.focus(), Focus::Local);
    }

    #[test]
    fn release_all_received_releases_locally() {
        let mut engine = engine(Side::Right, 0.0);
        let _ = engine.receive(CrossingFrame::Enter { edge_pos: 0.0 });
        let out = engine.receive(CrossingFrame::Key {
            hid_usage: 0x04,
            down: true,
        });
        assert!(out.actions.contains(&CrossingAction::InjectKey {
            hid_usage: 0x04,
            down: true
        }));
        let out = engine.receive(CrossingFrame::ReleaseAll);
        assert_eq!(
            out.actions,
            vec![CrossingAction::ReleaseLocal {
                keys: vec![0x04],
                buttons: vec![],
            }]
        );
        // Drained: a second ReleaseAll is silent.
        assert!(engine.receive(CrossingFrame::ReleaseAll).is_empty());
    }

    #[test]
    fn leave_received_returns_home() {
        let mut engine = engine(Side::Right, 0.0);
        cross_out(&mut engine, 1925.0, 540.0);
        let out = engine.receive(CrossingFrame::Leave { edge_pos: 0.25 });
        assert_eq!(engine.focus(), Focus::Local);
        assert_eq!(out.frames, vec![CrossingFrame::ReleaseAll]);
        assert_eq!(
            out.actions,
            vec![
                CrossingAction::WarpCursor {
                    x: 1919.0,
                    y: 270.0
                },
                CrossingAction::UnsuppressLocalInput,
                CrossingAction::ReleaseLocal {
                    keys: vec![],
                    buttons: vec![],
                },
            ]
        );
    }
}
