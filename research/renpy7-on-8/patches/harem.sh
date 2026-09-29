# Harem_Hotel: 8.5.3 rejects `scene X with fade:` followed by no indented block (7.4.11 / 8.0.1 accepted it).
sed -i '' 's/^\( *scene 2022-01-13_12-55-38 20220113193845904 with fade\):/\1/' mod/Lain_NewScenes.rpy
# `screen shower1` with no body (empty screen definition inside a label): 8.5.3 rejects; 7.x accepted. Drop it.
sed -i '' 's/^    screen shower1\(\r*\)$/    # screen shower1 (empty screen statement removed for 8.x)\1/' script.rpy
# py2 print statements (42 lines in 3 files) -> print(...). Mechanical (same as 2to3 -f print for these simple forms).
find . -name '*.rpy' -exec perl -pi -e 's/^(\s*)(\$ *)?print\s+(?![(=\s])(.*?)(\r?)$/$1$2print($3)$4/' {} +
# py2 orders None < bool; py3 raises TypeError. locked is None|bool: sort on bool(locked). (file was rpyc-only: patched on the unrpyc output)
sed -i '' "s/key=attrgetter('locked'))/key=lambda e: bool(e.locked))/" mod/scripts/enc.rpy
# py2 sorts int before str in one list; py3 raises. Sort key -> (kind, value), same order as py2.
sed -i '' "s/return SEED_TIER_MAPPING.get(item\['id'\], 0)/return (0, SEED_TIER_MAPPING.get(item['id'], 0))/; s/^            return item\['name'\]\(\r*\)$/            return (1, item['name'])\1/" scripts/Garden/GardenShop.rpy
# Normalise CRLF -> LF in all sources (SecretIsland/shared-engine-launcher hit the same 8.5.3 slast ValueError with CRLF sources)
find . \( -name '*.rpy' -o -name '*.rpym' \) -exec perl -pi -e 's/\r$//' {} +
rm -rf cache; find . -name '*.rpyc' -delete
