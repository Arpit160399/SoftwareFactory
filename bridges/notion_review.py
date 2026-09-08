#!/usr/bin/env python3
"""Notion review bridge v1. Explicit sync writes pages; verification only reads.
Environment: NOTION_TOKEN, NOTION_PAGE_ID. No token is returned or persisted.
"""
import hashlib,json,os,sys,urllib.request,urllib.parse,uuid
API='https://api.notion.com/v1'
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
def main(req):
    parent=uid(os.environ['NOTION_PAGE_ID']);op=req['operation']
    if op=='capabilities':
        api('/pages/'+parent)
        # Prove comment retrieval permission as well as page access.
        list(pages('/comments?block_id='+parent))
        return {'protocol_version':1,'attributable_decisions':True,'kind':'notion'}
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
