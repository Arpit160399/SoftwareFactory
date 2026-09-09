#!/usr/bin/env python3
"""Exercise a real 0.1.0 -> current release upgrade and both rollback paths.
Usage: python3 tests/upgrade_smoke.py /path/to/0.1.0/softwarefactory /path/to/new/softwarefactory
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

old, new = (str(Path(arg).resolve()) for arg in sys.argv[1:])

def run(binary, *args):
    return subprocess.run([binary, *map(str, args)], check=True, capture_output=True, text=True).stdout

assert run(old, '--version').strip() == 'softwarefactory 0.1.0'
new_version = run(new, '--version').strip().split()[-1]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    prefix = root / 'installation'
    project = root / 'project'
    other = root / 'other'
    project.mkdir()
    other.mkdir()
    profile = root / 'profile.json'
    profile.write_text(run(old, 'template', 'generic', '--id', 'upgrade-smoke'))
    for selected in (project, other):
        run(old, '--project', selected, 'setup', '--profile', profile, '--apply')
    run(old, 'install', '--prefix', prefix, '--apply')
    launcher = prefix / 'bin/softwarefactory'
    original_target = launcher.readlink()
    preview = json.loads(run(new, 'update', '--local', '--prefix', prefix))
    assert preview['from_version'] == '0.1.0'
    assert preview['to_version'] == new_version
    assert launcher.readlink() == original_target
    run(new, 'update', '--local', '--prefix', prefix, '--apply')
    assert run(launcher, '--version').strip() == f'softwarefactory {new_version}'
    lock = project / '.product-workflow/lock.json'
    assert json.loads(lock.read_text())['core'] == '0.1.0'
    previous_profile = (project / '.product-workflow/profile.json').read_bytes()
    run(launcher, '--project', project, 'project-update')
    assert json.loads(lock.read_text())['core'] == '0.1.0'
    output = run(launcher, '--project', project, 'project-update', '--apply')
    plan, _ = json.JSONDecoder().raw_decode(output)
    assert json.loads(lock.read_text())['core'] == new_version
    assert (project / '.product-workflow/profile.json').read_bytes() == previous_profile
    assert json.loads((other / '.product-workflow/lock.json').read_text())['core'] == '0.1.0'
    run(launcher, '--project', project, 'rollback', plan['id'], '--apply')
    assert json.loads(lock.read_text())['core'] == '0.1.0'
    run(new, 'update', '--prefix', prefix, '--to', '0.1.0', '--apply')
    assert run(launcher, '--version').strip() == 'softwarefactory 0.1.0'
    run(new, 'update', '--local', '--prefix', prefix, '--apply')
    assert run(launcher, '--version').strip() == f'softwarefactory {new_version}'
print(f'PASS: real 0.1.0 -> {new_version} update, project adoption, project isolation, both rollbacks and retry')
