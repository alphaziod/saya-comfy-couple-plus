"""Inspect saved instance wiring. No widget-order inference or API conversion."""


def inspect_exposed_inputs(document):
    graph = document
    if isinstance(graph.get('workflow'), dict):
        graph = graph['workflow']
    if 'nodes' not in graph:
        return []

    def identifier(value):
        if isinstance(value, bool) or not isinstance(value, (str, int)):
            raise ValueError('Identifiant de graphe invalide')
        return str(value)

    def unique(items):
        table, duplicates = {}, set()
        for item in items:
            key = identifier(item['id'])
            if key in table:
                duplicates.add(key)
            table[key] = item
        return table, duplicates

    def links(scope):
        values = scope.get('links', [])
        if isinstance(values, dict):
            values = list(values.values())
        normalized = []
        for link in values:
            if isinstance(link, list) and len(link) >= 6:
                link = dict(zip(['id', 'origin_id', 'origin_slot', 'target_id', 'target_slot', 'type'], link[:6]))
            if not isinstance(link, dict):
                raise ValueError('Lien de graphe invalide')
            for key in ['origin_slot', 'target_slot']:
                if type(link.get(key)) is not int or link[key] < 0:
                    raise ValueError('Indice de port invalide')
            normalized.append(link)
        return unique(normalized)

    def slot(value, items):
        if type(value) is not int or not 0 <= value < len(items):
            raise ValueError('Port absent ou ambigu')
        return items[value]

    definitions, duplicate_definitions = {}, set()
    pending = [graph]
    count = 0
    while pending:
        scope = pending.pop()
        count += len(scope.get('nodes', []))
        if count > 50000:
            raise ValueError('Plus de 50000 nodes à inspecter')
        for definition in scope.get('definitions', {}).get('subgraphs', []):
            key = identifier(definition['id'])
            if key in definitions:
                duplicate_definitions.add(key)
            definitions[key] = definition
            pending.append(definition)

    rows = []
    visited_nodes = 0

    def walk(scope, path, ancestors, parent_active=True):
        nonlocal visited_nodes
        visited_nodes += len(scope.get('nodes', []))
        if visited_nodes > 50000:
            raise ValueError('Plus de 50000 nodes dans les instances développées')
        nodes, duplicate_nodes = unique(scope.get('nodes', []))
        edges, duplicate_edges = links(scope)
        for node in scope.get('nodes', []):
            kind = node.get('type')
            if kind not in definitions:
                continue
            definition = definitions[kind]
            node_id = identifier(node['id'])
            instance_path = path + [node_id]
            inner_nodes, duplicate_inner = unique(definition.get('nodes', []))
            inner_edges, duplicate_inner_edges = links(definition)
            recursive = kind in ancestors or len(path) >= 30
            for index, exposed in enumerate(definition.get('inputs', [])):
                row = {'instancePath': instance_path, 'definition': kind, 'input': exposed.get('name', ''), 'type': exposed.get('type'), 'source': None, 'targets': [], 'editable': False, 'status': 'unresolved', 'reason': ''}
                try:
                    if recursive:
                        raise ValueError('Définition récursive ou profondeur excessive')
                    if kind in duplicate_definitions or node_id in duplicate_nodes:
                        raise ValueError('Identifiant de définition ou instance dupliqué')
                    if not parent_active or node.get('mode', 0) != 0:
                        raise ValueError('Instance désactivée ou contournée')
                    if not isinstance(exposed.get('name'), str) or not exposed['name']:
                        raise ValueError('Nom d’entrée absent')
                    inputs = node.get('inputs', [])
                    actual = slot(index, inputs)
                    if actual.get('name') != exposed.get('name') or actual.get('type') != exposed.get('type'):
                        raise ValueError('Entrée d’instance différente de la définition')
                    if sum(i.get('name') == exposed.get('name') for i in inputs) != 1:
                        raise ValueError('Nom d’entrée ambigu')
                    destinations = exposed.get('linkIds', [])
                    if not destinations or len({identifier(i) for i in destinations}) != len(destinations):
                        raise ValueError('Cible interne absente ou dupliquée')
                    boundary = identifier(definition['inputNode']['id'])
                    actual_destinations = {key for key, edge in inner_edges.items() if identifier(edge['origin_id']) == boundary and edge['origin_slot'] == index}
                    if actual_destinations != {identifier(i) for i in destinations}:
                        raise ValueError('Cibles de frontière différentes des linkIds déclarés')
                    for link_id in destinations:
                        key = identifier(link_id)
                        edge = inner_edges.get(key)
                        if key in duplicate_inner_edges or not edge or identifier(edge['origin_id']) != boundary or edge['origin_slot'] != index or edge.get('type') != exposed.get('type'):
                            raise ValueError('Lien de frontière incohérent')
                        target_id = identifier(edge['target_id'])
                        target = inner_nodes.get(target_id)
                        if not target or target_id in duplicate_inner or target.get('mode', 0) != 0:
                            raise ValueError('Cible interne absente, ambiguë ou inactive')
                        target_input = slot(edge['target_slot'], target.get('inputs', []))
                        if not isinstance(target_input.get('name'), str) or not target_input['name']:
                            raise ValueError('Nom de port interne absent')
                        if sum(i.get('name') == target_input.get('name') for i in target.get('inputs', [])) != 1:
                            raise ValueError('Nom de port interne ambigu')
                        if identifier(target_input.get('link')) != key or target_input.get('type') != exposed.get('type'):
                            raise ValueError('Port interne différent du lien exposé')
                        row['targets'].append({'node': target_id, 'input': target_input.get('name'), 'slot': edge['target_slot']})
                    incoming = actual.get('link')
                    if incoming is None:
                        row['status'] = 'widget_unresolved' if actual.get('widget') else 'unresolved'
                        row['reason'] = 'Cibles repérées ; valeur du widget promu non prouvée' if actual.get('widget') else 'Aucune source connectée ni widget exposé'
                    else:
                        key = identifier(incoming)
                        edge = edges.get(key)
                        if key in duplicate_edges or not edge or identifier(edge['target_id']) != node_id or edge['target_slot'] != index or edge.get('type') != exposed.get('type'):
                            raise ValueError('Lien parent incohérent')
                        source_id = identifier(edge['origin_id'])
                        source = nodes.get(source_id)
                        if not source or source_id in duplicate_nodes or source.get('mode', 0) != 0:
                            raise ValueError('Source absente, ambiguë ou inactive')
                        output = slot(edge['origin_slot'], source.get('outputs', []))
                        if output.get('type') != exposed.get('type'):
                            raise ValueError('Type de sortie différent de l’entrée exposée')
                        if output.get('links') is not None and key not in {identifier(i) for i in output['links']}:
                            raise ValueError('Lien absent du port source')
                        row.update(source={'node': source_id, 'slot': edge['origin_slot']}, status='wiring_verified', reason='Câblage sauvegardé vérifié ; valeur d’exécution non évaluée')
                except (ValueError, KeyError, TypeError) as error:
                    row.update(status='unresolved', reason=str(error), targets=[], source=None)
                rows.append(row)
                if len(rows) > 50000:
                    raise ValueError('Plus de 50000 entrées exposées')
            if not recursive and kind not in duplicate_definitions:
                walk(definition, instance_path, ancestors + [kind], parent_active and node.get('mode', 0) == 0)

    walk(graph, [], [])
    return rows
