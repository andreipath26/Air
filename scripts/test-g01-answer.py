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
    def test_contextual(self):
        self.assertEqual(a.extract("Thailand's capital city is Bangkok.","Thailand","capital"),("Bangkok","extracted"))
        self.assertEqual(a.extract("Boron has the chemical symbol B.","Boron","chemical symbol"),("B","extracted"))
        self.assertEqual(a.extract("Oxygen (O)","Oxygen","chemical symbol"),("O","extracted"))
        self.assertNotEqual(a.extract("Boron does not have the chemical symbol B.","Boron","chemical symbol")[0],"B")
    def test_verbose_symbol_answers(self):
        self.assertEqual(a.extract("Watts (W)","watt","SI unit symbol"),("W","extracted"))
        self.assertEqual(a.extract("Amps (A)","ampere","SI unit symbol"),("A","extracted"))
        self.assertEqual(a.extract("Coulomb is represented by the symbol C.","coulomb","SI unit symbol"),("C","extracted"))
        self.assertEqual(a.extract("T Tesla","tesla","SI unit symbol"),("T","extracted"))
    def test_extraction(self):
        self.assertEqual(a.extract("Answer: Paris."),("Paris","extracted"))
        self.assertEqual(a.extract('{"answer":null}'),(None,"abstained"))
        self.assertEqual(a.extract('{"answer":"Paris","extra":1}'),(None,"malformed"))
        self.assertEqual(a.extract(""),(None,"malformed"))
if __name__=="__main__":unittest.main()
