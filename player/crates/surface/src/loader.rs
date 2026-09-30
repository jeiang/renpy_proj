//! Image loader pool: decodes predicted images ahead of the interpreter.
//!
//! `prefetch(name, opener, priority)` queues a job. A worker thread calls
//! `opener()` (a Python callable that returns the file bytes) with the GIL held
//! for the read only, then decodes with the GIL released. `pygame.image.load`
//! asks [`take`] for the result before it decodes anything itself, so a
//! prefetched image costs the caller nothing, and a still-running job is waited
//! on instead of decoded twice.
//!
//! Jobs are keyed by the image name the game used. A result is handed out once
//! (the Python cache owns it after that). Unclaimed results are dropped oldest
//! first above [`MAX_DONE_BYTES`].

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;

use crate::image::{decode, Decoded};

/// Decoded pixels kept for callers that have not claimed them yet.
const MAX_DONE_BYTES: usize = 512 << 20;

/// Priorities, most urgent first. A caller that is blocked on a job bumps it to 0.
pub const PRIO_BLOCKED: u8 = 0;

enum State {
    Queued,
    Running,
    Done(Decoded),
    Failed,
    Taken,
}

struct Slot {
    name: String,
    state: Mutex<State>,
    changed: Condvar,
    opener: Mutex<Option<Py<PyAny>>>,
    prio: Mutex<u8>,
}

struct Job {
    prio: u8,
    seq: u64,
    slot: Arc<Slot>,
}

impl PartialEq for Job {
    fn eq(&self, o: &Self) -> bool {
        self.prio == o.prio && self.seq == o.seq
    }
}
impl Eq for Job {}
impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Job {
    /// `BinaryHeap` is a max-heap: the lowest (priority, sequence) pops first.
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        Reverse((self.prio, self.seq)).cmp(&Reverse((o.prio, o.seq)))
    }
}

#[derive(Default)]
struct Inner {
    heap: BinaryHeap<Job>,
    slots: HashMap<String, Arc<Slot>>,
    /// Names of slots that hold a result, oldest first.
    done: VecDeque<String>,
    done_bytes: usize,
    seq: u64,
    shutdown: bool,
}

struct Pool {
    inner: Mutex<Inner>,
    work: Condvar,
    submitted: AtomicU64,
    hits: AtomicU64,
    waited_hits: AtomicU64,
    misses: AtomicU64,
    wait_ns: AtomicU64,
}

static POOL: OnceLock<Pool> = OnceLock::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn workers() -> usize {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    cores.saturating_sub(2).max(2)
}

fn pool() -> &'static Pool {
    POOL.get_or_init(|| {
        let n = workers();
        for i in 0..n {
            std::thread::Builder::new()
                .name(format!("image-loader-{i}"))
                .spawn(worker_main)
                .expect("spawn image loader thread");
        }
        Pool {
            inner: Mutex::new(Inner::default()),
            work: Condvar::new(),
            submitted: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            waited_hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            wait_ns: AtomicU64::new(0),
        }
    })
}

fn worker_main() {
    // The pool is created by the first `pool()` call; it is in `POOL` by the time a job exists.
    let p = loop {
        if let Some(p) = POOL.get() {
            break p;
        }
        std::thread::yield_now();
    };
    loop {
        let job = {
            let mut g = lock(&p.inner);
            loop {
                if g.shutdown {
                    return;
                }
                if let Some(j) = g.heap.pop() {
                    break j;
                }
                g = p.work.wait(g).unwrap_or_else(|e| e.into_inner());
            }
        };
        // A bumped job is in the heap twice: the second copy finds the slot running or done.
        {
            let mut st = lock(&job.slot.state);
            if !matches!(*st, State::Queued) {
                continue;
            }
            *st = State::Running;
        }
        let opener = lock(&job.slot.opener).take();
        let result = match opener {
            Some(o) => read_and_decode(p, o),
            None => None,
        };
        finish(p, &job.slot, result);
    }
}

fn read_and_decode(p: &Pool, opener: Py<PyAny>) -> Option<Decoded> {
    if lock(&p.inner).shutdown {
        return None;
    }
    let data: Option<Vec<u8>> = Python::attach(|py| {
        let r = opener.bind(py).call0().ok()?;
        let b = r.extract::<PyBackedBytes>().ok()?;
        Some(b.to_vec())
    });
    drop(opener);
    decode(&data?).ok()
}

fn finish(p: &Pool, slot: &Arc<Slot>, result: Option<Decoded>) {
    let pixel_bytes = result.as_ref().map_or(0, |d| d.rgba.len());
    {
        let mut st = lock(&slot.state);
        *st = match result {
            Some(d) => State::Done(d),
            None => State::Failed,
        };
    }
    slot.changed.notify_all();
    if pixel_bytes == 0 {
        return;
    }
    let mut g = lock(&p.inner);
    if g.slots.get(&slot.name).is_some_and(|s| Arc::ptr_eq(s, slot)) {
        g.done.push_back(slot.name.clone());
        g.done_bytes += pixel_bytes;
    }
    while g.done_bytes > MAX_DONE_BYTES {
        let Some(old) = g.done.pop_front() else { break };
        if let Some(s) = g.slots.remove(&old) {
            let mut st = lock(&s.state);
            if let State::Done(d) = std::mem::replace(&mut *st, State::Taken) {
                g.done_bytes = g.done_bytes.saturating_sub(d.rgba.len());
            }
        }
    }
}

/// Queues `name` for decoding, or raises the priority of a job that waits.
pub fn prefetch(name: String, opener: Py<PyAny>, priority: u8) {
    let p = pool();
    let mut g = lock(&p.inner);
    if let Some(slot) = g.slots.get(&name).cloned() {
        bump(&mut g, &slot, priority);
        p.work.notify_one();
        return;
    }
    let slot = Arc::new(Slot {
        name: name.clone(),
        state: Mutex::new(State::Queued),
        changed: Condvar::new(),
        opener: Mutex::new(Some(opener)),
        prio: Mutex::new(priority),
    });
    g.slots.insert(name, slot.clone());
    g.seq += 1;
    let seq = g.seq;
    g.heap.push(Job {
        prio: priority,
        seq,
        slot,
    });
    p.submitted.fetch_add(1, Ordering::Relaxed);
    p.work.notify_one();
}

fn bump(g: &mut Inner, slot: &Arc<Slot>, priority: u8) {
    let mut cur = lock(&slot.prio);
    if priority < *cur && matches!(*lock(&slot.state), State::Queued) {
        *cur = priority;
        g.seq += 1;
        let seq = g.seq;
        g.heap.push(Job {
            prio: priority,
            seq,
            slot: slot.clone(),
        });
    }
}

/// Returns the prefetched image for `name`, waiting for a running job. `None`
/// when nothing was prefetched or the job failed (the caller decodes itself).
pub fn take(py: Python<'_>, name: &str) -> Option<Decoded> {
    let p = POOL.get()?;
    let slot = {
        let mut g = lock(&p.inner);
        let slot = g.slots.get(name).cloned();
        match slot {
            Some(s) => {
                bump(&mut g, &s, PRIO_BLOCKED);
                p.work.notify_one();
                s
            }
            None => {
                p.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        }
    };
    let ready = matches!(*lock(&slot.state), State::Done(_) | State::Failed);
    let t0 = Instant::now();
    let result = py.detach(|| {
        let mut st = lock(&slot.state);
        while matches!(*st, State::Queued | State::Running) {
            st = slot.changed.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        match std::mem::replace(&mut *st, State::Taken) {
            State::Done(d) => Some(d),
            _ => None,
        }
    });
    {
        let mut g = lock(&p.inner);
        if g.slots.get(name).is_some_and(|s| Arc::ptr_eq(s, &slot)) {
            g.slots.remove(name);
            if let Some(i) = g.done.iter().position(|k| k == name) {
                g.done.remove(i);
            }
            if let Some(d) = &result {
                g.done_bytes = g.done_bytes.saturating_sub(d.rgba.len());
            }
        }
    }
    if result.is_some() {
        if ready {
            p.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            p.waited_hits.fetch_add(1, Ordering::Relaxed);
            p.wait_ns
                .fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    } else {
        p.misses.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// Drops every unclaimed job result. Running jobs finish and are dropped.
pub fn clear() {
    if let Some(p) = POOL.get() {
        let mut g = lock(&p.inner);
        g.heap.clear();
        g.slots.clear();
        g.done.clear();
        g.done_bytes = 0;
    }
}

/// Stops the workers. After this, `prefetch` queues jobs nobody runs.
pub fn shutdown() {
    if let Some(p) = POOL.get() {
        let mut g = lock(&p.inner);
        g.shutdown = true;
        g.heap.clear();
        g.slots.clear();
        g.done.clear();
        g.done_bytes = 0;
        p.work.notify_all();
    }
}

/// (workers, submitted, ready hits, waited hits, misses, wait ms).
pub fn stats() -> (usize, u64, u64, u64, u64, f64) {
    match POOL.get() {
        Some(p) => (
            workers(),
            p.submitted.load(Ordering::Relaxed),
            p.hits.load(Ordering::Relaxed),
            p.waited_hits.load(Ordering::Relaxed),
            p.misses.load(Ordering::Relaxed),
            p.wait_ns.load(Ordering::Relaxed) as f64 / 1e6,
        ),
        None => (workers(), 0, 0, 0, 0, 0.0),
    }
}
