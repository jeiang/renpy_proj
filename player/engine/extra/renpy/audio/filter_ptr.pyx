# Exports the address of apply_audio_filter from renpy.audio.filter, so that the
# Rust mixer (crate `media`) can call it on the audio thread. The stock
# renpysound.pyx reads the same pointer with a cimport.
#
# Licence: MIT, as the rest of the player. Not part of Ren'Py.

from libc.stdint cimport uintptr_t

from renpy.audio.filter cimport get_apply_audio_filter


def get_apply_audio_filter_ptr():
    """
    Returns the address of `apply_audio_filter` as an int. Its C signature is
    void (*)(PyObject *filter, float *samples, int subchannels, int length,
    int samplerate), and it does not need the GIL.
    """

    return <uintptr_t> get_apply_audio_filter()
