#!/usr/bin/env python3
"""Deterministic test bridge. ONLY for synthetic fixtures, never human approval."""
import hashlib,json,os,pathlib,sys,uuid

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
    req=json.load(sys.stdin); root=pathlib.Path.cwd().resolve()
    if not (root/'.softwarefactory-synthetic-fixture').is_file():
        raise RuntimeError('Fixture bridge requires .softwarefactory-synthetic-fixture; never use it on a real project')
    op=req['operation']
    if op=='capabilities':
        return dict(protocol_version=1,roles=['planner','implementer','reviewer','research','opportunities','product-manager','harness-reviewer'],high_reasoning_models=['fixture-planner'],idempotent_dispatch=True,lookup=True,cancel=True,enforces_read_only=True,enforces_scope=True,attributable_decisions=True,fixture_only=True)
    if op=='kanban_capabilities':return {'ready':True,'properties':req['config']['properties']}
    if op=='sync_card':
        directory=root/'.product-workflow'/'fixture-board';directory.mkdir(exist_ok=True)
        card=req['card'];path=directory/(hashlib.sha256(card['record_key'].encode()).hexdigest()+'.json')
        if not path.exists() and not req['may_create']:return {'status':'blocked','error':'Uncertain fixture creation'}
        path.write_text(json.dumps(req))
        once=directory/'interrupted'
        if (root/'.fixture-sync-uncertain').exists() and not once.exists():
            once.write_text('yes');raise RuntimeError('simulated lost create response')
        return {'status':'completed','record_key':card['record_key'],'sequence':req['sequence'],'reference':'https://www.notion.so/'+hashlib.md5(card['record_key'].encode()).hexdigest()}
    if op=='verify_decision':
        claim=req['claim']
        # Test-only trusted source is a fixture receipt, separate from agent results.
        receipt=root/'.product-workflow'/'fixture-decisions'/f"{claim['id']}.json"
        actual=json.loads(receipt.read_text()) if receipt.exists() else None
        return dict(verified=actual==claim,claim=actual,actor_type='human',source_evidence='synthetic test receipt; NOT a real human approval')
    if op=='submit_packet':
        return dict(status='completed',reference='fixture://'+req['idempotency_key'])
    if op=='fetch_decisions':
        directory=root/'.product-workflow'/'fixture-decisions'
        decisions=[]
        for file in directory.glob('*.json') if directory.exists() else []:
            claim=json.loads(file.read_text())
            if all(claim[k]==req[k] for k in ('project_id','run_id','artifact_revision')) and claim['action'] in req['actions']:decisions.append(claim)
        return {'decisions':decisions}

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
    elif role=='harness-reviewer':
        feature=req.get('feature')
        reference=('.product-workflow/runs/'+feature['id']+'/run.json') if feature else ('.product-workflow/discovery/'+req['discovery']['id']+'.json')
        result={'disposition':'no_change','summary':'Synthetic retrospective inspected the completed cycle; no shared harness change proposed.','evidence_refs':[reference]}
        if (root/'.fixture-harness-candidate').exists() and feature:
            result['disposition']='candidate'
            result['learning_input']=comparison_fixture(root,pathlib.Path(req['artifact_directory']).resolve(),feature)
    elif role=='product-manager' and (root/'.fixture-propose').exists():
        required=[c['id'] for c in req['profile']['checks'] if c['required']]
        cycle=(req.get('workflow_context') or {}).get('cycle',1)
        result={'proposal':{'revision':f'proposal-cycle-{cycle}','title':'Synthetic visible and saved result','scope':'Write only result.txt and inspect its saved value','criteria':[{'id':'C1','description':'Persist verified result','required':True,'check_ids':required}],'journeys':[{'id':'J1','description':'Read result after write','criterion_ids':['C1'],'check_ids':required,'required':True}],'required_checks':required}}
    else:result={'disposition':'no_action','reason':'Synthetic evidence cannot justify a real feature'}
    response=dict(status='completed',context_id=req['context_id'],effective_model=req.get('model') or 'fixture-default',effective_reasoning=req.get('reasoning','default'),result=dict(role=role,result=result))
    job.write_text(json.dumps(response));return response

def comparison_fixture(root,directory,feature):
    # Actual deterministic baseline/candidate behavior for protocol tests only.
    baseline="def validate(value): return bool(value)\n"
    candidate="def validate(value): return value == 'required'\n"
    directory.mkdir(parents=True,exist_ok=True)
    def write(name,content):
        path=directory/name;path.write_text(content);return path.relative_to(root).as_posix()
    base_ref=write('baseline.py',baseline);candidate_ref=write('candidate.py',candidate)
    diagnosis=write('diagnosis.txt','Synthetic baseline accepts an unrelated nonempty value.\n')
    versions=[('baseline',baseline),('candidate',candidate)];comparisons=[]
    for sid,split,value,expected in [('fixed','development','other',False),('held','held_out','',False)]:
        refs=[]
        for version,code in versions:
            namespace={};exec(code,namespace);observed=namespace['validate'](value)
            trace=write(version+'-'+sid+'.txt',f'SYNTHETIC trial input={value!r}; expected={expected}; observed={observed}\n')
            trial={'project_id':feature['project_id'],'run_id':feature['id'],'version_hash':hashlib.sha256(code.encode()).hexdigest(),'scenario_id':sid,'split':split,'rubric_version':'fixture-1','exercise':'workflow_harness','execution':'completed','outcome':'passed' if observed==expected else 'failed','context_ids':['synthetic-'+str(uuid.uuid4())],'trace_ref':trace,'observations':['Synthetic function trial: '+str(observed)]}
            refs.append(write(version+'-'+sid+'.json',json.dumps(trial)))
        comparisons.append({'scenario_id':sid,'split':split,'rubric_version':'fixture-1','baseline_evidence_ref':refs[0],'candidate_evidence_ref':refs[1]})
    return {'owner':'prompt','disposition':'candidate','diagnosis':'Synthetic baseline accepts wrong nonempty input','reproducible_case':'Run validator on other and empty strings','diagnostic_evidence_refs':[diagnosis],'baseline':{'version':'fixture-1','artifact_ref':base_ref},'candidate':{'version':'fixture-2','artifact_ref':candidate_ref},'comparisons':comparisons,'gains':['Synthetic negative input corrected'],'regressions':[],'gaps':['No real model evaluation performed']}

if __name__=='__main__':
    try: print(json.dumps(main()))
    except Exception as error:
        print(str(error),file=sys.stderr);sys.exit(1)
