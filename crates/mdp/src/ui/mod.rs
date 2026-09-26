//! egui windows. `theme` holds the locked tokens; `arrange` is the Arrange window.

pub mod arrange;
pub mod theme;

use crate::config::Config;
use arrange::{ArrangeAction, ArrangeView, LinkStatus};
use eframe::egui;
use mdp_core::Desktop;

struct ArrangeApp {
    view: ArrangeView,
    config: Config,
}

impl eframe::App for ArrangeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(ArrangeAction::Save(arrangement)) = self.view.ui(ui, &mut self.config) {
            // ponytail: demo only prints; T8/T9 persist config and send the frame.
            println!("save: {arrangement:?}");
        }
    }
}

/// `mdp ui`: the Arrange window on demo data (in-memory config, fake peer).
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
            Ok(Box::new(ArrangeApp { view, config }))
        }),
    )
}
