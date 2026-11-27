import importlib.util
import unittest
from pathlib import Path

spec=importlib.util.spec_from_file_location("g01_real",Path(__file__).with_name("g01-real-development.py"))
g01=importlib.util.module_from_spec(spec);spec.loader.exec_module(g01)

class Tests(unittest.TestCase):
    def setUp(self):
        self.cases=[
            {"id":"known","answer":"Paris","group":"geography"},
            {"id":"unknown","answer":None,"group":"unknown"},
        ]

    def test_malformed_answer_is_a_quality_result_not_transport_failure(self):
        result=g01.score(self.cases,{
            "answers":{"known":None,"unknown":None},
            "statuses":{"known":"malformed","unknown":"abstained"},
            "errors":{},
            "wall_seconds":1.0,
        })
        self.assertEqual(result["errors"],0)
        self.assertEqual(result["malformed_outputs"],1)
        self.assertEqual(result["abstentions"],1)
        self.assertEqual(result["correct"],1)

    def test_transport_error_is_scored_incorrect_and_reported_separately(self):
        result=g01.score(self.cases,{
            "answers":{"known":None,"unknown":None},
            "statuses":{"known":"transport_error","unknown":"abstained"},
            "errors":{"known":"TimeoutError"},
            "wall_seconds":1.0,
        })
        self.assertEqual(result["errors"],1)
        self.assertEqual(result["malformed_outputs"],0)
        self.assertEqual(result["correct"],1)

if __name__=="__main__":unittest.main()
