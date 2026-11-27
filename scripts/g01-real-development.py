"""Execute draft real-fact workload using a resident FP32 teacher; not a quantized G01 gate."""
import json,subprocess,sys,time,hashlib,urllib.request
from pathlib import Path
import importlib.util
from pathlib import Path
_answer_spec=importlib.util.spec_from_file_location('g01_answer',Path(__file__).with_name('g01-answer.py'))
answer_contract=importlib.util.module_from_spec(_answer_spec);_answer_spec.loader.exec_module(answer_contract)
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
    cases=json.loads(dataset.read_text());answers={};statuses={};transport_errors={};started=time.monotonic()
    with (out/"teacher-responses.jsonl").open("w") as trace:
        for c in cases:
            answer=None;raw=None;status="transport_error"
            try:
                req=urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",data=json.dumps({"messages":[{"role":"system","content":"Give only the short factual answer, or unknown if uncertain."},{"role":"user","content":c["question"]}],"temperature":0,"max_tokens":64}).encode(),headers={"Content-Type":"application/json"})
                with urllib.request.urlopen(req,timeout=120) as r:raw=json.loads(r.read(65537))["choices"][0]["message"]["content"].strip()
                answer,status=answer_contract.extract(raw)
            except Exception as e:transport_errors[c["id"]]=type(e).__name__
            statuses[c["id"]]=status
            answers[c["id"]]=answer
            trace.write(json.dumps({"id":c["id"],"teacher_raw":raw,"answer":answer,"status":status,"transport_error":transport_errors.get(c["id"])},ensure_ascii=False)+"\n")
    systems={"fp32_teacher":{"answers":answers,"statuses":statuses,"errors":transport_errors,"wall_seconds":time.monotonic()-started}}
    for n in ["air","reference_retrieval"]:systems[n]=json.loads((out/n/"observations.json").read_text())
    summary={}
    for name,s in systems.items():summary[name]=score(cases,s)
    (out/"summary.json").write_text(json.dumps({"systems":summary,"gate_eligible":False,"note":"FP32 teacher remains resident during AIR measurement; timing/resource data not comparable to standalone G01; draft shared-author workload."},indent=2))
    if transport_errors:raise RuntimeError(f"{len(transport_errors)} teacher transport failures; see teacher-responses.jsonl")

def score(cases,system):
    groups={};statuses=system.get("statuses",{});errors=system.get("errors",{})
    for c in cases:
        case_id=c["id"];actual=system["answers"].get(case_id);expected=c["answer"];group=c["group"]
        status=statuses.get(case_id)
        correct=case_id not in errors and status!="malformed" and answer_contract.normalize(actual,group)==answer_contract.normalize(expected,group)
        row=groups.setdefault(group,{"correct":0,"total":0});row["correct"]+=int(correct);row["total"]+=1
    return {"correct":sum(x["correct"] for x in groups.values()),"total":len(cases),"groups":groups,"wall_seconds":system["wall_seconds"],"errors":len(errors),"malformed_outputs":sum(x=="malformed" for x in statuses.values()),"abstentions":sum(x=="abstained" for x in statuses.values())}
