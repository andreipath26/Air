import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location("answer",Path(__file__).with_name("g01-answer.py"))
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
class Tests(unittest.TestCase):
    def test_symbols(self):
        self.assertFalse(a.matches("CO","Co","science"))
        self.assertFalse(a.matches("mW","MW","engineering"))
        self.assertTrue(a.matches("μF","µF","engineering"))
        self.assertTrue(a.matches("PARIS","Paris","geography"))
    def test_extraction(self):
        self.assertEqual(a.extract("Answer: Paris."),("Paris","extracted"))
        self.assertEqual(a.extract('{"answer":null}'),(None,"abstained"))
        self.assertEqual(a.extract('{"answer":"Paris","extra":1}'),(None,"malformed"))
        self.assertEqual(a.extract(""),(None,"malformed"))
if __name__=="__main__":unittest.main()
