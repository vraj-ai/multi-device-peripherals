//! egui windows. `theme` holds the locked tokens; `arrange` is the Arrange
//! window; `pairing` the Pairing window; `permissions` the macOS grants
//! screen; `tray` the tray/menu-bar icon.
//!
//! The app runs the same peer loop as `mdp run` on a background thread
//! (`run::serve` with [`AppHooks`]). Everything that must work while the
//! window is hidden in the tray (menu picks, Pairing requests, status) runs
//! in [`eframe::App::logic`], which eframe calls even when hidden.

pub mod arrange;
pub mod pairing;
pub mod permissions;
pub mod theme;
pub mod tray;

use crate::config::Config;
use crate::platform::Native;
use crate::run::{self, AppHooks, LiveStatus};
use arrange::{ArrangeAction, ArrangeView, LinkStatus};
use eframe::egui;
use mdp_core::platform::{Desktop, Platform};
use mdp_core::proto::Frame;
use pairing::{PairingRequest, PairingView};
use permissions::{Permissions, PermissionsAction};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::watch;
use tray::{Tray, TrayCommand};

/// Shown for the peer's Desktop until the first Hello (a typical laptop).
const PLACEHOLDER_PEER: Desktop = Desktop {
    x: 0.0,
    y: 0.0,
    width: 1512.0,
    height: 982.0,
};

/// Tray / pill text for a live status, e.g. `Linked to Mac · 3 ms · focus: here`.
pub fn status_text(live: &LiveStatus) -> String {
    if !live.linked {
        return "Not linked".to_string();
    }
    let latency = live
        .rtt_ms
        .map(|ms| format!(" · {ms} ms"))
        .unwrap_or_default();
    let focus = if live.focus_here { "here" } else { "peer" };
    format!("Linked to {}{latency} · focus: {focus}", live.peer_name)
}

/// Live status -> header pills.
pub fn link_status(live: &LiveStatus) -> LinkStatus {
    LinkStatus {
        linked: live.linked,
        latency_ms: live.rtt_ms.map(|ms| ms.min(u32::MAX as u64) as u32),
        focus_here: !live.linked || live.focus_here,
    }
}

struct App {
    config: Arc<Mutex<Config>>,
    path: PathBuf,
    view: ArrangeView,
    live: watch::Receiver<LiveStatus>,
    session: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Frame>>>>,
    tray: Option<Tray>,
    pair_requests: Receiver<PairingRequest>,
    pairing: Option<(PairingView, PairingRequest)>,
    /// Live grants on macOS; `None` where no grants are needed.
    permissions: Option<Permissions>,
    arrangement_rev: u64,
    quitting: bool,
}

/// This Peer's permission grants, if the OS gates input on them.
fn current_permissions() -> Option<Permissions> {
    #[cfg(target_os = "macos")]
    {
        let (accessibility, input_monitoring) = crate::platform::macos::permission_flags();
        Some(Permissions {
            accessibility,
            input_monitoring,
        })
    }
    #[cfg(not(target_os = "macos"))]
    None
}

fn show(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

impl App {
    fn save_config(&self, config: &Config) {
        if let Err(err) = config.save(&self.path) {
            eprintln!("mdp: save config: {err}");
        }
    }

    /// End the live session cleanly (the loop reconnects or parks).
    fn end_session(&self) {
        self.session.lock().expect("session lock").take();
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        for command in tray.poll() {
            match command {
                TrayCommand::ShareInput(on) => {
                    let mut config = self.config.lock().expect("config lock");
                    config.share_input = on;
                    self.save_config(&config);
                    drop(config);
                    if !on {
                        self.end_session();
                    }
                }
                TrayCommand::ShareClipboard(on) => {
                    let mut config = self.config.lock().expect("config lock");
                    config.share_clipboard = on;
                    self.save_config(&config);
                    drop(config);
                    // The Peer reads the toggle per session: reconnect to apply.
                    self.end_session();
                }
                TrayCommand::OpenArrange => show(ctx),
                TrayCommand::Pair | TrayCommand::Unpair => {
                    let mut config = self.config.lock().expect("config lock");
                    config.clear_pinned_peer();
                    self.save_config(&config);
                    drop(config);
                    self.end_session();
                }
                TrayCommand::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        let live = self.live.borrow().clone();
        let tray = self.tray.as_ref().expect("tray checked above");
        tray.set_status(
            tray::tray_state(live.linked, live.focus_here),
            &status_text(&live),
        );
    }

    fn sync_status(&mut self) {
        let live = self.live.borrow().clone();
        self.view.status = link_status(&live);
        if let Some(peer) = live.peer_desktop {
            self.view.peer = peer;
        }
        if !live.peer_name.is_empty() {
            self.view.peer_name = live.peer_name.clone();
        }
        if live.arrangement_rev != self.arrangement_rev {
            self.arrangement_rev = live.arrangement_rev;
            self.view.discard_draft();
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        self.sync_status();
        if self.pairing.is_none() {
            if let Ok(request) = self.pair_requests.try_recv() {
                let view = PairingView::new(&request.peer_name, "", &request.code, Instant::now());
                self.pairing = Some((view, request));
                show(ctx);
            }
        }
        // Closing the window hides it; the tray keeps mdp running.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(perms) = self.permissions.filter(|p| !p.all_granted()) {
            match permissions::ui(ui, perms) {
                Some(PermissionsAction::OpenSettings(pane)) => permissions::open_settings(pane),
                Some(PermissionsAction::CheckAgain) | None => {}
            }
            // Grants land while System Settings is open: keep re-checking.
            self.permissions = current_permissions();
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }
        if let Some((view, _)) = &mut self.pairing {
            if let Some(matched) = view.ui(ui, Instant::now()) {
                if let Some((_, request)) = self.pairing.take() {
                    request.answer(matched);
                }
            }
            return;
        }
        let mut config = self.config.lock().expect("config lock");
        if let Some(ArrangeAction::Save(arrangement)) = self.view.ui(ui, &mut config) {
            self.save_config(&config);
            if let Err(err) = crate::autostart::set(config.start_at_login) {
                eprintln!("mdp: start at login: {err}");
            }
            let frame = Frame::Arrangement {
                side: run::config_side_to_wire(arrangement.side),
                offset: arrangement.offset,
            };
            if let Some(outbox) = self.session.lock().expect("session lock").as_ref() {
                // Ends this session on both Peers; they reconnect in agreement.
                let _ = outbox.send(frame);
            }
        }
    }
}

/// `mdp` / `mdp ui`: the live app (tray + windows) driving the real Peer.
pub fn run_app() -> Result<(), String> {
    let path = Config::default_path().map_err(|err| format!("config path: {err}"))?;
    let config = Config::load_or_create(&path).map_err(|err| format!("config: {err}"))?;
    let local = Native::new()
        .desktop_bounds()
        .map_err(|err| format!("desktop: {err}"))?;
    let peer_name = config
        .pinned_peer_name
        .clone()
        .unwrap_or_else(|| "Peer".to_string());
    let config = Arc::new(Mutex::new(config));
    let (status_tx, live) = watch::channel(LiveStatus::default());
    let session = Arc::new(Mutex::new(None));
    let (pairing_tx, pair_requests) = std::sync::mpsc::channel();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mdp — Arrange")
            .with_inner_size([960.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mdp",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
            let hooks = AppHooks {
                status: status_tx,
                session: Arc::clone(&session),
                pairing: pairing_tx,
                wake: Arc::clone(&wake),
            };
            let (share_input, share_clipboard) = {
                let config = config.lock().expect("config lock");
                (config.share_input, config.share_clipboard)
            };
            let tray_wake = Arc::clone(&wake);
            let tray = Tray::new(&peer_name, share_input, share_clipboard, move || {
                tray_wake()
            })
            .map_err(eframe::Error::AppCreation)?;
            let loop_config = Arc::clone(&config);
            let loop_path = path.clone();
            std::thread::Builder::new()
                .name("mdp-peer".into())
                .spawn(move || {
                    if let Err(err) = run::serve(loop_config, loop_path, Some(hooks)) {
                        eprintln!("mdp: peer loop stopped: {err}");
                    }
                })
                .map_err(|err| eframe::Error::AppCreation(err.into()))?;
            Ok(Box::new(App {
                config,
                path,
                view: ArrangeView::new(local, "this computer", PLACEHOLDER_PEER, &peer_name),
                live,
                session,
                tray: Some(tray),
                pair_requests,
                pairing: None,
                permissions: current_permissions(),
                arrangement_rev: 0,
                quitting: false,
            }))
        }),
    )
    .map_err(|err| format!("ui: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_status_maps_to_pills_and_tray_text() {
        let down = LiveStatus::default();
        assert_eq!(status_text(&down), "Not linked");
        let pills = link_status(&down);
        assert!(!pills.linked && pills.focus_here && pills.latency_ms.is_none());
        assert_eq!(
            tray::tray_state(down.linked, down.focus_here),
            tray::TrayState::NoPeer
        );

        let remote = LiveStatus {
            linked: true,
            peer_name: "Mac".into(),
            peer_desktop: Some(PLACEHOLDER_PEER),
            focus_here: false,
            rtt_ms: Some(3),
            arrangement_rev: 1,
        };
        assert_eq!(status_text(&remote), "Linked to Mac · 3 ms · focus: peer");
        let pills = link_status(&remote);
        assert!(pills.linked && !pills.focus_here);
        assert_eq!(pills.latency_ms, Some(3));
        assert_eq!(
            tray::tray_state(remote.linked, remote.focus_here),
            tray::TrayState::FocusPeer
        );
    }
}
