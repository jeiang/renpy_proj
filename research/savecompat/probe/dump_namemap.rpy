# Dropped into a cloned game's game/ dir. Dumps the script namemap (what load() checks with has_label).
init 999 python:
    import json, os
    # namemap is keyed by the Node itself (Node.__eq__/__hash__ compare by name), so take .name
    d = {n.name: n for n in renpy.game.script.namemap.values()}
    out = os.path.join(os.environ["RENPY_PATH_TO_SAVES"], "namemap.json")
    with open(out, "w") as f:
        json.dump({"str": [k for k in d if isinstance(k, str)],
                   "tuple": [list(k) for k in d if isinstance(k, tuple)],
                   "config_version": str(getattr(renpy.config, "version", None)),
                   "save_directory": str(renpy.config.save_directory)}, f)
