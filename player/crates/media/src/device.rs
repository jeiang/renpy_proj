//! Audio output. A cpal stream runs on a helper thread (cpal streams are not
//! `Send` on every platform) and calls the mixer. With `SDL_AUDIODRIVER=dummy`
//! (or `set_virtual_output`, or `PLAYER_HEADLESS=1`) a timer thread calls the
//! mixer instead, so that the audio system works without a sound device. An
//! optional PCM tap sees every mixer pass.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use parking_lot::Mutex;

use crate::mixer::MIXER;
use crate::stream;

/// Set by `global_pause`: the device then outputs silence and the mixer stands still.
pub static PAUSED: AtomicBool = AtomicBool::new(false);

static VIRTUAL: AtomicBool = AtomicBool::new(false);
static RATE: AtomicU32 = AtomicU32::new(48000);

type Tap = Box<dyn FnMut(&[f32]) + Send + 'static>;
static TAP: Mutex<Option<Tap>> = Mutex::new(None);

/// Makes the output the timer-driven dummy device (no cpal), regardless of `SDL_AUDIODRIVER`.
/// Call it before the first `renpysound.init`. Env `PLAYER_HEADLESS=1` does the same.
pub fn set_virtual_output(on: bool) {
    VIRTUAL.store(on, Ordering::SeqCst);
}

/// Sets or removes the PCM tap. The output thread calls it after every mixer pass with
/// interleaved stereo f32 at `mixer_rate()`, with silence when nothing plays.
pub fn set_pcm_tap(tap: Option<Tap>) {
    *TAP.lock() = tap;
}

/// The mixer sample rate: what the output was opened with (48000 before that).
pub fn mixer_rate() -> u32 {
    RATE.load(Ordering::Relaxed)
}

fn tap(pcm: &[f32]) {
    if let Some(t) = TAP.lock().as_mut() {
        t(pcm);
    }
}

pub struct Device {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start(
    rate: u32,
    buffer_frames: u32,
    equal_mono: bool,
    linear_fades: bool,
) -> Result<Device, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let dummy = VIRTUAL.load(Ordering::SeqCst)
        || std::env::var("PLAYER_HEADLESS").is_ok_and(|v| v == "1")
        || std::env::var("SDL_AUDIODRIVER").is_ok_and(|v| v == "dummy");

    let thread = std::thread::Builder::new()
        .name("audio output".into())
        .spawn(move || {
            let r = if dummy {
                run_dummy(
                    rate,
                    buffer_frames,
                    equal_mono,
                    linear_fades,
                    &ready_tx,
                    &stop_rx,
                )
            } else {
                run_cpal(
                    rate,
                    buffer_frames,
                    equal_mono,
                    linear_fades,
                    &ready_tx,
                    &stop_rx,
                )
            };
            if let Err(e) = r {
                let _ = ready_tx.send(Err(e));
            }
        })
        .map_err(|e| e.to_string())?;

    match ready_rx.recv() {
        Ok(Ok(_rate)) => Ok(Device {
            stop: stop_tx,
            thread: Some(thread),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err("audio thread ended before it started".into()),
    }
}

fn configure(rate: u32, equal_mono: bool, linear_fades: bool) {
    RATE.store(rate, Ordering::Relaxed);
    stream::set_params(rate, equal_mono);
    let mut m = MIXER.lock();
    m.rate = rate;
    m.linear_fades = linear_fades;
    m.reserve_dying();
}

fn run_dummy(
    rate: u32,
    frames: u32,
    equal_mono: bool,
    linear_fades: bool,
    ready: &mpsc::Sender<Result<u32, String>>,
    stop: &mpsc::Receiver<()>,
) -> Result<(), String> {
    configure(rate, equal_mono, linear_fades);
    let _ = ready.send(Ok(rate));
    let frames = frames.max(64) as usize;
    let period = Duration::from_secs_f64(frames as f64 / rate as f64);
    let mut mix = vec![0f32; frames * 2];
    let mut scratch = vec![0f32; frames * 2];
    let mut next = Instant::now() + period;
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        match stop.recv_timeout(wait) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        next += period;
        mix.fill(0.0);
        if !PAUSED.load(Ordering::Relaxed) {
            MIXER.lock().mix(&mut mix, &mut scratch);
        }
        tap(&mix);
    }
}

fn run_cpal(
    rate: u32,
    frames: u32,
    equal_mono: bool,
    linear_fades: bool,
    ready: &mpsc::Sender<Result<u32, String>>,
    stop: &mpsc::Receiver<()>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no audio output device")?;

    // Prefer stereo f32 at the requested rate; otherwise use what the device offers.
    let preferred = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .filter(|c| c.channels() == 2 && c.sample_format() == SampleFormat::F32)
        .find_map(|c| c.try_with_sample_rate(cpal::SampleRate(rate)));
    let supported = match preferred {
        Some(c) => c,
        None => device.default_output_config().map_err(|e| e.to_string())?,
    };
    let format = supported.sample_format();
    let mut config: StreamConfig = supported.config();
    let actual_rate = config.sample_rate.0;
    let channels = config.channels as usize;
    configure(actual_rate, equal_mono, linear_fades);

    let build = |config: &StreamConfig| match format {
        SampleFormat::F32 => build_stream::<f32>(&device, config, channels),
        SampleFormat::I16 => build_stream::<i16>(&device, config, channels),
        SampleFormat::U16 => build_stream::<u16>(&device, config, channels),
        SampleFormat::I32 => build_stream::<i32>(&device, config, channels),
        f => Err(format!("unsupported sample format {f:?}")),
    };

    config.buffer_size = cpal::BufferSize::Fixed(frames.max(64));
    let stream = match build(&config) {
        Ok(s) => s,
        Err(_) => {
            config.buffer_size = cpal::BufferSize::Default;
            build(&config)?
        }
    };
    stream.play().map_err(|e| e.to_string())?;
    let _ = ready.send(Ok(actual_rate));
    let _ = stop.recv();
    drop(stream);
    Ok(())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mut mix: Vec<f32> = vec![0.0; 16384 * 2];
    let mut scratch: Vec<f32> = vec![0.0; 16384 * 2];
    let silence = T::from_sample(0.0f32);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels;
                if mix.len() < frames * 2 {
                    mix.resize(frames * 2, 0.0);
                }
                let m = &mut mix[..frames * 2];
                m.fill(0.0);
                if !PAUSED.load(Ordering::Relaxed) {
                    MIXER.lock().mix(m, &mut scratch);
                }
                tap(m);
                for (f, frame) in data.chunks_exact_mut(channels).enumerate() {
                    let (l, r) = (m[f * 2], m[f * 2 + 1]);
                    if channels == 1 {
                        frame[0] = T::from_sample((l + r) * 0.5);
                    } else {
                        frame[0] = T::from_sample(l);
                        frame[1] = T::from_sample(r);
                        for s in &mut frame[2..] {
                            *s = silence;
                        }
                    }
                }
            },
            |e| log::error!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}
