//! OS implementations of [`mdp_core::Platform`], selected by `cfg(target_os)`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// This OS's [`mdp_core::Platform`] (what `mdp selftest` / `mdp run` construct).
#[cfg(target_os = "macos")]
pub type Native = macos::MacosPlatform;
/// This OS's [`mdp_core::Platform`] (what `mdp selftest` / `mdp run` construct).
#[cfg(target_os = "windows")]
pub type Native = windows::WindowsPlatform;
