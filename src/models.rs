//! Downloading and removing speech models from inside the app.
//!
//! sayit itself has no networking code: downloads run the same
//! `fetch-models.sh` that users can run by hand (system `curl`, pinned URLs,
//! SHA-256 checks). The app only passes a model name from a fixed list and
//! watches the files grow.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::paths;
use crate::stt::ModelId;

/// Environment variables passed through to the script, so downloads work
/// behind a proxy. Everything else is dropped, so nothing in the user's
/// environment (e.g. BASH_ENV) can change what the script does.
const PASSTHROUGH_ENV: [&str; 7] = [
    "HOME",
    "https_proxy",
    "HTTPS_PROXY",
    "http_proxy",
    "HTTP_PROXY",
    "all_proxy",
    "no_proxy",
];

/// A download in progress. Dropping it doesn't stop it; call `cancel`.
pub struct Download {
    pid: u32,
    state: Arc<Mutex<State>>,
}

/// Guarded by one lock so `cancel` can never signal a process that has
/// already exited (its pid could belong to something else by then).
#[derive(Default)]
struct State {
    exited: bool,
    cancelled: bool,
}

impl Download {
    /// Stops the download. The caller removes the partial files once it's
    /// reported as `Finished::Cancelled` (see `delete`).
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.exited {
            return;
        }
        state.cancelled = true;
        // SAFETY: plain syscall. The script runs in its own process group
        // (see `start`) that hasn't been reaped yet, so this stops the
        // script and its curl and nothing else.
        unsafe { libc::killpg(self.pid as libc::pid_t, libc::SIGTERM) };
    }
}

/// Outcome of a download, reported once it ends.
pub enum Finished {
    Installed,
    Cancelled,
    Failed(String),
}

/// Starts downloading `model` in the background. `progress` gets a fraction
/// from 0 to 1 a few times a second; `done` is called once at the end. Both
/// run on a background thread.
pub fn start(
    model: ModelId,
    progress: impl Fn(f64) + Send + 'static,
    done: impl FnOnce(Finished) + Send + 'static,
) -> Result<Download> {
    let script = script_path()?;
    let data_dir = paths::ensure_data_dir()?;
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(&script)
        .arg(model.key())
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("SAYIT_DATA_DIR", &data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for key in PASSTHROUGH_ENV {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    // Own process group, so `cancel` can stop curl along with the script.
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().context("starting the model download")?;

    let state = Arc::new(Mutex::new(State::default()));
    let handle = Download {
        pid: child.id(),
        state: state.clone(),
    };
    let mut stderr = child.stderr.take().expect("stderr is piped");

    std::thread::Builder::new()
        .name("sayit-download".into())
        .spawn(move || {
            // Collect error output on its own thread so a full pipe can
            // never block the script.
            let errors = std::thread::spawn(move || {
                let mut text = String::new();
                let _ = stderr.read_to_string(&mut text);
                text
            });
            let (status, cancelled) = loop {
                {
                    // Reap and mark exited under the lock (see `State`).
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    match child.try_wait() {
                        Ok(None) => {}
                        result => {
                            s.exited = true;
                            break (result, s.cancelled);
                        }
                    }
                }
                progress(downloaded_fraction(model));
                std::thread::sleep(Duration::from_millis(250));
            };
            let errors = errors.join().unwrap_or_default();
            done(match status {
                _ if cancelled => Finished::Cancelled,
                Ok(Some(s)) if s.success() && model.is_installed() => Finished::Installed,
                Ok(_) => Finished::Failed(last_line(&errors)),
                Err(e) => Finished::Failed(e.to_string()),
            });
        })?;
    Ok(handle)
}

/// Removes an installed model and any partial download of it.
pub fn delete(model: ModelId) -> Result<()> {
    let models = paths::data_dir().join("models");
    let Ok(entries) = std::fs::read_dir(&models) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(model.dir_name()) {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The bundled script inside sayit.app, or the repository copy when running
/// a development build from `target/release`.
fn script_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let exe_dir = exe.parent().context("executable has no folder")?;
    let candidates = [
        exe_dir.join("../Resources/fetch-models.sh"),
        exe_dir.join("../../scripts/fetch-models.sh"),
    ];
    match candidates.into_iter().find(|p| p.is_file()) {
        Some(path) => Ok(path),
        None => bail!("fetch-models.sh not found next to the app"),
    }
}

/// Bytes on disk for this model so far (finished files, partial `.part`
/// files and the Moonshine archive), as a fraction of the full download.
fn downloaded_fraction(model: ModelId) -> f64 {
    fn size(path: &std::path::Path) -> u64 {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => std::fs::read_dir(path)
                .map(|entries| entries.flatten().map(|e| size(&e.path())).sum())
                .unwrap_or(0),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    let models = paths::data_dir().join("models");
    let bytes: u64 = std::fs::read_dir(&models)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(model.dir_name())
                })
                .map(|e| size(&e.path()))
                .sum()
        })
        .unwrap_or(0);
    (bytes as f64 / model.download_bytes() as f64).min(1.0)
}

/// The most useful part of the script's error output for a status line.
fn last_line(errors: &str) -> String {
    errors
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("download failed")
        .to_string()
}
