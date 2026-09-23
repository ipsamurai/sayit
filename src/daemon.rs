//! The long-running dictation service.
//!
//! Threads:
//!   hotkey listener (handy-keys) -> controller (owns the mic) -> worker (owns model + injector)
//! The mic is only open while the hotkey is active. The caller's thread stays
//! free, so a UI can own the main thread (required on macOS).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use handy_keys::{Hotkey, HotkeyManager, HotkeyState};

use crate::audio::{self, Recorder};
use crate::config::{Config, Mode};
use crate::inject::Injector;
use crate::stt::{self, ModelId};
use crate::text;

/// Presses shorter than this are treated as accidental (e.g. Opt+key combos).
const MIN_HOLD: Duration = Duration::from_millis(250);
/// Upper bound for `max_recording_secs`: an hour of audio is ~200 MB in memory.
const MAX_RECORDING_SECS: u64 = 3600;

enum Job {
    /// Hotkey went down: load the model now so it overlaps with speaking.
    Warm,
    Transcribe(Vec<f32>),
}

/// What the service is doing, for UI (tray icon). Recording wins over
/// transcribing when both are true (user started a new take mid-transcription).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

/// Combines the controller's and worker's state into one `Status` and calls
/// the listener only when it changes.
struct Reporter {
    state: Mutex<Activity>,
    listener: Box<dyn Fn(Status) + Send + Sync>,
}

#[derive(Default)]
struct Activity {
    recording: bool,
    /// Takes queued for or being transcribed.
    pending: u32,
    last: Status,
}

impl Reporter {
    fn update(&self, change: impl FnOnce(&mut Activity)) {
        let mut a = self.state.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut a);
        let now = if a.recording {
            Status::Recording
        } else if a.pending > 0 {
            Status::Transcribing
        } else {
            Status::Idle
        };
        if a.last != now {
            a.last = now;
            (self.listener)(now);
        }
    }
}

/// Settings the UI can change while the service runs.
pub struct Controls {
    /// While set the hotkey is ignored and any take in progress is discarded.
    pub paused: AtomicBool,
    /// Microphone name for the next take (None = system default).
    pub input_device: Mutex<Option<String>>,
    /// Speech model; the worker switches before the next take.
    pub model: Mutex<ModelId>,
}

impl Controls {
    pub fn new(cfg: &Config) -> Arc<Self> {
        Arc::new(Self {
            paused: AtomicBool::new(false),
            input_device: Mutex::new(cfg.input_device.clone()),
            model: Mutex::new(cfg.model),
        })
    }

    pub fn model(&self) -> ModelId {
        *self.model.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn input_device(&self) -> Option<String> {
        self.input_device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// A running dictation service.
pub struct Dictation {
    controller: JoinHandle<Result<()>>,
}

impl Dictation {
    /// Blocks until the service fails (it never stops on its own).
    pub fn wait(self) -> Result<()> {
        self.controller
            .join()
            .map_err(|_| anyhow::anyhow!("controller thread panicked"))?
    }
}

/// Terminal mode: `sayit run`.
pub fn run(cfg: Config, verbose: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    if !accessibility_trusted(true) {
        // macOS grants permissions to the "responsible" app: when launched from
        // a terminal that's the terminal itself, not this binary.
        let app = std::env::var("__CFBundleIdentifier")
            .map(|id| format!("your terminal app ({id})"))
            .unwrap_or_else(|_| "your terminal app".into());
        eprintln!(
            "sayit needs Accessibility permission (to detect the hotkey and paste).\n\
             Enable {app} in System Settings > Privacy & Security > Accessibility,\n\
             then fully quit it (Cmd+Q) and reopen it. Adding the sayit binary itself has no effect."
        );
        std::process::exit(1);
    }
    let d = start(cfg.clone(), verbose, Controls::new(&cfg), |_| {})?;
    eprintln!(
        "sayit ready: {} {} to dictate. Config: {}",
        if cfg.mode == Mode::Hold {
            "hold"
        } else {
            "press"
        },
        cfg.hotkey,
        crate::config::path().display()
    );
    d.wait()
}

/// Starts the hotkey listener, controller and worker threads. Returns once the
/// model is loaded (unless idle unloading is on) and text insertion is set up,
/// so setup errors are returned here rather than killing the process.
/// Requires Accessibility permission on macOS.
pub fn start(
    cfg: Config,
    verbose: bool,
    controls: Arc<Controls>,
    on_status: impl Fn(Status) + Send + Sync + 'static,
) -> Result<Dictation> {
    let hotkey: Hotkey = cfg
        .hotkey
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid hotkey {:?}: {e}", cfg.hotkey))?;
    let manager = HotkeyManager::new().context("starting hotkey listener")?;
    manager.register(hotkey)?;

    let reporter = Arc::new(Reporter {
        state: Mutex::default(),
        listener: Box::new(on_status),
    });

    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
    let worker_cfg = cfg.clone();
    let worker_rep = reporter.clone();
    let worker_ctl = controls.clone();
    let worker = std::thread::Builder::new()
        .name("sayit-worker".into())
        .spawn(move || {
            worker(
                worker_cfg, job_rx, ready_tx, worker_rep, worker_ctl, verbose,
            )
        })?;
    ready_rx
        .recv()
        .context("worker thread exited during setup")??;

    let controller = std::thread::Builder::new()
        .name("sayit-controller".into())
        .spawn(move || controller(cfg, manager, job_tx, worker, reporter, controls))?;
    Ok(Dictation { controller })
}

fn controller(
    cfg: Config,
    manager: HotkeyManager,
    job_tx: mpsc::Sender<Job>,
    worker: JoinHandle<()>,
    reporter: Arc<Reporter>,
    controls: Arc<Controls>,
) -> Result<()> {
    // Bounded so a typo in the config can't let one take grow without limit.
    let max_len = Duration::from_secs(cfg.max_recording_secs.clamp(1, MAX_RECORDING_SECS));
    let mut recording: Option<(Recorder, Instant)> = None;

    // Stops the mic and either queues the audio for transcription or drops it.
    let finish = |rec: Recorder, keep: bool| -> Result<()> {
        let samples = rec.stop();
        reporter.update(|a| {
            a.recording = false;
            a.pending += keep as u32;
        });
        if keep {
            job_tx.send(Job::Transcribe(samples))?;
        }
        Ok(())
    };

    loop {
        // handy-keys has no blocking receive with a timeout, so poll every
        // 10 ms; the same loop enforces the maximum recording length.
        let event = manager.try_recv();
        if event.is_none() {
            std::thread::sleep(Duration::from_millis(10));
        }
        if worker.is_finished() {
            anyhow::bail!("worker thread exited");
        }

        if controls.paused.load(Ordering::Relaxed) {
            if let Some((rec, _)) = recording.take() {
                finish(rec, false)?;
            }
            continue;
        }
        if let Some((rec, _)) = recording.take_if(|(_, started)| started.elapsed() > max_len) {
            eprintln!("max recording length reached");
            finish(rec, true)?;
            continue;
        }
        let Some(event) = event else { continue };

        let start = match (cfg.mode, event.state, recording.is_some()) {
            (Mode::Hold, HotkeyState::Pressed, false) => true,
            (Mode::Hold, HotkeyState::Released, true) => false,
            (Mode::Toggle, HotkeyState::Pressed, rec) => !rec,
            _ => continue,
        };

        if start {
            job_tx.send(Job::Warm)?;
            match Recorder::start(controls.input_device().as_deref()) {
                Ok(rec) => {
                    recording = Some((rec, Instant::now()));
                    reporter.update(|a| a.recording = true);
                }
                Err(e) => eprintln!("could not open microphone: {e:#}"),
            }
        } else if let Some((rec, started)) = recording.take() {
            let accidental = cfg.mode == Mode::Hold && started.elapsed() < MIN_HOLD;
            finish(rec, !accidental)?;
        }
    }
}

fn worker(
    cfg: Config,
    rx: mpsc::Receiver<Job>,
    ready: mpsc::Sender<Result<()>>,
    reporter: Arc<Reporter>,
    controls: Arc<Controls>,
    verbose: bool,
) {
    let idle = match cfg.unload_after_idle_mins {
        0 => Duration::MAX,
        m => Duration::from_secs(m * 60),
    };
    let mut engine: Option<stt::Engine> = None;

    // Loads the chosen model, switching if the user picked another one.
    let load = |engine: &mut Option<stt::Engine>| -> Result<()> {
        let want = controls.model();
        if engine.as_ref().is_some_and(|e| e.id != want) {
            *engine = None; // free the old model first to keep peak RAM low
        }
        if engine.is_none() {
            let t = Instant::now();
            *engine = Some(stt::Engine::load(want).with_context(|| {
                format!(
                    "run scripts/fetch-models.sh {} to download the model",
                    want.key()
                )
            })?);
            if verbose {
                eprintln!("{} loaded in {:.2?}", want.key(), t.elapsed());
            }
        }
        Ok(())
    };

    let mut setup = || -> Result<Injector> {
        let injector = Injector::new().context("cannot set up text insertion")?;
        // Load eagerly unless the user opted into idle unloading.
        if cfg.unload_after_idle_mins == 0 {
            load(&mut engine)?;
        }
        Ok(injector)
    };
    let mut injector = match setup() {
        Ok(i) => {
            let _ = ready.send(Ok(()));
            i
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };

    loop {
        let job = match rx.recv_timeout(idle) {
            Ok(j) => j,
            Err(RecvTimeoutError::Timeout) => {
                if engine.take().is_some() && verbose {
                    eprintln!("idle: model unloaded");
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Err(e) = load(&mut engine) {
            eprintln!("{e:#}");
        }
        if let Job::Transcribe(samples) = job {
            if let Some(engine) = engine.as_mut() {
                transcribe_and_paste(&cfg, engine, &mut injector, &samples, verbose);
            }
            // The controller counted every Transcribe job as pending.
            reporter.update(|a| a.pending = a.pending.saturating_sub(1));
        }
    }
}

fn transcribe_and_paste(
    cfg: &Config,
    engine: &mut stt::Engine,
    injector: &mut Injector,
    samples: &[f32],
    verbose: bool,
) {
    let t = Instant::now();
    let speech = audio::trim_silence(samples, audio::TARGET_RATE);
    if audio::is_silent(speech) {
        return;
    }
    let text = match engine.transcribe(speech) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => return,
        Err(e) => {
            eprintln!("{e:#}");
            return;
        }
    };
    let t_stt = t.elapsed();
    let text = if cfg.remove_fillers {
        text::remove_fillers(&text)
    } else {
        text
    };
    if text.is_empty() {
        return; // the take was only "um"s
    }
    // Each take ends on a new line, so the next one starts fresh.
    let pasted = if cfg.newline_after_take {
        format!("{text}\n")
    } else {
        format!("{text} ")
    };
    if let Err(e) = injector.paste(&pasted, cfg.restore_clipboard) {
        eprintln!("paste failed: {e:#}");
    }
    if verbose {
        // Transcripts are only printed in verbose mode; nothing is logged to disk.
        eprintln!(
            "[{:.1}s audio | stt {t_stt:.0?}] {text}",
            samples.len() as f32 / audio::TARGET_RATE as f32
        );
    }
}

/// Whether sayit may watch and post keyboard events. With `prompt`, an
/// untrusted process also gets the system prompt and its responsible app is
/// added (unchecked) to the Accessibility list, so the user only has to tick it.
#[cfg(target_os = "macos")]
pub fn accessibility_trusted(prompt: bool) -> bool {
    use objc2_foundation::{NSDictionary, NSNumber, NSString};

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        static kAXTrustedCheckOptionPrompt: &'static NSString;
        fn AXIsProcessTrustedWithOptions(options: &NSDictionary<NSString, NSNumber>) -> bool;
    }

    let prompt = NSNumber::new_bool(prompt);
    // SAFETY: the declarations match ApplicationServices' AXUIElement.h. The
    // prompt key is a non-null constant CFString, and NSDictionary is
    // toll-free bridged to the CFDictionaryRef the function expects.
    unsafe {
        let opts = NSDictionary::from_slices(&[kAXTrustedCheckOptionPrompt], &[&*prompt]);
        AXIsProcessTrustedWithOptions(&opts)
    }
}
