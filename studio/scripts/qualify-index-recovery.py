#!/usr/bin/env python3
"""Read-only original library; destructive interruption targets only our copied DB process."""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time
import uuid

source, library = map(lambda p: Path(p).resolve(), sys.argv[1:3])
root = Path.cwd()
reuse = len(sys.argv) == 4
run = Path(sys.argv[3]).resolve() if reuse else root / 'test-results' / ('index-recovery-' + uuid.uuid4().hex[:10])
assert run.is_relative_to(root / 'test-results')
if not reuse: run.mkdir(mode=0o700)
cli = root / 'target/debug/studio-maintenance'


def readonly(path):
    return sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=3)


def original_hashes():
    return {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [source, Path(str(source) + '-wal')] if p.exists()}


def rows(path, table):
    with readonly(path) as db:
        return sorted(db.execute('SELECT * FROM "' + table + '"').fetchall(), key=repr)


def catalog(path):
    with readonly(path) as db:
        return db.execute('SELECT relative_path,content_sha256,availability FROM assets ORDER BY relative_path').fetchall()


def launch(path, name):
    log = (run / (name + '.log')).open('w')
    return subprocess.Popen([str(cli), 'index', str(path), str(library)], stdout=log, stderr=subprocess.STDOUT), log


def finish(process, log):
    try:
        code = process.wait(timeout=900)
        assert code == 0, code
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        log.close()


before = original_hashes()
report = {'ok': False, 'directory': str(run), 'originalModified': False, 'gpu': False}
try:
    with readonly(source) as db:
        tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")]
    protected = [t for t in tables if t in ['characters', 'character_variants', 'character_revisions', 'character_history', 'character_references', 'reference_history', 'generation_jobs', 'job_characters', 'job_outputs', 'workflow_revisions', 'workflow_bindings', 'prompt_proposals', 'import_records', 'pass_attempts', 'pass_artifacts', 'human_reviews', 'job_events']]
    original = {t: rows(source, t) for t in protected}
    original_associations=set(rows(source,'asset_characters'))
    with readonly(source) as db:
        annotations=dict((r[0],r[1:]) for r in db.execute('SELECT id,favorite,note,review FROM assets'))
    assert len(original['characters']) == 43, 'Expected source catalog changed; inspect before qualification'
    copies = {}
    for name in ['full', 'resume']:
        destination = run / name
        if not reuse: subprocess.run([str(cli), 'backup', str(source), str(destination)], check=True, stdout=subprocess.DEVNULL)
        copies[name] = destination / 'studio.sqlite3'
    if not reuse:
        started = time.monotonic()
        process, log = launch(copies['full'], 'full')
        finish(process, log)
        report['fullSeconds'] = round(time.monotonic() - started, 2)
        process, log = launch(copies['resume'], 'interrupted')
        stopped = None
        try:
            deadline = time.monotonic() + 120
            while process.poll() is None and time.monotonic() < deadline:
                try:
                    with readonly(copies['resume']) as db:
                        stopped = db.execute('SELECT state,completed,total FROM index_runs ORDER BY id DESC LIMIT 1').fetchone()
                    if stopped and stopped[0] == 'indexing' and 20 <= stopped[1] < stopped[2]:
                        process.terminate()
                        break
                except sqlite3.OperationalError:
                    pass
                time.sleep(0.01)
            assert stopped and stopped[0] == 'indexing' and 20 <= stopped[1] < stopped[2], stopped
            process.wait(timeout=10)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            log.close()
        with readonly(copies['resume']) as db:
            report['interruptedAt'] = db.execute('SELECT completed,total FROM index_runs ORDER BY id DESC LIMIT 1').fetchone()
        process, log = launch(copies['resume'], 'resumed')
        finish(process, log)
    baseline = {r[0]:r[1:] for r in catalog(copies['full'])}
    resumed = {r[0]:r[1:] for r in catalog(copies['resume'])}
    assert baseline.keys() <= resumed.keys()
    assert all(resumed[k] == v for k,v in baseline.items()), 'Existing source entries differ'
    additions = sorted(resumed.keys()-baseline.keys())
    assert all((library/p).is_file() for p in additions)
    report['addedBetweenScans'] = len(additions)
    (run/'added-between-scans.json').write_text(json.dumps(additions,indent=2))
    stable = resumed
    process, log = launch(copies['resume'], 'idempotent')
    finish(process, log)
    after = {r[0]:r[1:] for r in catalog(copies['resume'])}
    assert stable.keys() <= after.keys() and all(after[k] == v for k,v in stable.items())
    report['addedDuringIncrementalCheck'] = len(after.keys()-stable.keys())
    for name, path in copies.items():
        for table, expected in original.items():
            assert rows(path, table) == expected, (name, table, 'Original records changed')
        assert original_associations <= set(rows(path,'asset_characters'))
        with readonly(path) as db:
            copied_annotations=dict((r[0],r[1:]) for r in db.execute('SELECT id,favorite,note,review FROM assets'))
            assert all(copied_annotations[k]==v for k,v in annotations.items())
            assert db.execute('pragma integrity_check').fetchone()[0] == 'ok'
            assert not db.execute('pragma foreign_key_check').fetchall()
            states = db.execute('SELECT state,completed,total,indexed,unchanged,error_count FROM index_runs ORDER BY id').fetchall()
            assert states[-1][0] == 'completed' and states[-1][1] == states[-1][2]
            if name == 'resume':
                assert states[0][0] == 'interrupted'
                assert states[-1][3] == report['addedDuringIncrementalCheck']
            report[name] = {'runs': states, 'availability': dict(db.execute('SELECT availability,count(*) FROM assets GROUP BY availability')), 'distinctPresentHashes':db.execute("SELECT count(DISTINCT content_sha256) FROM assets WHERE availability='present'").fetchone()[0], 'metadata':dict(db.execute("SELECT metadata_state,count(*) FROM assets WHERE availability='present' GROUP BY metadata_state"))}
    # Abort a writer between two related, uncommitted updates on the copied DB.
    path=copies['resume']
    with readonly(path) as db:
        before_tx=db.execute('SELECT id,completed FROM index_runs ORDER BY id DESC LIMIT 1').fetchone()
    child=subprocess.Popen([sys.executable,'-c',"import sqlite3,sys,time; d=sqlite3.connect(sys.argv[1]); d.execute('BEGIN IMMEDIATE'); d.execute('UPDATE index_runs SET completed=999999 WHERE id=?',(int(sys.argv[2]),)); d.execute(\"INSERT INTO index_errors(run_id,path,message) VALUES(?,'synthetic','uncommitted')\",(int(sys.argv[2]),)); print('transaction-open',flush=True); time.sleep(60)",str(path),str(before_tx[0])],stdout=subprocess.PIPE,text=True)
    try:
        assert child.stdout.readline().strip()=='transaction-open'
        child.kill();child.wait(timeout=10)
    finally:
        if child.poll() is None:child.kill();child.wait(timeout=10)
    with readonly(path) as db:
        assert db.execute('SELECT id,completed FROM index_runs ORDER BY id DESC LIMIT 1').fetchone()==before_tx
        assert db.execute("SELECT count(*) FROM index_errors WHERE message='uncommitted'").fetchone()[0]==0
        assert db.execute('pragma integrity_check').fetchone()[0]=='ok'
    report['transactionKillRollback']=True
    report['annotationsPreserved']=len(annotations)
    report['originalAssociationsPreserved']=len(original_associations)
    report['preserved'] = {t: len(v) for t, v in original.items()}
    report['ok'] = True
except Exception as error:
    report['failure'] = str(error)
finally:
    report['originalModified'] = original_hashes() != before
    if report['originalModified']:
        report['ok'] = False
    (root / 'test-results/index-recovery-real-report.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
sys.exit(0 if report['ok'] else 1)
