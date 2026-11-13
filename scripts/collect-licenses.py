#!/usr/bin/env python3
"""Collect resolved dependency metadata and bundled license/notice files for release archives."""
import json
from pathlib import Path
import subprocess
import sys


def collect(destination: Path):
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], text=True, encoding='utf-8'))
    parts = ['Yank dependency notices\nExact versions/sources are recorded in Cargo.lock.\n']
    icon_license = Path(__file__).resolve().parent.parent / 'assets/icons/LICENSE'
    parts.append('\nKubai Icons (bundled SVG assets)\n' + icon_license.read_text(encoding='utf-8'))
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        root = Path(package['manifest_path']).parent
        parts.append(f'\n{"=" * 72}\n{package["name"]} {package["version"]}\nSPDX: {package.get("license") or "see bundled license"}\nSource: {package.get("repository") or package.get("source") or "Yank workspace"}\n')
        files = set()
        for pattern in ('LICENSE*', 'COPYING*', 'NOTICE*', 'license*'):
            for candidate in root.glob(pattern):
                if candidate.is_file():
                    files.add(candidate)
                elif candidate.is_dir():
                    files.update(p for p in candidate.iterdir() if p.is_file())
        if package.get('license_file'):
            path = Path(package['license_file'])
            files.add(path if path.is_absolute() else root / path)
        for path in sorted(files):
            if path.is_file() and path.stat().st_size < 2_000_000:
                parts.append(f'\n--- {path.name} ---\n' + path.read_text(encoding='utf-8', errors='replace') + '\n')
    destination.write_text(''.join(parts), encoding='utf-8')


if __name__ == '__main__':
    collect(Path(sys.argv[1]))
