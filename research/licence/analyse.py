"""Summarise licences of the normal (shipped) transitive dependencies of probe/Cargo.toml.
Usage: (cd probe && cargo metadata --format-version 1) > meta.json; python3 analyse.py meta.json"""
import json,sys,collections
m=json.load(open(sys.argv[1]))
pk={p['id']:p for p in m['packages']}
nodes={n['id']:n for n in m['resolve']['nodes']}
root=m['resolve']['root']
seen=set(); stack=[root]
while stack:
    i=stack.pop()
    if i in seen: continue
    seen.add(i)
    for d in nodes[i]['deps']:
        if any(k['kind'] is None for k in d['dep_kinds']): stack.append(d['pkg'])
seen.discard(root)
by=collections.defaultdict(list)
for i in seen:
    p=pk[i]; by[p.get('license') or ('FILE:'+str(p.get('license_file')))].append(f"{p['name']} {p['version']}")
print(len(seen),'shipped crates;',len(by),'distinct licence expressions\n')
for l,v in sorted(by.items(),key=lambda x:-len(x[1])):
    print(f"{len(v):4}  {l}")
    if len(v)<=12 or not any(t in l for t in ['MIT','Apache']) : print('        ',', '.join(sorted(v)))
print('\n## copyleft or unusual (no permissive alternative visible)')
def flag(l):
    up=l.upper()
    if any(x in up for x in ['GPL','MPL','CC-BY','UNLICENSE','WTFPL','FILE:','UNKNOWN','OPENSSL','BUSL','SSPL','CDLA','EPL']): return True
    return False
for l,v in sorted(by.items()):
    if flag(l): print(l,'->',', '.join(sorted(v)))
