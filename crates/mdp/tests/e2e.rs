//! Headless end-to-end: two Peers over loopback with FakePlatforms.
//!
//! The FakePlatforms stand in for real Desktops (scripted absolute cursor
//! reports, recorded injections); the Links are real Noise-encrypted TCP.
//! Reports fed while suppressed are raw and unbounded, like the real hooks
//! emit with the cursor frozen — the forwarded deltas must match exactly.

use mdp_core::crossing::{Arrangement, CrossingEngine, Focus, Side};
use mdp_core::link::{Link, StaticKeypair};
use mdp_core::peer::{exchange_hello, mirror_wire, side_to_wire, Peer, PeerError};
use mdp_core::platform::{
    Desktop, FakePlatform, InputEvent, PermissionStatus, Platform, PlatformError,
    CLIPBOARD_MAX_BYTES,
};
use mdp_core::proto::Frame;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

fn desktop() -> Desktop {
    Desktop::new(0.0, 0.0, 1920.0, 1080.0)
}

/// Pair two peers over loopback with mirrored arrangements, exchange Hello
/// both ways, and assert each side received the mirror of its own config.
/// Platforms come from the factories so tests can substitute clipboards.
async fn pair_with<PA, PB>(
    arr_a: Arrangement,
    arr_b: Arrangement,
    share: bool,
    make_a: impl FnOnce(Desktop) -> PA,
    make_b: impl FnOnce(Desktop) -> PB,
) -> (Peer<PA>, Peer<PB>)
where
    PA: Platform,
    PB: Platform,
{
    let key_a = StaticKeypair::generate().unwrap();
    let key_b = StaticKeypair::generate().unwrap();
    let desktop = desktop();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let connect = Link::connect(addr, &key_a, &[], |_| true);
    let accept = async {
        let (stream, _) = listener.accept().await.unwrap();
        Link::accept(stream, &key_b, &[], |_| true).await
    };
    let (connected, accepted) = tokio::join!(connect, accept);
    let (mut link_a, key_for_a) = connected.unwrap();
    let (mut link_b, key_for_b) = accepted.unwrap();
    assert_eq!(key_for_a, key_b.public_key());
    assert_eq!(key_for_b, key_a.public_key());

    let sent_a = (side_to_wire(arr_a.side), arr_a.offset);
    let sent_b = (side_to_wire(arr_b.side), arr_b.offset);
    let hello_a = exchange_hello(&mut link_a, &desktop, sent_a.0, sent_a.1);
    let hello_b = exchange_hello(&mut link_b, &desktop, sent_b.0, sent_b.1);
    let (hello_a, hello_b) = tokio::join!(hello_a, hello_b);
    let (peer_desktop_a, rx_a) = hello_a.unwrap();
    let (peer_desktop_b, rx_b) = hello_b.unwrap();
    assert_eq!(peer_desktop_a, desktop);
    assert_eq!(peer_desktop_b, desktop);
    assert_eq!(rx_a, mirror_wire(sent_a.0, sent_a.1));
    assert_eq!(rx_b, mirror_wire(sent_b.0, sent_b.1));

    let engine_a = CrossingEngine::new(
        desktop,
        (peer_desktop_a.width, peer_desktop_a.height),
        arr_a,
        key_a.public_key(),
        key_for_a,
    );
    let engine_b = CrossingEngine::new(
        desktop,
        (peer_desktop_b.width, peer_desktop_b.height),
        arr_b,
        key_b.public_key(),
        key_for_b,
    );
    let mut plat_a = make_a(desktop);
    let rx_cap_a = plat_a.start_capture().unwrap();
    let mut plat_b = make_b(desktop);
    let rx_cap_b = plat_b.start_capture().unwrap();
    (
        Peer::new(plat_a, link_a, rx_cap_a, engine_a, desktop, share),
        Peer::new(plat_b, link_b, rx_cap_b, engine_b, desktop, share),
    )
}

async fn pair(arr_a: Arrangement, arr_b: Arrangement) -> (Peer<FakePlatform>, Peer<FakePlatform>) {
    pair_with(arr_a, arr_b, true, FakePlatform::new, FakePlatform::new).await
}

fn feed(peer: &mut Peer<FakePlatform>, event: InputEvent) {
    peer.platform_mut()
        .feed_physical_event(event)
        .expect("feed physical event");
}

fn mouse(x: f64, y: f64) -> InputEvent {
    InputEvent::MouseMove { x, y }
}

/// Drive both peers until three consecutive quiet ticks (5 ms each).
/// Returns the first drive error seen (e.g. a dead Link).
async fn settle<PA, PB>(a: &mut Peer<PA>, b: &mut Peer<PB>) -> Option<PeerError>
where
    PA: Platform,
    PB: Platform,
{
    let mut quiet = 0;
    for _ in 0..500 {
        let mut round_quiet = true;
        match tokio::time::timeout(Duration::from_millis(5), a.drive_step()).await {
            Ok(Ok(())) => round_quiet = false,
            Ok(Err(err)) => return Some(err),
            Err(_) => {}
        }
        match tokio::time::timeout(Duration::from_millis(5), b.drive_step()).await {
            Ok(Ok(())) => round_quiet = false,
            Ok(Err(err)) => return Some(err),
            Err(_) => {}
        }
        if round_quiet {
            quiet += 1;
            if quiet >= 3 {
                return None;
            }
        } else {
            quiet = 0;
        }
    }
    panic!("peers did not quiesce");
}

#[tokio::test]
async fn cursor_crosses_right_and_back_with_remote_only_injection() {
    let (mut a, mut b) = pair(Arrangement::new(Side::Right), Arrangement::new(Side::Left)).await;
    // Cross A out through the right edge.
    feed(&mut a, mouse(1919.0, 540.0));
    feed(&mut a, mouse(1925.0, 540.0));
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(a.focus(), Focus::Remote);
    assert_eq!(b.focus(), Focus::Local);
    assert!(b.platform().warp_log().contains(&(1.0, 540.0)));
    assert!(a.platform().is_suppressed());
    // Keys typed while remote land only on B.
    feed(
        &mut a,
        InputEvent::Key {
            usage_id: 0x04,
            pressed: true,
        },
    );
    feed(
        &mut a,
        InputEvent::Key {
            usage_id: 0x04,
            pressed: false,
        },
    );
    assert!(settle(&mut a, &mut b).await.is_none());
    let injected_b = b.platform().recorded_injections();
    assert!(injected_b.contains(&InputEvent::Key {
        usage_id: 0x04,
        pressed: true
    }));
    assert!(injected_b.contains(&InputEvent::Key {
        usage_id: 0x04,
        pressed: false
    }));
    assert!(a.platform().recorded_injections().is_empty());
    // Pull back left: B's cursor exits its shared (left) edge, so the
    // cursor comes home through A's right edge, landing 1 px inside.
    feed(&mut a, mouse(1919.0, 540.0));
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(a.focus(), Focus::Local);
    assert_eq!(b.focus(), Focus::Remote);
    assert!(!a.platform().is_suppressed());
    assert!(a.platform().warp_log().contains(&(1919.0, 540.0)));
}

#[tokio::test]
async fn kill_link_while_remote_restores_focus_within_1s() {
    let (mut a, mut b) = pair(Arrangement::new(Side::Right), Arrangement::new(Side::Left)).await;
    feed(&mut a, mouse(1919.0, 540.0));
    feed(&mut a, mouse(1925.0, 540.0));
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(a.focus(), Focus::Remote);

    let started = Instant::now();
    drop(b);
    let err = tokio::time::timeout(Duration::from_secs(1), a.drive())
        .await
        .expect("drive returns within 1 s");
    assert!(
        matches!(err, PeerError::Link(_)),
        "link death ends the drive, got {err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(a.focus(), Focus::Local);
    assert!(!a.platform().is_suppressed());
    assert!(a.platform().warp_log().contains(&(1920.0, 540.0)));
}

#[tokio::test]
async fn physical_input_on_sink_claims_source() {
    let (mut a, mut b) = pair(Arrangement::new(Side::Right), Arrangement::new(Side::Left)).await;
    feed(
        &mut a,
        InputEvent::Key {
            usage_id: 0x04,
            pressed: true,
        },
    );
    assert!(settle(&mut a, &mut b).await.is_none());
    assert!(a.is_source());
    // B touches its hardware while not Source: B claims, A yields.
    feed(
        &mut b,
        InputEvent::Key {
            usage_id: 0x05,
            pressed: true,
        },
    );
    assert!(settle(&mut a, &mut b).await.is_none());
    assert!(b.is_source());
    assert!(!a.is_source());
    // The loser releases held input locally; the ReleaseAll A sends makes
    // B release too. Nothing is ever injected on the other peer.
    assert_eq!(
        a.platform().recorded_injections(),
        &[InputEvent::Key {
            usage_id: 0x04,
            pressed: false
        }]
    );
    assert_eq!(
        b.platform().recorded_injections(),
        &[InputEvent::Key {
            usage_id: 0x05,
            pressed: false
        }]
    );
}

#[tokio::test]
async fn arrangement_top_crosses_with_mirrored_wire_frames() {
    // pair() already asserted Top<->Bottom on the wire both ways.
    let (mut a, mut b) = pair(Arrangement::new(Side::Top), Arrangement::new(Side::Bottom)).await;
    feed(&mut a, mouse(960.0, 5.0));
    feed(&mut a, mouse(960.0, -5.0));
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(a.focus(), Focus::Remote);
    assert_eq!(b.focus(), Focus::Local);
    assert!(b.platform().warp_log().contains(&(960.0, 1079.0)));
}

/// Text set on A reaches B exactly once and never comes back: after two
/// full poll windows both clipboards agree and the Links are quiet (an echo
/// would ping-pong forever and trip the settle cap).
#[tokio::test]
async fn clipboard_text_syncs_without_echo() {
    let (mut a, mut b) = pair(Arrangement::new(Side::Right), Arrangement::new(Side::Left)).await;
    a.platform_mut().clipboard_set("hello peer").unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "hello peer");
    assert_eq!(a.platform().clipboard_get().unwrap(), "hello peer");
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "hello peer");
    assert_eq!(a.platform().clipboard_get().unwrap(), "hello peer");
}

#[tokio::test]
async fn connecting_does_not_push_what_was_already_copied() {
    let copied = |desktop| {
        let mut platform = FakePlatform::new(desktop);
        platform.clipboard_set("copied before connect").unwrap();
        platform
    };
    let (mut a, mut b) = pair_with(
        Arrangement::new(Side::Right),
        Arrangement::new(Side::Left),
        true,
        copied,
        FakePlatform::new,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "");
    a.platform_mut().clipboard_set("copied after").unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "copied after");
}

#[tokio::test]
async fn clipboard_toggle_off_sends_nothing() {
    let (mut a, mut b) = pair_with(
        Arrangement::new(Side::Right),
        Arrangement::new(Side::Left),
        false,
        FakePlatform::new,
        FakePlatform::new,
    )
    .await;
    a.platform_mut().clipboard_set("shared off").unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "");
    assert_eq!(a.platform().clipboard_get().unwrap(), "shared off");
}

/// A peer whose clipboard can hold oversize text (real Desktops do not cap
/// what the user copies; FakePlatform does, so this wrapper stands in).
struct OversizeClipboard {
    inner: FakePlatform,
    clipboard: String,
}

impl OversizeClipboard {
    fn new(desktop: Desktop, clipboard: String) -> Self {
        Self {
            inner: FakePlatform::new(desktop),
            clipboard,
        }
    }

    fn clipboard(&self) -> &str {
        &self.clipboard
    }
}

impl Platform for OversizeClipboard {
    fn desktop_bounds(&self) -> Result<Desktop, PlatformError> {
        self.inner.desktop_bounds()
    }
    fn start_capture(&mut self) -> Result<std::sync::mpsc::Receiver<InputEvent>, PlatformError> {
        self.inner.start_capture()
    }
    fn suppress_input(&mut self) -> Result<(), PlatformError> {
        self.inner.suppress_input()
    }
    fn unsuppress_input(&mut self) -> Result<(), PlatformError> {
        self.inner.unsuppress_input()
    }
    fn inject(&mut self, event: InputEvent) -> Result<(), PlatformError> {
        self.inner.inject(event)
    }
    fn warp_cursor(&mut self, x: f64, y: f64) -> Result<(), PlatformError> {
        self.inner.warp_cursor(x, y)
    }
    fn clipboard_get(&self) -> Result<String, PlatformError> {
        Ok(self.clipboard.clone())
    }
    fn clipboard_set(&mut self, text: &str) -> Result<(), PlatformError> {
        self.clipboard = text.to_string();
        Ok(())
    }
    fn check_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        self.inner.check_permissions()
    }
}

#[tokio::test]
async fn oversize_clipboard_is_not_sent() {
    let big = "x".repeat(CLIPBOARD_MAX_BYTES + 1);
    let (mut a, mut b) = pair_with(
        Arrangement::new(Side::Right),
        Arrangement::new(Side::Left),
        true,
        |desktop| OversizeClipboard::new(desktop, big.clone()),
        FakePlatform::new,
    )
    .await;
    // Two full poll windows with the oversize text present.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(settle(&mut a, &mut b).await.is_none());
    assert_eq!(b.platform().clipboard_get().unwrap(), "");
    assert_eq!(a.platform().clipboard(), big);
}

#[tokio::test]
#[ignore]
async fn mouse_move_loopback_latency_p95_under_10ms() {
    const N: usize = 200;
    let key_a = StaticKeypair::generate().unwrap();
    let key_b = StaticKeypair::generate().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let connect = Link::connect(addr, &key_a, &[], |_| true);
    let accept = async {
        let (stream, _) = listener.accept().await.unwrap();
        Link::accept(stream, &key_b, &[], |_| true).await
    };
    let (connected, accepted) = tokio::join!(connect, accept);
    let (mut link_a, _) = connected.unwrap();
    let (mut link_b, _) = accepted.unwrap();

    let echo = async {
        for _ in 0..N {
            let frame = link_b.recv_frame().await.unwrap();
            link_b.send_frame(&frame).await.unwrap();
        }
    };
    let ping = async {
        let mut samples = Vec::with_capacity(N);
        for i in 0..N {
            let frame = Frame::MouseMove {
                dx: i as f64,
                dy: 0.0,
            };
            let start = Instant::now();
            link_a.send_frame(&frame).await.unwrap();
            let echo = link_a.recv_frame().await.unwrap();
            assert_eq!(echo, frame);
            samples.push(start.elapsed());
        }
        samples
    };
    let (_, mut samples) = tokio::join!(echo, ping);
    samples.sort();
    let p50 = samples[N / 2];
    let p95 = samples[(N as f64 * 0.95) as usize];
    println!(
        "loopback MouseMove round trip: p50={p50:?} p95={p95:?} max={:?}",
        samples[N - 1]
    );
    assert!(
        p95 < Duration::from_millis(10),
        "p95 {p95:?} exceeds the 10 ms budget"
    );
}
