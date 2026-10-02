//! The channel mixer. A port of the mixing half of `renpysound_core.c`.
//!
//! All channel state sits behind one mutex (`MIXER`). The audio thread holds it
//! for the length of one callback. The Python API holds it for short updates.

use std::ffi::c_int;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;
use pyo3::ffi::PyObject;
use pyo3::prelude::*;

use crate::stream::Media;

/// Power level of magnitude 1.0 (see `MAX_POWER` in `renpysound_core.c`).
const MAX_POWER: f32 = 6.0;
const MIN_POWER: f32 = 0.0;
const ZERO_PAN: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The address of `apply_audio_filter` from `renpy.audio.filter`, or 0.
pub static APPLY_FILTER: AtomicUsize = AtomicUsize::new(0);

/// The address of a `void fn(float *stream, int length)` that adds audio to the mix, or 0.
pub static GENERATE: AtomicUsize = AtomicUsize::new(0);

type ApplyFilter = unsafe extern "C" fn(*mut PyObject, *mut f32, c_int, c_int, c_int);

/// An `AudioFilter` object (or `None`, which means no filter).
pub struct Filter {
    pub obj: Py<PyAny>,
    pub active: bool,
}

/// One file on a channel, playing or queued.
pub struct Track {
    pub media: Media,
    pub name: String,
    pub fadein_ms: i32,
    pub tight: bool,
    pub start_ms: i32,
    pub relative_volume: f32,
    pub synchro_start: bool,
    pub filter: Option<Filter>,
}

#[derive(Clone, Copy)]
pub struct Interp {
    done: u32,
    duration: u32,
    start: f32,
    end: f32,
}

impl Interp {
    fn new(v: f32) -> Interp {
        Interp {
            done: 0,
            duration: 0,
            start: v,
            end: v,
        }
    }

    fn tick(&mut self) {
        if self.done < self.duration {
            self.done += 1;
        }
    }

    fn get(&self) -> f32 {
        if self.done >= self.duration {
            self.end
        } else {
            self.start + (self.end - self.start) * (self.done as f32 / self.duration as f32)
        }
    }

    fn set(&mut self, end: f32, duration: u32) {
        self.start = self.get();
        self.end = end;
        self.done = 0;
        self.duration = duration;
    }
}

pub struct Channel {
    pub playing: Option<Track>,
    pub queued: Option<Track>,
    pub playing_pad: i64,
    pub paused: bool,
    pub mixer_volume: f32,
    pub secondary_volume: Interp,
    /// Samples mixed since the current file started.
    pub pos: i64,
    pub fade: Interp,
    /// Samples until the channel stops; -1 for never.
    pub stop_samples: i64,
    pan: Interp,
    pub video: i32,
    last_playing: bool,
    last_volume: f32,
}

impl Channel {
    fn new() -> Channel {
        Channel {
            playing: None,
            queued: None,
            playing_pad: 0,
            paused: false,
            mixer_volume: 1.0,
            secondary_volume: Interp::new(MAX_POWER),
            pos: 0,
            fade: Interp::new(MAX_POWER),
            stop_samples: 0,
            pan: Interp::new(0.0),
            video: 0,
            last_playing: false,
            last_volume: 0.0,
        }
    }
}

pub struct Mixer {
    pub channels: Vec<Channel>,
    pub rate: u32,
    pub linear_fades: bool,
    /// 1 mixes down to mono, anything else keeps stereo.
    pub channel_count: u8,
    dying: Vec<Track>,
}

pub static MIXER: Mutex<Mixer> = Mutex::new(Mixer::new());

fn log_power(power: f32, linear: bool) -> f32 {
    if linear {
        power * MAX_POWER
    } else if power <= 0.0 {
        MIN_POWER
    } else if power >= 1.0 {
        MAX_POWER
    } else {
        power.log2() + MAX_POWER
    }
}

fn power_to_magnitude(i: &Interp, linear: bool) -> f32 {
    let p = i.get();
    if linear {
        p / MAX_POWER
    } else if p == MIN_POWER {
        0.0
    } else if p == MAX_POWER {
        1.0
    } else {
        2f32.powf(p - MAX_POWER)
    }
}

impl Mixer {
    pub const fn new() -> Mixer {
        Mixer {
            channels: Vec::new(),
            rate: 44100,
            linear_fades: false,
            channel_count: 2,
            dying: Vec::new(),
        }
    }

    pub fn ms_to_samples(&self, ms: i32) -> u32 {
        ((ms as i64) * self.rate as i64 / 1000).max(0) as u32
    }

    fn samples_to_ms(&self, samples: i64) -> i64 {
        samples * 1000 / self.rate as i64
    }

    /// Returns channel `n`, allocating it and any lower ones as needed.
    pub fn channel(&mut self, n: i32) -> Result<&mut Channel, &'static str> {
        if n < 0 {
            return Err("Channel number out of range.");
        }
        let n = n as usize;
        while self.channels.len() <= n {
            self.channels.push(Channel::new());
        }
        Ok(&mut self.channels[n])
    }

    /// Takes the tracks that ended on the audio thread, so that the caller can
    /// drop them away from the audio thread.
    pub fn take_dying(&mut self) -> Vec<Track> {
        std::mem::take(&mut self.dying)
    }

    pub fn reserve_dying(&mut self) {
        self.dying.reserve(32);
    }

    /// Starts the stream that is now `playing`: as `start_stream`.
    pub fn start_stream(&mut self, ch: usize, reset_fade: bool) {
        let rate_ms = |ms: i32| ((ms as i64) * self.rate as i64 / 1000).max(0) as u32;
        let freq = self.rate as i64;
        let c = &mut self.channels[ch];
        c.pos = 0;
        if c.queued.is_none() {
            c.playing_pad = freq * 2;
        }
        if reset_fade {
            let fadein = c.playing.as_ref().map_or(0, |t| t.fadein_ms);
            c.fade.start = MIN_POWER;
            c.fade.end = MAX_POWER;
            c.fade.done = 0;
            c.fade.duration = rate_ms(fadein);
            c.stop_samples = -1;
        }
    }

    /// Moves `ch` to its queued file when the playing one has shown its last frame. The mixer does
    /// the same when the audio runs out, but that waits for the next audio callback, which leaves
    /// a looping movie on its last frame for a frame or two.
    pub fn hand_over_if_drained(&mut self, ch: usize) {
        let c = &mut self.channels[ch];
        if c.queued.is_none()
            || c.stop_samples == 0
            || !c.playing.as_ref().is_some_and(|t| t.media.drained())
        {
            return;
        }
        let mut old_tight = c.playing.as_ref().is_some_and(|t| t.tight);
        let old = c.playing.take();
        c.playing = c.queued.take();
        if let Some(o) = old {
            self.dying.push(o);
        }
        if c.playing.as_ref().is_some_and(|t| t.fadein_ms != 0) {
            old_tight = false;
        }
        self.start_stream(ch, !old_tight);
    }

    /// Position of channel `ch` in ms, or -1.
    pub fn get_pos_ms(&self, ch: usize) -> i64 {
        match self.channels.get(ch) {
            Some(c) if c.playing.is_some() => {
                let t = c.playing.as_ref();
                // A file with video and no audio is positioned by its video clock.
                if let Some(p) = t.and_then(|t| t.media.video_position()) {
                    return (p * 1000.0) as i64;
                }
                self.samples_to_ms(c.pos) + t.map_or(0, |t| t.start_ms as i64)
            }
            _ => -1,
        }
    }

    pub fn fadeout(&mut self, ch: usize, ms: i32) {
        let rate = self.rate;
        let to_samples = |ms: i64| (ms * rate as i64 / 1000).max(0) as u32;
        let pos_s = self.samples_to_ms(self.channels[ch].pos) as f32 / 1000.0;
        let c = &mut self.channels[ch];

        if c.queued.is_some() {
            let start = c.playing.as_ref().map_or(0, |t| t.start_ms) as f32 / 1000.0;
            let position = pos_s + start;
            let duration = c.playing.as_ref().map_or(0.0, |t| t.media.duration()) as f32;
            let tight = c.playing.as_ref().is_some_and(|t| t.tight);
            // If the fadeout fits into the current file, drop the queued file so the next one starts at once.
            if position + ms as f32 / 1000.0 < duration || !tight || ms <= 32 {
                self.dying.push(c.queued.take().unwrap());
            }
        }

        let synchro = c.playing.as_ref().is_some_and(|t| t.synchro_start);
        if ms == 0 || synchro {
            c.stop_samples = 0;
            if let Some(t) = c.playing.as_mut() {
                t.tight = false;
                t.synchro_start = false;
            }
            return;
        }

        if ms > 16 {
            c.fade.set(MIN_POWER, to_samples(ms as i64 - 16));
        } else {
            c.fade.start = MIN_POWER;
            c.fade.end = MIN_POWER;
            c.fade.done = 1;
            c.fade.duration = 1;
        }

        c.stop_samples = to_samples(ms as i64) as i64;
        if let Some(q) = c.queued.as_mut() {
            q.tight = false;
        }
        if c.queued.is_none()
            && let Some(t) = c.playing.as_mut()
        {
            t.tight = false;
        }
    }

    pub fn set_pan(&mut self, ch: usize, pan: f32, delay: f32) {
        let d = self.ms_to_samples((delay * 1000.0) as i32);
        self.channels[ch].pan.set(pan, d);
    }

    pub fn set_secondary_volume(&mut self, ch: usize, vol: f32, delay: f32) {
        let d = self.ms_to_samples((delay * 1000.0) as i32);
        let lp = log_power(vol, self.linear_fades);
        self.channels[ch].secondary_volume.set(lp, d);
    }

    /// As `handle_synchro_start`: hold synchro channels until all are ready.
    pub fn handle_synchro_start(&mut self) {
        let mut ready = true;
        for c in &mut self.channels {
            if let Some(p) = c.playing.as_mut()
                && p.synchro_start
            {
                if let Some(q) = c.queued.as_mut() {
                    q.synchro_start = false;
                }
                if !p.media.is_ready() {
                    ready = false;
                }
            }
            match c.queued.as_mut() {
                Some(q) if q.synchro_start => ready = false,
                Some(q) => q.synchro_start = false,
                None => {}
            }
        }
        if ready {
            for c in &mut self.channels {
                if let Some(p) = c.playing.as_mut() {
                    p.synchro_start = false;
                }
            }
        }
    }

    /// Mixes `out.len() / 2` stereo frames into `out`, which must be zero.
    pub fn mix(&mut self, out: &mut [f32], scratch: &mut Vec<f32>) {
        let length = out.len() / 2;
        if scratch.len() < length * 2 {
            scratch.resize(length * 2, 0.0);
        }
        let apply: Option<ApplyFilter> = match APPLY_FILTER.load(Ordering::Acquire) {
            0 => None,
            a => Some(unsafe { std::mem::transmute::<usize, ApplyFilter>(a) }),
        };
        let rate = self.rate;
        let linear = self.linear_fades;

        let generate = GENERATE.load(Ordering::Acquire);
        if generate != 0 {
            let f = unsafe {
                std::mem::transmute::<usize, unsafe extern "C" fn(*mut f32, c_int)>(generate)
            };
            unsafe { f(out.as_mut_ptr(), length as c_int) };
        }

        for ch in 0..self.channels.len() {
            let mut mixed = 0usize;

            if self.channels[ch].playing.is_none() || self.channels[ch].paused {
                self.channels[ch].last_playing = false;
                continue;
            }

            while mixed < length
                && self.channels[ch]
                    .playing
                    .as_ref()
                    .is_some_and(|p| !p.synchro_start)
            {
                let mixleft = length - mixed;
                let buf = &mut scratch[..mixleft * 2];
                let mut read_length = {
                    let c = &self.channels[ch];
                    c.playing.as_ref().unwrap().media.read_audio(buf)
                };

                let c = &mut self.channels[ch];
                if c.stop_samples == 0 || read_length == 0 {
                    let filter_active = c
                        .playing
                        .as_ref()
                        .and_then(|t| t.filter.as_ref())
                        .is_some_and(|f| f.active);
                    if !filter_active || c.queued.is_some() {
                        c.playing_pad = 0;
                    }

                    if c.playing_pad > 0 {
                        // Pad with silence so that filter tails ring out.
                        read_length = (c.playing_pad as usize).min(mixleft);
                        c.playing_pad -= read_length as i64;
                        buf[..read_length * 2].fill(0.0);
                    } else {
                        let mut old_tight = c.playing.as_ref().is_some_and(|t| t.tight);
                        let old = c.playing.take();
                        c.playing = c.queued.take();
                        if let Some(o) = old {
                            self.dying.push(o);
                        }
                        if c.playing.as_ref().is_some_and(|t| t.fadein_ms != 0) {
                            old_tight = false;
                        }
                        self.start_stream(ch, !old_tight);
                        continue;
                    }
                }

                let c = &mut self.channels[ch];
                if let (Some(f), Some(apply)) =
                    (c.playing.as_ref().and_then(|t| t.filter.as_ref()), apply)
                    && f.active
                {
                    unsafe {
                        apply(
                            f.obj.as_ptr(),
                            buf.as_mut_ptr(),
                            2,
                            read_length as c_int,
                            rate as c_int,
                        )
                    };
                }

                let rel = c.playing.as_ref().map_or(1.0, |t| t.relative_volume);
                let mut i = 0;
                while i < read_length && c.stop_samples != 0 {
                    let (left, right) = (buf[i * 2], buf[i * 2 + 1]);
                    c.fade.tick();
                    c.secondary_volume.tick();
                    c.pan.tick();

                    let pan = c.pan.get();
                    let (l, r) = if pan == 0.0 {
                        (left * ZERO_PAN, right * ZERO_PAN)
                    } else {
                        let theta = std::f32::consts::PI * (pan + 1.0) / 4.0;
                        (left * theta.cos(), right * theta.sin())
                    };

                    let target = power_to_magnitude(&c.fade, linear)
                        * power_to_magnitude(&c.secondary_volume, linear)
                        * rel
                        * c.mixer_volume;
                    let volume = if c.last_playing {
                        c.last_volume + 0.01 * (target - c.last_volume)
                    } else {
                        target
                    };
                    out[(mixed) * 2] += l * volume;
                    out[(mixed) * 2 + 1] += r * volume;
                    c.last_volume = volume;

                    if c.stop_samples > 0 {
                        c.stop_samples -= 1;
                    }
                    c.pos += 1;
                    mixed += 1;
                    i += 1;
                }
            }

            self.channels[ch].last_playing = true;
        }

        if self.channel_count == 1 {
            for f in out.as_chunks_mut::<2>().0 {
                let m = (f[0] + f[1]) / 2.0;
                f[0] = m;
                f[1] = m;
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}
