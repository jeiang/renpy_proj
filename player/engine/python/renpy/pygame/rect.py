"""renpy.pygame.rect.Rect in pure Python. The four fields are integers, as in the stock C-typed class."""


def _flatten(args):
    return args[0] if len(args) == 1 else args


def _rect(other):
    return other if isinstance(other, Rect) else Rect(other)


class Rect:
    __slots__ = ("_x", "_y", "_w", "_h")

    def __init__(self, *args):
        n = len(args)

        if n == 1 and isinstance(args[0], Rect):
            x, y, w, h = args[0]._x, args[0]._y, args[0]._w, args[0]._h
        elif n == 1 and len(args[0]) == 4:
            x, y, w, h = args[0]
        elif n == 1 and len(args[0]) == 2:
            x, y = args[0]
            w = h = 0
        elif n == 2:
            x, y = args[0]
            w, h = args[1]
        elif n == 4:
            x, y, w, h = args
        else:
            raise TypeError("Argument must be a rect style object.")

        self._x = int(x)
        self._y = int(y)
        self._w = int(w)
        self._h = int(h)

    def __reduce__(self):
        return (Rect, (self._x, self._y, self._w, self._h))

    def __repr__(self):
        return "<rect(%d, %d, %d, %d)>" % (self._x, self._y, self._w, self._h)

    def __len__(self):
        return 4

    def __iter__(self):
        return iter((self._x, self._y, self._w, self._h))

    def __eq__(self, other):
        try:
            other = _rect(other)
        except (TypeError, ValueError):
            return NotImplemented

        return tuple(self) == tuple(other)

    def __ne__(self, other):
        rv = self.__eq__(other)
        return rv if rv is NotImplemented else not rv

    __hash__ = None

    def __getitem__(self, key):
        return (self._x, self._y, self._w, self._h)[key]

    def __setitem__(self, key, val):
        if key == 0:
            self.x = val
        elif key == 1:
            self.y = val
        elif key == 2:
            self.w = val
        elif key == 3:
            self.h = val
        else:
            raise IndexError(key)

    @property
    def x(self):
        return self._x

    @x.setter
    def x(self, val):
        self._x = int(val)

    @property
    def y(self):
        return self._y

    @y.setter
    def y(self, val):
        self._y = int(val)

    @property
    def w(self):
        return self._w

    @w.setter
    def w(self, val):
        self._w = int(val)

    @property
    def h(self):
        return self._h

    @h.setter
    def h(self, val):
        self._h = int(val)

    left = x
    top = y
    width = w
    height = h

    @property
    def right(self):
        return self._x + self._w

    @right.setter
    def right(self, val):
        self.x = self._x + val - self.right

    @property
    def bottom(self):
        return self._y + self._h

    @bottom.setter
    def bottom(self, val):
        self.y = self._y + val - self.bottom

    @property
    def size(self):
        return (self._w, self._h)

    @size.setter
    def size(self, val):
        self.w, self.h = val

    @property
    def topleft(self):
        return (self._x, self._y)

    @topleft.setter
    def topleft(self, val):
        self.x, self.y = val

    @property
    def topright(self):
        return (self.right, self._y)

    @topright.setter
    def topright(self, val):
        self.right, self.y = val

    @property
    def bottomright(self):
        return (self.right, self.bottom)

    @bottomright.setter
    def bottomright(self, val):
        self.right, self.bottom = val

    @property
    def bottomleft(self):
        return (self._x, self.bottom)

    @bottomleft.setter
    def bottomleft(self, val):
        self.x, self.bottom = val

    @property
    def centerx(self):
        return self._x + self._w // 2

    @centerx.setter
    def centerx(self, val):
        self.x = self._x + val - self.centerx

    @property
    def centery(self):
        return self._y + self._h // 2

    @centery.setter
    def centery(self, val):
        self.y = self._y + val - self.centery

    @property
    def center(self):
        return (self.centerx, self.centery)

    @center.setter
    def center(self, val):
        self.centerx, self.centery = val

    @property
    def midtop(self):
        return (self.centerx, self._y)

    @midtop.setter
    def midtop(self, val):
        self.centerx, self.y = val

    @property
    def midleft(self):
        return (self._x, self.centery)

    @midleft.setter
    def midleft(self, val):
        self.x, self.centery = val

    @property
    def midbottom(self):
        return (self.centerx, self.bottom)

    @midbottom.setter
    def midbottom(self, val):
        self.centerx, self.bottom = val

    @property
    def midright(self):
        return (self.right, self.centery)

    @midright.setter
    def midright(self, val):
        self.right, self.centery = val

    def copy(self):
        return Rect(self)

    def move(self, *args):
        r = self.copy()
        r.move_ip(*args)
        return r

    def move_ip(self, *args):
        x, y = _flatten(args)
        self.x = self._x + x
        self.y = self._y + y

    def inflate(self, *args):
        r = self.copy()
        r.inflate_ip(*args)
        return r

    def inflate_ip(self, *args):
        x, y = _flatten(args)
        c = self.center
        self.w = self._w + x
        self.h = self._h + y
        self.center = c

    def clamp(self, other):
        r = self.copy()
        r.clamp_ip(other)
        return r

    def clamp_ip(self, other):
        other = _rect(other)

        if self._w > other._w or self._h > other._h:
            self.center = other.center
            return

        if self.left < other.left:
            self.left = other.left
        elif self.right > other.right:
            self.right = other.right

        if self.top < other.top:
            self.top = other.top
        elif self.bottom > other.bottom:
            self.bottom = other.bottom

    def clip(self, other, y=None, w=None, h=None):
        if type(other) is int:
            other = Rect(other, y, w, h)

        other = _rect(other)

        if not self.colliderect(other):
            return Rect(0, 0, 0, 0)

        r = self.copy()

        if r.left < other.left:
            d = other.left - r.left
            r.left += d
            r.width -= d
        if r.right > other.right:
            r.width -= r.right - other.right
        if r.top < other.top:
            d = other.top - r.top
            r.top += d
            r.height -= d
        if r.bottom > other.bottom:
            r.height -= r.bottom - other.bottom

        return r

    def union(self, other):
        r = self.copy()
        r.union_ip(other)
        return r

    def union_ip(self, other):
        other = _rect(other)

        x = min(self._x, other._x)
        y = min(self._y, other._y)
        w = max(self.right, other.right) - x
        h = max(self.bottom, other.bottom) - y
        self._x, self._y, self._w, self._h = x, y, w, h

    def unionall(self, other_seq):
        r = self.copy()
        r.unionall_ip(other_seq)
        return r

    def unionall_ip(self, other_seq):
        for other in other_seq:
            self.union_ip(other)

    def fit(self, other):
        other = _rect(other)

        r = self.copy()
        r.topleft = other.topleft
        factor = min(other._w / float(r._w), other._h / float(r._h))
        r.w = r._w * factor
        r.h = r._h * factor
        return r

    def normalize(self):
        if self._w < 0:
            self.x = self._x + self._w
            self.w = -self._w
        if self._h < 0:
            self.y = self._y + self._h
            self.h = -self._h

    def contains(self, other):
        other = _rect(other)

        return (
            other._x >= self._x
            and other.right <= self.right
            and other._y >= self._y
            and other.bottom <= self.bottom
            and other.left < self.right
            and other.top < self.bottom
        )

    def collidepoint(self, x, y=None):
        if type(x) is tuple:
            x, y = x

        return x >= self._x and y >= self._y and x < self.right and y < self.bottom

    def colliderect(self, other):
        other = _rect(other)

        return self.left < other.right and self.top < other.bottom and self.right > other.left and self.bottom > other.top

    def collidelist(self, other_list):
        for n, other in enumerate(other_list):
            if self.colliderect(other):
                return n

        return -1

    def collidelistall(self, other_list):
        return [n for n, other in enumerate(other_list) if self.colliderect(other)]

    def collidedict(self, other_dict, rects_values=0):
        for key, val in other_dict.items():
            if self.colliderect(val):
                return key, val

        return None

    def collidedictall(self, other_dict, rects_values=0):
        return [(key, val) for key, val in other_dict.items() if self.colliderect(val)]
