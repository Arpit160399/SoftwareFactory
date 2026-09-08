#!/usr/bin/env python3
"""Map an authorised native build/test command to check protocol v1.
Usage: native_check.py -- program argument ...
Works with xcodebuild, promptfoo eval, cargo test and other exit-code checks.
"""
import json,os,pathlib,platform,subprocess,sys,time,uuid

def main():
    req=json.load(sys.stdin)
    if req['operation'] not in ('check','lookup_check','cancel_check'):raise ValueError('Expected check or lookup_check request')
    key=req['idempotency_key']
    if not all(c.isalnum() or c=='-' for c in key):raise ValueError('Invalid retry key')
    jobs=pathlib.Path('.product-workflow/check-jobs');jobs.mkdir(parents=True,exist_ok=True)
    receipt=jobs/(key+'.json');intent=jobs/(key+'.pending')
    if receipt.exists():
        result=json.loads(receipt.read_text());result['status']='completed';return result
    if req['operation']=='cancel_check':
        if not intent.exists():return {'status':'cancelled'}
        saved=json.loads(intent.read_text());pid=saved.get('native_pid')
        if not pid:return {'status':'unknown'}
        try:os.kill(pid,0)
        except ProcessLookupError:return {'status':'cancelled'}
        return {'status':'unknown'}
    if req['operation']=='lookup_check' or intent.exists():return {'status':'unknown'}
    # Exclusive intent before side effects: an interrupted unknown job is never rerun.
    with intent.open('x') as pending:pending.write(json.dumps(req))
    args=sys.argv[1:]
    if args and args[0]=='--':args=args[1:]
    if not args:raise ValueError('Provide native command and arguments')
    run=str(uuid.UUID(req['run_id']));directory=pathlib.Path('.product-workflow/runs')/run/'evidence'
    directory.mkdir(parents=True,exist_ok=True);log=directory/(str(uuid.uuid4())+'.log')
    started=time.monotonic()
    with log.open('wb') as stream:
        result=subprocess.Popen(args,stdin=subprocess.DEVNULL,stdout=stream,stderr=stream)
        intent.write_text(json.dumps(dict(req,native_pid=result.pid)))
        result.wait()
    # Redact referenced credentials from locally retained logs.
    data=log.read_bytes()
    for name,value in os.environ.items():
        if any(word in name.upper() for word in ('TOKEN','SECRET','PASSWORD','API_KEY')) and len(value)>5:
            data=data.replace(value.encode(),b'[REDACTED]')
    log.write_bytes(data)
    response={'check_id':req['check_id'],'revision':req['revision'],'outcome':'passed' if result.returncode==0 else 'failed','exit_code':result.returncode,'log_ref':str(log.resolve()),'elapsed_seconds':round(time.monotonic()-started,3),'environment':platform.platform(),'category':req['category'],'fixture_version':os.environ.get('SOFTWAREFACTORY_FIXTURE_VERSION'),'device':os.environ.get('SOFTWAREFACTORY_DEVICE'),'note':'Exit status verifies this configured command only; usability and product usefulness require their own evidence.'}
    temporary=receipt.with_suffix('.tmp');temporary.write_text(json.dumps(response));temporary.replace(receipt)
    return response
if __name__=='__main__':
    try:print(json.dumps(main()))
    except Exception as error:print(type(error).__name__+': native check unavailable',file=sys.stderr);sys.exit(1)
