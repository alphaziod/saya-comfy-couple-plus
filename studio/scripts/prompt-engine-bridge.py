"""Local pure-function adapter; no MCP server, Settings instance or remote client."""
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1]).resolve(strict=True)
tool = sys.argv[2]
source = root / 'src' / 'saya_prompt_engine'
files = ['__init__.py', 'config.py', 'prompts.py', 'workflow.py']
fingerprint = hashlib.sha256()
for name in files:
    fingerprint.update(name.encode())
    fingerprint.update((source / name).read_bytes())
sys.dont_write_bytecode = True
sys.path.insert(0, str(root / 'src'))
from saya_prompt_engine import prompts, workflow

raw = sys.stdin.buffer.read(16_000_001)
if len(raw) > 16_000_000:
    raise ValueError('Request exceeds 16 MB')
args = json.loads(raw)
if not isinstance(args, dict):
    raise ValueError('Expected request object')
if tool == 'analyze':
    result = prompts.analyze(args['prompt'])
elif tool == 'optimize':
    loras = args.get('loras', [])
    if not isinstance(loras, list) or len(loras) > 100 or not all(isinstance(x, dict) for x in loras):
        raise ValueError('Expected at most 100 documented LoRA records')
    result = prompts.optimize(args['prompt'], args.get('checkpoint', ''), loras, args.get('target', 'P1'))
elif tool == 'validate':
    result = prompts.validate(args['original'], args['proposed'])
elif tool == 'inspect':
    if 'inspect_exposed_inputs' not in globals():
        from subgraph_inputs import inspect_exposed_inputs
    result = workflow.analyze_graph(args['graph'])
    result['exposedInputs'] = inspect_exposed_inputs(args['graph'])
    result['exposedInputsVersion'] = 1
else:
    raise ValueError('Unsupported local tool')
print(json.dumps({'result': result, 'sourceSha256': fingerprint.hexdigest(), 'adapterVersion': 1, 'transport': 'local_python_pure_functions'}, ensure_ascii=False, allow_nan=False))
