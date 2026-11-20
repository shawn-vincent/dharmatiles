"""Turn the hand-written traces (data/tr_ap.py, tr_t.py, tr_s.py) into data/traces.json."""
import json, os, sys
root=os.path.dirname(os.path.abspath(__file__)); here=root+'/data'; sys.path.insert(0,here)
P=lambda s:[[float(a) for a in p.split(',')] for p in s.split()] if s else None
FRAME={'smooth':{'plate':[[3.5,79],[48,96],[96,78]],'fig':{'base':[31,77.5,13.5,4.2],'foot':[30,77],'head':[30,38]}}}
out={}
def add(mod, sheet, pre):
    m=__import__(mod)
    for k,stones in m.CELLS.items():
        c={'id':k,'sheet':sheet,'img':(pre+k).lower()}
        fr=getattr(m,'FRAMES',{}).get(k) or FRAME.get(sheet)
        if fr: c.update(fr)
        c['stones']=[{'id':k+s[0],'kind':s[1],'rests':[k+r for r in s[2].split()],'out':P(s[3]),'top':P(s[4])} for s in stones]
        if hasattr(m,'MASS') and k in m.MASS: c['mass']=P(m.MASS[k])
        out[k]=c
for mod,sheet,pre in [('tr_ap','smooth','ap-'),('tr_t','twenty',''),('tr_s','sixteen','')]:
    if os.path.exists(f'{here}/{mod}.py'): add(mod,sheet,pre)
json.dump(out,open(f'{root}/data/traces.json','w'),indent=None,separators=(',',':'))
print(len(out),'cells',sum(len(c['stones']) for c in out.values()),'stones')
