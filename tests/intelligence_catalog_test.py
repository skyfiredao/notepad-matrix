import importlib.util
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
SOURCE = pathlib.Path('/home/dzf8tt/workspace/notepad-plus-plus/PowerEditor/installer')


class CatalogParity(unittest.TestCase):
    def test_every_source_field_round_trips(self):
        spec = importlib.util.spec_from_file_location('intelligence_catalog', ROOT / 'scripts/intelligence_catalog.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(module.generate(SOURCE), module.load_committed(ROOT / 'assets/intelligence'))
        catalog = module.generate(SOURCE)
        self.assertEqual(len(catalog['apis']), 34)
        self.assertEqual(len(catalog['parsers']), 47)
        python = catalog['apis']['python']['children'][0]
        self.assertEqual(python['tag'], 'Environment')
        self.assertEqual(python['attributes']['additionalWordChar'], '.')
        parser = catalog['parsers']['python']['definition']
        self.assertEqual(parser['children'][0]['attributes']['mainExpr'], r'^class\x20\K.*?(?=\n\S|\Z)')
        cpp = catalog['parsers']['cpp']['definition']['children'][0]['attributes']['mainExpr']
        self.assertIn('\n', cpp)


if __name__ == '__main__':
    unittest.main()
