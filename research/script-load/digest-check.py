"""Compare shipped .rpyc digests with the .rpy source in one folder.

usage: digest-check.py <folder>

For each <name>.rpy with a <name>.rpyc, the last 16 bytes of the .rpyc are the stored digest.
Ren'Py 8.3.4 stores md5(source). Ren'Py 8.5.3 stores md5(source + RPYC_MAGIC).
Prints how many digests match each rule.
"""

import hashlib
import os
import sys

RPYC_MAGIC = b"_2025-07-06"  # renpy/script.py line 55, Ren'Py 8.5.3

folder = sys.argv[1]
plain = magic = other = missing = 0
for fn in sorted(os.listdir(folder)):
    if not fn.endswith(".rpy"):
        continue
    rpyc = os.path.join(folder, fn + "c")
    with open(os.path.join(folder, fn), "rb") as f:
        src = f.read()
    if not os.path.exists(rpyc):
        missing += 1
        continue
    with open(rpyc, "rb") as f:
        stored = f.read()[-16:]
    if stored == hashlib.md5(src).digest():
        plain += 1
    elif stored == hashlib.md5(src + RPYC_MAGIC).digest():
        magic += 1
    else:
        other += 1
print(f"plain-md5 match: {plain}  magic-md5 match: {magic}  no match: {other}  rpyc missing: {missing}")
