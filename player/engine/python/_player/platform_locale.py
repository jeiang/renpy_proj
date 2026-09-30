"""macOS preferred language without pyobjus (not bundled in the player).

Stock Ren'Py asks NSLocale.preferredLanguages through pyobjus. The same list
comes from CoreFoundation's CFLocaleCopyPreferredLanguages, reached with ctypes.
"""

import ctypes
import ctypes.util

_kCFStringEncodingUTF8 = 0x08000100


def preferred_language():
    """Returns the first preferred language as a BCP 47 string, or None."""

    cf = ctypes.cdll.LoadLibrary(ctypes.util.find_library("CoreFoundation"))
    cf.CFLocaleCopyPreferredLanguages.restype = ctypes.c_void_p
    cf.CFArrayGetCount.restype = ctypes.c_long
    cf.CFArrayGetCount.argtypes = [ctypes.c_void_p]
    cf.CFArrayGetValueAtIndex.restype = ctypes.c_void_p
    cf.CFArrayGetValueAtIndex.argtypes = [ctypes.c_void_p, ctypes.c_long]
    cf.CFStringGetCString.restype = ctypes.c_bool
    cf.CFStringGetCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
    cf.CFRelease.argtypes = [ctypes.c_void_p]

    langs = cf.CFLocaleCopyPreferredLanguages()
    if not langs:
        return None
    try:
        if cf.CFArrayGetCount(langs) < 1:
            return None
        buf = ctypes.create_string_buffer(256)
        if not cf.CFStringGetCString(cf.CFArrayGetValueAtIndex(langs, 0), buf, len(buf), _kCFStringEncodingUTF8):
            return None
        return buf.value.decode("utf-8")
    finally:
        cf.CFRelease(langs)
