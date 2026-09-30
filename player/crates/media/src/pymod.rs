//! The Python module `renpy.audio.renpysound`.
//!
//! Function names, arguments and return values follow `renpysound.pyx` of
//! Ren'Py 8.5.3, so `renpy/audio/audio.py` runs unchanged.

use std::sync::atomic::Ordering;

use parking_lot::Mutex;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use crate::device::{self, Device, PAUSED};
use crate::mixer::{APPLY_FILTER, Filter, GENERATE, MIXER, Track};
use crate::source;
use crate::stream::{self, Media};
use crate::types::PyVideoFrame;

static DEVICE: Mutex<Option<Device>> = Mutex::new(None);

fn err(msg: impl Into<String>) -> PyErr {
    PyException::new_err(msg.into())
}

fn truthy(o: &Option<Bound<'_, PyAny>>) -> PyResult<bool> {
    match o {
        Some(o) => o.is_truthy(),
        None => Ok(false),
    }
}

fn require_init() -> PyResult<()> {
    if DEVICE.lock().is_some() {
        Ok(())
    } else {
        Err(err("The audio system is not initialized."))
    }
}

/// Loads the address of `apply_audio_filter` from the Cython helper.
fn ensure_filter_ptr(py: Python<'_>) -> PyResult<()> {
    if APPLY_FILTER.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    let m = py.import("renpy.audio.filter_ptr")?;
    let addr: usize = m.call_method0("get_apply_audio_filter_ptr")?.extract()?;
    if addr == 0 {
        return Err(err(
            "renpy.audio.filter_ptr returned a null function pointer.",
        ));
    }
    APPLY_FILTER.store(addr, Ordering::Release);
    Ok(())
}

fn prepare_filter(py: Python<'_>, f: &Option<Bound<'_, PyAny>>) -> PyResult<()> {
    if let Some(f) = f
        && !f.is_none() {
            ensure_filter_ptr(py)?;
            let rate = MIXER.lock().rate;
            f.call_method1("prepare", (rate,))?;
        }
    Ok(())
}

fn make_filter(py: Python<'_>, f: Option<Bound<'_, PyAny>>) -> Filter {
    let obj = f.map(|b| b.unbind()).unwrap_or_else(|| py.None());
    let active = !obj.is_none(py);
    Filter { obj, active }
}

#[allow(clippy::too_many_arguments)]
fn start_track(
    py: Python<'_>,
    channel: i32,
    file: &Bound<'_, PyAny>,
    name: &str,
    synchro_start: bool,
    fadein: f64,
    tight: bool,
    start: f64,
    end: f64,
    relative_volume: f64,
    audio_filter: Option<Bound<'_, PyAny>>,
    queue: bool,
) -> PyResult<()> {
    require_init()?;
    prepare_filter(py, &audio_filter)?;
    let src = source::open(file);

    // Read the channel's video mode, and decide between play and queue.
    let (video, queue) = {
        let mut m = MIXER.lock();
        let c = m.channel(channel).map_err(err)?;
        (c.video, queue && c.playing.is_some())
    };

    let media = Media::open(src, name.to_string(), start, end, video);
    let track = Track {
        media,
        name: name.to_string(),
        fadein_ms: (fadein * 1000.0) as i32,
        tight,
        start_ms: (start * 1000.0) as i32,
        relative_volume: relative_volume as f32,
        synchro_start,
        filter: Some(make_filter(py, audio_filter)),
    };

    let mut old: Vec<Track> = Vec::new();
    {
        let mut m = MIXER.lock();
        m.channel(channel).map_err(err)?;
        let ch = channel as usize;
        let c = &mut m.channels[ch];
        if queue && c.playing.is_some() {
            if let Some(q) = c.queued.take() {
                old.push(q);
            }
            c.queued = Some(track);
        } else {
            old.extend(c.playing.take());
            old.extend(c.queued.take());
            c.playing = Some(track);
            m.start_stream(ch, true);
        }
    }
    drop(old);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (channel, file, name, synchro_start=None, fadein=0.0, tight=None, start=0.0, end=-1.0, relative_volume=1.0, audio_filter=None))]
#[allow(clippy::too_many_arguments)]
fn play(
    py: Python<'_>,
    channel: i32,
    file: &Bound<'_, PyAny>,
    name: &str,
    synchro_start: Option<Bound<'_, PyAny>>,
    fadein: f64,
    tight: Option<Bound<'_, PyAny>>,
    start: f64,
    end: f64,
    relative_volume: f64,
    audio_filter: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let (s, t) = (truthy(&synchro_start)?, truthy(&tight)?);
    start_track(
        py,
        channel,
        file,
        name,
        s,
        fadein,
        t,
        start,
        end,
        relative_volume,
        audio_filter,
        false,
    )
}

#[pyfunction]
#[pyo3(signature = (channel, file, name, synchro_start=None, fadein=0.0, tight=None, start=0.0, end=-1.0, relative_volume=1.0, audio_filter=None))]
#[allow(clippy::too_many_arguments)]
fn queue(
    py: Python<'_>,
    channel: i32,
    file: &Bound<'_, PyAny>,
    name: &str,
    synchro_start: Option<Bound<'_, PyAny>>,
    fadein: f64,
    tight: Option<Bound<'_, PyAny>>,
    start: f64,
    end: f64,
    relative_volume: f64,
    audio_filter: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let (s, t) = (truthy(&synchro_start)?, truthy(&tight)?);
    start_track(
        py,
        channel,
        file,
        name,
        s,
        fadein,
        t,
        start,
        end,
        relative_volume,
        audio_filter,
        true,
    )
}

#[pyfunction]
fn stop(channel: i32) -> PyResult<()> {
    let old = {
        let mut m = MIXER.lock();
        let c = m.channel(channel).map_err(err)?;
        (c.playing.take(), c.queued.take())
    };
    drop(old);
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (channel, even_tight=None))]
fn dequeue(channel: i32, even_tight: Option<Bound<'_, PyAny>>) -> PyResult<()> {
    let even_tight = truthy(&even_tight)?;
    let mut dropped = None;
    let mut filter = None;
    {
        let mut m = MIXER.lock();
        let c = m.channel(channel).map_err(err)?;
        let tight = c.playing.as_ref().is_some_and(|t| t.tight);
        if c.queued.is_some() && (!tight || even_tight) {
            dropped = c.queued.take();
        } else if let Some(q) = c.queued.as_mut() {
            q.tight = false;
            q.start_ms = 0;
            q.synchro_start = false;
            filter = q.filter.take();
        }
    }
    drop((dropped, filter));
    Ok(())
}

#[pyfunction]
fn queue_depth(channel: i32) -> PyResult<i32> {
    let mut m = MIXER.lock();
    let c = m.channel(channel).map_err(err)?;
    Ok(c.playing.is_some() as i32 + c.queued.is_some() as i32)
}

#[pyfunction]
fn playing_name(channel: i32) -> PyResult<Option<String>> {
    let mut m = MIXER.lock();
    let c = m.channel(channel).map_err(err)?;
    Ok(c.playing.as_ref().map(|t| t.name.clone()))
}

fn set_paused(channel: i32, pause: bool) -> PyResult<()> {
    let mut m = MIXER.lock();
    let c = m.channel(channel).map_err(err)?;
    c.paused = pause;
    if let Some(t) = c.playing.as_ref() {
        t.media.pause(pause);
    }
    Ok(())
}

#[pyfunction]
fn pause(channel: i32) -> PyResult<()> {
    set_paused(channel, true)
}

#[pyfunction]
fn unpause(channel: i32) -> PyResult<()> {
    set_paused(channel, false)
}

#[pyfunction]
fn global_pause(pause: &Bound<'_, PyAny>) -> PyResult<()> {
    let pause = pause.is_truthy()?;
    PAUSED.store(pause, Ordering::Relaxed);
    let m = MIXER.lock();
    for c in &m.channels {
        if let Some(t) = c.playing.as_ref() {
            t.media.pause(pause);
        }
    }
    Ok(())
}

#[pyfunction]
fn fadeout(channel: i32, delay: f64) -> PyResult<()> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?;
    m.fadeout(channel as usize, (delay * 1000.0) as i32);
    Ok(())
}

#[pyfunction]
fn busy(channel: i32) -> PyResult<bool> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?;
    Ok(m.get_pos_ms(channel as usize) != -1)
}

/// The position in seconds. Like stock, this is -0.001 when nothing is playing.
#[pyfunction]
fn get_pos(channel: i32) -> PyResult<f64> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?;
    Ok(m.get_pos_ms(channel as usize) as f64 / 1000.0)
}

#[pyfunction]
fn get_duration(channel: i32) -> PyResult<f64> {
    let mut m = MIXER.lock();
    let c = m.channel(channel).map_err(err)?;
    Ok(c.playing.as_ref().map_or(0.0, |t| t.media.duration()))
}

#[pyfunction]
fn set_volume(channel: i32, volume: f64) -> PyResult<()> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?.mixer_volume = volume as f32;
    Ok(())
}

#[pyfunction]
fn get_volume(channel: i32) -> PyResult<f64> {
    let mut m = MIXER.lock();
    Ok(m.channel(channel).map_err(err)?.mixer_volume as f64)
}

#[pyfunction]
fn set_pan(channel: i32, pan: f64, delay: f64) -> PyResult<()> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?;
    m.set_pan(channel as usize, pan as f32, delay as f32);
    Ok(())
}

#[pyfunction]
fn set_secondary_volume(channel: i32, volume: f64, delay: f64) -> PyResult<()> {
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?;
    m.set_secondary_volume(channel as usize, volume as f32, delay as f32);
    Ok(())
}

#[pyfunction]
fn replace_audio_filter(
    py: Python<'_>,
    channel: i32,
    audio_filter: Option<Bound<'_, PyAny>>,
    playing: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let primary = playing.is_truthy()?;
    prepare_filter(py, &audio_filter)?;
    let mut old = Vec::new();
    {
        let mut m = MIXER.lock();
        let c = m.channel(channel).map_err(err)?;
        if primary
            && let Some(t) = c.playing.as_mut()
                && t.filter.is_some() {
                    old.extend(t.filter.replace(make_filter(py, audio_filter.clone())));
                }
        if let Some(q) = c.queued.as_mut()
            && q.filter.is_some() {
                old.extend(q.filter.replace(make_filter(py, audio_filter)));
            }
    }
    drop(old);
    Ok(())
}

/// Does nothing on this backend, as in stock.
#[pyfunction]
fn deallocate_audio_filter(_audio_filter: &Bound<'_, PyAny>) {}

#[pyfunction]
fn video_ready(channel: i32) -> PyResult<bool> {
    let sh = {
        let mut m = MIXER.lock();
        m.channel(channel).map_err(err)?;
        // Ren'Py redraws only when this says a frame is ready, so the move to the queued file
        // must happen here too, or a loop waits for the next audio callback.
        m.hand_over_if_drained(channel as usize);
        m.channels[channel as usize]
            .playing
            .as_ref()
            .map(|t| t.media.shared())
    };
    Ok(sh.is_none_or(|s| s.video_ready()))
}

#[pyfunction]
fn read_video(py: Python<'_>, channel: i32) -> PyResult<Option<Py<PyVideoFrame>>> {
    let sh = {
        let mut m = MIXER.lock();
        m.channel(channel).map_err(err)?;
        m.hand_over_if_drained(channel as usize);
        m.channels[channel as usize]
            .playing
            .as_ref()
            .map(|t| t.media.shared())
    };
    let Some(sh) = sh else { return Ok(None) };
    let r = py.detach(move || sh.read_video());
    match r {
        Ok(Some(f)) => Ok(Some(Py::new(py, PyVideoFrame(f))?)),
        Ok(None) => Ok(None),
        Err(e) => Err(err(e)),
    }
}

#[pyfunction]
#[pyo3(signature = (channel, video, r#loop=None))]
fn set_video(
    channel: i32,
    video: &Bound<'_, PyAny>,
    r#loop: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let _ = r#loop;
    let v = if video.eq(1)? {
        1
    } else if video.is_truthy()? {
        2
    } else {
        0
    };
    let mut m = MIXER.lock();
    m.channel(channel).map_err(err)?.video = v;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (freq, stereo, samples, status=None, equal_mono=None, linear_fades=None))]
fn init(
    py: Python<'_>,
    freq: u32,
    stereo: &Bound<'_, PyAny>,
    samples: u32,
    status: Option<Bound<'_, PyAny>>,
    equal_mono: Option<Bound<'_, PyAny>>,
    linear_fades: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let _ = stereo;
    if DEVICE.lock().is_some() {
        return Ok(());
    }
    let (status, equal_mono, linear_fades) = (
        truthy(&status)?,
        truthy(&equal_mono)?,
        truthy(&linear_fades)?,
    );
    if freq == 0 {
        return Err(PyValueError::new_err("The sample rate must not be 0."));
    }
    unsafe {
        ffmpeg_sys_next::av_log_set_level(if status {
            ffmpeg_sys_next::AV_LOG_INFO
        } else {
            ffmpeg_sys_next::AV_LOG_ERROR
        });
    }
    let dev = py
        .detach(|| device::start(freq, samples, equal_mono, linear_fades))
        .map_err(err)?;
    *DEVICE.lock() = Some(dev);
    Ok(())
}

#[pyfunction]
fn quit(py: Python<'_>) {
    let Some(dev) = DEVICE.lock().take() else {
        return;
    };
    let tracks = {
        let mut m = MIXER.lock();
        let mut v = m.take_dying();
        for c in &mut m.channels {
            v.extend(c.playing.take());
            v.extend(c.queued.take());
        }
        m.channels.clear();
        v
    };
    drop(tracks);
    // The decode threads may wait for the GIL while they read a Python file.
    py.detach(move || {
        drop(dev);
        stream::reap(true);
    });
    PAUSED.store(false, Ordering::Relaxed);
}

#[pyfunction]
fn periodic(py: Python<'_>) {
    let notes = stream::take_notes();
    if !notes.is_empty()
        && let Ok(log) = py.import("renpy.display").and_then(|d| d.getattr("log"))
    {
        for n in notes {
            let _ = log.call_method1("write", ("media: %s", n));
        }
    }
    let dying = {
        let mut m = MIXER.lock();
        m.handle_synchro_start();
        m.take_dying()
    };
    drop(dying);
    stream::reap(false);
}

#[pyfunction]
fn advance_time() {
    stream::advance_time();
}

#[pyfunction]
fn set_channel_count(count: i32) {
    MIXER.lock().channel_count = count.clamp(1, 255) as u8;
}

#[pyfunction]
fn get_sample_rate() -> u32 {
    MIXER.lock().rate
}

/// Nothing to check: errors raise from the call that finds them.
#[pyfunction]
fn check_error() {}

/// Installs `void fn(float *stream, int length)` that adds audio to the mix.
/// `fn` is an address, or a ctypes function pointer.
#[pyfunction]
fn set_generate_audio_c_function(py: Python<'_>, r#fn: &Bound<'_, PyAny>) -> PyResult<()> {
    let addr: usize = if r#fn.is_instance_of::<pyo3::types::PyInt>() {
        r#fn.extract()?
    } else {
        let ctypes = py.import("ctypes")?;
        let p = ctypes
            .getattr("cast")?
            .call1((r#fn, ctypes.getattr("c_void_p")?))?;
        p.getattr("value")?.extract::<Option<usize>>()?.unwrap_or(0)
    };
    GENERATE.store(addr, Ordering::Release);
    Ok(())
}

/// Ren'Py hands over sample surfaces so that a software path knows the pixel
/// format. Frames here are YUV planes that the GPU converts, so they are unused.
#[pyfunction]
fn sample_surfaces(_rgb: &Bound<'_, PyAny>, _rgba: &Bound<'_, PyAny>) {}

#[pymodule]
#[pyo3(name = "renpysound")]
pub mod renpysound {
    #[pymodule_export]
    use super::{
        advance_time, busy, check_error, deallocate_audio_filter, dequeue, fadeout, get_duration,
        get_pos, get_sample_rate, get_volume, global_pause, init, pause, periodic, play,
        playing_name, queue, queue_depth, quit, read_video, replace_audio_filter, sample_surfaces,
        set_channel_count, set_generate_audio_c_function, set_pan, set_secondary_volume, set_video,
        set_volume, stop, unpause, video_ready,
    };

    #[pymodule_export]
    use crate::types::PyVideoFrame;

    /// No video will be played from this channel.
    #[pymodule_export]
    const NO_VIDEO: i32 = 0;
    /// The video will be played while avoiding frame drops.
    #[pymodule_export]
    const NODROP_VIDEO: i32 = 1;
    /// The video will be played, allowing frame drops.
    #[pymodule_export]
    const DROP_VIDEO: i32 = 2;
    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    const is_webaudio: bool = false;
}
