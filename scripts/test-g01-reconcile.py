import importlib.util,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location("r",Path(__file__).with_name("g01-reconcile.py"));r=importlib.util.module_from_spec(s);s.loader.exec_module(r)
class Tests(unittest.TestCase):
    def test_reference_not_vote(self):
        f={"subject":"France","relation":"capital","answer":"Paris","group":"geography","source":"test"}
        row=lambda value:{"subject":"France","relation":"capital","teacher_raw":value}
        self.assertEqual(r.reconcile([f],{"a":[row("Lyon")],"b":[row("Lyon") ]})[0],[])
        self.assertEqual(len(r.reconcile([f],{"a":[row("Lyon")],"b":[row("Paris")]})[0]),1)
        with self.assertRaises(ValueError):r.reconcile([f],{"a":[],"b":[row("Paris")]})
if __name__=="__main__":unittest.main()
