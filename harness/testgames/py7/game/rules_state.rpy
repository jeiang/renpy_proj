# State that a Python 2 save holds: long, str bytes, unicode, set, a classic class, nested containers. The after-load
# check raises when a loaded value differs, so a bad resume fails the gate.
init python:
    import collections

    class OldStyle:
        def __init__(self, v):
            self.v = v

        def double(self):
            return self.v * 2

    def synth_check_loaded():
        if not state_ready:   # a save made before case_state holds the defaults
            return
        bad = []
        if saved_long != 2 ** 70:
            bad.append("long")
        if saved_bytes != "caf\xc3\xa9":
            bad.append("bytes")
        if saved_uni != u"caf\xe9":
            bad.append("unicode")
        if saved_set != set([1, 2, 3]):
            bad.append("set")
        if saved_old.double() != 14:
            bad.append("classic")
        if saved_dict["k"] != [1, 2] or saved_dict[u"u"] != (3, 4):
            bad.append("dict")
        if list(saved_od.items()) != [("x", 1), ("y", 2)]:
            bad.append("ordered")
        if saved_tuple != (1, 2.5, None, True):
            bad.append("tuple")
        if bad:
            raise Exception("loaded state differs: " + ", ".join(bad))

    config.after_load_callbacks.append(synth_check_loaded)

default state_ready = False
default saved_long = 0
default saved_bytes = ""
default saved_uni = u""
default saved_set = set()
default saved_old = None
default saved_dict = {}
default saved_od = None
default saved_tuple = ()

label case_state:
    python:
        saved_long = 2 ** 70
        saved_bytes = "caf\xc3\xa9"
        saved_uni = u"caf\xe9"
        saved_set = set([3, 1, 2])
        saved_old = OldStyle(7)
        saved_dict = {"k": [1, 2], u"u": (3, 4)}
        saved_od = collections.OrderedDict([("x", 1), ("y", 2)])
        saved_tuple = (1, 2.5, None, True)
        state_ready = True
        synth_check_loaded()
        state_sum = "%d %d %d" % (len(saved_bytes) > 0, len(saved_set), saved_old.double())
    e "Saved state is ready: [state_sum]."
    e "A save made here holds a Python 2 long, a classic class, a set and an ordered dictionary."
    e "The state checks passed."
    return
