#!/usr/bin/env python3
"""Combine reference-validated independent teacher traces; never vote unverified facts into existence."""
import argparse,json,hashlib,importlib.util
from pathlib import Path
spec=importlib.util.spec_from_file_location("answer",Path(__file__).with_name("g01-answer.py"))
answer=importlib.util.module_from_spec(spec);spec.loader.exec_module(answer)
def read(p):
    with open(p,"rb") as f:raw=f.read(8*1024*1024+1)
    if len(raw)>8*1024*1024:raise ValueError("input limit")
    return raw
def reconcile(corpus,teachers):
    if not 2<=len(teachers)<=8:raise ValueError("2-8 distinct teachers required")
    keys={(x["subject"],x["relation"]):x for x in corpus}
    if len(keys)!=len(corpus):raise ValueError("duplicate corpus key")
    evidence={k:[] for k in keys}
    for teacher,rows in teachers.items():
        seen=set()
        for row in rows:
            k=(row["subject"],row["relation"])
            if k not in keys or k in seen:raise ValueError("unknown or duplicate trace key")
            seen.add(k)
            f=keys[k]
            candidate,status=answer.extract(row.get("teacher_raw"),*k)
            matched=not row.get("error") and status=="extracted" and answer.matches(candidate,f["answer"],f["group"])
            evidence[k].append({"teacher":teacher,"candidate":candidate,"status":"reference_match" if matched else status if status!="extracted" else "incorrect","matched":matched})
        if seen!=set(keys):raise ValueError("incomplete trace")
    accepted=[]
    for k,f in keys.items():
        matches=[e["teacher"] for e in evidence[k] if e["matched"]]
        if matches:
            accepted.append(dict(f,source=f["source"]+" | matched teachers: "+", ".join(matches)))
    return accepted,[{"subject":k[0],"relation":k[1],"observations":v} for k,v in evidence.items()]
def main():
    p=argparse.ArgumentParser();p.add_argument("corpus");p.add_argument("output");p.add_argument("--trace",action="append",nargs=2,metavar=("TEACHER_ID","TRACE"),required=True);a=p.parse_args()
    corpus=json.loads(read(a.corpus));teachers={};hashes={}
    for name,path in a.trace:
        if name in teachers or not name.strip() or len(name)>256:raise ValueError("duplicate or invalid teacher")
        raw=read(path);digest=hashlib.sha256(raw).hexdigest()
        if digest in hashes.values():raise ValueError("identical trace bytes")
        teachers[name]=[json.loads(line) for line in raw.splitlines()];hashes[name]=digest
    accepted,evidence=reconcile(corpus,teachers)
    out=Path(a.output);out.mkdir(parents=True,exist_ok=False)
    (out/"matched-corpus.json").write_text(json.dumps(accepted,indent=2))
    (out/"reconciliation.json").write_text(json.dumps({"trace_hashes":hashes,"total":len(corpus),"matched":len(accepted),"evidence":evidence,"note":"Teacher identities declared; reviewed reference required; no majority-vote truth claim."},indent=2))
if __name__=="__main__":main()
