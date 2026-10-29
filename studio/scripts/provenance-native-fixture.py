"""Synthetic native UI fixture only. Never opens a user's application database."""
import hashlib, json, os, pathlib, sqlite3, struct, zlib
root = pathlib.Path('test-results/provenance-native').resolve()
data = root / 'data/studio.saya.desktop'
images = root / 'synthetic-images'
data.mkdir(parents=True, exist_ok=True)
images.mkdir(exist_ok=True)
def png(color):
 def chunk(kind, body): return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body)&0xffffffff)
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(b'\0'+bytes(color)*16 for _ in range(16))))+chunk(b'IEND',b'')
with sqlite3.connect(data/'studio.sqlite3') as db:
 version = db.execute('pragma user_version').fetchone()[0]
 for migration in sorted(pathlib.Path('migrations').glob('*.sql')):
  if int(migration.name[:4])>version: db.executescript(migration.read_text())
 db.execute('pragma user_version=9')
 db.execute("INSERT OR IGNORE INTO libraries(id,label,root_path,platform) VALUES('fixture-library','Synthetic',?,'linux')",(str(images),))
 for char,name in [('alpha','Alpha'),('beta','Beta')]:
  db.execute('INSERT OR IGNORE INTO characters(id,name,identity_json) VALUES(?,?,?)',(char,name,json.dumps({'identity_prompt':name+', exact'})))
  db.execute('INSERT OR IGNORE INTO character_variants(id,character_id,name,prompt_blocks_json) VALUES(?,?,?,?)',(char+'-variant',char,name+' variant',json.dumps({'P1':name+', exact'})))
 for job,characters in [('pair-job',['alpha','beta']),('solo-job',['alpha'])]:
  workflow=job+'-workflow'
  db.execute("INSERT OR IGNORE INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES(?,?,?,'comfy_api','{}')",(workflow,job,'synthetic'))
  people=[{'role':'P'+str(i+1),'characterId':c,'name':c.title(),'revision':1,'identity':{},'variant':{'id':c+'-variant','name':c.title()+' variant','revision':1}} for i,c in enumerate(characters)]
  snapshot=json.dumps({'synthetic':True,'workflowId':workflow,'characters':people,'prompt':{'save':{'class_type':'SaveImage','inputs':{'seed':18446744073709551615}}}})
  db.execute("INSERT OR IGNORE INTO generation_jobs(id,workflow_revision_id,state,seed,request_snapshot_json) VALUES(?,?,'succeeded','18446744073709551615',?)",(job,workflow,snapshot))
  for person in people:
   db.execute('INSERT OR IGNORE INTO job_characters(job_id,role,character_id,variant_id,snapshot_json) VALUES(?,?,?,?,?)',(job,person['role'],person['characterId'],person['variant']['id'],json.dumps(person)))
  outputs=[]
  for i in range(2 if job=='pair-job' else 1):
   asset=f'{job}-{i}';filename=asset+'.png';raw=png((90+i*60,100,150 if job=='pair-job' else 220));(images/filename).write_bytes(raw)
   db.execute("INSERT OR IGNORE INTO assets(id,library_id,relative_path,content_sha256,availability,metadata_state,width,height,favorite,review) VALUES(?,'fixture-library',?,?,'present','absent',16,16,?,?)",(asset,filename,hashlib.sha256(raw).hexdigest(),int(job=='pair-job' and i==0),'accepted' if job=='pair-job' and i==0 else 'unreviewed'))
   for char in characters: db.execute("INSERT OR IGNORE INTO asset_characters VALUES(?,?,'depicted','inferred')",(asset,char))
   db.execute("INSERT OR IGNORE INTO job_outputs VALUES(?,?,'save',?,'saved')",(job,asset,i))
   outputs.append({'assetId':asset,'nodeId':'save','outputIndex':i,'kind':'saved','source':{'filename':filename,'type':'output'}})
  db.execute('UPDATE generation_jobs SET outputs_json=? WHERE id=?',(json.dumps(outputs),job))
