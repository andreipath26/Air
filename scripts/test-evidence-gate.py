import importlib.util,json,hashlib,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location("gate",Path(__file__).with_name("evidence-gate.py"))
gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
class GateTests(unittest.TestCase):
    def fixture(self):
        policy=json.loads(Path("research/evidence-gate-01.json").read_text())
        data=[{"id":str(i),"group":g,"question":f"{g} question {i}","answer":None if g=="unknown" else "yes","source":"synthetic test only"} for i,g in enumerate(sum(([g]*48 for g in policy["workload"]["groups"]+["unknown"]),[]))]
        raw=json.dumps(data).encode()
        ans={x["id"]:x["answer"] for x in data}
        systems={k:{"answers":dict(ans),"wall_seconds":v,"peak_ram_bytes":v*1000,"energy_joules":v*10} for k,v in [("quantized_teacher",10),("reference_retrieval",1),("air",4)]}
        ev={"dataset_sha256":hashlib.sha256(raw).hexdigest(),"hardware":policy["hardware"],"systems":systems,"conversion":{"seconds":1,"energy_joules":5,"storage_bytes":10}}
        return policy,raw,data,ev
    def test_numeric_pass_not_certification(self):
        result=gate.evaluate(*self.fixture())
        self.assertTrue(result["numeric_checks_passed"])
        self.assertEqual(result["gate_status"],"requires_independent_review")
    def test_failed_energy(self):
        p,r,d,e=self.fixture();e["systems"]["air"]["energy_joules"]=1000
        self.assertFalse(gate.evaluate(p,r,d,e)["numeric_checks_passed"])
    def test_missing_case(self):
        p,r,d,e=self.fixture();e["systems"]["air"]["answers"].pop("0")
        with self.assertRaises(ValueError):gate.evaluate(p,r,d,e)
    def test_wrong_hash(self):
        p,r,d,e=self.fixture();e["dataset_sha256"]="wrong"
        self.assertFalse(gate.evaluate(p,r,d,e)["checks"]["frozen_workload_hash"])
if __name__=="__main__":unittest.main()
