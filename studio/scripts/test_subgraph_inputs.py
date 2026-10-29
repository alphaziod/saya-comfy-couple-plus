"""Synthetic structural tests; never imports ComfyUI or runs any workflow."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('subgraph_inputs', Path(__file__).with_name('subgraph_inputs.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture():
    return {'nodes': [
        {'id': 1, 'type': 'PrimitiveStringMultiline', 'mode': 0, 'outputs': [{'type': 'STRING'}], 'widgets_values': ['source exact']},
        {'id': 2, 'type': 'sub', 'mode': 0, 'inputs': [{'name': 'prompt', 'type': 'STRING', 'link': 10, 'widget': {'name': 'prompt'}}], 'widgets_values': ['ignored default']},
        {'id': 3, 'type': 'sub', 'mode': 0, 'inputs': [{'name': 'prompt', 'type': 'STRING', 'link': None, 'widget': {'name': 'prompt'}}], 'widgets_values': ['second instance']}
    ], 'links': [[10, 1, 0, 2, 0, 'STRING']], 'definitions': {'subgraphs': [{
        'id': 'sub', 'inputNode': {'id': -10}, 'inputs': [{'name': 'prompt', 'type': 'STRING', 'linkIds': [20]}],
        'nodes': [{'id': 5, 'type': 'CLIPTextEncode', 'mode': 0, 'inputs': [{'name': 'text', 'type': 'STRING', 'link': 20}]}],
        'links': [{'id': 20, 'origin_id': -10, 'origin_slot': 0, 'target_id': 5, 'target_slot': 0, 'type': 'STRING'}]
    }]}}


class ExposedInputs(unittest.TestCase):
    def test_instances_keep_separate_paths_and_linked_widget_is_not_effective(self):
        graph = fixture(); original = copy.deepcopy(graph)
        rows = module.inspect_exposed_inputs(graph)
        self.assertEqual(graph, original)
        self.assertEqual([r['instancePath'] for r in rows], [['2'], ['3']])
        self.assertEqual(rows[0]['status'], 'wiring_verified')
        self.assertEqual(rows[0]['source'], {'node': '1', 'slot': 0})
        self.assertEqual(rows[0]['targets'], [{'node': '5', 'input': 'text', 'slot': 0}])
        self.assertEqual(rows[1]['status'], 'widget_unresolved')
        self.assertTrue(all(not r['editable'] and 'effectiveValue' not in r for r in rows))

    def test_inconsistent_or_ambiguous_wiring_is_refused(self):
        mutations = [
            lambda g: g['nodes'][1]['inputs'][0].update(name='wrong'),
            lambda g: g['links'][0].__setitem__(3, 3),
            lambda g: g['links'][0].__setitem__(2, 9),
            lambda g: g['nodes'][0]['outputs'][0].update(type='MODEL'),
            lambda g: g['nodes'][1].update(mode=4),
            lambda g: g['links'].append(copy.deepcopy(g['links'][0])),
            lambda g: g['definitions']['subgraphs'].append(copy.deepcopy(g['definitions']['subgraphs'][0])),
            lambda g: g['definitions']['subgraphs'][0]['nodes'][0]['inputs'][0].update(link=999),
            lambda g: g['definitions']['subgraphs'][0]['inputs'][0].update(linkIds=[]),
        ]
        for mutate in mutations:
            graph = fixture(); mutate(graph)
            self.assertEqual(module.inspect_exposed_inputs(graph)[0]['status'], 'unresolved')

    def test_recursive_definitions_are_bounded_and_api_has_no_ui_inputs(self):
        graph = fixture(); definition = graph['definitions']['subgraphs'][0]
        definition['nodes'].append(copy.deepcopy(graph['nodes'][2]))
        rows = module.inspect_exposed_inputs(graph)
        self.assertLess(len(rows), 10)
        self.assertTrue(any('récurs' in r['reason'] for r in rows))
        self.assertEqual(module.inspect_exposed_inputs({'1': {'class_type': 'Text', 'inputs': {}}}), [])

    def test_malformed_slots_and_expansion_limits_fail_closed(self):
        graph = fixture(); graph['links'][0][2] = True
        with self.assertRaisesRegex(ValueError, 'Indice de port'):
            module.inspect_exposed_inputs(graph)
        graph = fixture(); graph['nodes'] = graph['nodes'] * 17000
        with self.assertRaisesRegex(ValueError, '50000'):
            module.inspect_exposed_inputs(graph)


if __name__ == '__main__':
    unittest.main()
