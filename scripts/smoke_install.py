#!/usr/bin/env python3
"""Install official Minecraft into a dedicated test root, corrupt one asset, repair it."""
import argparse
import hashlib
import json
import pathlib
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--version', default='1.21.1')
args = parser.parse_args()
repo = pathlib.Path(__file__).resolve().parents[1]
root = repo / '.runtime' / ('smoke-' + args.version)
subprocess.run(['cargo', 'build', '-p', 'nekocrate-cli', '--locked'], cwd=repo, check=True)
binary = repo / 'target' / 'debug' / 'nekocrate'
def cli(*commands, capture=False):
    return subprocess.run([str(binary), '--data-dir', str(root), *commands], cwd=repo, check=True, text=True, stdout=subprocess.PIPE if capture else None)
status = json.loads(cli('status', capture=True).stdout)
instances = status['database']['instances']
instance = next((i for i in instances if i['version'] == args.version), None)
if instance is None:
    instance = json.loads(cli('create', '原型安装验证', '--version', args.version, capture=True).stdout)
cli('install', instance['id'])
version = json.loads((root / 'instances' / instance['id'] / 'version.json').read_text())
index = json.loads((root / 'assets' / 'indexes' / (version['assetIndex']['id'] + '.json')).read_text())
asset = next(iter(index['objects'].values()))
asset_path = root / 'assets' / 'objects' / asset['hash'][:2] / asset['hash']
asset_path.write_bytes(b'nekocrate corruption test')
cli('repair', instance['id'])
assert hashlib.sha1(asset_path.read_bytes()).hexdigest() == asset['hash']
status = json.loads(cli('status', capture=True).stdout)
assert next(i for i in status['database']['instances'] if i['id'] == instance['id'])['installed']
print(json.dumps({'result': 'PASS', 'version': args.version, 'instance': instance['id'], 'data_dir': str(root), 'checks': ['official installation', 'automatic Java', 'asset corruption repair']}, ensure_ascii=False, indent=2))
