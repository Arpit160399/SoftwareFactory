#!/usr/bin/env python3
"""Validate a tagged native build and prepare its GitHub release asset."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--target', required=True, choices=[
    'aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-unknown-linux-gnu'])
parser.add_argument('--tag', required=True)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
if args.tag != f'v{version}' or '-' in version or '+' in version:
    parser.error('Release tag must match the stable Cargo.toml package version')
result = subprocess.run([str(args.binary.resolve()), '--version'], check=True, capture_output=True, text=True)
if result.stdout.strip() != f'softwarefactory {version}':
    parser.error('Built binary version does not match the release tag')
args.output.mkdir(parents=True, exist_ok=True)
shutil.copy2(args.binary, args.output / f'softwarefactory-{args.target}')
print(f'Prepared {version} for {args.target}')
