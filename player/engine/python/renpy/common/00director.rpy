# Player stub for the interactive director (replaces the stock file of the same name; its licence is
# non-commercial only). The names other engine files use stay defined, and the director is inert.

init offset = -1101

default persistent._director_bottom = False

init python in director:

    _constant = True

    from store import Action

    # Inert copies of the documented config values, so `define director.X = ...` in games never fails.
    tag_blacklist = { "black", "text", "vtext", "side" }
    scene_tags = { "bg" }
    show_tags = set()
    transforms = [ "left", "center", "right" ]
    transitions = [ "dissolve", "pixellate" ]
    audio_channels = [ "music", "sound", "audio" ]
    voice_channel = "voice"
    audio_patterns = [ "*.opus", "*.ogg", "*.mp3" ]
    audio_channel_patterns = { }
    button = True
    spacing = 1
    director_spacing = 0
    other_spacing = 0
    viewport_height = 280

    class _State(object):
        active = False
        show_director = False
        mode = "lines"

    state = _State()

    class Start(Action):
        def __call__(self):
            renpy.notify(_("The interactive director is not enabled here."))

        def get_sensitive(self):
            return False

    class Stop(Action):
        def __call__(self):
            return None
