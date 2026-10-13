import json,sys
rows=[json.loads(l) for l in open(sys.argv[1])]
keys=["Loading script","Running init code","Loading save slot metadata","Loading analysis data","Analyze and compile ATL","Reloading save slot metadata","Initial gc","Creating interface object","Init translation","Analyze screens","Cleaning stores"]
print("%-22s %5s %6s | %5s %5s %5s %5s %5s | %s"%("tag","rc","wall","init","menu","click","label","1st","took(ms): load init slotmeta analyzeATL gc iface"))
for r in rows:
    m=r["ms"];t=r["took"]
    print("%-22s %5s %6s | %5s %5s %5s %5s %5s | %s"%(r["tag"],r["rc"],r["wall"],m.get("init-done"),m.get("menu-interact"),m.get("click-start"),m.get("label-start"),m.get("start-first-interact"),
      " ".join(str(t.get(k,"-")) for k in ["Early init","Loading script","Loading save slot metadata","Running init code","Analyze and compile ATL","Initial gc","Creating interface object","Analyze screens"])))
