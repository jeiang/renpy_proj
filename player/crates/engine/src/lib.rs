//! Ren'Py 8.5.3 layer: patched source, rebuilt Cython modules, embedded zips.
//! Contract: player/CONTRACTS.md.

include!(concat!(env!("OUT_DIR"), "/engine_generated.rs"));

/// `renpy/**/*.py` from the patched tree plus `player/engine/python/`, as unchecked-hash `.pyc`.
/// Includes `_player/boot.py`.
pub static LAYER_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/layer.zip"));

/// `renpy/common` (sources and assets; `.rpyc` files are a later build step). It holds a `.renpy-common`
/// marker entry that `_player.boot` uses to find the zip among the mounted zips.
pub static COMMON_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/common.zip"));
