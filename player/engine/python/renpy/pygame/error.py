"""renpy.pygame.error, without SDL: the error text is kept in this module."""

_last_error = ""


class error(RuntimeError):
    def __init__(self, message=None):
        if message is None:
            message = _last_error

        RuntimeError.__init__(self, message)


def get_error():
    return _last_error


def set_error(message):
    global _last_error

    if isinstance(message, bytes):
        message = message.decode("utf-8", "replace")

    _last_error = message
