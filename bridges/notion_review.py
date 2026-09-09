#!/usr/bin/env python3
"""Notion review bridge v1. Explicit sync writes pages; verification only reads.
Environment: NOTION_TOKEN, NOTION_PAGE_ID. No token is returned or persisted.
"""
import hashlib,json,os,sys,urllib.request,urllib.parse,uuid
API='https://api.notion.com/v1'
ACTIONS={'build','accept','merge','release','harness_adopt','harness_defer','request_changes'}
def uid(value): return str(uuid.UUID(value))
def api(path,method='GET',data=None):
    req=urllib.request.Request(API+path,data=None if data is None else json.dumps(data).encode(),method=method,headers={'Authorization':'Bearer '+os.environ['NOTION_TOKEN'],'Notion-Version':'2025-09-03','Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=25) as response: return json.load(response)
def pages(path):
    cursor=None
    while True:
        sep='&' if '?' in path else '?'
        response=api(path+sep+urllib.parse.urlencode(dict(page_size=100,**({'start_cursor':cursor} if cursor else {}))))
        yield from response['results']
        if not response.get('has_more'):break
        cursor=response['next_cursor']
def text(items):return ''.join(i.get('plain_text',i.get('text',{}).get('content','')) for i in items)
def paragraph(content):return {'object':'block','type':'paragraph','paragraph':{'rich_text':[{'type':'text','text':{'content':content}}]}}
def verify(claim,parent):
    if claim['action'] not in ACTIONS:raise ValueError('Unsupported decision action')
    # Source format is notion://<packet-page-uuid>/<comment-uuid>.
    parsed=urllib.parse.urlparse(claim['source']);page=uid(parsed.netloc);comment_id=uid(parsed.path.strip('/'))
    if parsed.scheme!='notion':raise ValueError('Not a Notion decision source')
    page_data=api('/pages/'+page)
    if uid(page_data.get('parent',{}).get('page_id',''))!=parent:raise ValueError('Decision page is outside configured review root')
    comments=list(pages('/comments?block_id='+page))
    matches=[c for c in comments if uid(c['id'])==comment_id]
    if len(matches)!=1:raise ValueError('Decision comment is unavailable, resolved, or ambiguous')
    comment=matches[0];author=api('/users/'+uid(comment['created_by']['id']))
    if author.get('type')!='person':raise ValueError('Approval author is not a human user')
    payload=json.loads(text(comment['rich_text']))
    # Human writes only immutable scope fields; server supplies identity and time.
    expected={k:claim[k] for k in ('project_id','run_id','artifact_revision','action')}
    if payload!=expected:raise ValueError('Human comment does not match exact requested action and revision')
    if claim['actor']!=comment['created_by']['id'] or claim['decided_at']!=comment['created_time'] or claim['id']!=comment['id']:raise ValueError('Claim identity or timestamp differs from Notion metadata')
    blocks=list(pages('/blocks/'+page+'/children'))
    marker='SoftwareFactory project='+claim['project_id']+' run='+claim['run_id']
    if not any(text(b.get('paragraph',{}).get('rich_text',[]))==marker for b in blocks):raise ValueError('Decision packet belongs to a different project or run')
    return {'verified':True,'claim':claim,'actor_type':'human','source_evidence':claim['source']}
BOARD_STATUSES={'Discovery','Needs approval','In progress','Ready for review','Blocked','Done','Deferred','Cancelled'}
BOARD_TYPES={'name':'title','status':'status','record_key':'rich_text','project_id':'rich_text','workflow_id':'rich_text','cycle':'number','kind':'rich_text','stage':'rich_text','agent':'rich_text','iteration':'number','summary':'rich_text','blocker':'rich_text','next_action':'rich_text','revision':'rich_text','event_time':'date','synced_at':'date','sequence':'number','review_url':'url'}
def board_schema(config):
    database=uid(config['database_id']);source=uid(config['data_source_id'])
    schema=api('/data_sources/'+source)
    if uid(schema.get('parent',{}).get('database_id',''))!=database:raise ValueError('Data source belongs to another database')
    if schema.get('archived') or schema.get('in_trash'):raise ValueError('Data source is archived')
    properties=schema['properties'];mapping={}
    for field,kind in BOARD_TYPES.items():
        chosen=config['properties'][field]
        found=[p for name,p in properties.items() if name==chosen or p['id']==chosen]
        if len(found)!=1 or found[0]['type']!=kind:raise ValueError('Missing or incompatible board property: '+field)
        mapping[field]=found[0]['id']
        if field=='status' and not BOARD_STATUSES.issubset({o['name'] for o in found[0]['status']['options']}):raise ValueError('Board Status options are incomplete')
    if len(set(mapping.values()))!=len(mapping):raise ValueError('Board properties must be distinct')
    return mapping

def query_cards(source,key_property,key):
    cursor=None;results=[]
    while True:
        body={'page_size':100,'filter':{'property':key_property,'rich_text':{'equals':key}}}
        if cursor:body['start_cursor']=cursor
        response=api('/data_sources/'+uid(source)+'/query','POST',body);results.extend(response['results'])
        if not response.get('has_more'):return results
        cursor=response['next_cursor']

def board_properties(card,sequence,mapping):
    import datetime
    props={}
    for field,kind in BOARD_TYPES.items():
        value=sequence if field=='sequence' else card.get(field,'')
        if field=='synced_at':value=datetime.datetime.now(datetime.timezone.utc).isoformat()
        if field=='event_time':value=datetime.datetime.fromtimestamp(value,datetime.timezone.utc).isoformat()
        if kind in ('title','rich_text'):out={kind:[{'text':{'content':str(value)[:1800]}}]} if value else {kind:[]}
        elif kind=='status':
            if value not in BOARD_STATUSES:raise ValueError('Unknown board state')
            out={'status':{'name':value}}
        elif kind=='number':out={'number':value}
        elif kind=='date':out={'date':{'start':value}}
        else:
            if value and not value.startswith(('https://www.notion.so/','https://notion.so/')):raise ValueError('Untrusted review URL')
            out={'url':value or None}
        props[mapping[field]]=out
    return props

def property_value(prop):
    kind=prop.get('type')
    if kind in ('title','rich_text'):return text(prop[kind])
    if kind=='status':return (prop.get('status') or {}).get('name')
    if kind=='date':return (prop.get('date') or {}).get('start')
    return prop.get(kind)

def sync_card(req):
    attempted=False;write_completed=False
    try:
        config=req['config']
        if not config.get('enabled'):raise ValueError('Board synchronization is disabled')
        mapping=board_schema(config);card=req['card'];sequence=req['sequence'];key=card['record_key']
        if not isinstance(sequence,int) or sequence<1:raise ValueError('Invalid event sequence')
        found=query_cards(config['data_source_id'],mapping['record_key'],key)
        if len(found)>1:raise ValueError('Duplicate cards require reconciliation')
        if not found and req.get('reference'):
            raise ValueError('Previously synchronized card is missing or archived; restore it before retrying')
        if not found and not req.get('may_create'):
            return {'status':'blocked','error':'Card creation is uncertain; restore or locate the original card before retrying','record_key':key}
        properties=board_properties(card,sequence,mapping)
        if found:
            page=found[0]
            if page.get('archived') or page.get('in_trash'):raise ValueError('Card was archived; restore it before retrying')
            current={p['id']:p for p in page['properties'].values()}
            if property_value(current[mapping['project_id']])!=card['project_id'] or property_value(current[mapping['workflow_id']])!=card['workflow_id']:raise ValueError('Card scope does not match')
            old=property_value(current[mapping['sequence']]) or 0
            if old>sequence:raise ValueError('Remote card is newer; reconcile event order')
            attempted=True
            page=api('/pages/'+uid(page['id']),'PATCH',{'properties':properties})
        else:
            attempted=True
            page=api('/pages','POST',{'parent':{'type':'data_source_id','data_source_id':uid(config['data_source_id'])},'properties':properties})
        write_completed=True
        confirmed=api('/pages/'+uid(page['id']))
        actual={p['id']:p for p in confirmed['properties'].values()}
        for field,prop_id in mapping.items():
            if field in ('synced_at','event_time'):continue
            expected=dict(properties[prop_id],type=BOARD_TYPES[field])
            if property_value(actual[prop_id])!=property_value(expected):raise ValueError('Board read-back does not match the saved event')
        return {'status':'completed','record_key':key,'sequence':sequence,'reference':'https://www.notion.so/'+uid(page['id']).replace('-','')}
    except Exception as error:
        retry=60
        if hasattr(error,'headers'):
            try:retry=max(1,int(error.headers.get('Retry-After','60')))
            except (ValueError,TypeError):pass
        if hasattr(error,'close'):error.close()
        return {'status':'blocked','error':('Notion returned HTTP '+str(error.code)) if hasattr(error,'code') else (str(error) if isinstance(error,ValueError) else 'Notion connection or response unavailable'), 'no_create_attempt':not attempted or (not write_completed and getattr(error,'code',None) in (400,401,403,429)), 'rate_limited':getattr(error,'code',None)==429,'retry_after':retry}

def main(req):
    op=req['operation']
    if op=='verify_reviewers':
        people=[api('/users/'+uid(id)) for id in req['reviewers']]
        return {'verified':bool(people) and all(person.get('type')=='person' for person in people)}
    if op=='kanban_capabilities':return {'ready':True,'properties':board_schema(req['config'])}
    if op=='sync_card':return sync_card(req)
    parent=uid(os.environ['NOTION_PAGE_ID'])
    if op=='capabilities':
        api('/pages/'+parent)
        # Prove comment retrieval permission as well as page access.
        list(pages('/comments?block_id='+parent))
        return {'protocol_version':1,'attributable_decisions':True,'kind':'notion'}
    if op=='fetch_decisions':
        decisions=[]
        marker='SoftwareFactory project='+req['project_id']+' run='+req['run_id']
        for child in pages('/blocks/'+parent+'/children'):
            if not child.get('child_page',{}).get('title','').startswith('SoftwareFactory '):continue
            page=uid(child['id']);blocks=list(pages('/blocks/'+page+'/children'))
            if not any(text(b.get('paragraph',{}).get('rich_text',[]))==marker for b in blocks):continue
            for comment in pages('/comments?block_id='+page):
                try:body=json.loads(text(comment['rich_text']))
                except (ValueError,KeyError):continue
                if not isinstance(body,dict):continue
                if body.get('project_id')!=req['project_id'] or body.get('run_id')!=req['run_id'] or body.get('artifact_revision')!=req['artifact_revision'] or body.get('action') not in req['actions'] or body.get('action') not in ACTIONS:continue
                claim=dict(id=comment['id'],project_id=body['project_id'],run_id=body['run_id'],actor=comment['created_by']['id'],decided_at=comment['created_time'],artifact_revision=body['artifact_revision'],action=body['action'],source='notion://'+page+'/'+uid(comment['id']))
                # Polling does not confer trust; verifier repeats authoritative checks.
                decisions.append(claim)
        decisions.sort(key=lambda c:(c['decided_at'],c['id']),reverse=True)
        return {'decisions':decisions}
    if op=='verify_decision':return verify(req['claim'],parent)
    if op!='submit_packet':raise ValueError('Unsupported review operation')
    packet=req['packet'];key=hashlib.sha256(req['idempotency_key'].encode()).hexdigest()[:24]
    title='SoftwareFactory '+key
    existing=[b for b in pages('/blocks/'+parent+'/children') if b.get('child_page',{}).get('title')==title]
    if len(existing)>1:raise ValueError('Duplicate review packets require reconciliation')
    if existing:page_id=existing[0]['id']
    else:
        # Send review material only; exclude runtime credentials, absolute context and raw logs.
        review={k:packet[k] for k in ('project_id','id','proposal','stage','iteration','iterations','decisions')}
        if 'harness_candidate' in packet:review['harness_candidate']=packet['harness_candidate']
        content=json.dumps(review,indent=2,ensure_ascii=False)
        chunks=[content[i:i+1800] for i in range(0,len(content),1800)]
        if len(chunks)>95:raise ValueError('Review packet exceeds inline limit; configure artifact-link adapter')
        blocks=[paragraph('SoftwareFactory project='+packet['project_id']+' run='+packet['id']),paragraph('Immutable packet '+key+'; human comments grant exact revision-specific actions. Acceptance never grants merge or release.')]+[paragraph(chunk) for chunk in chunks]
        page_id=api('/pages','POST',{'parent':{'page_id':parent},'properties':{'title':{'type':'title','title':[{'type':'text','text':{'content':title}}]}},'children':blocks})['id']
    return {'status':'completed','reference':'https://www.notion.so/'+page_id.replace('-',''),'page_id':page_id,'idempotency_key':req['idempotency_key']}
if __name__=='__main__':
    try: print(json.dumps(main(json.load(sys.stdin))))
    except Exception as exc:
        # Do not dump HTTP payloads, request headers or token-bearing objects.
        print('Notion bridge failed: '+type(exc).__name__+'. Check connection, permissions and exact decision source.',file=sys.stderr);sys.exit(1)
