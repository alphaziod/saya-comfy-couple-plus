import json, sys, pathlib
root = pathlib.Path(sys.argv[1])
locales = {p.stem: p for p in (root / 'src/locales').glob('*.json')}
data = {code: json.loads(p.read_text()) for code, p in locales.items()}
added = conflicts = 0
missing = []
for staging in sorted((root / 'src/locales-staging').glob('*.json')):
    for key, translations in json.loads(staging.read_text()).items():
        for code in locales:
            value = translations.get(code)
            if value is None:
                # Plural suffix keys only exist for languages that need them.
                if '|' not in key: missing.append((staging.name, key, code))
                continue
            if key in data[code]:
                if data[code][key] != value: conflicts += 1
                continue
            data[code][key] = value; added += 1
for code, p in locales.items():
    p.write_text(json.dumps(dict(sorted(data[code].items())), ensure_ascii=False, indent=1))
print('added', added, 'kept-existing-on-conflict', conflicts, 'missing', len(missing), missing[:5])
