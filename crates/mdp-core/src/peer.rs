//! One Peer's wiring: Crossing engine <-> Link frames, and the drive loop.
//!
//! [`to_wire`]/[`from_wire`] map the engine's [`CrossingFrame`]s 1:1 onto
//! the [`Frame`]s the [`Link`] carries. The other handshake frames never
//! reach the engine: the [`Peer`] answers Ping, timestamps Pong for the
//! tray latency, and consumes the rest.
//!
//! Clipboard text sync lives here too, so `mdp run` and the app share it:
//! the [`Peer`] polls the local clipboard, sends changed UTF-8 text (at
//! most 1 MiB, never empty) when sharing is on, and on receive sets the
//! local clipboard while remembering the value so it is never echoed back.
//!
//! [`mirror_wire`] adopts a received Arrangement with the same semantics as
//! `ui::arrange::mirror`: the side flips, the offset negates.
//!
//! Frozen-cursor rule: capture keeps reporting while suppressed, with the
//! real cursor frozen at the shared edge. The [`Peer`] feeds those reports
//! to the engine RAW, never pre-clamped: the engine anchors its cursor at
//! the exit edge point (where the OS cursor froze) and diffs raw positions,
//! so every forwarded delta is `reported − frozen`. Pre-clamping would
//! collapse deltas to zero, and warping back per move would corrupt the
//! engine's diffs, so the local cursor is left alone while remote.

use crate::crossing::{CrossingAction, CrossingEngine, CrossingFrame, EngineOutput, Focus, Side};
use crate::link::{Link, LinkError};
use crate::platform::{Desktop, InputEvent, Platform, PlatformError, CLIPBOARD_MAX_BYTES};
use crate::proto::{ArrangementSide, Frame, PROTOCOL_VERSION};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

/// Ping interval: keepalive plus fresh round-trip samples for the tray.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// Local clipboard poll interval.
const CLIPBOARD_POLL: Duration = Duration::from_millis(500);
/// Shutdown poll while the capture bridge idles.
const BRIDGE_POLL: Duration = Duration::from_millis(50);

/// Whether a clipboard read may be sent: sharing is on, the text is
/// non-empty UTF-8 within the 1 MiB cap, and it differs from what was last
/// sent or received (the echo guard).
fn clipboard_sendable(share: bool, last: &Option<String>, current: &str) -> bool {
    share
        && !current.is_empty()
        && current.len() <= CLIPBOARD_MAX_BYTES
        && last.as_deref() != Some(current)
}

/// Engine frame -> wire frame. The engine only emits crossing frames.
pub fn to_wire(frame: &CrossingFrame) -> Frame {
    match frame {
        CrossingFrame::Enter { edge_pos } => Frame::Enter {
            edge_pos: *edge_pos,
        },
        CrossingFrame::Leave { edge_pos } => Frame::Leave {
            edge_pos: *edge_pos,
        },
        CrossingFrame::MouseMove { dx, dy } => Frame::MouseMove { dx: *dx, dy: *dy },
        CrossingFrame::MouseButton { button, down } => Frame::MouseButton {
            button: *button,
            down: *down,
        },
        CrossingFrame::Wheel { dx, dy } => Frame::Wheel { dx: *dx, dy: *dy },
        CrossingFrame::Key { hid_usage, down } => Frame::Key {
            hid_usage: *hid_usage,
            down: *down,
        },
        CrossingFrame::ReleaseAll => Frame::ReleaseAll,
        CrossingFrame::ClaimSource { seq } => Frame::ClaimSource { seq: *seq },
    }
}

/// Wire frame -> engine frame. Hello, Arrangement, Clipboard, and Ping/Pong
/// return [`None`]; the [`Peer`] handles those itself.
pub fn from_wire(frame: &Frame) -> Option<CrossingFrame> {
    match frame {
        Frame::Enter { edge_pos } => Some(CrossingFrame::Enter {
            edge_pos: *edge_pos,
        }),
        Frame::Leave { edge_pos } => Some(CrossingFrame::Leave {
            edge_pos: *edge_pos,
        }),
        Frame::MouseMove { dx, dy } => Some(CrossingFrame::MouseMove { dx: *dx, dy: *dy }),
        Frame::MouseButton { button, down } => Some(CrossingFrame::MouseButton {
            button: *button,
            down: *down,
        }),
        Frame::Wheel { dx, dy } => Some(CrossingFrame::Wheel { dx: *dx, dy: *dy }),
        Frame::Key { hid_usage, down } => Some(CrossingFrame::Key {
            hid_usage: *hid_usage,
            down: *down,
        }),
        Frame::ReleaseAll => Some(CrossingFrame::ReleaseAll),
        Frame::ClaimSource { seq } => Some(CrossingFrame::ClaimSource { seq: *seq }),
        Frame::Hello { .. }
        | Frame::Arrangement { .. }
        | Frame::Clipboard { .. }
        | Frame::Ping { .. }
        | Frame::Pong { .. } => None,
    }
}

/// Adopt a received Arrangement for the local engine: mirror the side and
/// negate the offset (same semantics as `ui::arrange::mirror`).
pub fn mirror_wire(side: ArrangementSide, offset: f64) -> (ArrangementSide, f64) {
    (side.mirror(), -offset)
}

/// Engine side -> wire side (both cover all four sides).
pub fn side_to_wire(side: Side) -> ArrangementSide {
    match side {
        Side::Left => ArrangementSide::Left,
        Side::Right => ArrangementSide::Right,
        Side::Top => ArrangementSide::Top,
        Side::Bottom => ArrangementSide::Bottom,
    }
}

/// Wire side -> engine side (both cover all four sides).
pub fn side_from_wire(side: ArrangementSide) -> Side {
    match side {
        ArrangementSide::Left => Side::Left,
        ArrangementSide::Right => Side::Right,
        ArrangementSide::Top => Side::Top,
        ArrangementSide::Bottom => Side::Bottom,
    }
}

/// Peer failures: the drive loop ends on the first one.
#[derive(Debug)]
pub enum PeerError {
    Link(LinkError),
    Platform(PlatformError),
    Version {
        expected: u32,
        got: u32,
    },
    Handshake(String),
    /// A new Arrangement was saved on either Peer: the session ends so both
    /// sides reconnect and agree on it in the Hello. `side`/`offset` are the
    /// *sender's* view (adopt with [`mirror_wire`]); `None` when this side
    /// sent it.
    ArrangementChanged(Option<(ArrangementSide, f64)>),
    /// The app dropped the session's outbox sender (unpair, pause, quit).
    Closed,
}

/// Live state of a driving Peer, for the tray and header pills.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PeerStatus {
    pub focus_here: bool,
    pub rtt_ms: Option<u64>,
}

impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Link(err) => write!(f, "peer link failed: {err}"),
            Self::Platform(err) => write!(f, "peer platform failed: {err}"),
            Self::Version { expected, got } => {
                write!(
                    f,
                    "peer protocol mismatch: expected v{expected}, got v{got}"
                )
            }
            Self::Handshake(message) => write!(f, "peer handshake failed: {message}"),
            Self::ArrangementChanged(_) => write!(f, "arrangement changed; reconnecting"),
            Self::Closed => write!(f, "session closed by the app"),
        }
    }
}

impl std::error::Error for PeerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Link(err) => Some(err),
            Self::Platform(err) => Some(err),
            _ => None,
        }
    }
}

impl From<LinkError> for PeerError {
    fn from(err: LinkError) -> Self {
        Self::Link(err)
    }
}

impl From<PlatformError> for PeerError {
    fn from(err: PlatformError) -> Self {
        Self::Platform(err)
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Exchange Hello + Arrangement: send ours, then read theirs. Fails on a
/// protocol version mismatch or out-of-order frames.
pub async fn exchange_hello(
    link: &mut Link,
    desktop: &Desktop,
    side: ArrangementSide,
    offset: f64,
) -> Result<(Desktop, (ArrangementSide, f64)), PeerError> {
    link.send_frame(&Frame::Hello {
        desktop: *desktop,
        version: PROTOCOL_VERSION,
    })
    .await?;
    link.send_frame(&Frame::Arrangement { side, offset })
        .await?;
    let peer_desktop = match link.recv_frame().await? {
        Frame::Hello { desktop, version } => {
            if version != PROTOCOL_VERSION {
                return Err(PeerError::Version {
                    expected: PROTOCOL_VERSION,
                    got: version,
                });
            }
            desktop
        }
        other => {
            return Err(PeerError::Handshake(format!(
                "expected Hello, got {other:?}"
            )));
        }
    };
    let peer_arrangement = match link.recv_frame().await? {
        Frame::Arrangement { side, offset } => (side, offset),
        other => {
            return Err(PeerError::Handshake(format!(
                "expected Arrangement, got {other:?}"
            )));
        }
    };
    Ok((peer_desktop, peer_arrangement))
}

fn capture_bridge(
    capture: std::sync::mpsc::Receiver<InputEvent>,
    tx: UnboundedSender<InputEvent>,
    done: Arc<AtomicBool>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    loop {
        match capture.recv_timeout(BRIDGE_POLL) {
            Ok(event) => {
                if tx.send(event).is_err() || done.load(Ordering::SeqCst) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if done.load(Ordering::SeqCst) {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// One live Peer: platform + engine + Link, driven until one of them fails.
pub struct Peer<P> {
    platform: P,
    engine: CrossingEngine,
    link: Link,
    inbox: UnboundedReceiver<InputEvent>,
    done: Arc<AtomicBool>,
    cursor: (f64, f64),
    last_rtt: Option<Duration>,
    share_clipboard: bool,
    last_clipboard: Option<String>,
    last_poll: Instant,
    status: Option<watch::Sender<PeerStatus>>,
    outbox: Option<UnboundedReceiver<Frame>>,
}

impl<P: Platform> Peer<P> {
    /// Wire it up and start the capture bridge (call from async context).
    /// `local` seeds the Sink cursor; capture reports are fed to the engine
    /// raw (see the module docs: frozen-cursor rule).
    pub fn new(
        platform: P,
        link: Link,
        capture: std::sync::mpsc::Receiver<InputEvent>,
        engine: CrossingEngine,
        local: Desktop,
        share_clipboard: bool,
    ) -> Self {
        // Seed the echo guard with what is already copied: only changes made
        // during the session sync, so connecting never clobbers either side.
        let last_clipboard = platform.clipboard_get().ok();
        let (tx, inbox) = unbounded_channel();
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        tokio::task::spawn_blocking(move || capture_bridge(capture, tx, flag));
        Self {
            platform,
            engine,
            link,
            inbox,
            done,
            cursor: (local.x + local.width / 2.0, local.y + local.height / 2.0),
            last_rtt: None,
            share_clipboard,
            last_clipboard,
            last_poll: Instant::now(),
            status: None,
            outbox: None,
        }
    }

    /// Publish [`PeerStatus`] after every step (for the app's tray / pills).
    pub fn with_status(mut self, status: watch::Sender<PeerStatus>) -> Self {
        self.status = Some(status);
        self
    }

    /// Frames the app wants sent mid-session (e.g. a saved Arrangement).
    pub fn with_outbox(mut self, outbox: UnboundedReceiver<Frame>) -> Self {
        self.outbox = Some(outbox);
        self
    }

    fn publish_status(&self) {
        if let Some(status) = &self.status {
            let now = PeerStatus {
                focus_here: self.engine.focus() == Focus::Local,
                rtt_ms: self.last_rtt_ms(),
            };
            status.send_if_modified(|old| std::mem::replace(old, now) != now);
        }
    }

    /// Next app frame (`None` = the app closed the session), or never when
    /// there is no outbox.
    async fn next_outgoing(outbox: &mut Option<UnboundedReceiver<Frame>>) -> Option<Frame> {
        match outbox {
            Some(rx) => rx.recv().await,
            None => std::future::pending().await,
        }
    }

    /// Which Peer currently receives input.
    pub fn focus(&self) -> Focus {
        self.engine.focus()
    }

    /// Whether this Peer is the Source.
    pub fn is_source(&self) -> bool {
        self.engine.is_source()
    }

    /// Last measured Ping/Pong round trip, if any.
    pub fn last_rtt(&self) -> Option<Duration> {
        self.last_rtt
    }

    /// Last round trip in whole milliseconds, for the tray status.
    pub fn last_rtt_ms(&self) -> Option<u64> {
        self.last_rtt.map(|rtt| rtt.as_millis() as u64)
    }

    /// The platform, to feed physical events in tests.
    pub fn platform(&self) -> &P {
        &self.platform
    }

    /// The platform, mutably.
    pub fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }

    /// Process one local event or one Link frame, whichever comes first.
    /// Also runs the clipboard poll when it is due.
    pub async fn drive_step(&mut self) -> Result<(), PeerError> {
        self.poll_clipboard().await?;
        tokio::select! {
            biased;
            event = self.inbox.recv() => {
                if let Some(event) = event {
                    self.note_local(event).await?;
                }
            }
            frame = self.link.recv_frame() => {
                self.note_remote(frame?).await?;
            }
        }
        Ok(())
    }

    /// Drive until the Link or the platform fails, then report why. Runs
    /// the disconnect actions (warp back + un-suppress) best-effort first.
    pub async fn drive(&mut self) -> PeerError {
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut outbox = self.outbox.take();
        loop {
            let step: Result<(), PeerError> = tokio::select! {
                biased;
                _ = heartbeat.tick() => {
                    self.link
                        .send_frame(&Frame::Ping { t: now_millis() })
                        .await
                        .map_err(PeerError::Link)
                }
                frame = Self::next_outgoing(&mut outbox) => match frame {
                    None => Err(PeerError::Closed),
                    Some(frame) => {
                        let saved = matches!(frame, Frame::Arrangement { .. });
                        match self.link.send_frame(&frame).await {
                            Ok(()) if saved => Err(PeerError::ArrangementChanged(None)),
                            Ok(()) => Ok(()),
                            Err(err) => Err(PeerError::Link(err)),
                        }
                    }
                },
                outcome = self.drive_step() => outcome,
            };
            self.publish_status();
            if let Err(err) = step {
                return self.finish(err);
            }
        }
    }

    fn finish(&mut self, cause: PeerError) -> PeerError {
        let out = self.engine.peer_disconnected();
        for action in &out.actions {
            let _ = self.apply(action);
        }
        cause
    }

    async fn note_local(&mut self, event: InputEvent) -> Result<(), PeerError> {
        let out = match event {
            InputEvent::MouseMove { x, y } => self.engine.note_local_cursor(x, y),
            InputEvent::Key { usage_id, pressed } => self.engine.note_local_key(usage_id, pressed),
            InputEvent::MouseButton { button, pressed } => {
                self.engine.note_local_button(button, pressed)
            }
            InputEvent::Scroll { dx, dy } => self.engine.note_local_wheel(dx, dy),
        };
        self.emit(out).await
    }

    /// Poll the local clipboard when due; send changed text within the
    /// cap while sharing is on. Get failures (locked, empty, non-text)
    /// just wait for the next tick.
    async fn poll_clipboard(&mut self) -> Result<(), PeerError> {
        if !self.share_clipboard || self.last_poll.elapsed() < CLIPBOARD_POLL {
            return Ok(());
        }
        self.last_poll = Instant::now();
        let Ok(current) = self.platform.clipboard_get() else {
            return Ok(());
        };
        if !clipboard_sendable(self.share_clipboard, &self.last_clipboard, &current) {
            // Oversize text is still remembered so a stuck clipboard does
            // not retry every tick; the codec would refuse it anyway.
            if current.len() > CLIPBOARD_MAX_BYTES {
                self.last_clipboard = Some(current);
            }
            return Ok(());
        }
        self.link
            .send_frame(&Frame::Clipboard {
                text: current.clone(),
            })
            .await?;
        self.last_clipboard = Some(current);
        Ok(())
    }

    async fn note_remote(&mut self, frame: Frame) -> Result<(), PeerError> {
        match frame {
            Frame::Ping { t } => {
                self.link.send_frame(&Frame::Pong { t }).await?;
            }
            Frame::Pong { t } => {
                self.last_rtt = Some(Duration::from_millis(now_millis().saturating_sub(t)));
            }
            Frame::Arrangement { side, offset } => {
                return Err(PeerError::ArrangementChanged(Some((side, offset))));
            }
            Frame::Hello { .. } => {
                // Late handshake frames after exchange_hello; ignored.
            }
            Frame::Clipboard { text } => {
                // Best effort: a locked clipboard just misses this update;
                // the value is remembered only once it actually sticks, so
                // a failed set cannot desync the echo guard.
                if self.share_clipboard
                    && text.len() <= CLIPBOARD_MAX_BYTES
                    && self.platform.clipboard_set(&text).is_ok()
                {
                    self.last_clipboard = Some(text);
                }
            }
            other => {
                if let Some(crossing) = from_wire(&other) {
                    let out = self.engine.receive(crossing);
                    self.emit(out).await?;
                }
            }
        }
        Ok(())
    }

    async fn emit(&mut self, out: EngineOutput) -> Result<(), PeerError> {
        for frame in &out.frames {
            self.link.send_frame(&to_wire(frame)).await?;
        }
        for action in &out.actions {
            self.apply(action)?;
        }
        Ok(())
    }

    fn apply(&mut self, action: &CrossingAction) -> Result<(), PeerError> {
        match action {
            CrossingAction::SuppressLocalInput => self.platform.suppress_input()?,
            CrossingAction::UnsuppressLocalInput => self.platform.unsuppress_input()?,
            CrossingAction::WarpCursor { x, y } => {
                self.cursor = (*x, *y);
                self.platform.warp_cursor(*x, *y)?;
            }
            CrossingAction::InjectKey { hid_usage, down } => {
                self.platform.inject(InputEvent::Key {
                    usage_id: *hid_usage,
                    pressed: *down,
                })?;
            }
            CrossingAction::InjectMouseMove { dx, dy } => {
                self.cursor.0 += dx;
                self.cursor.1 += dy;
                let (x, y) = self.cursor;
                self.platform.inject(InputEvent::MouseMove { x, y })?;
            }
            CrossingAction::InjectMouseButton { button, down } => {
                self.platform.inject(InputEvent::MouseButton {
                    button: *button,
                    pressed: *down,
                })?;
            }
            CrossingAction::InjectWheel { dx, dy } => {
                self.platform
                    .inject(InputEvent::Scroll { dx: *dx, dy: *dy })?;
            }
            CrossingAction::ReleaseLocal { keys, buttons } => {
                for usage_id in keys {
                    self.platform.inject(InputEvent::Key {
                        usage_id: *usage_id,
                        pressed: false,
                    })?;
                }
                for button in buttons {
                    self.platform.inject(InputEvent::MouseButton {
                        button: *button,
                        pressed: false,
                    })?;
                }
            }
        }
        Ok(())
    }
}

impl<P> Drop for Peer<P> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::MouseButton;

    fn every_crossing_frame() -> Vec<CrossingFrame> {
        vec![
            CrossingFrame::Enter { edge_pos: 0.25 },
            CrossingFrame::Leave { edge_pos: 0.75 },
            CrossingFrame::MouseMove { dx: 3.0, dy: -2.0 },
            CrossingFrame::MouseButton {
                button: MouseButton::Right,
                down: true,
            },
            CrossingFrame::Wheel { dx: 0.0, dy: 1.0 },
            CrossingFrame::Key {
                hid_usage: 0x04,
                down: true,
            },
            CrossingFrame::ReleaseAll,
            CrossingFrame::ClaimSource { seq: 42 },
        ]
    }

    #[test]
    fn wire_mapping_round_trips_all_crossing_frames() {
        for frame in every_crossing_frame() {
            let wire = to_wire(&frame);
            assert_eq!(from_wire(&wire), Some(frame));
        }
    }

    #[test]
    fn non_crossing_frames_have_no_engine_mapping() {
        let others = vec![
            Frame::Hello {
                desktop: Desktop::new(0.0, 0.0, 1920.0, 1080.0),
                version: PROTOCOL_VERSION,
            },
            Frame::Arrangement {
                side: ArrangementSide::Top,
                offset: 0.0,
            },
            Frame::Clipboard {
                text: "hi".to_string(),
            },
            Frame::Ping { t: 1 },
            Frame::Pong { t: 1 },
        ];
        for frame in others {
            assert_eq!(from_wire(&frame), None);
        }
    }

    #[test]
    fn mirror_wire_flips_all_four_sides_and_negates_offset() {
        assert_eq!(
            mirror_wire(ArrangementSide::Left, 100.0),
            (ArrangementSide::Right, -100.0)
        );
        assert_eq!(
            mirror_wire(ArrangementSide::Right, -40.5),
            (ArrangementSide::Left, 40.5)
        );
        assert_eq!(
            mirror_wire(ArrangementSide::Top, 0.0),
            (ArrangementSide::Bottom, -0.0)
        );
        assert_eq!(
            mirror_wire(ArrangementSide::Bottom, 33.25),
            (ArrangementSide::Top, -33.25)
        );
    }

    #[test]
    fn side_conversions_cover_all_four_sides() {
        use crate::crossing::Side;
        for (crossing, wire) in [
            (Side::Left, ArrangementSide::Left),
            (Side::Right, ArrangementSide::Right),
            (Side::Top, ArrangementSide::Top),
            (Side::Bottom, ArrangementSide::Bottom),
        ] {
            assert_eq!(side_to_wire(crossing), wire);
            assert_eq!(side_from_wire(wire), crossing);
        }
    }

    #[test]
    fn clipboard_send_gate() {
        assert!(clipboard_sendable(true, &None, "hello"));
        assert!(clipboard_sendable(
            true,
            &None,
            &"x".repeat(CLIPBOARD_MAX_BYTES)
        ));
        // Echo guard: what was last sent or received never goes out again.
        assert!(!clipboard_sendable(
            true,
            &Some("hello".to_string()),
            "hello"
        ));
        // Sharing off, empty text, and oversize text never send.
        assert!(!clipboard_sendable(false, &None, "hello"));
        assert!(!clipboard_sendable(true, &None, ""));
        assert!(!clipboard_sendable(
            true,
            &None,
            &"x".repeat(CLIPBOARD_MAX_BYTES + 1)
        ));
    }
}
