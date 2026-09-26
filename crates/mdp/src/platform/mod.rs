//! OS implementations of [`mdp_core::Platform`], selected by `cfg(target_os)`.

// ponytail: stubs are unused until T5/T6 wire them into `mdp run`; drop this allow then.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
