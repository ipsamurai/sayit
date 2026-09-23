//! Where sayit keeps its data.

use std::fs::{DirBuilder, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// macOS: ~/Library/Application Support/sayit, Linux: $XDG_DATA_HOME/sayit.
/// Must match DATA_DIR in scripts/fetch-models.sh.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SAYIT_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_dir().expect("no data dir").join("sayit")
}

/// Creates the data dir if needed, readable only by the user.
pub fn ensure_data_dir() -> Result<PathBuf> {
    let dir = data_dir();
    create_private_dir(&dir)?;
    Ok(dir)
}

/// Creates `dir` and any missing parents, readable only by the user. On
/// macOS the config and data dirs are the same folder, so both go through here.
pub fn create_private_dir(dir: &Path) -> Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    Ok(())
}

/// Takes an exclusive lock that lasts as long as the returned file is open.
/// A second copy of sayit fails here instead of adding a second hotkey
/// listener, which would paste everything twice.
pub fn single_instance_lock() -> Result<File> {
    lock(&ensure_data_dir()?.join("sayit.lock"))
}

fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => bail!("sayit is already running (see the menu bar)"),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn second_lock_is_refused_until_the_first_is_dropped() {
        let path = std::env::temp_dir().join(format!("sayit-lock-test-{}", std::process::id()));
        let first = super::lock(&path).unwrap();
        assert!(super::lock(&path).is_err());
        drop(first);
        assert!(super::lock(&path).is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn private_dirs_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("sayit-dir-test-{}", std::process::id()));
        super::create_private_dir(&dir.join("sayit")).unwrap();
        for d in [&dir, &dir.join("sayit")] {
            let mode = std::fs::metadata(d).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{}", d.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
