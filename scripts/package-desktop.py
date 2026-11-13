#!/usr/bin/env python3
"""Package already-built Cargo binaries; Linux additionally needs dpkg-dev.

Usage: python scripts/package-desktop.py --platform linux-x86_64 [--out dist]
Linux deb dependencies are derived from the actual ELF binaries on Ubuntu 24.04.
No credentials, generated NSH workspace, or local application data are bundled.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', required=True, choices=[
        'linux-x86_64', 'linux-arm64', 'macos-arm64', 'macos-x86_64', 'windows-x86_64'])
    parser.add_argument('--out', type=Path, default=ROOT / 'dist')
    parser.add_argument('--target-dir', type=Path, default=ROOT / 'target')
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise SystemExit('Expected a stable Cargo package version')
    if json.loads((ROOT / 'crates/yank-client/nsh.json').read_text())['version'] != version:
        raise SystemExit('Cargo and nsh.json versions differ')
    tag = os.environ.get('GITHUB_REF_NAME', '')
    if os.environ.get('GITHUB_REF_TYPE') == 'tag' and tag != f'v{version}':
        raise SystemExit(f'Tag {tag!r} does not match v{version}')
    args.out.mkdir(parents=True, exist_ok=True)
    out = args.out.resolve()
    suffix = '.exe' if args.platform.startswith('windows') else ''
    assets = []
    with tempfile.TemporaryDirectory(prefix='yank-package-') as scratch:
        stage = Path(scratch)
        bundle = stage / f'yank-{version}-{args.platform}'
        bundle.mkdir()
        for name in ('yank-client', 'yank-server'):
            source = args.target_dir / 'release' / f'{name}{suffix}'
            if not source.is_file() or source.stat().st_size == 0:
                raise SystemExit(f'Missing binary: {source}')
            shutil.copy2(source, bundle / source.name)
            (bundle / source.name).chmod(0o755)
        for license_file in ROOT.glob('LICENSE*'):
            if license_file.is_file():
                shutil.copy2(license_file, bundle / license_file.name)
        shutil.copy2(ROOT / 'THIRD_PARTY.md', bundle / 'THIRD_PARTY.md')
        docs = bundle / 'docs'
        docs.mkdir()
        shutil.copy2(ROOT / 'docs/ditto-protocol.md', docs / 'ditto-protocol.md')
        subprocess.run([sys.executable, str(ROOT / 'scripts/collect-licenses.py'),
                        str(bundle / 'THIRD-PARTY-LICENSES.txt')], cwd=ROOT, check=True)
        (bundle / 'RUNNING.txt').write_text(
            'Yank client and server ' + version + '\n'
            'Linux binaries require Ubuntu 24.04 or compatible GTK 4.14/GStreamer libraries; '
            'install the deb to resolve runtime dependencies.\n'
            'macOS/Windows binaries are unsigned; macOS is not notarized.\n'
            'Server first run: set YANK_ADMIN_USERNAME and YANK_ADMIN_PASSWORD. '
            'Keep YANK_DB and its sibling .users directory on persistent storage.\n'
            'Ports: 7219/tcp API, 7220/tcp relay, optional 7842/udp NAT discovery.\n'
            'Set YANK_RELAY_URL to the client-accessible relay URL.\n', encoding='utf-8')
        if suffix:
            archive = out / f'{bundle.name}.zip'
            with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as stream:
                for item in sorted(bundle.rglob('*')):
                    if item.is_file():
                        stream.write(item, f'{bundle.name}/{item.relative_to(bundle).as_posix()}')
        else:
            archive = out / f'{bundle.name}.tar.gz'
            with tarfile.open(archive, 'w:gz') as stream:
                stream.add(bundle, arcname=bundle.name)
        assets.append(archive)
        if args.platform.startswith('linux'):
            arch = {'linux-x86_64': 'amd64', 'linux-arm64': 'arm64'}[args.platform]
            package = stage / 'deb'
            binaries = package / 'usr/bin'
            binaries.mkdir(parents=True)
            for name in ('yank-client', 'yank-server'):
                shutil.copy2(bundle / name, binaries / name)
            desktop = package / 'usr/share/applications/io.github.cax.yank.desktop'
            desktop.parent.mkdir(parents=True)
            desktop.write_text('[Desktop Entry]\nType=Application\nName=Yank\n'
                               'Exec=yank-client\nIcon=io.github.cax.yank\n'
                               'Terminal=false\nCategories=Utility;\n')
            icon = package / 'usr/share/icons/hicolor/512x512/apps/io.github.cax.yank.png'
            icon.parent.mkdir(parents=True)
            shutil.copy2(ROOT / 'crates/yank-client/assets/yank-icon-512.png', icon)
            docs = package / 'usr/share/doc/yank'
            shutil.copytree(bundle, docs, ignore=shutil.ignore_patterns('yank-client', 'yank-server'))
            # dpkg-shlibdeps needs a source control file, even for a binary-only package.
            debian = stage / 'debian'
            debian.mkdir()
            (debian / 'control').write_text('Source: yank\nSection: utils\nPriority: optional\n'
                                           'Maintainer: Yank contributors <noreply@github.com>\n\n'
                                           'Package: yank\nArchitecture: any\nDescription: Clipboard sync\n')
            dependencies = subprocess.check_output(
                ['dpkg-shlibdeps', '-O', *[f'-e{binaries / name}' for name in
                                           ('yank-client', 'yank-server')]], cwd=stage, text=True)
            depends = next(line.split('=', 1)[1] for line in dependencies.splitlines()
                           if line.startswith('shlibs:Depends='))
            control = package / 'DEBIAN'
            control.mkdir()
            (control / 'control').write_text(
                f'Package: yank\nVersion: {version}\nArchitecture: {arch}\n'
                'Maintainer: Yank contributors <noreply@github.com>\nSection: utils\nPriority: optional\n'
                f'Depends: {depends}, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good\n'
                'Description: Native clipboard history and self-hosted sync\n'
                ' Yank NSH desktop client and server.\n')
            deb = out / f'yank_{version}_{arch}.deb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(package), str(deb)], check=True)
            subprocess.run(['dpkg-deb', '--info', str(deb)], check=True)
            subprocess.run(['dpkg-deb', '--contents', str(deb)], check=True)
            assets.append(deb)
    checksums = out / f'SHA256SUMS-{args.platform}.txt'
    lines = []
    for item in assets:
        with item.open('rb') as stream:
            lines.append(f'{hashlib.file_digest(stream, "sha256").hexdigest()}  {item.name}\n')
    checksums.write_text(''.join(lines))
    print('\n'.join(str(item) for item in [*assets, checksums]))


if __name__ == '__main__':
    main()
