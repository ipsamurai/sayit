//! Desktop UI: the menu-bar icon and the Settings window. macOS only for now.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::run;

/// Placeholder until the Linux tray (StatusNotifierItem) is built.
#[cfg(not(target_os = "macos"))]
pub fn run(_cfg: crate::config::Config, _verbose: bool) -> anyhow::Result<()> {
    anyhow::bail!("the menu-bar app is macOS-only for now; use `sayit run` on Linux")
}
