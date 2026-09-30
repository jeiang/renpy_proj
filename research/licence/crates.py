import json,glob,sys,os
for f in sorted(glob.glob(os.path.join(os.path.dirname(__file__),'upstream','crate-*.json'))):
    try: d=json.load(open(f))
    except Exception as e: print(f,'bad'); continue
    c=d['crate']; vs=[x for x in d["versions"] if not x["yanked"]] or d["versions"]; v=vs[0] if vs else {"num":"?","license":"?","updated_at":"????"}
    print(f"{c['name']:26} {v['num']:14} {v['license']!s:40} {v['updated_at'][:10]}  {c.get('repository')}")
