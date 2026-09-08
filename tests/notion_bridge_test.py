#!/usr/bin/env python3
"""Offline contract tests; no account/network calls."""
import importlib.util,json,pathlib,unittest,uuid
module=pathlib.Path(__file__).resolve().parents[1]/'bridges/notion_review.py'
spec=importlib.util.spec_from_file_location('notion_review',module);bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)
class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.parent,self.page,self.comment,self.actor=[str(uuid.uuid4()) for _ in range(4)]
        self.claim=dict(id=self.comment,project_id='alpha',run_id=str(uuid.uuid4()),actor=self.actor,decided_at='2026-09-08T00:00:00Z',artifact_revision='rev1',source=f'notion://{self.page}/{self.comment}',action='build')
        self.human='person';self.payload={k:self.claim[k] for k in ('project_id','run_id','artifact_revision','action')}
        self.marker='SoftwareFactory project=alpha run='+self.claim['run_id']
        bridge.api=self.api;bridge.pages=self.pages
    def api(self,path,*args):
        if path.startswith('/pages/'):return {'parent':{'page_id':self.parent}}
        if path.startswith('/users/'):return {'type':self.human}
        raise AssertionError(path)
    def pages(self,path):
        if path.startswith('/comments'):return iter([dict(id=self.comment,created_by={'id':self.actor},created_time=self.claim['decided_at'],rich_text=[{'plain_text':json.dumps(self.payload)}])])
        if path.startswith('/blocks/'):return iter([{'paragraph':{'rich_text':[{'plain_text':self.marker}]}}])
        raise AssertionError(path)
    def test_exact_human_source_passes(self):self.assertTrue(bridge.verify(self.claim,self.parent)['verified'])
    def test_bot_author_rejected(self):
        self.human='bot'
        with self.assertRaises(ValueError):bridge.verify(self.claim,self.parent)
    def test_changed_revision_rejected(self):
        self.payload['artifact_revision']='different'
        with self.assertRaises(ValueError):bridge.verify(self.claim,self.parent)
    def test_other_project_packet_rejected(self):
        self.marker='SoftwareFactory project=beta run='+self.claim['run_id']
        with self.assertRaises(ValueError):bridge.verify(self.claim,self.parent)
    def test_fabricated_actor_rejected(self):
        claim=dict(self.claim,actor=str(uuid.uuid4()))
        with self.assertRaises(ValueError):bridge.verify(claim,self.parent)
if __name__=='__main__':unittest.main()
