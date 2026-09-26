//! egui windows. `theme` holds the locked tokens; `arrange` is the Arrange
//! window; `pairing` the Pairing window; `tray` the tray/menu-bar icon.

pub mod arrange;
pub mod pairing;
pub mod theme;
pub mod tray;

use crate::config::Config;
use arrange::{ArrangeAction, ArrangeView, LinkStatus};
use eframe::egui;
use mdp_core::Desktop;
use pairing::{PairingRequest, PairingView};
use std::sync::mpsc::Receiver;
use std::time::Instant;
use tray::{Tray, TrayCommand};

struct App {
    view: ArrangeView,
    config: Config,
    tray: Option<Tray>,
    /// Pairing requests from the Link (`pairing::bridge`).
    pair_requests: Option<Receiver<PairingRequest>>,
    pairing: Option<(PairingView, PairingRequest)>,
}

impl App {
    fn handle_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        for command in tray.poll() {
            match command {
                TrayCommand::ShareInput(on) => self.config.share_input = on,
                TrayCommand::ShareClipboard(on) => self.config.share_clipboard = on,
                TrayCommand::OpenArrange => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                // ponytail: demo has no Link; T8 re-runs pairing / clears the pin.
                TrayCommand::Pair | TrayCommand::Unpair => {}
                TrayCommand::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
        let status = self.view.status;
        let state = tray::tray_state(status.linked, status.focus_here);
        let text = if status.linked {
            format!(
                "Linked to {} · focus: {}",
                self.view.peer_name,
                if status.focus_here { "here" } else { "peer" }
            )
        } else {
            "Not linked".to_string()
        };
        tray.set_status(state, &text);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_tray(&ctx);
        if self.pairing.is_none() {
            if let Some(request) = self
                .pair_requests
                .as_ref()
                .and_then(|rx| rx.try_recv().ok())
            {
                let view = PairingView::new(&request.peer_name, "", &request.code, Instant::now());
                self.pairing = Some((view, request));
            }
        }
        if let Some((view, _)) = &mut self.pairing {
            if let Some(matched) = view.ui(ui, Instant::now()) {
                if let Some((_, request)) = self.pairing.take() {
                    request.answer(matched);
                }
            }
            return;
        }
        if let Some(ArrangeAction::Save(arrangement)) = self.view.ui(ui, &mut self.config) {
            // ponytail: demo only prints; T8/T9 persist config and send the frame.
            println!("save: {arrangement:?}");
        }
        // Keep polling the tray menu while idle.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// `mdp ui`: the Arrange window + tray on demo data (in-memory config, fake peer).
pub fn run_demo() -> eframe::Result {
    let config = Config::generate().map_err(|err| eframe::Error::AppCreation(err.into()))?;
    let mut view = ArrangeView::new(
        Desktop::new(0.0, 0.0, 3840.0, 1080.0),
        "this PC",
        Desktop::new(0.0, 0.0, 1512.0, 982.0),
        "Mac",
    );
    view.status = LinkStatus {
        linked: true,
        latency_ms: Some(3),
        focus_here: true,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mdp — Arrange")
            .with_inner_size([960.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mdp",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let tray = Tray::new("Mac", config.share_input, config.share_clipboard)
                .map_err(eframe::Error::AppCreation)?;
            Ok(Box::new(App {
                view,
                config,
                tray: Some(tray),
                pair_requests: None,
                pairing: None,
            }))
        }),
    )
}
