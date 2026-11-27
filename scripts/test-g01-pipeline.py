import importlib.util,json,hashlib,unittest
from pathlib import Path
def load(name):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(name+".py"));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
corpus=load("g01-corpus");assemble=load("g01-assemble")
class Tests(unittest.TestCase):
    def test_duplicate_corpus(self):
        f=dict(subject="France",relation="capital",answer="Paris",group="geography",source="fixture",question="Capital?",reviewer="fixture",review_rationale="test")
        self.assertEqual(len(corpus.validate([f])),1)
        with self.assertRaises(ValueError):corpus.validate([f,f])
    def test_reject_package_energy(self):
        cases=[{"id":"a"}];raw=json.dumps(cases).encode();h=hashlib.sha256(raw).hexdigest();names=("air","reference_retrieval","quantized_teacher")
        obs={n:{"system":n,"dataset_sha256":h,"errors":{},"answers":{"a":"x"},"identity":{"router_sha256":"r","binary_sha256":"b"},"wall_seconds":1,"_sha256":"pin"} for n in names}
        m={"dataset_sha256":h,"hardware":{},"conversion":{},"systems":{n:{"observations_sha256":"pin","method":"fixture","instrument":"fixture","energy_scope":"whole_system","memory_scope":"whole_system","peak_ram_bytes":1,"energy_joules":1} for n in names}}
        self.assertTrue(assemble.assemble(raw,cases,obs,m)["review_required"])
        m["systems"]["air"]["energy_scope"]="cpu_package"
        with self.assertRaises(ValueError):assemble.assemble(raw,cases,obs,m)
if __name__=="__main__":unittest.main()
