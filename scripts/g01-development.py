#!/usr/bin/env python3
"""Generate synthetic development fixtures only. No independent knowledge claim."""
import argparse,json
from pathlib import Path
def main():
    p=argparse.ArgumentParser();p.add_argument("output");a=p.parse_args();out=Path(a.output);out.mkdir(parents=True,exist_ok=False)
    groups=["geography","science","engineering","general_reference"]
    corpus=[];cases=[];entities=[]
    for g in groups:
        for i in range(50):
            entity=f"Fixture {g} {i:03d}";answer=f"Value {g} {i:03d}"
            entities.append({"canonical":entity,"aliases":[f"Alias {g} {i:03d}"]})
            corpus.append(dict(subject=entity,relation="fixture code",answer=answer,group=g,source="synthetic development fixture",question=f"What is the fixture code of {entity}?",reviewer="fixture generator",review_rationale="synthetic mapping, not independent review"))
            cases.append(dict(id=f"{g}-{i}",group=g,question=f"Which code belongs to Alias {g} {i:03d}?",answer=answer,source="synthetic development fixture"))
    for i in range(40):
        cases.append(dict(id=f"unknown-{i}",group="unknown",question=f"Which code belongs to Unregistered fixture {i}?",answer=None,source="synthetic unsupported entity"))
    router={"entities":entities,"relations":[{"canonical":"fixture code","patterns":["What is the fixture code of {entity}?","Which code belongs to {entity}?"]}]}
    for name,value in [("corpus",corpus),("dataset",cases),("router",router)]:
        (out/(name+".json")).write_text(json.dumps(value,indent=2))
    (out/"NOTICE.txt").write_text("Synthetic development only. Not eligible for G01 acceptance. No teacher knowledge or independent review established.\n")
if __name__=="__main__":main()
