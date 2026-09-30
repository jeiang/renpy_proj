"""Stand-in for renpy.gl2.assimp (3D model loading is not part of the player yet).

The names other Ren'Py modules use exist so `renpy.import_all()` succeeds and games that never load a
model run. Loading a model raises an error that says so.
"""

import threading

import renpy
from renpy.display.displayable import Displayable

loader = None
loader_lock = threading.RLock()

MESSAGE = "GLTFModel needs the assimp 3D loader, which this player build does not include."


def free_memory():
    pass


def finish_predict():
    pass


def preload():
    pass


class GLTFModel(Displayable):
    """Raises when a game creates a model."""

    def __init__(self, *args, **kwargs):
        raise NotImplementedError(MESSAGE)
