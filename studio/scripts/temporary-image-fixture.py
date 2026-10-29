import json,struct,zlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
info=json.loads((root/'scripts/fixtures/object_info-subset.json').read_text())
info['SmokeImage']={'input':{'required':{'seed':['INT',{}],'text':['STRING',{}],'model_name':[['available.safetensors'],{}]}},'output':['IMAGE'],'output_node':False}
Path('/tmp/saya-temp-object-info.json').write_text(json.dumps(info))
# Node 3: unwired loader (exact shape of the engine's PRELOADED scenery on imported
# PNGs). Valid for object_info but never
# consumed: the submission must prune it (saya.prune_unreachable_nodes) — the mock's
# journal must receive ONLY nodes 1 and 2.
graph={'1':{'class_type':'SmokeImage','inputs':{'seed':18446744073709551615,'text':'exact,  prompt\nline','model_name':'missing.safetensors'}},'2':{'class_type':'SaveImage','inputs':{'images':['1',0],'filename_prefix':'temporary-smoke'}},'3':{'class_type':'CheckpointLoaderSimple','inputs':{'ckpt_name':'fixture-ckpt_name.safetensors'}}}
def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
b=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,2,8,2,0,0,0))+chunk(b'tEXt',b'prompt\0'+json.dumps(graph).encode())+chunk(b'IDAT',zlib.compress((b'\0'+b'\x88\xaa\xbb'*2)*2))+chunk(b'IEND',b'')
Path('/tmp/saya-temporary-smoke.png').write_bytes(b)

# UI-only PNG exercises conversion without an embedded API fallback.
workflow=json.loads((root/'scripts/fixtures/v15.2-subgraph-ui.json').read_text())
for node in workflow['nodes']: node['mode']=2
for definition in workflow['definitions']['subgraphs']:
 for node in definition['nodes']: node['mode']=4
Path('/tmp/saya-normal-workflow.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,2,8,2,0,0,0))+chunk(b'tEXt',b'workflow\0'+json.dumps(workflow).encode())+chunk(b'IDAT',zlib.compress((b'\0'+b'\x88\xaa\xbb'*2)*2))+chunk(b'IEND',b''))
