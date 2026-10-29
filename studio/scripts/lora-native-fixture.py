"""Synthetic images and isolated preferences; real indexing is performed by Studio."""
import argparse, json, pathlib, sqlite3, struct, zlib
parser=argparse.ArgumentParser()
parser.add_argument('--root',default='test-results/lora-native-v2')
root=pathlib.Path(parser.parse_args().root).resolve()
data=root/'data/studio.saya.desktop';images=root/'images'
data.mkdir(parents=True,exist_ok=True);images.mkdir(exist_ok=True)
def chunk(kind,body):return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body)&0xffffffff)
for name,loras in [('ab',['A','B']),('abc',['A','B','C']),('a',['A']),('empty',[]),('missing',None)]:
 raw=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))
 if loras is not None:
  graph={'1':{'class_type':'CheckpointLoaderSimple','inputs':{'ckpt_name':'model'}},'2':{'class_type':'Lora Loader (LoraManager)','inputs':{'loras':[{'name':n,'active':True} for n in loras]}}}
  raw+=chunk(b'tEXt',b'prompt\0'+json.dumps(graph).encode())
 raw+=chunk(b'IDAT',zlib.compress((b'\0'+bytes([90,120,180])*16)*16))+chunk(b'IEND',b'')
 path=images/(name+'.png')
 if not path.exists():path.write_bytes(raw)
with sqlite3.connect(data/'studio.sqlite3') as db:
 version=db.execute('pragma user_version').fetchone()[0]
 # "v9" database: apply ONLY migrations 1..9. Applying all the
 # migrations then forcing user_version=9 made 0011+ replay at app
 # start (CREATE TABLE recipes → panic since 16-10); the app migrates 10+ itself.
 cible=9
 for migration in sorted(pathlib.Path('migrations').glob('*.sql')):
  if int(migration.name[:4])<=cible:db.executescript(migration.read_text())
 db.execute('pragma user_version=9')
 db.execute('INSERT OR IGNORE INTO preferences VALUES(?,?)',('libraryRoot',json.dumps(str(images))))

 db.execute("INSERT INTO preferences VALUES('selectedTab', '\"gallery\"') ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
