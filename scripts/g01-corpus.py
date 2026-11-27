#!/usr/bin/env python3
"""Build separate reviewed reference and teacher-fed stores from a pre-reviewed corpus.
Never reads final evaluation questions. Teacher errors never become reviewed knowledge.
"""
import argparse, hashlib, json, pathlib, subprocess, time, urllib.request
import importlib.util
from pathlib import Path
_answer_spec=importlib.util.spec_from_file_location('g01_answer',Path(__file__).with_name('g01-answer.py'))
answer_contract=importlib.util.module_from_spec(_answer_spec);_answer_spec.loader.exec_module(answer_contract)
def read(path,limit=8*1024*1024):
    with open(path,"rb") as f:raw=f.read(limit+1)
    if len(raw)>limit:raise ValueError("input limit")
    return raw,json.loads(raw)
def normalize(x):
    return " ".join(x.casefold().split())
def field(x,n=1024):
    return isinstance(x,str) and 0<len(x)<=n and x.strip() and not any(ord(c)<32 or ord(c)==127 for c in x)
def validate(corpus):
    if not isinstance(corpus,list) or not 1<=len(corpus)<=10000:raise ValueError("corpus size")
    seen=set()
    for f in corpus:
        if set(f)!={"subject","relation","answer","group","source","question","reviewer","review_rationale"}:raise ValueError("corpus fields")
        if not all(field(v,4096 if k in ("source","review_rationale") else 256 if k=="reviewer" else 1024) for k,v in f.items()):raise ValueError("invalid corpus field")
        key=(normalize(f["subject"]),normalize(f["relation"]))
        if key in seen:raise ValueError("duplicate or conflicting fact")
        seen.add(key)
    return corpus
def build(corpus,out,binary,teacher=None):
    out.mkdir(parents=True,exist_ok=False)
    tsv=out/"facts.tsv";trace=out/"conversion.jsonl";accepted=[]
    started=time.monotonic()
    with trace.open("w") as log:
        for f in corpus:
            reply=None;error=None;status='reference';value=f["answer"] if teacher is None else None
            if teacher is not None:
                try:
                    reply=teacher(f["question"])
                    candidate,status=answer_contract.extract(reply)
                    if status=="extracted":
                        if answer_contract.matches(candidate,f["answer"],f["group"]):value=f["answer"];status="reference_match"
                        else:status="incorrect"
                except Exception as exc:error=type(exc).__name__;status="transport_error"
            log.write(json.dumps({"subject":f["subject"],"relation":f["relation"],"teacher_raw":reply,"status":status,"accepted":value is not None,"error":error})+"\n")
            if value is not None:accepted.append((f,value))
    with tsv.open("w") as f:
        for item,value in accepted:
            f.write("\t".join([item["subject"],item["relation"],value,item["group"],item["source"]])+"\n")
    if not accepted:raise RuntimeError("no reviewed facts available; conversion trace retained")
    subprocess.run([binary,"index-import",str(tsv),str(out/"knowledge.db")],check=True)
    review_rows=[{"subject":f["subject"],"relation":f["relation"],"object":v,"reviewer":f["reviewer"],"rationale":f["review_rationale"]} for f,v in accepted]
    reviews=out/"reviews.json";reviews.write_text(json.dumps(review_rows))
    subprocess.run([binary,"review-batch",str(out/"policy.db"),str(reviews)],check=True)
    return {"total":len(corpus),"accepted":len(accepted),"seconds":time.monotonic()-started,"storage_bytes":sum((out/n).stat().st_size for n in ["knowledge.db","policy.db"]),"review_scope":"supplied reviewer assertions; not authenticated"}
def main():
    p=argparse.ArgumentParser()
    p.add_argument("corpus");p.add_argument("output");p.add_argument("--binary",default="target/release/air-cli")
    p.add_argument("--mode",choices=["reference","teacher"],required=True)
    p.add_argument("--port",type=int);p.add_argument("--teacher-id");p.add_argument("--teacher-model-sha")
    a=p.parse_args();raw,corpus=read(a.corpus);validate(corpus)
    teacher=None
    if a.mode=="teacher":
        if not a.port or not 1024<=a.port<=65535 or not a.teacher_id or not a.teacher_model_sha or len(a.teacher_model_sha)!=64 or any(c not in "0123456789abcdef" for c in a.teacher_model_sha):raise ValueError("teacher identity and loopback port required")
        def teacher(q):
            body={"messages":[{"role":"system","content":"Give only the short factual answer, or unknown."},{"role":"user","content":q}],"temperature":0,"max_tokens":64,"stream":False}
            req=urllib.request.Request(f"http://127.0.0.1:{a.port}/v1/chat/completions",data=json.dumps(body).encode(),headers={"Content-Type":"application/json"})
            with urllib.request.urlopen(req,timeout=120) as r:
                raw=r.read(65537)
                if len(raw)>65536:raise ValueError("response limit")
                return json.loads(raw)["choices"][0]["message"]["content"]
    out=pathlib.Path(a.output);result=build(corpus,out,a.binary,teacher);result.update({"mode":a.mode,"corpus_sha256":hashlib.sha256(raw).hexdigest(),"teacher_id":a.teacher_id,"declared_teacher_model_sha256":a.teacher_model_sha,"energy_joules":None})
    (out/"conversion-summary.json").write_text(json.dumps(result,indent=2))
if __name__=="__main__":main()
