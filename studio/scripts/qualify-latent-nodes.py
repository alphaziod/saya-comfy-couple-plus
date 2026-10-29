"""CPU-only installed SaveLatent/LoadLatent contract check; no server or node imports."""
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
from types import SimpleNamespace

os.environ['CUDA_VISIBLE_DEVICES'] = ''
os.environ['HIP_VISIBLE_DEVICES'] = ''
import torch
import safetensors.torch

parser = argparse.ArgumentParser()
parser.add_argument('--comfy', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)

# Extract only these installed definitions, avoiding ComfyUI imports/device initialization.
def extract(path, names, namespace):
    source = path.read_text()
    tree = ast.parse(source)
    selected = [n for n in tree.body if isinstance(n, (ast.ClassDef, ast.FunctionDef)) and n.name in names]
    assert {n.name for n in selected} == set(names)
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(path), 'exec'), namespace)
    return hashlib.sha256(source.encode()).hexdigest()

folder = SimpleNamespace(
    get_output_directory=lambda: str(args.output),
    get_input_directory=lambda: str(args.output),
    get_save_image_path=lambda prefix, root: (root, prefix, 1, '', prefix),
    get_annotated_filepath=lambda name: str(args.output / name),
    exists_annotated_filepath=lambda name: (args.output / name).is_file(),
)
namespace = dict(torch=torch, safetensors=safetensors, os=os, json=json, hashlib=hashlib,
                 folder_paths=folder, args=SimpleNamespace(disable_metadata=False), FileLocator=dict)
utils_hash = extract(args.comfy / 'comfy/utils.py', ['save_torch_file'], namespace)
namespace['comfy'] = SimpleNamespace(utils=SimpleNamespace(save_torch_file=namespace['save_torch_file']))
nodes_hash = extract(args.comfy / 'nodes.py', ['SaveLatent', 'LoadLatent'], namespace)
save, load = namespace['SaveLatent'](), namespace['LoadLatent']()
checks = []
for dtype in (torch.float32, torch.float16, torch.bfloat16):
    tensor = torch.arange(2*4*8*8, dtype=torch.float32).reshape(2,4,8,8).to(dtype)
    sample = {'samples':tensor, 'noise_mask':torch.ones(2,8,8), 'batch_index':[3,7], 'state_info':{'sigma_next':0.2}}
    prefix = str(dtype).replace('.', '_')
    result = save.save(sample, filename_prefix=prefix, prompt={'fixture':'cpu_only'})
    name = result['ui']['latents'][0]['filename']
    restored = load.load(name)[0]
    assert result['result'][0] is sample
    assert set(restored) == {'samples'}, 'Contract changed: review newly persisted fields'
    assert restored['samples'].dtype == torch.float32
    assert restored['samples'].device.type == 'cpu'
    assert torch.equal(restored['samples'], tensor.float())
    assert restored['samples'].shape == tensor.shape
    with safetensors.safe_open(str(args.output/name), framework='pt', device='cpu') as saved:
        assert set(saved.keys()) == {'latent_tensor','latent_format_version_0'}
        assert json.loads(saved.metadata()['prompt']) == {'fixture':'cpu_only'}
    checks.append({'inputDtype':str(dtype), 'outputDtype':'torch.float32','samplesExact':True,'batchShapePreserved':True,'lostKeys':['noise_mask','batch_index','state_info']})
legacy = args.output/'legacy.latent'
safetensors.torch.save_file({'latent_tensor':torch.ones(1,4,2,2)}, str(legacy))
assert torch.equal(load.load(legacy.name)[0]['samples'], torch.ones(1,4,2,2)*(1.0/0.18215))
corrupt = args.output/'corrupt.latent'
corrupt.write_bytes(b'not safetensors')
try:
    load.load(corrupt.name)
except safetensors.SafetensorError:
    pass
else:
    raise AssertionError('Corrupt file accepted')
assert load.VALIDATE_INPUTS('missing.latent') is not True
report = {'ok':True,'device':'cpu','installedDefinitionsOnly':True,'folderPathsStubbed':True,
          'nodesSha256':nodes_hash,'utilsSha256':utils_hash,'torch':torch.__version__,
          'checks':checks,'legacyScaling':True,'corruptRejected':True,'gpuGeneration':False,
          'resumeCapability':'samples_boundary_only_not_sampler_resume'}
(args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
