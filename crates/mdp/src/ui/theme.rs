//! Locked visual tokens from `CONTEXT/ui-design.md` plus the bundled IBM Plex
//! fonts. Every window (Arrange, Pairing, Tray menus) calls [`apply`] once.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Theme,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x16);
pub const CANVAS: Color32 = Color32::from_rgb(0x16, 0x19, 0x1C);
/// Canvas dot grid (20 px pitch).
pub const CANVAS_DOT: Color32 = Color32::from_rgb(0x26, 0x2A, 0x30);
pub const PANEL: Color32 = Color32::from_rgb(0x1B, 0x1E, 0x22);
pub const RAISED: Color32 = Color32::from_rgb(0x23, 0x27, 0x2C);
pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x35, 0x3C);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x4A, 0x52, 0x5C);
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
pub const TEXT_2: Color32 = Color32::from_rgb(0xC5, 0xCB, 0xD2);
pub const MUTED: Color32 = Color32::from_rgb(0xA0, 0xA6, 0xAE);
pub const ACCENT: Color32 = Color32::from_rgb(0xF0, 0xA2, 0x3B);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1A, 0x12, 0x06);
/// Focus pill fill.
pub const ACCENT_BG: Color32 = Color32::from_rgb(0x3A, 0x2A, 0x12);
/// Attention card fill.
#[allow(dead_code)] // ponytail: T12 (#13) permissions screen uses it.
pub const ACCENT_BG_CARD: Color32 = Color32::from_rgb(0x23, 0x1D, 0x14);
pub const LINK: Color32 = Color32::from_rgb(0x5C, 0xC2, 0xB5);
pub const PEER_FILL: Color32 = Color32::from_rgb(0x39, 0x43, 0x50);
pub const PEER_BORDER: Color32 = Color32::from_rgb(0x8A, 0x96, 0xA4);

pub const RADIUS_CONTROL: u8 = 6;
pub const RADIUS_CARD: u8 = 8;
pub const RADIUS_MENU: u8 = 10;

const SANS_MEDIUM: &str = "plex-sans-medium";
const SANS_SEMIBOLD: &str = "plex-sans-semibold";
const MONO_MEDIUM: &str = "plex-mono-medium";

/// IBM Plex Sans 400.
pub fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}

/// IBM Plex Sans 500.
pub fn sans_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SANS_MEDIUM.into()))
}

/// IBM Plex Sans 600.
pub fn sans_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SANS_SEMIBOLD.into()))
}

/// IBM Plex Mono 400.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// IBM Plex Mono 500.
pub fn mono_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_MEDIUM.into()))
}

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 5] = [
        (
            "plex-sans",
            include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"),
        ),
        (
            SANS_MEDIUM,
            include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf"),
        ),
        (
            SANS_SEMIBOLD,
            include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
        ),
        (
            "plex-mono",
            include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf"),
        ),
        (
            MONO_MEDIUM,
            include_bytes!("../../assets/fonts/IBMPlexMono-Medium.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        defs.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // Plex first; egui's defaults stay as glyph fallbacks (symbols, CJK).
    let fallback = defs.families[&FontFamily::Proportional].clone();
    for (family, primary) in [
        (FontFamily::Proportional, "plex-sans"),
        (FontFamily::Monospace, "plex-mono"),
        (FontFamily::Name(SANS_MEDIUM.into()), SANS_MEDIUM),
        (FontFamily::Name(SANS_SEMIBOLD.into()), SANS_SEMIBOLD),
        (FontFamily::Name(MONO_MEDIUM.into()), MONO_MEDIUM),
    ] {
        let list = defs.families.entry(family).or_insert(fallback.clone());
        list.insert(0, primary.into());
    }
    defs
}

/// Install the Plex fonts and the dark "instrument panel" style.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(Theme::Dark);
    ctx.style_mut_of(Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Small, sans(12.0)),
            (TextStyle::Body, sans(14.0)),
            (TextStyle::Button, sans(14.0)),
            (TextStyle::Heading, sans_semibold(22.0)),
            (TextStyle::Monospace, mono(13.0)),
        ]
        .into();
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
        v.menu_corner_radius = CornerRadius::same(RADIUS_MENU);
        v.extreme_bg_color = CANVAS;
        v.faint_bg_color = RAISED;
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.0, ON_ACCENT);
        let w = &mut v.widgets;
        for (state, fill) in [
            (&mut w.noninteractive, PANEL),
            (&mut w.inactive, RAISED),
            (&mut w.hovered, RAISED),
            (&mut w.active, RAISED),
            (&mut w.open, RAISED),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.corner_radius = CornerRadius::same(RADIUS_CONTROL);
            state.fg_stroke = Stroke::new(1.5, TEXT);
            state.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_2);
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.bg_stroke = Stroke::new(1.5, ACCENT);
        // Checkmarks and focus rings read as the accent.
        w.inactive.fg_stroke = Stroke::new(2.0, ACCENT);
        w.hovered.fg_stroke = Stroke::new(2.0, ACCENT);
        w.active.fg_stroke = Stroke::new(2.0, ACCENT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = include_str!("../../../../CONTEXT/ui-design.md");

    fn hex(c: Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
    }

    #[test]
    fn tokens_match_ui_design_md() {
        let rows = [
            ("bg", vec![BG]),
            ("canvas", vec![CANVAS]),
            ("panel", vec![PANEL]),
            ("raised", vec![RAISED]),
            ("border", vec![BORDER]),
            ("border-strong", vec![BORDER_STRONG]),
            ("text", vec![TEXT]),
            ("text-2", vec![TEXT_2]),
            ("muted", vec![MUTED]),
            ("accent", vec![ACCENT]),
            ("on-accent", vec![ON_ACCENT]),
            ("accent-bg", vec![ACCENT_BG, ACCENT_BG_CARD]),
            ("link", vec![LINK]),
            ("peer tile", vec![PEER_FILL, PEER_BORDER]),
        ];
        for (token, colors) in rows {
            let row = SPEC
                .lines()
                .find(|line| line.starts_with(&format!("| {token} |")))
                .unwrap_or_else(|| panic!("ui-design.md has no `{token}` row"));
            for color in colors {
                assert!(
                    row.contains(&hex(color)),
                    "{token}: {} not in {row}",
                    hex(color)
                );
            }
        }
        let canvas = SPEC.lines().find(|l| l.starts_with("| canvas |")).unwrap();
        assert!(canvas.contains(&hex(CANVAS_DOT)));
    }

    #[test]
    fn apply_installs_plex_fonts() {
        let ctx = egui::Context::default();
        apply(&ctx);
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        let family = ctx.fonts(|f| f.definitions().families[&FontFamily::Proportional].clone());
        assert_eq!(family[0], "plex-sans");
    }
}
