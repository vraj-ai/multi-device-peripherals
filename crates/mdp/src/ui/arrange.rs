//! The Arrange window: drag the other Peer's Desktop tile to the side it sits
//! on, then Save. Pure geometry ([`snap`], [`mirror`]) is ported from the
//! reference script in `docs/design/Main.dc.html`.

use super::theme;
use crate::config::{Arrangement, Config, Side};
use eframe::egui::{
    self, vec2, Align, Align2, CentralPanel, Frame, Key, Layout, Margin, Panel, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};
use mdp_core::Desktop;

/// Minimum overlap along the shared edge, in canvas px.
pub const MIN_OVERLAP: f32 = 24.0;
/// Right panel width.
const PANEL_WIDTH: f32 = 272.0;

/// Snap a dropped peer tile (top-left at `peer_tile_pos`) to the nearest side
/// of `local`. Returns the side and the offset along that edge in px: the
/// peer tile's top (Left/Right) or left (Top/Bottom) minus the local tile's,
/// clamped so the tiles overlap by at least [`MIN_OVERLAP`].
pub fn snap(peer_tile_pos: Pos2, local: Rect, peer_size: Vec2) -> (Side, f32) {
    let c = peer_tile_pos + peer_size / 2.0 - local.center();
    let clamp =
        |v: f32, len: f32, peer_len: f32| v.max(-(peer_len - MIN_OVERLAP)).min(len - MIN_OVERLAP);
    if c.x.abs() / ((local.width() + peer_size.x) / 2.0)
        >= c.y.abs() / ((local.height() + peer_size.y) / 2.0)
    {
        let side = if c.x < 0.0 { Side::Left } else { Side::Right };
        let off = clamp(peer_tile_pos.y - local.top(), local.height(), peer_size.y);
        (side, off)
    } else {
        let side = if c.y < 0.0 { Side::Top } else { Side::Bottom };
        let off = clamp(peer_tile_pos.x - local.left(), local.width(), peer_size.x);
        (side, off)
    }
}

/// The Arrangement the other Peer stores for the same physical layout: the
/// opposite side, and the offset measured from its own Desktop.
pub fn mirror(a: Arrangement) -> Arrangement {
    let side = match a.side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
    };
    Arrangement {
        side,
        offset: -a.offset,
    }
}

/// Top-left of the peer tile for a side + px offset.
fn placed(side: Side, off: f32, local: Rect, peer: Vec2) -> Pos2 {
    match side {
        Side::Left => egui::pos2(local.left() - peer.x, local.top() + off),
        Side::Right => egui::pos2(local.right(), local.top() + off),
        Side::Top => egui::pos2(local.left() + off, local.top() - peer.y),
        Side::Bottom => egui::pos2(local.left() + off, local.bottom()),
    }
}

/// The 4 px amber bar on the shared edge.
fn shared_edge(side: Side, local: Rect, peer: Rect) -> Rect {
    match side {
        Side::Left | Side::Right => {
            let x = if side == Side::Left {
                local.left()
            } else {
                local.right()
            };
            Rect::from_x_y_ranges(
                x - 2.0..=x + 2.0,
                local.top().max(peer.top())..=local.bottom().min(peer.bottom()),
            )
        }
        Side::Top | Side::Bottom => {
            let y = if side == Side::Top {
                local.top()
            } else {
                local.bottom()
            };
            Rect::from_x_y_ranges(
                local.left().max(peer.left())..=local.right().min(peer.right()),
                y - 2.0..=y + 2.0,
            )
        }
    }
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Left => "LEFT of",
        Side::Right => "RIGHT of",
        Side::Top => "ABOVE",
        Side::Bottom => "BELOW",
    }
}

/// Link/Focus state for the header pills.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinkStatus {
    pub linked: bool,
    pub latency_ms: Option<u32>,
    pub focus_here: bool,
}

/// What the caller must do after a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrangeAction {
    /// Persist the config and send this Arrangement over the Link (T8).
    Save(Arrangement),
}

pub struct ArrangeView {
    pub local: Desktop,
    /// Shown in the readout, e.g. "this PC".
    pub local_name: String,
    pub peer: Desktop,
    pub peer_name: String,
    pub status: LinkStatus,
    /// Unsaved edit; `None` shows `config.arrangement`.
    draft: Option<Arrangement>,
    /// While dragging: (tile pos at grab, live tile pos), relative to the local tile.
    drag: Option<(Vec2, Vec2)>,
}

impl ArrangeView {
    pub fn new(local: Desktop, local_name: &str, peer: Desktop, peer_name: &str) -> Self {
        Self {
            local,
            local_name: local_name.into(),
            peer,
            peer_name: peer_name.into(),
            status: LinkStatus::default(),
            draft: None,
            drag: None,
        }
    }

    /// The other Peer saved `from_peer`; last save wins, so it replaces any
    /// unsaved edit here.
    #[allow(dead_code)] // ponytail: T8 (#9) calls this on a received Frame::Arrangement.
    pub fn receive(&mut self, config: &mut Config, from_peer: Arrangement) {
        config.arrangement = mirror(from_peer);
        self.draft = None;
    }

    pub fn ui(&mut self, ui: &mut Ui, config: &mut Config) -> Option<ArrangeAction> {
        self.header(ui);
        let action = Panel::right("arrange-panel")
            .exact_size(PANEL_WIDTH)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(20)),
            )
            .show(ui, |ui| self.side_panel(ui, config))
            .inner;
        CentralPanel::no_frame().show(ui, |ui| self.canvas(ui, config));
        action
    }

    fn header(&self, ui: &mut Ui) {
        Panel::top("arrange-header")
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (_, p) = ui.allocate_painter(vec2(28.0, 20.0), Sense::hover());
                    let o = p.clip_rect().min;
                    let stroke = Stroke::new(2.0, theme::ACCENT);
                    let back = Rect::from_min_size(o + vec2(1.0, 3.0), vec2(14.0, 10.0));
                    let front = Rect::from_min_size(o + vec2(13.0, 7.0), vec2(14.0, 10.0));
                    p.rect_stroke(back, 1.5, stroke, StrokeKind::Middle);
                    p.rect(front, 1.5, theme::ACCENT, stroke, StrokeKind::Middle);
                    ui.label(RichText::new("mdp").font(theme::mono_medium(18.0)));
                    ui.label(RichText::new("Arrange").size(15.0).color(theme::MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let focus = if self.status.focus_here {
                            &self.local_name
                        } else {
                            &self.peer_name
                        };
                        let focus = format!("Focus: {focus}");
                        pill(ui, theme::ACCENT_BG, Stroke::NONE, |ui| {
                            ui.label(RichText::new(focus).size(13.0).color(theme::ACCENT));
                        });
                        // Right-to-left: items go in reverse visual order.
                        pill(ui, theme::BG, Stroke::new(1.0, theme::BORDER), |ui| {
                            let s = self.status;
                            if s.linked {
                                let ms = s.latency_ms.map_or("–".into(), |m| m.to_string());
                                ui.label(
                                    RichText::new(format!("encrypted · {ms} ms"))
                                        .font(theme::mono(13.0))
                                        .color(theme::MUTED),
                                );
                                let peer = format!("Linked to {}", self.peer_name);
                                ui.label(RichText::new(peer).size(13.0));
                            } else {
                                ui.label(RichText::new("Not linked").size(13.0));
                            }
                            let (dot, p) = ui.allocate_painter(vec2(8.0, 8.0), Sense::hover());
                            let color = if s.linked { theme::LINK } else { theme::MUTED };
                            p.circle_filled(dot.rect.center(), 4.0, color);
                        });
                    });
                });
            });
    }

    fn side_panel(&mut self, ui: &mut Ui, config: &mut Config) -> Option<ArrangeAction> {
        let current = self.draft.unwrap_or(config.arrangement);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(
            RichText::new("ARRANGEMENT")
                .font(theme::sans_semibold(12.0))
                .extra_letter_spacing(0.96)
                .color(theme::MUTED),
        );
        ui.label(
            RichText::new(format!(
                "{} is {} {}",
                self.peer_name,
                side_label(current.side),
                self.local_name
            ))
            .font(theme::sans_medium(16.0)),
        );
        ui.label(
            RichText::new(format!("offset {} pt", current.offset.round() + 0.0))
                .font(theme::mono(13.0))
                .color(theme::MUTED),
        );
        ui.add_space(14.0);
        toggle_row(ui, &mut config.share_input, "Share mouse & keyboard");
        toggle_row(ui, &mut config.share_clipboard, "Sync clipboard text");
        toggle_row(ui, &mut config.start_at_login, "Start at login");
        ui.add_space(14.0);
        Frame::new()
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(theme::RADIUS_CONTROL)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("Take control back, always")
                        .size(12.0)
                        .color(theme::MUTED),
                );
                ui.label(RichText::new("Ctrl + Alt + Shift + Esc").font(theme::mono(13.0)));
            });
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let size = vec2((ui.available_width() - 10.0) / 2.0, 44.0);
                let revert = egui::Button::new("Revert")
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                    .min_size(size);
                if ui.add(revert).clicked() {
                    self.draft = None;
                }
                let save = egui::Button::new(
                    RichText::new("Save")
                        .font(theme::sans_semibold(14.0))
                        .color(theme::ON_ACCENT),
                )
                .fill(theme::ACCENT)
                .stroke(Stroke::NONE)
                .min_size(size);
                if ui.add(save).clicked() {
                    let saved = self.draft.take().unwrap_or(config.arrangement);
                    config.arrangement = saved;
                    return Some(ArrangeAction::Save(saved));
                }
                None
            })
            .inner
        })
        .inner
    }

    fn canvas(&mut self, ui: &mut Ui, config: &Config) {
        let (canvas, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let area = canvas.rect;
        painter.rect_filled(area, 0.0, theme::CANVAS);
        let mut y = area.top() + 10.0;
        while y < area.bottom() {
            let mut x = area.left() + 10.0;
            while x < area.right() {
                painter.circle_filled(egui::pos2(x, y), 1.0, theme::CANVAS_DOT);
                x += 20.0;
            }
            y += 20.0;
        }
        painter.text(
            area.min + vec2(24.0, 16.0),
            Align2::LEFT_TOP,
            format!(
                "Drag {} to the edge it sits against on your desk. Arrow keys work too.",
                self.peer_name
            ),
            theme::sans(13.0),
            theme::MUTED,
        );

        // One scale for both Desktops so tiles stay proportional; room for the
        // peer tile on every side.
        let (lw, lh) = (self.local.width as f32, self.local.height as f32);
        let (pw, ph) = (self.peer.width as f32, self.peer.height as f32);
        let scale = 0.9 * (area.width() / (lw + 2.0 * pw)).min(area.height() / (lh + 2.0 * ph));
        let local = Rect::from_center_size(area.center(), vec2(lw, lh) * scale);
        let peer_size = vec2(pw, ph) * scale;

        let current = self.draft.unwrap_or(config.arrangement);
        let at_rest = placed(
            current.side,
            current.offset as f32 * scale,
            local,
            peer_size,
        );
        let rel = self.drag.map_or(at_rest - local.min, |(_, live)| live);
        let peer_rect = Rect::from_min_size(local.min + rel, peer_size);

        let resp = ui.interact(peer_rect, ui.id().with("peer-tile"), Sense::drag());
        resp.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Button,
                true,
                format!(
                    "{} display. Drag, or use arrow keys, to set which side it is on.",
                    self.peer_name
                ),
            )
        });
        if resp.dragged() {
            let (base, _) = *self.drag.get_or_insert((rel, rel));
            let live = base + resp.total_drag_delta().unwrap_or_default();
            self.drag = Some((base, live));
        }
        if resp.drag_stopped() {
            if let Some((_, live)) = self.drag.take() {
                let (side, off) = snap(local.min + live, local, peer_size);
                self.draft = Some(Arrangement {
                    side,
                    offset: (off / scale) as f64,
                });
            }
        }
        if self.drag.is_none() {
            let arrow = ui.input(|i| {
                [
                    (Key::ArrowLeft, Side::Left),
                    (Key::ArrowRight, Side::Right),
                    (Key::ArrowUp, Side::Top),
                    (Key::ArrowDown, Side::Bottom),
                ]
                .into_iter()
                .find(|(key, _)| i.key_pressed(*key))
            });
            if let Some((_, side)) = arrow {
                self.draft = Some(Arrangement { side, offset: 0.0 });
            }
        }

        // Repaint from the post-input state so drag and snap have no lag.
        let current = self.draft.unwrap_or(config.arrangement);
        let rel = self.drag.map_or_else(
            || {
                placed(
                    current.side,
                    current.offset as f32 * scale,
                    local,
                    peer_size,
                ) - local.min
            },
            |(_, live)| live,
        );
        let peer_rect = Rect::from_min_size(local.min + rel, peer_size);
        tile(
            &painter,
            local,
            theme::RAISED,
            theme::BORDER_STRONG,
            &capitalize(&self.local_name),
            self.local,
            theme::MUTED,
        );
        if self.drag.is_none() {
            let edge = shared_edge(current.side, local, peer_rect);
            painter.rect_filled(edge.expand(4.0), 4.0, theme::ACCENT.gamma_multiply(0.25));
            painter.rect_filled(edge, 2.0, theme::ACCENT);
        }
        tile(
            &painter,
            peer_rect,
            theme::PEER_FILL,
            theme::PEER_BORDER,
            &self.peer_name,
            self.peer,
            theme::TEXT_2,
        );
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

fn tile(
    painter: &egui::Painter,
    rect: Rect,
    fill: egui::Color32,
    border: egui::Color32,
    title: &str,
    desktop: Desktop,
    caption: egui::Color32,
) {
    painter.rect(
        rect,
        theme::RADIUS_CONTROL,
        fill,
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    let painter = painter.with_clip_rect(rect.shrink(2.0));
    painter.text(
        rect.min + vec2(14.0, 12.0),
        Align2::LEFT_TOP,
        title,
        theme::sans_semibold(14.0),
        theme::TEXT,
    );
    painter.text(
        rect.left_bottom() + vec2(14.0, -12.0),
        Align2::LEFT_BOTTOM,
        format!("{}×{} pt", desktop.width, desktop.height),
        theme::mono(12.0),
        caption,
    );
}

fn pill(ui: &mut Ui, fill: egui::Color32, stroke: Stroke, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(14)
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, add);
}

/// A 44 px row: label left, checkbox right, the whole row toggles.
fn toggle_row(ui: &mut Ui, value: &mut bool, label: &str) {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *value, label));
    let p = ui.painter();
    p.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        theme::sans(14.0),
        theme::TEXT,
    );
    let bx = Rect::from_center_size(rect.right_center() - vec2(10.0, 0.0), vec2(20.0, 20.0));
    if *value {
        p.rect_filled(bx, 4.0, theme::ACCENT);
        let s = Stroke::new(2.0, theme::ON_ACCENT);
        p.line_segment(
            [
                bx.left_center() + vec2(4.0, 0.0),
                bx.center_bottom() - vec2(1.0, 5.0),
            ],
            s,
        );
        p.line_segment(
            [
                bx.center_bottom() - vec2(1.0, 5.0),
                bx.right_top() + vec2(-4.0, 5.0),
            ],
            s,
        );
    } else {
        let border = if resp.hovered() {
            theme::ACCENT
        } else {
            theme::BORDER_STRONG
        };
        p.rect_stroke(bx, 4.0, Stroke::new(1.5, border), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};

    const LOCAL: Rect = Rect {
        min: Pos2::new(340.0, 190.0),
        max: Pos2::new(620.0, 348.0),
    };
    const PEER: Vec2 = Vec2::new(210.0, 136.0);

    #[test]
    fn snap_picks_nearest_side() {
        assert_eq!(
            snap(egui::pos2(630.0, 200.0), LOCAL, PEER),
            (Side::Right, 10.0)
        );
        assert_eq!(
            snap(egui::pos2(100.0, 180.0), LOCAL, PEER),
            (Side::Left, -10.0)
        );
        assert_eq!(
            snap(egui::pos2(360.0, 20.0), LOCAL, PEER),
            (Side::Top, 20.0)
        );
        assert_eq!(
            snap(egui::pos2(300.0, 360.0), LOCAL, PEER),
            (Side::Bottom, -40.0)
        );
    }

    #[test]
    fn snap_clamps_to_min_overlap() {
        // Far below-right on the right side: overlap would be negative.
        let (side, off) = snap(egui::pos2(900.0, 340.0), LOCAL, PEER);
        assert_eq!(side, Side::Right);
        assert_eq!(off, LOCAL.height() - MIN_OVERLAP);
        let (side, off) = snap(egui::pos2(900.0, 40.0), LOCAL, PEER);
        assert_eq!(side, Side::Right);
        assert_eq!(off, -(PEER.y - MIN_OVERLAP));
        // The clamped placement really overlaps by exactly MIN_OVERLAP.
        let peer = Rect::from_min_size(placed(side, off, LOCAL, PEER), PEER);
        assert_eq!(shared_edge(side, LOCAL, peer).height(), MIN_OVERLAP);
    }

    #[test]
    fn mirror_flips_side_and_offset() {
        let a = Arrangement {
            side: Side::Left,
            offset: 120.0,
        };
        assert_eq!(
            mirror(a),
            Arrangement {
                side: Side::Right,
                offset: -120.0
            }
        );
        let top = Arrangement {
            side: Side::Top,
            offset: -3.5,
        };
        assert_eq!(mirror(top).side, Side::Bottom);
        for a in [a, top] {
            assert_eq!(mirror(mirror(a)), a);
        }
    }

    fn config(side: Side) -> Config {
        let mut config = Config::from_toml("static_keypair_hex = \"ab\"").expect("config");
        config.arrangement = Arrangement { side, offset: 0.0 };
        config
    }

    /// View, config, emitted actions, and whether the theme is installed.
    type State = (ArrangeView, Config, Vec<ArrangeAction>, bool);

    fn harness<'a>(side: Side) -> Harness<'a, State> {
        let mut view = ArrangeView::new(
            Desktop::new(0.0, 0.0, 1920.0, 1080.0),
            "this PC",
            Desktop::new(0.0, 0.0, 1512.0, 982.0),
            "Mac",
        );
        view.status = LinkStatus {
            linked: true,
            latency_ms: Some(3),
            focus_here: true,
        };
        Harness::builder()
            .with_size(vec2(960.0, 620.0))
            .build_ui_state(
                |ui, (view, config, actions, themed): &mut State| {
                    // Fonts land one frame after `apply`, as in the real app.
                    if !*themed {
                        theme::apply(ui.ctx());
                        *themed = true;
                        ui.ctx().request_repaint();
                        return;
                    }
                    actions.extend(view.ui(ui, config));
                },
                (view, config(side), Vec::new(), false),
            )
    }

    #[test]
    fn drag_right_then_save_emits_right() {
        let mut h = harness(Side::Left);
        h.run();
        h.get_by_label("Mac is LEFT of this PC");
        let tile = h.get_by_label_contains("Mac display").rect();
        let local_right = 960.0 - PANEL_WIDTH - 40.0;
        let start = tile.center();
        let end = egui::pos2(local_right, tile.center().y);
        h.drag_at(start);
        h.run();
        for i in 1..=5 {
            h.hover_at(start + (end - start) * i as f32 / 5.0);
            h.run();
        }
        h.drop_at(end);
        h.run();
        h.get_by_label("Mac is RIGHT of this PC");
        assert!(h.state().2.is_empty(), "nothing saved before Save");

        h.get_by_label("Save").click();
        h.run();
        let (_, config, actions, _) = h.state();
        let [ArrangeAction::Save(saved)] = actions.as_slice() else {
            panic!("expected one Save, got {actions:?}");
        };
        assert_eq!(saved.side, Side::Right);
        assert_eq!(config.arrangement, *saved);
    }

    #[test]
    fn arrow_keys_revert_and_receive() {
        let mut h = harness(Side::Right);
        h.run();
        h.key_press(Key::ArrowUp);
        h.run();
        h.get_by_label("Mac is ABOVE this PC");
        h.get_by_label("Revert").click();
        h.run();
        h.get_by_label("Mac is RIGHT of this PC");

        let (view, config, _, _) = h.state_mut();
        view.receive(
            config,
            Arrangement {
                side: Side::Right,
                offset: 40.0,
            },
        );
        h.run();
        h.get_by_label("Mac is LEFT of this PC");
        h.get_by_label("offset -40 pt");
    }

    #[test]
    fn pills_and_toggles() {
        let mut h = harness(Side::Right);
        h.run();
        h.get_by_label("Linked to Mac");
        h.get_by_label("encrypted · 3 ms");
        h.get_by_label("Focus: this PC");
        h.state_mut().0.status = LinkStatus {
            linked: false,
            latency_ms: None,
            focus_here: false,
        };
        h.run();
        h.get_by_label("Not linked");
        h.get_by_label("Focus: Mac");

        assert!(!h.state().1.start_at_login);
        h.get_by_label("Start at login").click();
        h.run();
        assert!(h.state().1.start_at_login);
    }
}
