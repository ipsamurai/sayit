//! sayit: private, local-only push-to-talk dictation.
//!
//! `sayit app` runs the menu-bar app (the default inside `sayit.app`),
//! `sayit run` the same service without a UI; `listen` and `transcribe` are
//! for testing and benchmarks.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod audio;
mod config;
mod daemon;
mod inject;
#[cfg(target_os = "macos")]
mod models;
mod paths;
mod stt;
mod text;
mod ui;

#[derive(Parser)]
#[command(version, about = "Local-only dictation")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the dictation service (hotkey -> transcribe -> paste).
    Run {
        /// Print timings and transcripts to stderr.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run as a menu-bar app (macOS).
    App {
        /// Print timings and transcripts to stderr.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Record from the mic (press Enter to stop each take), transcribe, print timings.
    Listen {
        /// Model to use (default: the one in config.toml).
        #[arg(short, long)]
        model: Option<stt::ModelId>,
    },
    /// Transcribe a WAV file (benchmarking / debugging).
    Transcribe {
        wav: PathBuf,
        /// Model to use (default: the one in config.toml).
        #[arg(short, long)]
        model: Option<stt::ModelId>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Finder launches the .app bundle with no arguments.
    let cmd = match cli.cmd {
        Some(cmd) => cmd,
        None if paths::in_app_bundle() => Cmd::App { verbose: false },
        None => {
            <Cli as clap::CommandFactory>::command().print_help()?;
            return Ok(());
        }
    };
    match cmd {
        Cmd::Run { verbose } => {
            let _lock = paths::single_instance_lock()?;
            daemon::run(config::Config::load()?, verbose)
        }
        Cmd::App { verbose } => {
            let _lock = paths::single_instance_lock()?;
            ui::run(config::Config::load()?, verbose)
        }
        Cmd::Listen { model } => listen(model_or_config(model)?),
        Cmd::Transcribe { wav, model } => transcribe_file(&wav, model_or_config(model)?),
    }
}

fn model_or_config(model: Option<stt::ModelId>) -> Result<stt::ModelId> {
    Ok(match model {
        Some(m) => m,
        None => config::Config::load()?.model,
    })
}

/// Loads the model and runs one warm-up pass, so the first real
/// transcription is timed like the rest.
fn load_engine(model: stt::ModelId) -> Result<stt::Engine> {
    let t = Instant::now();
    let mut engine = stt::Engine::load(model)?;
    eprintln!("{} loaded in {:.2?}", model.key(), t.elapsed());
    engine.transcribe(&vec![0.0; audio::TARGET_RATE as usize])?;
    Ok(engine)
}

fn listen(model: stt::ModelId) -> Result<()> {
    let mut engine = load_engine(model)?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        eprintln!("\nPress Enter to start recording (Ctrl-D to quit)");
        line.clear();
        if stdin.lock().read_line(&mut line)? == 0 {
            return Ok(());
        }
        let t_open = Instant::now();
        let rec = audio::Recorder::start(None)?;
        eprintln!(
            "recording... (mic opened in {:.0?}) press Enter to stop",
            t_open.elapsed()
        );
        line.clear();
        stdin.lock().read_line(&mut line)?;

        let t_stop = Instant::now();
        let samples = rec.stop();
        let speech = audio::trim_silence(&samples, audio::TARGET_RATE);
        let t_pre = t_stop.elapsed();
        if audio::is_silent(speech) {
            eprintln!("(no speech detected)");
            continue;
        }
        let t_stt = Instant::now();
        let text = engine.transcribe(speech)?;
        let stt = t_stt.elapsed();
        let secs = samples.len() as f32 / audio::TARGET_RATE as f32;
        println!("{text}");
        eprintln!(
            "audio {secs:.1}s (speech {:.1}s) | preprocess {t_pre:.0?} | stt {stt:.0?} | total {:.0?} | {:.0}x realtime",
            speech.len() as f32 / audio::TARGET_RATE as f32,
            t_stop.elapsed(),
            secs / stt.as_secs_f32()
        );
    }
}

fn transcribe_file(path: &Path, model: stt::ModelId) -> Result<()> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()?
        }
    };
    let ch = spec.channels as usize;
    let mono: Vec<f32> = raw
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    let samples = audio::resample(&mono, spec.sample_rate, audio::TARGET_RATE);

    let mut engine = load_engine(model)?;
    let t = Instant::now();
    let text = engine.transcribe(&samples)?;
    let el = t.elapsed();
    let secs = samples.len() as f32 / audio::TARGET_RATE as f32;
    println!("{text}");
    eprintln!(
        "audio {secs:.1}s | stt {el:.0?} | {:.0}x realtime",
        secs / el.as_secs_f32()
    );
    Ok(())
}
