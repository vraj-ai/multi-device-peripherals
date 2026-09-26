//! Pairing: the window that shows the 6-digit code, and the bridge from the
//! Link's synchronous `confirm` callback to that window.

use super::theme;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Stroke, Ui};
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, Instant};

/// How long a Pairing code stays valid before it counts as rejected.
pub const EXPIRY: Duration = Duration::from_secs(60);

/// One pending Pairing, handed from the Link to the UI.
#[derive(Debug)]
pub struct PairingRequest {
    pub peer_name: String,
    pub code: String,
    reply: SyncSender<bool>,
}

impl PairingRequest {
    /// Tell the waiting handshake whether the codes matched.
    pub fn answer(self, matched: bool) {
        // The handshake may already have timed out; nothing left to tell.
        let _ = self.reply.send(matched);
    }
}

/// A `confirm` callback for `Link::connect`/`accept` plus the receiver the UI
/// polls. The callback blocks its caller until the UI answers or [`EXPIRY`]
/// passes (expiry rejects).
#[cfg(test)]
pub fn bridge(
    peer_name: String,
) -> (
    impl Fn(&str) -> bool + Send + Sync + 'static,
    mpsc::Receiver<PairingRequest>,
) {
    let (tx, rx) = mpsc::channel();
    (confirm_via(tx, peer_name), rx)
}

/// A `confirm` callback that posts each Pairing to `requests` and waits for
/// the answer (expiry or a closed UI rejects).
// ponytail: blocks the peer loop's thread for up to EXPIRY; fine, nothing
// else needs that thread while a handshake is pending.
pub fn confirm_via(
    requests: mpsc::Sender<PairingRequest>,
    peer_name: String,
) -> impl Fn(&str) -> bool + Send + Sync + 'static {
    move |code: &str| {
        let (reply, answer) = mpsc::sync_channel(1);
        let request = PairingRequest {
            peer_name: peer_name.clone(),
            code: code.to_string(),
            reply,
        };
        requests.send(request).is_ok() && answer.recv_timeout(EXPIRY).unwrap_or(false)
    }
}

/// The Pairing window (`CONTEXT/ui-design.md` §Pairing).
pub struct PairingView {
    pub peer_name: String,
    /// e.g. `MacBook-Pro.local · 192.168.1.42`.
    pub peer_host: String,
    code: String,
    deadline: Instant,
}

impl PairingView {
    pub fn new(peer_name: &str, peer_host: &str, code: &str, shown_at: Instant) -> Self {
        Self {
            peer_name: peer_name.into(),
            peer_host: peer_host.into(),
            code: code.into(),
            deadline: shown_at + EXPIRY,
        }
    }

    /// Draw one frame. `Some(true)` = codes match, `Some(false)` = rejected
    /// or expired.
    pub fn ui(&mut self, ui: &mut Ui, now: Instant) -> Option<bool> {
        let left = self.deadline.saturating_duration_since(now);
        if left.is_zero() {
            return Some(false);
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        let mut answer = None;
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(36, 40)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 24.0;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(caps("New device found"));
                    ui.label(
                        RichText::new(format!("Pair with {}?", self.peer_name))
                            .font(theme::sans_semibold(22.0))
                            .color(theme::TEXT),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "The same code is showing on your {} right now. Pair only if the two codes match exactly.",
                        self.peer_name
                    ))
                    .font(theme::sans(15.0))
                    .color(theme::TEXT_2),
                );
                code_box(ui, &self.code);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(
                        RichText::new(&self.peer_host)
                            .font(theme::mono(13.0))
                            .color(theme::MUTED),
                    );
                    let secs = left.as_secs_f32().ceil() as u64;
                    ui.label(
                        RichText::new(format!("Expires in {}:{:02}", secs / 60, secs % 60))
                            .font(theme::sans(13.0))
                            .color(theme::MUTED),
                    );
                });
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.label(
                        RichText::new(
                            "After pairing, only this device can connect. Everything sent between the two is encrypted. Unpair from the tray menu at any time.",
                        )
                        .font(theme::sans(12.0))
                        .color(theme::MUTED),
                    );
                    let size = egui::vec2(ui.available_width(), 48.0);
                    let reject = egui::Button::new(
                        RichText::new("They don't match")
                            .font(theme::sans(15.0))
                            .color(theme::TEXT),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                    .min_size(size);
                    if ui.add(reject).clicked() {
                        answer = Some(false);
                    }
                    let pair = egui::Button::new(
                        RichText::new("Codes match — pair")
                            .font(theme::sans_semibold(15.0))
                            .color(theme::ON_ACCENT),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::NONE)
                    .min_size(size);
                    if ui.add(pair).clicked() {
                        answer = Some(true);
                    }
                });
            });
        answer
    }
}

/// The code in two mono groups of three; the second group amber.
fn code_box(ui: &mut Ui, code: &str) {
    let (first, second) = code.split_at(code.len().min(3));
    Frame::new()
        .fill(theme::CANVAS)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(theme::RADIUS_CARD)
        .inner_margin(Margin::symmetric(0, 28))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let centered = Layout::left_to_right(Align::Center).with_main_align(Align::Center);
            ui.with_layout(centered, |ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                let group = |text: &str, color| {
                    RichText::new(text)
                        .font(theme::mono_medium(52.0))
                        .color(color)
                };
                ui.label(group(first, theme::TEXT));
                ui.label(group(second, theme::ACCENT));
            });
        });
}

fn caps(text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .font(theme::sans_semibold(12.0))
        .color(theme::MUTED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    use std::thread;

    #[test]
    fn bridge_passes_code_and_answer() {
        let (confirm, requests) = bridge("Mac".into());
        let ui = thread::spawn(move || {
            let request = requests.recv().expect("request");
            assert_eq!(request.code, "482913");
            assert_eq!(request.peer_name, "Mac");
            request.answer(true);
        });
        assert!(confirm("482913"));
        ui.join().expect("ui thread");
    }

    #[test]
    fn bridge_rejects_when_ui_is_gone() {
        let (confirm, requests) = bridge("Mac".into());
        drop(requests);
        assert!(!confirm("482913"));
    }

    type State = (PairingView, Instant, Vec<bool>, bool);

    fn harness<'a>(now: Instant) -> Harness<'a, State> {
        let view = PairingView::new("Mac", "MacBook-Pro.local · 192.168.1.42", "482913", now);
        Harness::builder()
            .with_size(egui::vec2(480.0, 620.0))
            .build_ui_state(
                |ui, (view, now, answers, themed): &mut State| {
                    // Fonts land one frame after `apply`, as in the real app.
                    if !*themed {
                        theme::apply(ui.ctx());
                        *themed = true;
                        ui.ctx().request_repaint();
                        return;
                    }
                    answers.extend(view.ui(ui, *now));
                },
                (view, now, Vec::new(), false),
            )
    }

    #[test]
    fn shows_code_in_two_groups_and_confirms() {
        let mut h = harness(Instant::now());
        h.run();
        h.get_by_label("482");
        h.get_by_label("913");
        h.get_by_label("Pair with Mac?");
        h.get_by_label("Expires in 1:00");
        h.get_by_label("Codes match — pair").click();
        h.run();
        assert_eq!(h.state().2, vec![true]);
    }

    #[test]
    fn mismatch_rejects() {
        let mut h = harness(Instant::now());
        h.run();
        h.get_by_label("They don't match").click();
        h.run();
        assert_eq!(h.state().2, vec![false]);
    }

    #[test]
    fn expiry_rejects() {
        let shown = Instant::now();
        let mut h = harness(shown);
        h.run();
        assert!(h.state().2.is_empty());
        h.state_mut().1 = shown + EXPIRY;
        h.run();
        assert_eq!(h.state().2.first(), Some(&false));
    }
}
