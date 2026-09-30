"""Which pickled renpy.*/pygame_sdl2 classes live in native (Cython, built-in in the SDK) modules. Run under the SDK python with SDK=<sdk dir>."""
import sys,os,json,importlib
sys.path.insert(0,os.environ["SDK"]); sys.path.insert(0,".")
import renpy; renpy.import_all()
import savescan as S
from collections import Counter
c=Counter()
import re
saves=Counter()
for l in open("out/census.jsonl"):
    r=json.loads(l)
    for k in r.get("globals",{}): saves[k]+=1
for k in saves:
    m,n=k.split(" ",1)
    if not m.startswith(("renpy","pygame")) : continue
    mm,nn=S._fix_imports(m,n)
    try:
        o=getattr(importlib.import_module(mm),nn); ow=importlib.import_module(o.__module__)
        f=getattr(ow,"__file__","?")
        c[f.rsplit(".",1)[-1]]+=1
        if not f.endswith(".py"): print(saves[k],k,o.__module__,f[-40:])
    except Exception as e: print("ERR",k,e)
print(c)
