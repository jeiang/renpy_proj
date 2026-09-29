# Extra Astral port patch found by the visual run (cwd = game/). py2 `exec "x = ..."` inside a function binds a visible local;
# py3 exec() inside a function does not. update_outfits() (called from after_load on New Game) reads `point` afterwards -> NameError.
sed -i '' 's/^\( *\)exec("point = " + outf\[0\])/\1point = eval(outf[0])/' updates/loading/update_outfits.rpy
sed -i '' 's/^\( *\)exec("point =" + str(point\[0\]))/\1point = eval(str(point[0]))/; s/^\( *\)exec("pointer = " + str(ev\[0\]))/\1pointer = eval(str(ev[0]))/' functions/qol/find_events.rpy
# py2 dict.keys()/values()/items() return lists; py3 views (no .sort(), no indexing). Wrap plain `x = d.keys()` assignments in list().
# (First hit: Inventory.restoreOrder -> AttributeError 'dict_keys' has no attribute 'sort'.)
find . -name '*.rpy' -exec perl -pi -e 's/^(\s*(?:\$\s*)?[\w.\[\]]+\s*=\s*)([\w.\[\]]+\.(?:keys|values|items)\(\))(\s*)$/$1list($2)$3/' {} +
