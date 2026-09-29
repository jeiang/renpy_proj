# Second Astral port edit found by this ticket (run in the clone's game/ dir after renpy7-on-8/patches/astral.sh):
# Python 2 `exec("point = ...")` inside a function created a local; on Python 3 it does not, so `label start` -> after_load
# hooks raised NameError: name 'point' is not defined (prologue never starts). Use eval instead.
perl -pi -e 's/exec\("point = " \+ outf\[0\]\)/point = eval(outf[0])/' updates/loading/update_outfits.rpy
# Python 2 `dict.keys()/items()` returned lists; on Python 3 they are views (no .sort, no indexing). Seven assignments in the source.
perl -pi -e 's/^(\s*(?:default\s+)?[A-Za-z_.]+\s*=\s*)([A-Za-z_.\[\]"]+\.(?:keys|values|items)\(\))\s*$/$1list($2)\n/' \
  screens/menu/craft.rpy screens/menus/girls/sexpos_counter.rpy variables/classes/Being.rpy variables/classes/Inventory.rpy variables/classes/Astral.rpy variables/classes/RandomBag.rpy
