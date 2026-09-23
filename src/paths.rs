use std::path::PathBuf;

/// macOS: ~/Library/Application Support/sayit, Linux: $XDG_DATA_HOME/sayit.
/// Must match DATA_DIR in scripts/fetch-models.sh.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SAYIT_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_dir().expect("no data dir").join("sayit")
}

pub fn model_dir(name: &str) -> PathBuf {
    data_dir().join("models").join(name)
}

/// True when running as `sayit.app/Contents/MacOS/sayit`.
pub fn in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.ends_with("Contents/MacOS")))
        .unwrap_or(false)
}
