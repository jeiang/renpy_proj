# Classes: __metaclass__, __nonzero__, next, __div__, __cmp__, __eq__ without __hash__.
init python:
    class Meta(type):
        def __new__(mcs, name, bases, ns):
            ns["tagged"] = "meta-" + name
            return type.__new__(mcs, name, bases, ns)

    class Tagged(object):
        __metaclass__ = Meta

    class Flag(object):
        def __init__(self, v):
            self.v = v

        def __nonzero__(self):
            return self.v != 0

    class Count3(object):
        def __init__(self):
            self.i = 0

        def __iter__(self):
            return self

        def next(self):
            self.i += 1
            if self.i > 3:
                raise StopIteration
            return self.i

    class Money(object):
        def __init__(self, v):
            self.v = v

        def __div__(self, o):
            return Money(self.v / o)

    class Ver(object):
        def __init__(self, n):
            self.n = n

        def __cmp__(self, o):
            return cmp(self.n, o.n)

    class Pt(object):
        def __init__(self, x):
            self.x = x

        def __eq__(self, o):
            return self.x == o.x

label case_classes:
    python:
        tag = Tagged.tagged
        flags = "%s %s" % (bool(Flag(0)), bool(Flag(2)))
        counted = [i for i in Count3()]
        counted_text = ",".join([str(i) for i in counted])
        half = (Money(7) / 2).v
        ver_lt = Ver(1) < Ver(2)
        ver_ge = Ver(1) >= Ver(2)
        p = Pt(4)
        in_set = len(set([p, p]))
        as_key = {p: "found"}[p]
    e "The metaclass tagged the class as [tag]."
    e "Truth values: [flags]. Iterator: [counted_text]. Division operator: [half]."
    e "Comparison: [ver_lt] and [ver_ge]; set size [in_set]; dict key [as_key]."
    return
