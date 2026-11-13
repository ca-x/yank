#!/usr/bin/env python3
"""Generate the NSH updater manifest from verified desktop release archives."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = ('linux-x86_64', 'linux-arm64', 'macos-x86_64', 'macos-arm64', 'windows-x86_64')


def generate(directory, version):
    platforms = {}
    for platform in PLATFORMS:
        extension = 'zip' if platform.startswith('windows') else 'tar.gz'
        archive = directory / f'yank-{version}-{platform}.{extension}'
        with archive.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if archive.stat().st_size == 0:
            raise ValueError(f'Empty archive: {archive}')
        platforms[platform] = {
            'url': f'https://github.com/ca-x/yank/releases/download/v{version}/{archive.name}',
            'sha256': digest,
            'size': archive.stat().st_size,
        }
    return {'id': 'io.github.cax.yank', 'version': version, 'platforms': platforms}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    manifest = generate(args.directory, version)
    (args.directory / 'latest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
