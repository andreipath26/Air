"""Execute draft real-fact workload using a resident FP32 teacher; not a quantized G01 gate."""
import json,subprocess,sys,time,hashlib,urllib.request
from pathlib import Path
def run(out,port,teacher_id,weights):
    root=Path("knowledge/development")
    corpus=root/"g01-corpus.json";dataset=root/"g01-dataset.json";router=root/"g01-router.json"
    teacher_hash=hashlib.sha256(json.dumps(weights,sort_keys=True).encode()).hexdigest()
    out.mkdir()
    def script(name,*args):
        subprocess.run([sys.executable,"scripts/"+name,*map(str,args)],check=True,timeout=2400)
    script("g01-corpus.py",corpus,out/"reference-store","--mode","reference")
    script("g01-corpus.py",corpus,out/"teacher-store","--mode","teacher","--port",port,"--teacher-id",teacher_id,"--teacher-model-sha",teacher_hash)
    for name,store in [("reference_retrieval","reference-store"),("air","teacher-store")]:
        script("g01-benchmark.py",dataset,out/name,"--system",name,"--db",out/store/"knowledge.db","--policy",out/store/"policy.db","--router",router)
    cases=json.loads(dataset.read_text());answers={};errors={};started=time.monotonic()
    with (out/"teacher-responses.jsonl").open("w") as trace:
        for c in cases:
            answer=None
            try:
                req=urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",data=json.dumps({"messages":[{"role":"system","content":"Give only the short factual answer, or unknown if uncertain."},{"role":"user","content":c["question"]}],"temperature":0,"max_tokens":64}).encode(),headers={"Content-Type":"application/json"})
                with urllib.request.urlopen(req,timeout=120) as r:raw=json.loads(r.read(65537))["choices"][0]["message"]["content"].strip()
                answer=None if raw.lower().rstrip(".") in ("unknown","null","i don't know") else raw.rstrip(".")
            except Exception as e:errors[c["id"]]=type(e).__name__
            answers[c["id"]]=answer;trace.write(json.dumps({"id":c["id"],"answer":answer,"error":errors.get(c["id"])})+"\n")
    systems={"fp32_teacher":{"answers":answers,"errors":errors,"wall_seconds":time.monotonic()-started}}
    for n in ["air","reference_retrieval"]:systems[n]=json.loads((out/n/"observations.json").read_text())
    summary={}
    for name,s in systems.items():
        groups={}
        for c in cases:
            actual=s["answers"][c["id"]];expected=c["answer"];group=c["group"]
            norm=lambda x: None if x is None else " ".join(x.split()) if group in ("science","engineering") else " ".join(x.casefold().split())
            correct=c["id"] not in s["errors"] and norm(actual)==norm(expected)
            row=groups.setdefault(group,{"correct":0,"total":0});row["correct"]+=int(correct);row["total"]+=1
        summary[name]={"correct":sum(x["correct"] for x in groups.values()),"total":len(cases),"groups":groups,"wall_seconds":s["wall_seconds"],"errors":len(s["errors"])}
    (out/"summary.json").write_text(json.dumps({"systems":summary,"gate_eligible":False,"note":"FP32 teacher remains resident during AIR measurement; timing/resource data not comparable to standalone G01; draft shared-author workload."},indent=2))
    if errors:raise RuntimeError("teacher transport failures")
