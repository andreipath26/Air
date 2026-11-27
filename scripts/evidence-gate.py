#!/usr/bin/env python3
"""Evaluate proposed G01 evidence; never certify provenance or hardware automatically."""
import argparse
import hashlib
import json
import math
from pathlib import Path

def read(path, cap=8*1024*1024):
    with Path(path).open("rb") as f:
        raw=f.read(cap+1)
    if len(raw)>cap:
        raise ValueError("input too large")
    return raw,json.loads(raw)

def positive(v):
    return type(v) in (int,float) and math.isfinite(v) and v>0

def evaluate(policy, dataset_raw, dataset, evidence):
    checks={}
    def check(k,v):
        checks[k]=bool(v)
    w=policy["workload"];t=policy["thresholds"]
    if not isinstance(dataset,list) or not 1<=len(dataset)<=10000:
        raise ValueError("invalid workload size")
    ids={};questions=set();groups={}
    for case in dataset:
        key=case["id"];group=case["group"];q=case["question"]
        if not all(isinstance(x,str) and x.strip() and len(x)<=4096 for x in [key,group,q,case["source"]]):
            raise ValueError("invalid case fields")
        normalized=" ".join(q.casefold().split())
        if key in ids or normalized in questions:
            raise ValueError("duplicate case or question")
        expected=case["answer"]
        if group==w["unknown_group"]:
            if expected is not None: raise ValueError("unknown case must abstain")
        elif group not in w["groups"] or not isinstance(expected,str) or not expected.strip():
            raise ValueError("invalid domain or reference")
        ids[key]=case;questions.add(normalized);groups[group]=groups.get(group,0)+1
    check("frozen_workload_hash",evidence.get("dataset_sha256")==hashlib.sha256(dataset_raw).hexdigest())
    check("workload_size",len(ids)>=w["minimum_cases"])
    check("domain_coverage",all(groups.get(g,0)>=w["minimum_per_group"] for g in w["groups"]))
    check("unknown_coverage",groups.get(w["unknown_group"],0)>=w["minimum_unknown_cases"])
    hw=evidence.get("hardware",{})
    check("declared_target_hardware",hw.get("model")==policy["hardware"]["model"] and hw.get("cpu")==policy["hardware"]["cpu"] and hw.get("ram_gib")==16 and hw.get("gpu") is False)
    metrics={}
    systems=evidence["systems"]
    for name in ("quantized_teacher","reference_retrieval","air"):
        s=systems[name];answers=s["answers"]
        if not isinstance(answers,dict) or set(answers)!=set(ids):
            raise ValueError("all systems must answer identical case IDs")
        if any(x is not None and (not isinstance(x,str) or len(x)>4096) for x in answers.values()):
            raise ValueError("invalid answer type")
        def norm(x):
            return None if x is None else " ".join(x.casefold().split())
        scores={k:norm(answers[k])==norm(c["answer"]) for k,c in ids.items()}
        correct=sum(scores.values())
        if not all(positive(s.get(k)) for k in ("wall_seconds","peak_ram_bytes","energy_joules")):
            raise ValueError("missing or invalid resource measurement")
        metrics[name]={"correct":correct,"total":len(ids),"accuracy":correct/len(ids),"correct_per_second":correct/s["wall_seconds"],"joules_per_correct":s["energy_joules"]/correct if correct else None,"peak_ram_bytes":s["peak_ram_bytes"],"group_accuracy":{g:sum(scores[k] for k,c in ids.items() if c["group"]==g)/n for g,n in groups.items()}}
    a=metrics["air"];b=metrics["quantized_teacher"]
    check("absolute_accuracy",a["accuracy"]*100>=t["absolute_accuracy_percent"])
    check("relative_accuracy",b["correct"]>0 and a["correct"]*100>=b["correct"]*t["relative_teacher_accuracy_percent"])
    check("domain_accuracy",all(a["group_accuracy"].get(g,0)*100>=t["min_group_accuracy_percent"] for g in w["groups"]))
    check("unknown_abstention",a["group_accuracy"].get(w["unknown_group"],0)*100>=t["unknown_abstention_percent"])
    check("throughput",b["correct_per_second"]>0 and a["correct_per_second"]>=b["correct_per_second"]*t["throughput_ratio"])
    check("ram",a["peak_ram_bytes"]*100<=b["peak_ram_bytes"]*(100-t["peak_ram_reduction_percent"]))
    check("energy",a["joules_per_correct"] is not None and b["joules_per_correct"] is not None and a["joules_per_correct"]*100<=b["joules_per_correct"]*(100-t["energy_per_correct_reduction_percent"]))
    cost=evidence.get("conversion",{})
    check("conversion_accounted",all(type(cost.get(k)) in (int,float) and math.isfinite(cost[k]) and cost[k]>=0 for k in ("seconds","energy_joules","storage_bytes")))
    saving=None if a["joules_per_correct"] is None or b["joules_per_correct"] is None else b["joules_per_correct"]-a["joules_per_correct"]
    break_even=cost["energy_joules"]/saving if checks["conversion_accounted"] and saving is not None and saving>0 else None
    return {"numeric_checks_passed":all(checks.values()),"gate_status":"requires_independent_review" if all(checks.values()) else "not_passed","checks":checks,"metrics":metrics,"energy_break_even_correct_tasks":break_even,"warning":"Submitted evidence is not authenticated. Review dataset independence, hardware, measurement boundaries, baseline configuration, conversion and raw traces before gate approval."}

def main():
    p=argparse.ArgumentParser()
    p.add_argument("dataset");p.add_argument("evidence");p.add_argument("--policy",default="research/evidence-gate-01.json")
    args=p.parse_args()
    _,policy=read(args.policy);raw,dataset=read(args.dataset);_,evidence=read(args.evidence)
    print(json.dumps(evaluate(policy,raw,dataset,evidence),indent=2,allow_nan=False))
if __name__=="__main__": main()
