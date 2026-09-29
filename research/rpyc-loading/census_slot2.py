import sys,zlib,collections,pathlib
sys.path.insert(0,'.')
from inspect_rpyc import slots, globals_of
c1=collections.Counter(); c2=collections.Counter(); v1=[]; store=[]
for f in pathlib.Path(sys.argv[1]).rglob('*.rpyc'):
    k,sl,_=slots(f.read_bytes())
    if k=='v1': v1.append(str(f))
    for n,b in sl.items():
        _,names,_=globals_of(zlib.decompress(b))
        (c1 if n==1 else c2).update(names)
        if n==1 and any(x.startswith('store.') for x in names): store.append((str(f),[x for x in names if x.startswith('store.')]))
print("only in slot2:", sorted(set(c2)-set(c1)))
print("only in slot1:", sorted(set(c1)-set(c2)))
print("v1 files:", v1); print("store refs:", store)
