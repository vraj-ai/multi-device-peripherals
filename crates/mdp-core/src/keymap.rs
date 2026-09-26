//! USB HID keymaps: Windows set-1 scancodes and macOS virtual keycodes ⇄
//! USB HID keyboard usages (page `0x07`).
//!
//! Keys travel the Link as physical HID usages so the Sink injects them
//! under its own OS layout, as if the keyboard were plugged in there. These
//! tables translate at the edges: capture (OS code → HID) and injection
//! (HID → OS code). All lookups are `match`-based with no allocation;
//! unknown codes return [`None`] (callers drop and log them once).
//!
//! Conventions:
//! - Windows `extended` is the `E0` prefix flag. `(0x37, false)` is numpad
//!   `*` while `(0x37, true)` is Print Screen; `(0x45, false)` is Num Lock
//!   while `(0x45, true)` means Pause (an approximation: real Pause arrives
//!   as an `E1` sequence, which the capture hook reports via this flag).
//! - Media transport keys other than mute/volume/stop live on the HID
//!   consumer page (`0x0C`), outside page `0x07`, so they map to [`None`].
//! - The ISO extra key (Windows scancode `0x56`, macOS `0x0A`) maps to HID
//!   `0x64` on both sides; JIS Ro/Yen/Henkan/Muhenkan/Kana map to the same
//!   `0x87`–`0x8B` usages on both sides, so ISO/JIS keys survive a Crossing.

/// Windows set-1 scan code (+ `E0` `extended` flag) to USB HID usage.
pub fn windows_scancode_to_hid(scancode: u16, extended: bool) -> Option<u16> {
    match (scancode, extended) {
        // Esc and the digit row.
        (0x01, false) => Some(0x29),
        (0x02, false) => Some(0x1E),
        (0x03, false) => Some(0x1F),
        (0x04, false) => Some(0x20),
        (0x05, false) => Some(0x21),
        (0x06, false) => Some(0x22),
        (0x07, false) => Some(0x23),
        (0x08, false) => Some(0x24),
        (0x09, false) => Some(0x25),
        (0x0A, false) => Some(0x26),
        (0x0B, false) => Some(0x27),
        (0x0C, false) => Some(0x2D),
        (0x0D, false) => Some(0x2E),
        (0x0E, false) => Some(0x2A),
        // Tab row.
        (0x0F, false) => Some(0x2B),
        (0x10, false) => Some(0x14),
        (0x11, false) => Some(0x1A),
        (0x12, false) => Some(0x08),
        (0x13, false) => Some(0x15),
        (0x14, false) => Some(0x17),
        (0x15, false) => Some(0x1C),
        (0x16, false) => Some(0x18),
        (0x17, false) => Some(0x0C),
        (0x18, false) => Some(0x12),
        (0x19, false) => Some(0x13),
        (0x1A, false) => Some(0x2F),
        (0x1B, false) => Some(0x30),
        (0x1C, false) => Some(0x28),
        (0x1D, false) => Some(0xE0),
        // Caps row.
        (0x1E, false) => Some(0x04),
        (0x1F, false) => Some(0x16),
        (0x20, false) => Some(0x07),
        (0x21, false) => Some(0x09),
        (0x22, false) => Some(0x0A),
        (0x23, false) => Some(0x0B),
        (0x24, false) => Some(0x0D),
        (0x25, false) => Some(0x0E),
        (0x26, false) => Some(0x0F),
        (0x27, false) => Some(0x33),
        (0x28, false) => Some(0x34),
        (0x29, false) => Some(0x35),
        // Shift row.
        (0x2A, false) => Some(0xE1),
        (0x2B, false) => Some(0x31),
        (0x2C, false) => Some(0x1D),
        (0x2D, false) => Some(0x1B),
        (0x2E, false) => Some(0x06),
        (0x2F, false) => Some(0x19),
        (0x30, false) => Some(0x05),
        (0x31, false) => Some(0x11),
        (0x32, false) => Some(0x10),
        (0x33, false) => Some(0x36),
        (0x34, false) => Some(0x37),
        (0x35, false) => Some(0x38),
        (0x36, false) => Some(0xE5),
        // Bottom row and function keys.
        (0x37, false) => Some(0x55),
        (0x38, false) => Some(0xE2),
        (0x39, false) => Some(0x2C),
        (0x3A, false) => Some(0x39),
        (0x3B, false) => Some(0x3A),
        (0x3C, false) => Some(0x3B),
        (0x3D, false) => Some(0x3C),
        (0x3E, false) => Some(0x3D),
        (0x3F, false) => Some(0x3E),
        (0x40, false) => Some(0x3F),
        (0x41, false) => Some(0x40),
        (0x42, false) => Some(0x41),
        (0x43, false) => Some(0x42),
        (0x44, false) => Some(0x43),
        (0x45, false) => Some(0x53),
        (0x46, false) => Some(0x47),
        // Numpad.
        (0x47, false) => Some(0x5F),
        (0x48, false) => Some(0x60),
        (0x49, false) => Some(0x61),
        (0x4A, false) => Some(0x56),
        (0x4B, false) => Some(0x5C),
        (0x4C, false) => Some(0x5D),
        (0x4D, false) => Some(0x5E),
        (0x4E, false) => Some(0x57),
        (0x4F, false) => Some(0x59),
        (0x50, false) => Some(0x5A),
        (0x51, false) => Some(0x5B),
        (0x52, false) => Some(0x62),
        (0x53, false) => Some(0x63),
        // ISO extra key and F11/F12.
        (0x56, false) => Some(0x64),
        (0x57, false) => Some(0x44),
        (0x58, false) => Some(0x45),
        // F13-F24.
        (0x64, false) => Some(0x68),
        (0x65, false) => Some(0x69),
        (0x66, false) => Some(0x6A),
        (0x67, false) => Some(0x6B),
        (0x68, false) => Some(0x6C),
        (0x69, false) => Some(0x6D),
        (0x6A, false) => Some(0x6E),
        (0x6B, false) => Some(0x6F),
        (0x6C, false) => Some(0x70),
        (0x6D, false) => Some(0x71),
        (0x6E, false) => Some(0x72),
        (0x6F, false) => Some(0x73),
        // JIS keys.
        (0x70, false) => Some(0x88),
        (0x73, false) => Some(0x87),
        (0x79, false) => Some(0x8A),
        (0x7B, false) => Some(0x8B),
        (0x7D, false) => Some(0x89),
        // E0 media keys (page 0x07 only; transport keys use 0x0C).
        (0x20, true) => Some(0x7F),
        (0x2E, true) => Some(0x81),
        (0x30, true) => Some(0x80),
        (0x24, true) => Some(0x78),
        // E0 numpad Enter and slash.
        (0x1C, true) => Some(0x58),
        (0x35, true) => Some(0x54),
        // E0 nav cluster.
        (0x52, true) => Some(0x49),
        (0x53, true) => Some(0x4C),
        (0x47, true) => Some(0x4A),
        (0x4F, true) => Some(0x4D),
        (0x49, true) => Some(0x4B),
        (0x51, true) => Some(0x4E),
        (0x48, true) => Some(0x52),
        (0x50, true) => Some(0x51),
        (0x4B, true) => Some(0x50),
        (0x4D, true) => Some(0x4F),
        // E0 Print Screen and Pause (see module docs).
        (0x37, true) => Some(0x46),
        (0x45, true) => Some(0x48),
        // E0 right modifiers and Windows keys.
        (0x1D, true) => Some(0xE4),
        (0x38, true) => Some(0xE6),
        (0x5B, true) => Some(0xE3),
        (0x5C, true) => Some(0xE7),
        (0x5D, true) => Some(0x65),
        _ => None,
    }
}

/// USB HID usage to Windows set-1 scan code + `E0` `extended` flag.
pub fn hid_to_windows_scancode(hid: u16) -> Option<(u16, bool)> {
    match hid {
        0x29 => Some((0x01, false)),
        0x1E => Some((0x02, false)),
        0x1F => Some((0x03, false)),
        0x20 => Some((0x04, false)),
        0x21 => Some((0x05, false)),
        0x22 => Some((0x06, false)),
        0x23 => Some((0x07, false)),
        0x24 => Some((0x08, false)),
        0x25 => Some((0x09, false)),
        0x26 => Some((0x0A, false)),
        0x27 => Some((0x0B, false)),
        0x2D => Some((0x0C, false)),
        0x2E => Some((0x0D, false)),
        0x2A => Some((0x0E, false)),
        0x2B => Some((0x0F, false)),
        0x14 => Some((0x10, false)),
        0x1A => Some((0x11, false)),
        0x08 => Some((0x12, false)),
        0x15 => Some((0x13, false)),
        0x17 => Some((0x14, false)),
        0x1C => Some((0x15, false)),
        0x18 => Some((0x16, false)),
        0x0C => Some((0x17, false)),
        0x12 => Some((0x18, false)),
        0x13 => Some((0x19, false)),
        0x2F => Some((0x1A, false)),
        0x30 => Some((0x1B, false)),
        0x28 => Some((0x1C, false)),
        0xE0 => Some((0x1D, false)),
        0x04 => Some((0x1E, false)),
        0x16 => Some((0x1F, false)),
        0x07 => Some((0x20, false)),
        0x09 => Some((0x21, false)),
        0x0A => Some((0x22, false)),
        0x0B => Some((0x23, false)),
        0x0D => Some((0x24, false)),
        0x0E => Some((0x25, false)),
        0x0F => Some((0x26, false)),
        0x33 => Some((0x27, false)),
        0x34 => Some((0x28, false)),
        0x35 => Some((0x29, false)),
        0xE1 => Some((0x2A, false)),
        0x31 => Some((0x2B, false)),
        0x1D => Some((0x2C, false)),
        0x1B => Some((0x2D, false)),
        0x06 => Some((0x2E, false)),
        0x19 => Some((0x2F, false)),
        0x05 => Some((0x30, false)),
        0x11 => Some((0x31, false)),
        0x10 => Some((0x32, false)),
        0x36 => Some((0x33, false)),
        0x37 => Some((0x34, false)),
        0x38 => Some((0x35, false)),
        0xE5 => Some((0x36, false)),
        0x55 => Some((0x37, false)),
        0xE2 => Some((0x38, false)),
        0x2C => Some((0x39, false)),
        0x39 => Some((0x3A, false)),
        0x3A => Some((0x3B, false)),
        0x3B => Some((0x3C, false)),
        0x3C => Some((0x3D, false)),
        0x3D => Some((0x3E, false)),
        0x3E => Some((0x3F, false)),
        0x3F => Some((0x40, false)),
        0x40 => Some((0x41, false)),
        0x41 => Some((0x42, false)),
        0x42 => Some((0x43, false)),
        0x43 => Some((0x44, false)),
        0x53 => Some((0x45, false)),
        0x47 => Some((0x46, false)),
        0x5F => Some((0x47, false)),
        0x60 => Some((0x48, false)),
        0x61 => Some((0x49, false)),
        0x56 => Some((0x4A, false)),
        0x5C => Some((0x4B, false)),
        0x5D => Some((0x4C, false)),
        0x5E => Some((0x4D, false)),
        0x57 => Some((0x4E, false)),
        0x59 => Some((0x4F, false)),
        0x5A => Some((0x50, false)),
        0x5B => Some((0x51, false)),
        0x62 => Some((0x52, false)),
        0x63 => Some((0x53, false)),
        0x64 => Some((0x56, false)),
        0x44 => Some((0x57, false)),
        0x45 => Some((0x58, false)),
        0x68 => Some((0x64, false)),
        0x69 => Some((0x65, false)),
        0x6A => Some((0x66, false)),
        0x6B => Some((0x67, false)),
        0x6C => Some((0x68, false)),
        0x6D => Some((0x69, false)),
        0x6E => Some((0x6A, false)),
        0x6F => Some((0x6B, false)),
        0x70 => Some((0x6C, false)),
        0x71 => Some((0x6D, false)),
        0x72 => Some((0x6E, false)),
        0x73 => Some((0x6F, false)),
        0x88 => Some((0x70, false)),
        0x87 => Some((0x73, false)),
        0x8A => Some((0x79, false)),
        0x8B => Some((0x7B, false)),
        0x89 => Some((0x7D, false)),
        0x7F => Some((0x20, true)),
        0x81 => Some((0x2E, true)),
        0x80 => Some((0x30, true)),
        0x78 => Some((0x24, true)),
        0x58 => Some((0x1C, true)),
        0x54 => Some((0x35, true)),
        0x49 => Some((0x52, true)),
        0x4C => Some((0x53, true)),
        0x4A => Some((0x47, true)),
        0x4D => Some((0x4F, true)),
        0x4B => Some((0x49, true)),
        0x4E => Some((0x51, true)),
        0x52 => Some((0x48, true)),
        0x51 => Some((0x50, true)),
        0x50 => Some((0x4B, true)),
        0x4F => Some((0x4D, true)),
        0x46 => Some((0x37, true)),
        0x48 => Some((0x45, true)),
        0xE4 => Some((0x1D, true)),
        0xE6 => Some((0x38, true)),
        0xE3 => Some((0x5B, true)),
        0xE7 => Some((0x5C, true)),
        0x65 => Some((0x5D, true)),
        _ => None,
    }
}

/// macOS virtual keycode to USB HID usage.
pub fn mac_keycode_to_hid(keycode: u16) -> Option<u16> {
    match keycode {
        // Letters (ANSI positions).
        0x00 => Some(0x04),
        0x01 => Some(0x16),
        0x02 => Some(0x07),
        0x03 => Some(0x09),
        0x04 => Some(0x0B),
        0x05 => Some(0x0A),
        0x06 => Some(0x1D),
        0x07 => Some(0x1B),
        0x08 => Some(0x06),
        0x09 => Some(0x19),
        0x0B => Some(0x05),
        0x0C => Some(0x14),
        0x0D => Some(0x1A),
        0x0E => Some(0x08),
        0x0F => Some(0x15),
        0x10 => Some(0x1C),
        0x11 => Some(0x17),
        0x1F => Some(0x12),
        0x20 => Some(0x18),
        0x22 => Some(0x0C),
        0x23 => Some(0x13),
        0x26 => Some(0x0D),
        0x28 => Some(0x0E),
        0x25 => Some(0x0F),
        0x2D => Some(0x11),
        0x2E => Some(0x10),
        // Digits.
        0x12 => Some(0x1E),
        0x13 => Some(0x1F),
        0x14 => Some(0x20),
        0x15 => Some(0x21),
        0x17 => Some(0x22),
        0x16 => Some(0x23),
        0x1A => Some(0x24),
        0x1C => Some(0x25),
        0x19 => Some(0x26),
        0x1D => Some(0x27),
        // Punctuation.
        0x18 => Some(0x2E),
        0x1B => Some(0x2D),
        0x1E => Some(0x30),
        0x21 => Some(0x2F),
        0x27 => Some(0x34),
        0x29 => Some(0x33),
        0x2A => Some(0x31),
        0x2B => Some(0x36),
        0x2C => Some(0x38),
        0x2F => Some(0x37),
        0x32 => Some(0x35),
        0x0A => Some(0x64),
        // Control keys.
        0x30 => Some(0x2B),
        0x31 => Some(0x2C),
        0x33 => Some(0x2A),
        0x24 => Some(0x28),
        0x35 => Some(0x29),
        0x39 => Some(0x39),
        // Modifiers (no right GUI on Mac keyboards).
        0x37 => Some(0xE3),
        0x38 => Some(0xE1),
        0x3A => Some(0xE2),
        0x3B => Some(0xE0),
        0x3C => Some(0xE5),
        0x3D => Some(0xE6),
        0x3E => Some(0xE4),
        // F1-F12.
        0x7A => Some(0x3A),
        0x78 => Some(0x3B),
        0x63 => Some(0x3C),
        0x76 => Some(0x3D),
        0x60 => Some(0x3E),
        0x61 => Some(0x3F),
        0x62 => Some(0x40),
        0x64 => Some(0x41),
        0x65 => Some(0x42),
        0x6D => Some(0x43),
        0x67 => Some(0x44),
        0x6F => Some(0x45),
        // F13-F20.
        0x69 => Some(0x68),
        0x6B => Some(0x69),
        0x71 => Some(0x6A),
        0x6A => Some(0x6B),
        0x40 => Some(0x6C),
        0x4F => Some(0x6D),
        0x50 => Some(0x6E),
        0x5A => Some(0x6F),
        // Arrows and nav cluster.
        0x7B => Some(0x50),
        0x7C => Some(0x4F),
        0x7D => Some(0x51),
        0x7E => Some(0x52),
        0x72 => Some(0x49),
        0x73 => Some(0x4A),
        0x74 => Some(0x4B),
        0x75 => Some(0x4C),
        0x77 => Some(0x4D),
        0x79 => Some(0x4E),
        // Keypad.
        0x41 => Some(0x63),
        0x43 => Some(0x55),
        0x45 => Some(0x57),
        0x47 => Some(0x53),
        0x4B => Some(0x54),
        0x4C => Some(0x58),
        0x4E => Some(0x56),
        0x51 => Some(0x67),
        0x52 => Some(0x62),
        0x53 => Some(0x59),
        0x54 => Some(0x5A),
        0x55 => Some(0x5B),
        0x56 => Some(0x5C),
        0x57 => Some(0x5D),
        0x58 => Some(0x5E),
        0x59 => Some(0x5F),
        0x5B => Some(0x60),
        0x5C => Some(0x61),
        0x5F => Some(0x85),
        // Volume keys.
        0x48 => Some(0x80),
        0x49 => Some(0x81),
        0x4A => Some(0x7F),
        // JIS keys.
        0x5D => Some(0x89),
        0x5E => Some(0x87),
        0x66 => Some(0x90),
        0x68 => Some(0x88),
        _ => None,
    }
}

/// USB HID usage to macOS virtual keycode.
pub fn hid_to_mac_keycode(hid: u16) -> Option<u16> {
    match hid {
        0x04 => Some(0x00),
        0x16 => Some(0x01),
        0x07 => Some(0x02),
        0x09 => Some(0x03),
        0x0B => Some(0x04),
        0x0A => Some(0x05),
        0x1D => Some(0x06),
        0x1B => Some(0x07),
        0x06 => Some(0x08),
        0x19 => Some(0x09),
        0x05 => Some(0x0B),
        0x14 => Some(0x0C),
        0x1A => Some(0x0D),
        0x08 => Some(0x0E),
        0x15 => Some(0x0F),
        0x1C => Some(0x10),
        0x17 => Some(0x11),
        0x12 => Some(0x1F),
        0x18 => Some(0x20),
        0x0C => Some(0x22),
        0x13 => Some(0x23),
        0x0D => Some(0x26),
        0x0E => Some(0x28),
        0x0F => Some(0x25),
        0x11 => Some(0x2D),
        0x10 => Some(0x2E),
        0x1E => Some(0x12),
        0x1F => Some(0x13),
        0x20 => Some(0x14),
        0x21 => Some(0x15),
        0x22 => Some(0x17),
        0x23 => Some(0x16),
        0x24 => Some(0x1A),
        0x25 => Some(0x1C),
        0x26 => Some(0x19),
        0x27 => Some(0x1D),
        0x2E => Some(0x18),
        0x2D => Some(0x1B),
        0x30 => Some(0x1E),
        0x2F => Some(0x21),
        0x34 => Some(0x27),
        0x33 => Some(0x29),
        0x31 => Some(0x2A),
        0x36 => Some(0x2B),
        0x38 => Some(0x2C),
        0x37 => Some(0x2F),
        0x35 => Some(0x32),
        0x64 => Some(0x0A),
        0x2B => Some(0x30),
        0x2C => Some(0x31),
        0x2A => Some(0x33),
        0x28 => Some(0x24),
        0x29 => Some(0x35),
        0x39 => Some(0x39),
        0xE3 => Some(0x37),
        0xE1 => Some(0x38),
        0xE2 => Some(0x3A),
        0xE0 => Some(0x3B),
        0xE5 => Some(0x3C),
        0xE6 => Some(0x3D),
        0xE4 => Some(0x3E),
        0x3A => Some(0x7A),
        0x3B => Some(0x78),
        0x3C => Some(0x63),
        0x3D => Some(0x76),
        0x3E => Some(0x60),
        0x3F => Some(0x61),
        0x40 => Some(0x62),
        0x41 => Some(0x64),
        0x42 => Some(0x65),
        0x43 => Some(0x6D),
        0x44 => Some(0x67),
        0x45 => Some(0x6F),
        0x68 => Some(0x69),
        0x69 => Some(0x6B),
        0x6A => Some(0x71),
        0x6B => Some(0x6A),
        0x6C => Some(0x40),
        0x6D => Some(0x4F),
        0x6E => Some(0x50),
        0x6F => Some(0x5A),
        0x50 => Some(0x7B),
        0x4F => Some(0x7C),
        0x51 => Some(0x7D),
        0x52 => Some(0x7E),
        0x49 => Some(0x72),
        0x4A => Some(0x73),
        0x4B => Some(0x74),
        0x4C => Some(0x75),
        0x4D => Some(0x77),
        0x4E => Some(0x79),
        0x63 => Some(0x41),
        0x55 => Some(0x43),
        0x57 => Some(0x45),
        0x53 => Some(0x47),
        0x54 => Some(0x4B),
        0x58 => Some(0x4C),
        0x56 => Some(0x4E),
        0x67 => Some(0x51),
        0x62 => Some(0x52),
        0x59 => Some(0x53),
        0x5A => Some(0x54),
        0x5B => Some(0x55),
        0x5C => Some(0x56),
        0x5D => Some(0x57),
        0x5E => Some(0x58),
        0x5F => Some(0x59),
        0x60 => Some(0x5B),
        0x61 => Some(0x5C),
        0x85 => Some(0x5F),
        0x80 => Some(0x48),
        0x81 => Some(0x49),
        0x7F => Some(0x4A),
        0x89 => Some(0x5D),
        0x87 => Some(0x5E),
        0x90 => Some(0x66),
        0x88 => Some(0x68),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Representative Windows mappings: every row, modifiers, nav, numpad,
    /// media, and ISO/JIS extras.
    const WINDOWS_CASES: &[((u16, bool), u16)] = &[
        ((0x01, false), 0x29),
        ((0x02, false), 0x1E),
        ((0x0B, false), 0x27),
        ((0x0C, false), 0x2D),
        ((0x0D, false), 0x2E),
        ((0x0E, false), 0x2A),
        ((0x0F, false), 0x2B),
        ((0x10, false), 0x14),
        ((0x19, false), 0x13),
        ((0x1A, false), 0x2F),
        ((0x1B, false), 0x30),
        ((0x1C, false), 0x28),
        ((0x1D, false), 0xE0),
        ((0x1E, false), 0x04),
        ((0x26, false), 0x0F),
        ((0x27, false), 0x33),
        ((0x28, false), 0x34),
        ((0x29, false), 0x35),
        ((0x2A, false), 0xE1),
        ((0x2B, false), 0x31),
        ((0x2C, false), 0x1D),
        ((0x32, false), 0x10),
        ((0x33, false), 0x36),
        ((0x35, false), 0x38),
        ((0x36, false), 0xE5),
        ((0x37, false), 0x55),
        ((0x38, false), 0xE2),
        ((0x39, false), 0x2C),
        ((0x3A, false), 0x39),
        ((0x3B, false), 0x3A),
        ((0x44, false), 0x43),
        ((0x45, false), 0x53),
        ((0x46, false), 0x47),
        ((0x47, false), 0x5F),
        ((0x4C, false), 0x5D),
        ((0x52, false), 0x62),
        ((0x53, false), 0x63),
        ((0x56, false), 0x64),
        ((0x57, false), 0x44),
        ((0x58, false), 0x45),
        ((0x64, false), 0x68),
        ((0x6B, false), 0x6F),
        ((0x6F, false), 0x73),
        ((0x70, false), 0x88),
        ((0x73, false), 0x87),
        ((0x79, false), 0x8A),
        ((0x7B, false), 0x8B),
        ((0x7D, false), 0x89),
        ((0x20, true), 0x7F),
        ((0x2E, true), 0x81),
        ((0x30, true), 0x80),
        ((0x24, true), 0x78),
        ((0x1C, true), 0x58),
        ((0x35, true), 0x54),
        ((0x52, true), 0x49),
        ((0x53, true), 0x4C),
        ((0x47, true), 0x4A),
        ((0x4F, true), 0x4D),
        ((0x49, true), 0x4B),
        ((0x51, true), 0x4E),
        ((0x48, true), 0x52),
        ((0x50, true), 0x51),
        ((0x4B, true), 0x50),
        ((0x4D, true), 0x4F),
        ((0x37, true), 0x46),
        ((0x45, true), 0x48),
        ((0x1D, true), 0xE4),
        ((0x38, true), 0xE6),
        ((0x5B, true), 0xE3),
        ((0x5C, true), 0xE7),
        ((0x5D, true), 0x65),
    ];

    /// Representative macOS mappings across the whole keyboard.
    const MAC_CASES: &[(u16, u16)] = &[
        (0x00, 0x04),
        (0x06, 0x1D),
        (0x0B, 0x05),
        (0x12, 0x1E),
        (0x1D, 0x27),
        (0x18, 0x2E),
        (0x1B, 0x2D),
        (0x1E, 0x30),
        (0x21, 0x2F),
        (0x27, 0x34),
        (0x29, 0x33),
        (0x2A, 0x31),
        (0x2B, 0x36),
        (0x2C, 0x38),
        (0x2F, 0x37),
        (0x32, 0x35),
        (0x0A, 0x64),
        (0x30, 0x2B),
        (0x31, 0x2C),
        (0x33, 0x2A),
        (0x24, 0x28),
        (0x35, 0x29),
        (0x39, 0x39),
        (0x37, 0xE3),
        (0x38, 0xE1),
        (0x3A, 0xE2),
        (0x3B, 0xE0),
        (0x3C, 0xE5),
        (0x3D, 0xE6),
        (0x3E, 0xE4),
        (0x7A, 0x3A),
        (0x6F, 0x45),
        (0x69, 0x68),
        (0x5A, 0x6F),
        (0x7B, 0x50),
        (0x7C, 0x4F),
        (0x7D, 0x51),
        (0x7E, 0x52),
        (0x72, 0x49),
        (0x73, 0x4A),
        (0x74, 0x4B),
        (0x75, 0x4C),
        (0x77, 0x4D),
        (0x79, 0x4E),
        (0x41, 0x63),
        (0x43, 0x55),
        (0x4C, 0x58),
        (0x51, 0x67),
        (0x52, 0x62),
        (0x5F, 0x85),
        (0x48, 0x80),
        (0x49, 0x81),
        (0x4A, 0x7F),
        (0x5D, 0x89),
        (0x5E, 0x87),
        (0x66, 0x90),
        (0x68, 0x88),
    ];

    #[test]
    fn windows_round_trip_exhaustive() {
        // Every mapped entry returns to itself; nothing else is assumed.
        for scancode in 0..=u16::MAX {
            for extended in [false, true] {
                if let Some(hid) = windows_scancode_to_hid(scancode, extended) {
                    assert_eq!(
                        hid_to_windows_scancode(hid),
                        Some((scancode, extended)),
                        "scancode {scancode:#04X} extended {extended}"
                    );
                }
            }
        }
        for hid in 0..=u16::MAX {
            if let Some((scancode, extended)) = hid_to_windows_scancode(hid) {
                assert_eq!(
                    windows_scancode_to_hid(scancode, extended),
                    Some(hid),
                    "HID {hid:#04X}"
                );
            }
        }
    }

    #[test]
    fn mac_round_trip_exhaustive() {
        for keycode in 0..=u16::MAX {
            if let Some(hid) = mac_keycode_to_hid(keycode) {
                assert_eq!(
                    hid_to_mac_keycode(hid),
                    Some(keycode),
                    "keycode {keycode:#04X}"
                );
            }
        }
        for hid in 0..=u16::MAX {
            if let Some(keycode) = hid_to_mac_keycode(hid) {
                assert_eq!(mac_keycode_to_hid(keycode), Some(hid), "HID {hid:#04X}");
            }
        }
    }

    #[test]
    fn windows_known_keys() {
        for &((scancode, extended), hid) in WINDOWS_CASES {
            assert_eq!(
                windows_scancode_to_hid(scancode, extended),
                Some(hid),
                "scancode {scancode:#04X} extended {extended}"
            );
            assert_eq!(
                hid_to_windows_scancode(hid),
                Some((scancode, extended)),
                "HID {hid:#04X}"
            );
        }
    }

    #[test]
    fn mac_known_keys() {
        for &(keycode, hid) in MAC_CASES {
            assert_eq!(
                mac_keycode_to_hid(keycode),
                Some(hid),
                "keycode {keycode:#04X}"
            );
            assert_eq!(hid_to_mac_keycode(hid), Some(keycode), "HID {hid:#04X}");
        }
    }

    #[test]
    fn cross_os_modifiers_and_letters() {
        // Win key = Cmd on the Mac; Alt = Option; Ctrl = Control.
        assert_eq!(windows_scancode_to_hid(0x5B, true), Some(0xE3));
        assert_eq!(hid_to_mac_keycode(0xE3), Some(0x37));
        assert_eq!(windows_scancode_to_hid(0x38, false), Some(0xE2));
        assert_eq!(hid_to_mac_keycode(0xE2), Some(0x3A));
        assert_eq!(windows_scancode_to_hid(0x1D, false), Some(0xE0));
        assert_eq!(hid_to_mac_keycode(0xE0), Some(0x3B));
        assert_eq!(windows_scancode_to_hid(0x2A, false), Some(0xE1));
        assert_eq!(hid_to_mac_keycode(0xE1), Some(0x38));
        // Letters and digits agree both ways.
        for (scancode, keycode, hid) in [(0x1E, 0x00, 0x04), (0x02, 0x12, 0x1E), (0x1C, 0x24, 0x28)]
        {
            assert_eq!(windows_scancode_to_hid(scancode, false), Some(hid));
            assert_eq!(mac_keycode_to_hid(keycode), Some(hid));
            assert_eq!(hid_to_windows_scancode(hid), Some((scancode, false)));
            assert_eq!(hid_to_mac_keycode(hid), Some(keycode));
        }
        // Right Win has no Mac counterpart; it stays Windows-only.
        assert_eq!(hid_to_windows_scancode(0xE7), Some((0x5C, true)));
        assert_eq!(hid_to_mac_keycode(0xE7), None);
        // ISO/JIS extras agree across both OSes.
        assert_eq!(windows_scancode_to_hid(0x56, false), Some(0x64));
        assert_eq!(mac_keycode_to_hid(0x0A), Some(0x64));
        assert_eq!(windows_scancode_to_hid(0x73, false), Some(0x87));
        assert_eq!(mac_keycode_to_hid(0x5E), Some(0x87));
        assert_eq!(windows_scancode_to_hid(0x7D, false), Some(0x89));
        assert_eq!(mac_keycode_to_hid(0x5D), Some(0x89));
        // Volume keys agree across both OSes.
        assert_eq!(windows_scancode_to_hid(0x20, true), Some(0x7F));
        assert_eq!(mac_keycode_to_hid(0x4A), Some(0x7F));
    }

    #[test]
    fn unknown_codes_return_none() {
        for (scancode, extended) in [
            (0x00, false),
            (0xFF, false),
            (0x00, true),
            (0x22, true),
            (0x7F, false),
        ] {
            assert_eq!(
                windows_scancode_to_hid(scancode, extended),
                None,
                "scancode {scancode:#04X} extended {extended}"
            );
        }
        for hid in [0x00, 0x03, 0x32, 0x66, 0x90, 0x9A, 0xE8, 0xFFFF] {
            assert_eq!(
                hid_to_windows_scancode(hid),
                None,
                "HID {hid:#04X} has no Windows code"
            );
        }
        // 0x90 (LANG1) and 0x67 (keypad =) exist only on the Mac side.
        assert_eq!(hid_to_mac_keycode(0x90), Some(0x66));
        assert_eq!(hid_to_mac_keycode(0x67), Some(0x51));
        for keycode in [0x34, 0x3F, 0x42, 0xFF] {
            assert_eq!(mac_keycode_to_hid(keycode), None, "keycode {keycode:#04X}");
        }
        for hid in [0x00, 0x32, 0x65, 0xE7, 0xFFFF] {
            assert_eq!(
                hid_to_mac_keycode(hid),
                None,
                "HID {hid:#04X} has no Mac code"
            );
        }
    }
}
