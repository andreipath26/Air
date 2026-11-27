#!/usr/bin/env python3
"""Assemble observations and separately supplied, documented whole-system measurements.
Never silently substitute package energy, process RSS or missing results.
"""
import argparse,hashlib,json,pathlib,math
def read(p):
    with open(p,"rb") as f:raw=f.read(8*1024*1024+1)
    if len(raw)>8*1024*1024:raise ValueError("input limit")
    return raw,json.loads(raw)
def assemble(dataset_raw,cases,observations,measurement):
    digest=hashlib.sha256(dataset_raw).hexdigest()
    ids={c["id"] for c in cases}
    if len(ids)!=len(cases):raise ValueError("duplicate IDs")
    if measurement.get("dataset_sha256")!=digest:raise ValueError("measurement workload mismatch")
    systems={}
    for name in ("air","reference_retrieval","quantized_teacher"):
        o=observations[name];m=measurement["systems"][name]
        if o["system"]!=name or o["dataset_sha256"]!=digest or o["errors"] or set(o["answers"])!=ids:raise ValueError("incomplete or mismatched observations")
        if m.get("observations_sha256")!=o["_sha256"]:raise ValueError("measurement observation mismatch")
        if not isinstance(m.get("method"),str) or not m["method"].strip() or not isinstance(m.get("instrument"),str) or not m["instrument"].strip():raise ValueError("measurement provenance required")
        for k in ("peak_ram_bytes","energy_joules"):
            if type(m.get(k)) not in (float,int) or not math.isfinite(m[k]) or m[k]<=0:raise ValueError("invalid measurement")
        if m.get("energy_scope")!="whole_system":raise ValueError("whole-system energy required")
        if m.get("memory_scope")!="whole_system":raise ValueError("whole-system memory required")
        systems[name]={"answers":o["answers"],"wall_seconds":o["wall_seconds"],"peak_ram_bytes":m["peak_ram_bytes"],"energy_joules":m["energy_joules"],"identity":o["identity"],"measurement":m}
    if systems["air"]["identity"].get("router_sha256")!=systems["reference_retrieval"]["identity"].get("router_sha256"):raise ValueError("different AIR routers")
    if systems["air"]["identity"].get("binary_sha256")!=systems["reference_retrieval"]["identity"].get("binary_sha256"):raise ValueError("different AIR binaries")
    return {"dataset_sha256":digest,"hardware":measurement["hardware"],"systems":systems,"conversion":measurement["conversion"],"review_required":True,"note":"Measurement declarations are supplied by the operator and are not authenticated."}
def main():
    p=argparse.ArgumentParser();p.add_argument("dataset");p.add_argument("measurements");p.add_argument("output");p.add_argument("--air",required=True);p.add_argument("--reference",required=True);p.add_argument("--teacher",required=True);a=p.parse_args()
    raw,cases=read(a.dataset);_,m=read(a.measurements);obs={}
    for name,path in [("air",a.air),("reference_retrieval",a.reference),("quantized_teacher",a.teacher)]:
        r,o=read(path);o["_sha256"]=hashlib.sha256(r).hexdigest();obs[name]=o
    result=assemble(raw,cases,obs,m)
    with open(a.output,"x") as f:json.dump(result,f,indent=2)
if __name__=="__main__":main()
