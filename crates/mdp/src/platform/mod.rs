//! OS implementations of [`mdp_core::Platform`], selected by `cfg(target_os)`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
