//! Microphone capture. Audio lives only in memory and is never written to disk.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat};

pub const TARGET_RATE: u32 = 16_000;

/// Records from an input device until stopped.
pub struct Recorder {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

/// Names of the connected input devices, for the device picker.
pub fn input_device_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devs| devs.filter_map(|d| device_name(&d)).collect())
        .unwrap_or_default()
}

fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|desc| desc.name().to_string())
}

/// The input device called `name`, or the system default if `name` is None
/// or that device isn't connected.
fn input_device(name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name {
        let found = host
            .input_devices()
            .ok()
            .and_then(|mut devs| devs.find(|d| device_name(d).as_deref() == Some(name)));
        if let Some(d) = found {
            return Ok(d);
        }
        eprintln!("input device {name:?} not connected, using the system default");
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("no input device found"))
}

impl Recorder {
    /// Opens `device` (by name; None = system default) and starts recording.
    pub fn start(device: Option<&str>) -> Result<Self> {
        let device = input_device(device)?;
        let config = device
            .default_input_config()
            .context("no default input config")?;
        let channels = config.channels() as usize;
        let rate = config.sample_rate();

        // Pre-allocate ~30s so the audio callback rarely reallocates.
        let buf = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
        // CoreAudio reports a processor overload (Xrun) when the stream starts
        // while the model is warming up on the other cores. At worst one buffer
        // is dropped, which doesn't affect transcription, so don't report it.
        let err_fn = |e: cpal::Error| {
            if e.kind() != cpal::ErrorKind::Xrun {
                eprintln!("audio stream error: {e}");
            }
        };

        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                build::<f32>(&device, config.into(), channels, buf.clone(), err_fn)?
            }
            SampleFormat::I16 => {
                build::<i16>(&device, config.into(), channels, buf.clone(), err_fn)?
            }
            SampleFormat::I32 => {
                build::<i32>(&device, config.into(), channels, buf.clone(), err_fn)?
            }
            SampleFormat::U16 => {
                build::<u16>(&device, config.into(), channels, buf.clone(), err_fn)?
            }
            f => return Err(anyhow!("unsupported sample format {f}")),
        };
        stream.play()?;
        Ok(Self { stream, buf, rate })
    }

    /// Stops recording and returns 16 kHz mono samples.
    pub fn stop(self) -> Vec<f32> {
        drop(self.stream);
        let raw = std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()));
        resample(&raw, self.rate, TARGET_RATE)
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    buf: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
            // Downmix interleaved frames to mono.
            for frame in data.chunks_exact(channels) {
                let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                b.push(sum / channels as f32);
            }
        },
        err_fn,
        None,
    )?)
}

/// Windowed-sinc resampler. Runs once on the whole clip after recording stops,
/// which costs a few ms for typical dictation lengths.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    const HALF_TAPS: isize = 16;
    let ratio = to as f64 / from as f64;
    // Low-pass cutoff at the lower Nyquist to avoid aliasing when downsampling.
    let cutoff = ratio.min(1.0);
    let out_len = (input.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 / ratio;
        let base = center.floor() as isize;
        let (mut acc, mut wsum) = (0.0f64, 0.0f64);
        for k in (base - HALF_TAPS + 1)..=(base + HALF_TAPS) {
            if k < 0 || k as usize >= input.len() {
                continue;
            }
            let x = center - k as f64;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                let px = std::f64::consts::PI * x * cutoff;
                px.sin() / px
            };
            // Blackman window over the kernel span.
            let n = (x / HALF_TAPS as f64 + 1.0) / 2.0;
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * n).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * n).cos();
            let c = sinc * w;
            acc += input[k as usize] as f64 * c;
            wsum += c;
        }
        out.push(if wsum.abs() > 1e-9 {
            (acc / wsum) as f32
        } else {
            0.0
        });
    }
    out
}

/// Trims leading/trailing silence using short-window RMS energy, keeping a
/// small margin so word onsets are not clipped.
pub fn trim_silence(samples: &[f32], rate: u32) -> &[f32] {
    let win = (rate / 50) as usize; // 20 ms
    if samples.len() < win * 2 {
        return samples;
    }
    let rms = |w: &[f32]| (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt();
    let frames: Vec<f32> = samples.chunks(win).map(rms).collect();
    let peak = frames.iter().cloned().fold(0.0f32, f32::max);
    let threshold = (peak * 0.05).max(0.002);
    let first = frames.iter().position(|&e| e > threshold);
    let last = frames.iter().rposition(|&e| e > threshold);
    match (first, last) {
        (Some(f), Some(l)) => {
            let margin = 10; // 200 ms
            let start = f.saturating_sub(margin) * win;
            let end = ((l + 1 + margin) * win).min(samples.len());
            &samples[start..end]
        }
        _ => &samples[0..0],
    }
}

pub fn is_silent(samples: &[f32]) -> bool {
    samples.len() < (TARGET_RATE as usize / 5) // < 200 ms of speech
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_length_and_tone() {
        let from = 48_000;
        let tone: Vec<f32> = (0..from)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / from as f32).sin())
            .collect();
        let out = resample(&tone, from, TARGET_RATE);
        assert_eq!(out.len(), TARGET_RATE as usize);
        // Amplitude of an in-band tone is preserved.
        let peak = out[1000..15000].iter().cloned().fold(0.0f32, f32::max);
        assert!((peak - 1.0).abs() < 0.05, "peak {peak}");
    }

    #[test]
    fn trim_removes_silence() {
        let mut s = vec![0.0f32; 16_000];
        s.extend((0..8_000).map(|i| (i as f32 * 0.1).sin() * 0.5));
        s.extend(vec![0.0f32; 16_000]);
        let t = trim_silence(&s, TARGET_RATE);
        assert!(t.len() < 16_000 && t.len() >= 8_000);
    }
}
