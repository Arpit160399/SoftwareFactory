#!/usr/bin/env python3
"""Keyboard-driven GitHub intake, repair, failure recovery and narrow-screen checks.
Runs the sibling console smoke first, then uses its actual PTY driver.
"""
import json, os, pathlib, runpy, subprocess, sys, tempfile, time, uuid
repo = pathlib.Path(__file__).resolve().parents[1]
binary = str(pathlib.Path(sys.argv[1]).resolve())
driver = runpy.run_path(str(repo / 'tests/console_smoke.py'))
Session = driver['Session']

def decision(root, action):
    queue = json.loads((root / '.product-workflow/issues/queue.json').read_text())
    workflow_id = queue['tasks'][0]['workflow_id']
    workflow = json.loads((root / f'.product-workflow/workflows/{workflow_id}/workflow.json').read_text())
    run_id = workflow['cycles'][0]['run_id']
    run = json.loads((root / f'.product-workflow/runs/{run_id}/run.json').read_text())
    revision = run['proposal']['revision'] if action == 'build' else run['iterations'][-1]['implementation']['candidate_revision']
    claim = dict(id=str(uuid.uuid4()), project_id=run['project_id'], run_id=run_id, actor='fixture-human', decided_at='2026-09-12T12:00:00Z', artifact_revision=revision, source='fixture://receipt', action=action)
    directory = root / '.product-workflow/fixture-decisions'
    directory.mkdir(exist_ok=True)
    (directory / (claim['id'] + '.json')).write_text(json.dumps(claim))
    return workflow_id

with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as commands:
    root = pathlib.Path(temporary)
    commands = pathlib.Path(commands)
    for name, content in [('.softwarefactory-synthetic-fixture', 'test only'), ('.fixture-propose', 'yes'), ('.fixture-regression-first', 'yes'), ('existing.txt', 'preserved\n')]:
        (root / name).write_text(content)
    bridge = str(repo / 'examples/fixture_bridge.py')
    profile = json.loads(subprocess.check_output([binary, 'template', 'generic', '--id', 'github-tui']))
    profile['product_brief'] = 'Fix saving while preserving existing content'
    profile['runtime']['command'] = dict(program=bridge, args=[])
    profile['runtime']['planner_model'] = 'fixture-planner'
    profile['review']['command'] = dict(program=bridge, args=[])
    profile['review']['reviewers'] = ['fixture-human']
    profile['checks'] = [dict(id=check, category='delivered_behaviour', required=True, command=dict(program=bridge, args=[]), criterion_ids=[criterion], timeout_seconds=10) for check, criterion in [('result','ISSUE-FIX'),('regression','ISSUE-REGRESSION')]]
    draft = root / 'draft.json'; draft.write_text(json.dumps(profile))
    subprocess.run([binary, '--project', str(root), 'setup', '--profile', str(draft), '--apply'], check=True, capture_output=True)
    issue = dict(number=17, title='Save result without losing existing content', body='Saving must persist verified while preserving existing content.', html_url='https://github.com/owner/repo/issues/17', state='open', updated_at='2026-09-12T12:00:00Z', labels=[dict(name='bug')])
    pages = commands / 'pages.json'; pages.write_text(json.dumps([[issue]]))
    failure = commands / 'fail'
    gh = commands / 'gh'
    gh.write_text('#!/usr/bin/env python3\nimport pathlib,sys,time\n' + f'assert sys.argv[1:5] == ["api", "--hostname", "github.com", "--method"]\nif pathlib.Path({str(failure)!r}).exists(): sys.exit(1)\nprint(pathlib.Path({str(pages)!r}).read_text())\n')
    gh.chmod(0o755)
    previous_path = os.environ.get('PATH', '')
    os.environ['PATH'] = str(commands) + ':' + previous_path
    try:
        s = Session(root)
        try:
            s.wait('github-tui'); s.key(b'7'); s.wait('No GitHub issue queue yet')
            s.key(b'g'); s.wait('Repository (OWNER/REPO)'); s.key(b'\r'); s.wait('Use OWNER/REPO')
            s.key(b'owner/repo\tbug\r'); s.wait('#17 Save result'); s.wait('queued')
            assert not (root / '.product-workflow/workflows').exists(), 'Scan dispatched a workflow'
            s.key(b'\r'); s.wait('No fix has been verified'); s.key(b'\x1b'); s.wait('Issue queue')
            s.key(b'/'); s.wait('Filter tasks'); s.key(b'not-a-match\r'); s.wait('No issues match'); s.key(b'c'); s.wait('#17 Save result')
            failure.write_text('offline')
            s.key(b'g'); s.wait('Repository (OWNER/REPO)'); s.key(b'\r'); s.wait('Scan/read failed'); s.wait('#17 Save result')
            failure.unlink()
            s.key(b'g'); s.wait('Repository (OWNER/REPO)'); s.key(b'\r'); s.wait('GitHub scan complete')
            s.key(b'r'); s.wait('Run issue loop'); s.key(b'\x1b'); s.wait('Issue queue')
            assert not (root / '.product-workflow/workflows').exists(), 'Cancelled run dispatched a workflow'
            s.key(b'r'); s.wait('Run issue loop'); s.key(b'y'); s.wait('awaiting build approval', seconds=25)
            assert not (root / 'result.txt').exists(), 'Unapproved implementation ran'
            s.key(b' '); s.wait('Monitoring paused')
            workflow_id = decision(root, 'build')
            s.key(b'r'); s.wait('Run issue loop'); s.key(b'y'); s.wait('awaiting acceptance', seconds=30)
            assert (root / 'existing.txt').read_text() == 'preserved\n'
            s.key(b'\r'); s.wait('ISSUE-REGRESSION'); s.wait('Attempt 2')
            (repo / 'docs/issues-terminal.txt').write_text(s.screen())
            s.key(b' '); s.wait('Runner paused'); s.key(b'\x1b'); s.wait('Issue queue')
            s.key(b'x'); s.wait('Stop workflow'); s.key(b'\x1b'); s.wait('Issue queue')
            s.key(b'x'); s.wait('Stop workflow'); s.key(b'y'); s.wait('stopped', seconds=20)
            s.key(b't'); s.wait('Retry issue #17'); s.key(b'y'); s.wait('queued for a fresh attempt')
            queue = json.loads((root / '.product-workflow/issues/queue.json').read_text())
            assert len(queue['tasks']) == 1
            assert queue['tasks'][0]['workflow_id'] is None
            assert queue['tasks'][0]['previous_workflows'] == [workflow_id]
        finally:
            s.close()
        for width,height in [(44,16),(42,12)]:
            s = Session(root,width,height)
            try:
                s.wait('SOFTWARE FACTORY'); s.key(b'7'); s.wait('7 Issues'); s.wait('#17 Save result')
                s.key(b'g'); s.wait('Repository (OWNER/REPO)'); s.wait('Enter Scan'); s.wait('Esc Cancel')
                assert 'owner/repo' in s.screen() and 'bug' in s.screen()
                s.key(b'\x1b'); s.wait('Issue queue')
                s.key(b'r'); s.wait('Run issue loop'); s.wait('Y Continue'); s.wait('Esc Cancel')
                s.key(b'\x1b'); s.wait('Issue queue')
            finally:
                s.close()
        # Idle monitoring must remain visibly active and pause without starting new work.
        pages.write_text('[[]]')
        s = Session(root)
        try:
            s.wait('github-tui'); s.key(b'7'); s.wait('Issue queue'); s.key(b'g'); s.wait('Repository (OWNER/REPO)'); s.key(b'\r'); s.wait('not eligible')
            s.key(b'r'); s.wait('Run issue loop'); s.key(b'y'); s.wait('GitHub issue loop active')
            s.key(b' '); s.wait('Monitoring paused')
        finally:
            s.close()
        assert len(list((root / '.product-workflow/workflows').iterdir())) == 1
    finally:
        os.environ['PATH'] = previous_path
print('PASS: TUI issue scan, validation, search, failed-scan recovery, run/cancel, approval gates, regression repair, stop/retry, idle pause, narrow forms and terminal restoration')
