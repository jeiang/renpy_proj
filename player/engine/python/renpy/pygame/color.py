"""renpy.pygame.color.Color in pure Python. The colour names come from the generated _color_dict module."""

import binascii
import colorsys
import struct

from renpy.pygame._color_dict import colors


def _byte(v):
    v = int(v)

    if not 0 <= v <= 255:
        raise ValueError(v)

    return v


class Color:
    __slots__ = ("r", "g", "b", "a", "length", "__weakref__")

    def __init__(self, *args):
        self.r = self.g = self.b = 0
        self.a = 255
        self.length = 4

        if len(args) == 1:
            c = args[0]

            if isinstance(c, str):
                if c.startswith("#"):
                    self._from_hex(c[1:])
                elif c.startswith("0x"):
                    self._from_hex(c[2:])
                else:
                    self._from_name(c)
            elif isinstance(c, (tuple, list, Color)):
                if len(c) == 4:
                    self._set(c[0], c[1], c[2], c[3])
                elif len(c) == 3:
                    self._set(c[0], c[1], c[2], 255)
                else:
                    raise ValueError(c)
            else:
                self._from_hex("%08x" % c)
        elif len(args) == 3:
            self._set(*args, 255)
        elif len(args) == 4:
            self._set(*args)

    def _set(self, r, g, b, a):
        self.r, self.g, self.b, self.a = _byte(r), _byte(g), _byte(b), _byte(a)

    def _from_hex(self, c):
        if len(c) in (3, 4):
            c = "".join(ch * 2 for ch in c)

        try:
            if len(c) == 6:
                r, g, b = struct.unpack("BBB", binascii.unhexlify(c))
                a = 255
            elif len(c) == 8:
                r, g, b, a = struct.unpack("BBBB", binascii.unhexlify(c))
            else:
                raise ValueError(c)
        except (TypeError, binascii.Error):
            raise ValueError(c)

        self._set(r, g, b, a)

    def _from_name(self, c):
        c = "".join(c.split()).lower()

        try:
            r, g, b = colors[c]
        except KeyError:
            raise ValueError(c)

        self._set(r, g, b, 255)

    def __eq__(self, other):
        if isinstance(other, tuple):
            try:
                other = Color(other)
            except ValueError:
                return False

        if not isinstance(other, Color):
            return False

        return (self.r, self.g, self.b, self.a) == (other.r, other.g, other.b, other.a)

    def __ne__(self, other):
        return not self.__eq__(other)

    __hash__ = None

    def __repr__(self):
        return str((self.r, self.g, self.b, self.a))

    def __int__(self):
        return struct.unpack(">L", struct.pack("BBBB", self.r, self.g, self.b, self.a))[0]

    def __index__(self):
        return int(self)

    def __float__(self):
        return float(int(self))

    def __hex__(self):
        return hex(int(self))

    def __oct__(self):
        return oct(int(self))

    def __reduce__(self):
        return (Color, (), {"rgba": (self.r, self.g, self.b, self.a)})

    def __setstate__(self, d):
        self.r, self.g, self.b, self.a = d["rgba"]

    def __len__(self):
        return self.length

    def __getitem__(self, key):
        if isinstance(key, slice):
            return tuple(self)[key]

        if key >= self.length:
            raise IndexError(key)

        return (self.r, self.g, self.b, self.a)[key]

    def __iter__(self):
        return iter((self.r, self.g, self.b, self.a)[: self.length])

    def __setitem__(self, key, val):
        if not isinstance(val, int):
            raise ValueError(val)

        if key >= self.length or key > 3:
            raise IndexError(key)

        val = _byte(val)

        if key == 0:
            self.r = val
        elif key == 1:
            self.g = val
        elif key == 2:
            self.b = val
        else:
            self.a = val

    def _op(self, rhs, fn):
        if not isinstance(rhs, Color):
            return NotImplemented

        return type(self)(*(fn(x, y) for x, y in zip((self.r, self.g, self.b, self.a), (rhs.r, rhs.g, rhs.b, rhs.a))))

    def __mul__(self, rhs):
        return self._op(rhs, lambda x, y: min(255, x * y))

    def __add__(self, rhs):
        return self._op(rhs, lambda x, y: min(255, x + y))

    def __sub__(self, rhs):
        return self._op(rhs, lambda x, y: max(0, x - y))

    def __mod__(self, rhs):
        return self._op(rhs, lambda x, y: 0 if y == 0 else x % y)

    def __truediv__(self, rhs):
        return self._op(rhs, lambda x, y: min(255, x // y) if y else 255)

    def __floordiv__(self, rhs):
        return self._op(rhs, lambda x, y: 0 if y == 0 else min(255, x // y))

    @property
    def cmy(self):
        return 1 - (self.r / 255.0), 1 - (self.g / 255.0), 1 - (self.b / 255.0)

    @cmy.setter
    def cmy(self, val):
        c, m, y = val
        self.r, self.g, self.b = int((1 - c) * 255), int((1 - m) * 255), int((1 - y) * 255)

    @property
    def hsva(self):
        h, s, v = colorsys.rgb_to_hsv(self.r / 255.0, self.g / 255.0, self.b / 255.0)
        return h * 360.0, s * 100.0, v * 100.0, self.a / 255.0 * 100

    @hsva.setter
    def hsva(self, val):
        if len(val) == 3:
            h, s, v = val
            a = 0.0
        else:
            h, s, v, a = val

        r, g, b = colorsys.hsv_to_rgb((h % 360.0) / 360.0, s / 100.0, v / 100.0)
        self.r, self.g, self.b, self.a = int(255 * r), int(255 * g), int(255 * b), int(255 * a / 100.0)

    @property
    def hsla(self):
        h, l, s = colorsys.rgb_to_hls(self.r / 255.0, self.g / 255.0, self.b / 255.0)
        return h * 360.0, min(100.0, s * 100), min(100.0, l * 100), self.a / 255.0 * 100

    @hsla.setter
    def hsla(self, val):
        if len(val) == 3:
            h, s, l = val
            a = 0.0
        else:
            h, s, l, a = val

        r, g, b = colorsys.hls_to_rgb((h % 360.0) / 360.0, l / 100.0, s / 100.0)
        self.r, self.g, self.b, self.a = int(255 * r), int(255 * g), int(255 * b), int(255 * a / 100.0)

    @property
    def i1i2i3(self):
        r, g, b = self.r / 255.0, self.g / 255.0, self.b / 255.0
        return (r + g + b) / 3.0, (r - b) / 2.0, (2 * g - r - b) / 4.0

    @i1i2i3.setter
    def i1i2i3(self, val):
        i1, i2, i3 = val
        self.r = int((i1 + i2 - (2.0 / 3.0 * i3)) * 255)
        self.g = int((i1 + (4.0 / 3.0 * i3)) * 255)
        self.b = int((i1 - i2 - (2.0 / 3.0 * i3)) * 255)

    def normalize(self):
        return self.r / 255.0, self.g / 255.0, self.b / 255.0, self.a / 255.0

    def correct_gamma(self, gamma):
        return type(self)(tuple(int(round(pow(x / 255.0, gamma) * 255)) for x in tuple(self)))

    def set_length(self, n):
        if n > 4 or n < 1:
            raise ValueError(n)

        self.length = n
