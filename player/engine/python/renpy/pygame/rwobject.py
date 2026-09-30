"""renpy.pygame.rwobject.RWopsIO in pure Python.

Ren'Py's file layer (renpy.loader) returns RWopsIO objects for files, for byte ranges inside archives
(`base` and `length`), for in-memory buffers (`from_buffer`) and for two objects read as one
(`from_split`). The stock class wraps an SDL_RWops; this one wraps a small Python source object.
"""

import io
import os
import sys

RW_SEEK_SET = 0
RW_SEEK_CUR = 1
RW_SEEK_END = 2


class _WindowSource:
    """A byte range [base, base + length) of a seekable binary file."""

    def __init__(self, f, base, length, owns=True):
        self.f = f
        self.base = base or 0
        self.owns = owns

        if length is None:
            length = f.seek(0, os.SEEK_END) - self.base

        self.length = length
        self.pos = 0

    def size(self):
        return self.length

    def tell(self):
        return self.pos

    def seek(self, offset, whence=0):
        if whence == RW_SEEK_CUR:
            offset += self.pos
        elif whence == RW_SEEK_END:
            offset += self.length

        if offset < 0:
            raise OSError(22, "negative seek position")

        self.pos = offset
        return offset

    def readinto(self, b):
        n = min(len(b), max(0, self.length - self.pos))

        if n <= 0:
            return 0

        self.f.seek(self.base + self.pos)
        data = self.f.read(n)
        b[: len(data)] = data
        self.pos += len(data)
        return len(data)

    def write(self, b):
        self.f.seek(self.base + self.pos)
        n = self.f.write(b)
        self.pos += n
        self.length = max(self.length, self.pos)
        return n

    def close(self):
        if self.owns:
            self.f.close()


class _BufferSource(_WindowSource):
    def __init__(self, buffer):
        self.view = memoryview(buffer).cast("B")
        self.length = len(self.view)
        self.pos = 0

    def readinto(self, b):
        n = min(len(b), max(0, self.length - self.pos))
        b[:n] = self.view[self.pos : self.pos + n]
        self.pos += n
        return n

    def write(self, b):
        raise OSError(9, "buffer is read-only")

    def close(self):
        self.view.release()


class _SplitSource(_WindowSource):
    """The concatenation of two sources."""

    def __init__(self, a, b):
        self.a = a
        self.b = b
        self.split = a.size()
        self.length = self.split + b.size()
        self.pos = 0

    def readinto(self, b):
        total = 0
        view = memoryview(b)

        if self.pos < self.split:
            self.a.seek(self.pos)
            n = self.a.readinto(view[: self.split - self.pos])
            total += n
            self.pos += n

        if total < len(view) and self.pos >= self.split and self.pos < self.length:
            self.b.seek(self.pos - self.split)
            n = self.b.readinto(view[total:])
            total += n
            self.pos += n

        return total

    def write(self, b):
        raise OSError(9, "split file is read-only")

    def close(self):
        self.a.close()
        self.b.close()


def _open_file(name, mode):
    if isinstance(name, bytes):
        name = name.decode(sys.getfilesystemencoding(), "surrogateescape")

    return io.open(name, mode if "b" in mode else mode + "b", buffering=0)


class RWopsIO(io.RawIOBase):
    def __init__(self, filelike, mode="rb", base=None, length=None, name=None):
        io.RawIOBase.__init__(self)

        self._source = None
        self.name = name if name is not None else filelike
        self.base = base
        self.length = length

        if filelike is None:
            return

        if isinstance(mode, bytes):
            mode = mode.decode("ascii")

        # Unwrap buffered wrappers to the object below.
        inner = filelike
        while hasattr(inner, "raw") and inner.raw is not inner:
            inner = inner.raw

        if isinstance(inner, RWopsIO):
            if inner._source is None:
                raise ValueError("I/O on closed file.")

            self._source, inner._source = inner._source, None
            self.base, self.length = inner.base, inner.length
            return

        path = None

        if isinstance(filelike, (str, bytes, os.PathLike)):
            path = os.fspath(filelike)
        elif isinstance(inner, io.IOBase) and "r" in mode:
            path = getattr(inner, "name", None)

            if not isinstance(path, (str, bytes)):
                path = None

        if path is not None and "r" in mode:
            if base is None and length is None:
                base = getattr(filelike, "base", None)
                length = getattr(filelike, "length", None)

            try:
                f = _open_file(path, mode)
            except OSError as e:
                raise IOError("Could not open %r: %s" % (path, e)) from None

            self._source = _WindowSource(f, base, length)
            self.base, self.length = base, length
            self.name = name if name is not None else path

            try:
                filelike.close()
            except Exception:
                pass

            return

        if path is not None:
            self._source = _WindowSource(_open_file(path, mode), None, None)
            return

        if not (hasattr(filelike, "read") or hasattr(filelike, "write")):
            raise IOError("%r is not a filename or file-like object." % (filelike,))

        self._source = _WindowSource(filelike, base, length, owns=True)

    def __repr__(self):
        if self.base is not None:
            return "<RWopsIO %r base=%r length=%r>" % (self.name, self.base, self.length)

        return "<RWopsIO %r>" % (self.name,)

    def _src(self):
        if self._source is None:
            raise ValueError("I/O on closed file.")

        return self._source

    def close(self):
        if self._source is not None:
            src, self._source = self._source, None
            src.close()

        io.RawIOBase.close(self)

    @property
    def closed(self):
        return self._source is None

    def fileno(self):
        raise OSError()

    def readable(self):
        return True

    def writable(self):
        return True

    def seekable(self):
        return True

    def seek(self, offset, whence=0):
        return self._src().seek(offset, whence)

    def tell(self):
        return self._src().tell()

    def truncate(self, size=None):
        raise OSError()

    def readinto(self, b):
        return self._src().readinto(memoryview(b).cast("B"))

    def write(self, b):
        return self._src().write(b)

    @staticmethod
    def from_buffer(buffer, mode="rb", name=None):
        """Creates a new RWopsIO over a buffer (no copy)."""

        try:
            memoryview(buffer)
        except TypeError:
            raise ValueError("Passed in object does not support buffer protocol") from None

        rv = RWopsIO(None, name=name)
        rv._source = _BufferSource(buffer)
        return rv

    @staticmethod
    def from_split(a, b, name=None):
        """Creates a new RWopsIO that reads `a` followed by `b`. Both are consumed."""

        sources = []

        for x in (a, b):
            inner = x
            while hasattr(inner, "raw") and inner.raw is not inner:
                inner = inner.raw

            if not isinstance(inner, RWopsIO):
                inner = RWopsIO(x)

            sources.append(inner._src())
            inner._source = None

        rv = RWopsIO(None, name=name)
        rv._source = _SplitSource(*sources)
        return rv
