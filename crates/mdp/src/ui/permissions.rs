//! macOS permissions screen (`CONTEXT/ui-design.md` §macOS permissions):
//! shown instead of Arrange while Accessibility or Input Monitoring is
//! missing.

use super::theme;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Stroke, Ui};

/// Which macOS grants this Peer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Permissions {
    pub accessibility: bool,
    pub input_monitoring: bool,
}

impl Permissions {
    pub fn all_granted(self) -> bool {
        self.accessibility && self.input_monitoring
    }
}

/// A System Settings privacy pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Accessibility,
    InputMonitoring,
}

impl Pane {
    pub fn url(self) -> &'static str {
        match self {
            Pane::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Pane::InputMonitoring => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionsAction {
    OpenSettings(Pane),
    CheckAgain,
}

/// Open the pane in System Settings (no-op off macOS).
pub fn open_settings(pane: Pane) {
    let url = pane.url();
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = url;
}

/// Draw the screen for `perms`.
pub fn ui(ui: &mut Ui, perms: Permissions) -> Option<PermissionsAction> {
    let mut action = None;
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(theme::PANEL)
                .inner_margin(Margin::symmetric(40, 36)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 22.0;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let done = [perms.accessibility, perms.input_monitoring]
                    .iter()
                    .filter(|granted| **granted)
                    .count();
                ui.label(
                    RichText::new(format!("ONE-TIME SETUP · STEP {done} OF 2 DONE"))
                        .font(theme::sans_semibold(12.0))
                        .color(theme::MUTED),
                );
                ui.label(
                    RichText::new("mdp needs two macOS permissions")
                        .font(theme::sans_semibold(24.0))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(
                        "macOS asks for these so mdp can move the cursor and type for you, and read your mouse and keyboard when they're plugged into this Mac.",
                    )
                    .font(theme::sans(15.0))
                    .color(theme::TEXT_2),
                );
            });
            let cards = [
                (
                    Pane::Accessibility,
                    "Accessibility",
                    "Move the cursor and type on this Mac",
                    perms.accessibility,
                ),
                (
                    Pane::InputMonitoring,
                    "Input Monitoring",
                    "Read the mouse and keyboard plugged into this Mac",
                    perms.input_monitoring,
                ),
            ];
            for (pane, title, detail, granted) in cards {
                if card(ui, title, detail, granted) {
                    action = Some(PermissionsAction::OpenSettings(pane));
                }
            }
            ui.label(
                RichText::new(
                    "mdp never records keystrokes. Input goes only to your paired computer, encrypted, and nothing is stored.",
                )
                .font(theme::sans(13.0))
                .color(theme::MUTED),
            );
            ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                let again = egui::Button::new(
                    RichText::new("Check again")
                        .font(theme::sans(14.0))
                        .color(theme::TEXT),
                )
                .fill(egui::Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                .min_size(egui::vec2(120.0, 44.0));
                if ui.add(again).clicked() {
                    action = Some(PermissionsAction::CheckAgain);
                }
            });
        });
    action
}

/// One permission card; returns true when "Open Settings" was clicked.
fn card(ui: &mut Ui, title: &str, detail: &str, granted: bool) -> bool {
    let (fill, stroke) = if granted {
        (egui::Color32::TRANSPARENT, Stroke::new(1.0, theme::BORDER))
    } else {
        (theme::ACCENT_BG_CARD, Stroke::new(1.5, theme::ACCENT))
    };
    let mut clicked = false;
    Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(theme::RADIUS_CARD)
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                status_icon(ui, granted);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new(title)
                            .font(theme::sans_semibold(15.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(detail)
                            .font(theme::sans(13.0))
                            .color(if granted { theme::MUTED } else { theme::TEXT_2 }),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if granted {
                        ui.label(
                            RichText::new("Granted")
                                .font(theme::sans_medium(13.0))
                                .color(theme::LINK),
                        );
                    } else {
                        let open = egui::Button::new(
                            RichText::new("Open Settings")
                                .font(theme::sans_semibold(14.0))
                                .color(theme::ON_ACCENT),
                        )
                        .fill(theme::ACCENT)
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(0.0, 44.0));
                        clicked = ui.add(open).clicked();
                    }
                });
            });
        });
    clicked
}

/// 28 px stroke icon: teal check circle when granted, amber "!" circle when not.
fn status_icon(ui: &mut Ui, granted: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
    let color = if granted { theme::LINK } else { theme::ACCENT };
    let stroke = Stroke::new(2.2, color);
    let p = ui.painter();
    let c = rect.center();
    p.circle_stroke(c, 11.5, stroke);
    if granted {
        p.line(
            vec![
                c + egui::vec2(-5.0, 0.5),
                c + egui::vec2(-1.8, 3.7),
                c + egui::vec2(5.0, -3.0),
            ],
            stroke,
        );
    } else {
        p.line_segment(
            [c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 1.0)],
            stroke,
        );
        p.circle_filled(c + egui::vec2(0.0, 5.0), 1.3, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};

    type State = (Permissions, Vec<PermissionsAction>, bool);

    fn harness<'a>(perms: Permissions) -> Harness<'a, State> {
        Harness::builder()
            .with_size(egui::vec2(620.0, 560.0))
            .build_ui_state(
                |ui, (perms, actions, themed): &mut State| {
                    // Fonts land one frame after `apply`, as in the real app.
                    if !*themed {
                        theme::apply(ui.ctx());
                        *themed = true;
                        ui.ctx().request_repaint();
                        return;
                    }
                    actions.extend(super::ui(ui, *perms));
                },
                (perms, Vec::new(), false),
            )
    }

    #[test]
    fn missing_input_monitoring_offers_its_settings_pane() {
        let mut h = harness(Permissions {
            accessibility: true,
            input_monitoring: false,
        });
        h.run();
        h.get_by_label("ONE-TIME SETUP · STEP 1 OF 2 DONE");
        h.get_by_label("Granted");
        h.get_by_label("Open Settings").click();
        h.run();
        h.get_by_label("Check again").click();
        h.run();
        assert_eq!(
            h.state().1,
            vec![
                PermissionsAction::OpenSettings(Pane::InputMonitoring),
                PermissionsAction::CheckAgain
            ]
        );
    }

    #[test]
    fn deep_links_target_the_right_panes() {
        assert!(Pane::Accessibility.url().ends_with("Privacy_Accessibility"));
        assert!(Pane::InputMonitoring.url().ends_with("Privacy_ListenEvent"));
        assert!(Permissions {
            accessibility: true,
            input_monitoring: true
        }
        .all_granted());
        assert!(!Permissions::default().all_granted());
    }
}
