#!/usr/bin/env python3
"""Offline API contract tests. No account, token, or network access."""
import copy,importlib.util,pathlib,unittest,uuid,urllib.error
spec=importlib.util.spec_from_file_location('bridge',pathlib.Path(__file__).resolve().parents[1]/'bridges/notion_review.py')
bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)
class BoardTests(unittest.TestCase):
    def setUp(self):
        self.database,self.source=str(uuid.uuid4()),str(uuid.uuid4())
        self.config={'enabled':True,'database_id':self.database,'data_source_id':self.source,'properties':{field:field for field in bridge.BOARD_TYPES}}
        self.schema={'parent':{'database_id':self.database},'properties':{field:{'id':field,'type':kind,kind:({'options':[{'name':s} for s in bridge.BOARD_STATUSES]} if kind=='status' else {})} for field,kind in bridge.BOARD_TYPES.items()}}
        self.card=dict(record_key='project:workflow:feature:1',name='Improve onboarding',status='Needs approval',project_id='project',workflow_id='workflow',cycle=1,kind='Feature',stage='Awaiting build approval',agent='none',iteration=1,summary='A scoped improvement',blocker='',next_action='Review proposal',revision='rev1',event_time=1788912167,review_url='')
        self.items=[];self.creates=0;self.patches=0;self.fail_after_create=False;self.rate_limit=False;self.rate_limit_create=False
        bridge.api=self.api
    def api(self,path,method='GET',data=None):
        if path=='/data_sources/'+self.source:return copy.deepcopy(self.schema)
        if path.endswith('/query'):
            if self.rate_limit:raise urllib.error.HTTPError('https://api.notion.com',429,'rate limited',{'Retry-After':'120'},None)
            return {'results':[copy.deepcopy(p) for p in self.items if not p.get('archived')],'has_more':False}
        if path=='/pages' and method=='POST':
            if self.rate_limit_create:raise urllib.error.HTTPError('https://api.notion.com',429,'rate limited',{'Retry-After':'120'},None)
            self.creates+=1;page={'id':str(uuid.uuid4()),'properties':{},'parent':data['parent']};self.properties(page,data['properties']);page['properties']['notes']={'id':'notes','type':'rich_text','rich_text':[{'plain_text':'Human notes preserved'}]};self.items.append(page)
            if self.fail_after_create:self.fail_after_create=False;raise TimeoutError('Response lost')
            return copy.deepcopy(page)
        if path.startswith('/pages/'):
            page=next(p for p in self.items if p['id']==path.split('/')[-1])
            if method=='PATCH':self.patches+=1;self.properties(page,data['properties'])
            return copy.deepcopy(page)
        raise AssertionError((path,method,data))
    def properties(self,page,props):
        for field,value in props.items():
            kind=next(iter(value));page['properties'][field]=dict(copy.deepcopy(value),id=field,type=kind)
    def sync(self,sequence=1,may_create=True,reference=None):return bridge.main({'operation':'sync_card','config':self.config,'card':self.card,'sequence':sequence,'may_create':may_create,'reference':reference})
    def test_schema_probe_reads_without_writing(self):
        self.assertTrue(bridge.main({'operation':'kanban_capabilities','config':self.config})['ready']);self.assertEqual(self.creates+self.patches,0)
    def test_create_update_readback_and_notes_preserved(self):
        first=self.sync();self.assertEqual(first['status'],'completed')
        self.card['status']='In progress';self.card['stage']='Planning';second=self.sync(2,False,first['reference'])
        self.assertEqual(second['status'],'completed');self.assertEqual(first['reference'],second['reference']);self.assertEqual(self.creates,1)
        self.assertEqual(bridge.property_value(self.items[0]['properties']['notes']),'Human notes preserved')
    def test_lost_create_response_finds_original_without_duplicate(self):
        self.fail_after_create=True;failed=self.sync();self.assertEqual(failed['status'],'blocked');self.assertFalse(failed['no_create_attempt'])
        self.assertEqual(self.sync(may_create=False)['status'],'completed');self.assertEqual(self.creates,1)
    def test_unknown_creation_does_not_blindly_insert(self):
        self.assertEqual(self.sync(may_create=False)['status'],'blocked');self.assertEqual(self.creates,0)
    def test_deleted_card_requires_restore(self):
        first=self.sync();self.items[0]['archived']=True
        self.assertEqual(self.sync(2,False,first['reference'])['status'],'blocked');self.assertEqual(self.creates,1)
    def test_duplicate_or_cross_scope_card_is_rejected(self):
        self.sync();self.items.append(copy.deepcopy(self.items[0]));self.assertEqual(self.sync(2)['status'],'blocked');self.assertEqual(self.patches,0)
        self.items.pop();self.items[0]['properties']['project_id']['rich_text']=[{'plain_text':'other'}]
        self.assertEqual(self.sync(2)['status'],'blocked');self.assertEqual(self.patches,0)
    def test_old_event_cannot_overwrite_newer_status(self):
        self.sync(3);self.card['status']='Discovery';self.assertEqual(self.sync(2)['status'],'blocked');self.assertEqual(self.patches,0)
    def test_schema_mismatch_and_disabled_sync_never_write(self):
        self.schema['properties']['status']['type']='select';result=self.sync();self.assertEqual(result['status'],'blocked');self.assertTrue(result['no_create_attempt']);self.assertEqual(self.creates,0)
        self.config['enabled']=False;self.assertEqual(self.sync()['status'],'blocked')
    def test_rate_limit_exposes_backoff_and_known_no_create(self):
        self.rate_limit=True;result=self.sync();self.assertEqual(result['retry_after'],120);self.assertTrue(result['no_create_attempt']);self.assertEqual(self.creates,0)
    def test_rate_limited_create_can_retry_without_uncertain_duplicate(self):
        self.rate_limit_create=True;result=self.sync();self.assertTrue(result['no_create_attempt']);self.assertTrue(result['rate_limited']);self.assertEqual(self.creates,0)
        self.rate_limit_create=False;self.assertEqual(self.sync()['status'],'completed');self.assertEqual(self.creates,1)
if __name__=='__main__':unittest.main()
