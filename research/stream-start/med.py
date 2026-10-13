import json,statistics as st,collections,sys
rows=[]
for fn in sys.argv[1:]:
    rows+= [json.loads(l) for l in open(fn)]
g=collections.defaultdict(list)
for r in rows:
    tag=r['tag']; 
    name,_,kind=tag.rpartition('-')
    kind=''.join(c for c in kind if not c.isdigit())
    g[(name,kind)].append(r)
def med(lst,f): 
    v=[f(r) for r in lst if f(r) is not None]; return round(st.median(v),2) if v else None
print("%-28s %3s | %6s %6s %6s | %6s %6s %6s %6s %6s %6s %6s"%("game,kind","n","init","menu","start1","load","slotm","init","atl","gc","iface","1st-click"))
for (n,k),l in sorted(g.items()):
    T=lambda key: (lambda r: r['took'].get(key))
    print("%-28s %3d | %6s %6s %6s | %6s %6s %6s %6s %6s %6s"%(n+","+k,len(l),med(l,lambda r:r['ms'].get('init-done')),med(l,lambda r:r['ms'].get('menu-interact')),med(l,lambda r:r['ms'].get('start-first-interact')),
      med(l,T('Loading script')),med(l,T('Loading save slot metadata')),med(l,T('Running init code')),med(l,T('Analyze and compile ATL')),med(l,T('Initial gc')),med(l,T('Interface start'))))
