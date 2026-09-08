#!/usr/bin/env python3
"""Deterministic test bridge. ONLY for synthetic fixtures, never human approval."""
import hashlib,json,os,pathlib,sys

def snapshot(root):
    files={}
    for directory,dirs,names in os.walk(root,followlinks=False):
        dirs[:]=sorted(d for d in dirs if d not in {'.git','.product-workflow','target','node_modules'})
        for name in sorted(names):
            p=pathlib.Path(directory)/name
            files[p.relative_to(root).as_posix()]=('symlink:'+os.readlink(p)) if p.is_symlink() else hashlib.sha256(p.read_bytes()).hexdigest()
    files=dict(sorted(files.items()))
    return hashlib.sha256(json.dumps(files,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()

def main():
    req=json.load(sys.stdin); root=pathlib.Path.cwd()
    if not (root/'.softwarefactory-synthetic-fixture').is_file():
        raise RuntimeError('Fixture bridge requires .softwarefactory-synthetic-fixture; never use it on a real project')
    op=req['operation']
    if op=='capabilities':
        return dict(protocol_version=1,roles=['planner','implementer','reviewer','research','opportunities','product-manager','harness-reviewer'],high_reasoning_models=['fixture-planner'],idempotent_dispatch=True,lookup=True,cancel=True,enforces_read_only=True,enforces_scope=True,attributable_decisions=True,fixture_only=True)
    if op=='verify_decision':
        claim=req['claim']
        # Test-only trusted source is a fixture receipt, separate from agent results.
        receipt=root/'.product-workflow'/'fixture-decisions'/f"{claim['id']}.json"
        actual=json.loads(receipt.read_text()) if receipt.exists() else None
        return dict(verified=actual==claim,claim=actual,actor_type='human',source_evidence='synthetic test receipt; NOT a real human approval')
    if op=='submit_packet':
        return dict(status='completed',reference='fixture://'+req['idempotency_key'])
    if op in ('check','lookup_check'):
        cache=root/'.product-workflow'/'fixture-checks';cache.mkdir(exist_ok=True)
        receipt=cache/(req['idempotency_key']+'.json')
        if receipt.exists():return json.loads(receipt.read_text())
        if op=='lookup_check':return dict(status='unknown')
        passed=(root/'result.txt').read_text()=='verified\n'
        response=dict(check_id=req['check_id'],revision=req['revision'],outcome='passed' if passed else 'failed',environment='synthetic',fixture_version='1',visible_outcome='result file',persisted_outcome='verified' if passed else 'incorrect')
        receipt.write_text(json.dumps(response));return response
    jobs=root/'.product-workflow'/'fixture-jobs';jobs.mkdir(parents=True,exist_ok=True)
    key=req['idempotency_key']; job=jobs/f'{key}.json'
    if op=='lookup':
        return json.loads(job.read_text()) if job.exists() else dict(status='unknown')
    if op=='cancel': return dict(status='cancelled')
    if job.exists():return json.loads(job.read_text())
    role=req['role']; proposal=req.get('proposal',{'criteria':[]}); criteria=[c['id'] for c in proposal['criteria']]
    if role=='planner':
        result=dict(summary='Write observable fixture result',tasks=[dict(id='T1',criterion_ids=criteria,files=['result.txt'],description='Create verified result')],required_checks=proposal['required_checks'],guidance_impact='No durable guidance change',risks=[])
    elif role=='implementer':
        iteration=req['input']['iteration']['number']
        fail=(root/'.fixture-fail-first').exists() and iteration==1
        (root/'result.txt').write_text('incorrect\n' if fail else 'verified\n')
        artifact=pathlib.Path(req['artifact_directory']);diff=artifact/f'diff-{iteration}.txt';guidance=artifact/f'guidance-{iteration}.txt'
        diff.write_text('result.txt: '+('incorrect' if fail else 'verified'));guidance.write_text('No AGENTS.md changes')
        result=dict(summary='Created result file',candidate_revision=snapshot(root),diff_ref=str(diff),guidance_diff_ref=str(guidance),known_gaps=[])
    elif role=='reviewer':
        iteration=req['input']['iteration']; revision=iteration['implementation']['candidate_revision']
        checks=iteration['checks']; passed=all(c['outcome']=='passed' for c in checks)
        refs=[ref for c in checks for ref in c['evidence_refs']]
        result=dict(summary='Inspected actual result and recorded checks',candidate_revision=revision,guidance_consistent=True,verdicts=[dict(criterion_id=c,passed=passed,evidence_refs=refs) for c in criteria],findings=[] if passed else [dict(criterion_id=criteria[0],expected='verified',observed='incorrect fixture output',severity='major',cause='implementation',evidence_ref=refs[0])])
    elif role=='research':result={'findings':[],'limitations':['Synthetic fixture has no external research']}
    elif role=='opportunities':result={'opportunities':[]}
    else:result={'disposition':'no_action','reason':'Synthetic evidence cannot justify a real feature'}
    response=dict(status='completed',context_id=req['context_id'],effective_model=req.get('model') or 'fixture-default',effective_reasoning=req.get('reasoning','default'),result=dict(role=role,result=result))
    job.write_text(json.dumps(response));return response
if __name__=='__main__':
    try: print(json.dumps(main()))
    except Exception as error:
        print(str(error),file=sys.stderr);sys.exit(1)
