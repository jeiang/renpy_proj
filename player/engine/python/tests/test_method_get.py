"""Regression: method.__get__(None, owner) returns the bound method (CPython 3.9 and 3.13+ behavior).

CPython 3.11 and 3.12 lost this, and Ren'Py 7 game code calls the form. urp's CPython 3.12.8 build
applies patches/0001-method-get-returns-bound-method.patch to restore it. An interpreter without the
patch cannot pass this test: it is skipped there, and the build check is the enforcement.

    python3 -m unittest discover -s player/engine/python/tests
"""
import pathlib
import sys
import types
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

PATCHED = hasattr(types.MethodType, "__get__")


class A:
    def get_value(self, value):
        return value


@unittest.skipUnless(PATCHED, "interpreter lacks the method __get__ fix (CPython 3.11/3.12 without patches/0001)")
class MethodGet(unittest.TestCase):
    def test_none_instance_returns_the_bound_method(self):
        bound = A().get_value
        self.assertIs(bound.__get__(None, A), bound)

    def test_instance_argument_returns_the_bound_method(self):
        a = A()
        bound = a.get_value
        self.assertIs(bound.__get__(a, A), bound)

    def test_hasattr_form_used_by_game_code_calls_bound_method(self):
        # Ren'Py 7 game code: `if hasattr(v, '__get__'): return v.__get__(None, self)`.
        bound = A().get_value
        self.assertTrue(hasattr(bound, "__get__"))
        self.assertEqual(bound.__get__(None, A)(5), 5)


if __name__ == "__main__":
    unittest.main()
