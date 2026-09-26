//! `mdp-core`: platform-free logic plus the [`platform::Platform`] trait.
//!
//! Durable decisions live in `CONTEXT/architecture.md`. Pure logic (protocol,
//! Arrangement geometry, role arbitration, keymap) stays platform-free and
//! unit-tested on any host; everything touching the OS goes through
//! [`platform::Platform`]; the `windows` and `macos` implementations live in
//! the `mdp` crate, selected by `cfg(target_os)`.

pub mod crossing;
pub mod link;
pub mod platform;
pub mod proto;

pub use platform::{
    Desktop, FakePlatform, InputEvent, MouseButton, PermissionStatus, Platform, PlatformError,
    CLIPBOARD_MAX_BYTES,
};
