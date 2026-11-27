#!/usr/bin/env python3
"""Freeze independently reviewed workload inputs; never invent a review or approve fixtures."""
import argparse,hashlib,json,pathlib
def read(path):
    raw=pathlib.Path(path).read_bytes()
    if len(raw)>8*1024*1024:raise ValueError("input bound")
    return raw,json.loads(raw)
def main():
    p=argparse.ArgumentParser();p.add_argument("corpus");p.add_argument("dataset");p.add_argument("router");p.add_argument("review");p.add_argument("output");a=p.parse_args()
    inputs={}
    for k in ("corpus","dataset","router"):
        raw,data=read(getattr(a,k));inputs[k]={"sha256":hashlib.sha256(raw).hexdigest(),"data":data}
    _,review=read(a.review)
    if review.get("input_hashes")!={k:v["sha256"] for k,v in inputs.items()}:raise ValueError("review not bound to exact inputs")
    for k in ("reviewer","date","method"):
        if not isinstance(review.get(k),str) or not review[k].strip():raise ValueError("review metadata missing")
    for k in ("independent_authorship","source_verification","license_review","no_test_tuning","domain_diversity"):
        if review.get(k) is not True:raise ValueError("review incomplete: "+k)
    corpus=inputs["corpus"]["data"];dataset=inputs["dataset"]["data"]
    train={" ".join(x["question"].casefold().split()) for x in corpus}
    test={" ".join(x["question"].casefold().split()) for x in dataset}
    if train&test or len(test)!=len(dataset):raise ValueError("question overlap")
    groups={}
    for row in dataset:
        groups[row["group"]]=groups.get(row["group"],0)+1
        if any(x in row["source"].casefold() for x in ("fixture","synthetic","not independently audited")):raise ValueError("development-only source")
    for row in corpus:
        if any(x in row["reviewer"].casefold() for x in ("fixture","development")):raise ValueError("development-only review")
    if len(dataset)<240 or any(groups.get(g,0)<40 for g in ("geography","science","engineering","general_reference","unknown")):raise ValueError("insufficient workload")
    result={"input_hashes":review["input_hashes"],"review":review,"counts":groups,"status":"review_assertions_recorded","warning":"Independent human review is not authenticated by this program."}
    with open(a.output,"x") as f:json.dump(result,f,indent=2)
if __name__=="__main__":main()
