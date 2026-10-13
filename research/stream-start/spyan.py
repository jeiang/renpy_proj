import json,sys,collections
d=json.load(open(sys.argv[1]))
frames=d['shared']['frames']
prof=d['profiles'][0] if len(d['profiles'])==1 else None
tot=collections.Counter(); self_=collections.Counter(); n=0
for p in d['profiles']:
    if p['type']!='sampled': continue
    w=p['weights']
    for s,wt in zip(p['samples'],w):
        n+=wt
        seen=set()
        for fi in s:
            f=frames[fi]; key=f['name']+' '+(f.get('file','') or '').split('/')[-1]+':'+str(f.get('line',''))
            k2=f['name']+' ('+(f.get('file','') or '').split('/')[-1]+')'
            if k2 not in seen:
                tot[k2]+=wt; seen.add(k2)
        if s:
            f=frames[s[-1]]; self_[f['name']+' ('+(f.get('file','') or '').split('/')[-1]+')']+=wt
print('profiles',len(d['profiles']),'total weight',n, 'unit',d['profiles'][0].get('unit'))
pat=sys.argv[2:] 
print('--- inclusive top')
for k,v in tot.most_common(45): print('%8.3f %s'%(v,k))
print('--- self top')
for k,v in self_.most_common(25): print('%8.3f %s'%(v,k))
