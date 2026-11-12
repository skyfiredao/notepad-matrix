import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'aarch64-apple-darwin'


def release_packages():
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
        '--filter-platform', TARGET, '--no-default-features', '--features', 'native-ui',
    ], cwd=ROOT))
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    selected = set()
    pending = [metadata['resolve']['root']]
    while pending:
        identifier = pending.pop()
        if identifier in selected:
            continue
        selected.add(identifier)
        pending.extend(dependency['pkg'] for dependency in nodes[identifier]['deps']
                       if any(kind['kind'] != 'dev' for kind in dependency['dep_kinds']))
    return [package for package in metadata['packages'] if package['id'] in selected]


def install_licenses(resources, packages):
    artwork = resources / 'licenses/notepad-plus-plus'
    artwork.mkdir(parents=True)
    for name in ['LICENSE', 'README.md']:
        shutil.copy2(ROOT / 'assets/notepad-plus-plus' / name, artwork / name)
    notices = ['# Third-party dependencies', '',
               'Notepad Matrix is a separate application. Notepad++ artwork is used under its bundled license.',
               'This development build is ad-hoc signed, not notarized.', '']
    for package in sorted(packages, key=lambda item: (item['name'], item['version'])):
        if package['name'] == 'notepad-matrix':
            continue
        key = f"{package['name']}-{package['version']}"
        notices.extend([f'## {key}', f"License: {package.get('license') or 'See bundled files'}",
                        f"Source: {package.get('repository') or package['source']}", ''])
        dependency = Path(package['manifest_path']).parent
        files = {path for path in dependency.rglob('*') if path.is_file()
                 and path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE', 'AUTHORS'))
                 and path.suffix not in {'.rs', '.c', '.h', '.cpp'}
                 and not any(part in {'target', '.git', '__pycache__'}
                             for part in path.relative_to(dependency).parts)}
        if package.get('license_file'):
            files.add(dependency / package['license_file'])
        for source in sorted(files):
            destination = resources / 'licenses' / key / source.relative_to(dependency)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
    (resources / 'THIRD_PARTY_NOTICES.md').write_text('\n'.join(notices), encoding='utf-8')


def package(binary, output):
    if sys.platform != 'darwin':
        raise SystemExit('Packaging requires macOS')
    binary = binary.resolve(strict=True)
    output = output.resolve()
    with binary.open('rb') as stream:
        data = stream.read(8)
    if len(data) != 8 or struct.unpack('<II', data) != (0xfeedfacf, 0x100000c):
        raise ValueError('Expected an ARM64 Mach-O executable')
    if output == ROOT or output in ROOT.parents:
        raise ValueError('Output must not be the source root or an ancestor')
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError('Refusing to overwrite existing package artifacts')
    packages = release_packages()
    version = next(item['version'] for item in packages if item['name'] == 'notepad-matrix')
    app = output / 'notepad-matrix.app'
    with tempfile.TemporaryDirectory(prefix='.package-', dir=output) as temporary:
        contents = Path(temporary) / app.name / 'Contents'
        resources = contents / 'Resources'
        resources.mkdir(parents=True)
        executable = contents / 'MacOS/notepad-matrix'
        executable.parent.mkdir()
        shutil.copy2(binary, executable)
        executable.chmod(0o755)
        shutil.copy2(ROOT / 'assets/notepad-plus-plus/npp.icns', resources / 'npp.icns')
        identity = {
            'CFBundleName': 'notepad-matrix',
            'CFBundleDisplayName': 'notepad-matrix',
            'CFBundleIdentifier': 'org.notepad-matrix.app',
            'CFBundleExecutable': 'notepad-matrix',
            'CFBundleIconFile': 'npp.icns',
            'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': version,
            'CFBundleVersion': version,
            'LSMinimumSystemVersion': os.environ['MACOSX_DEPLOYMENT_TARGET'],
            'NSHighResolutionCapable': True,
            'NSSupportsAutomaticGraphicsSwitching': True,
        }
        (contents / 'Info.plist').write_bytes(plistlib.dumps(identity))
        install_licenses(resources, packages)
        subprocess.run(['/usr/bin/strip', '-S', '-x', str(executable)], check=True)
        bundle = contents.parent
        subprocess.run(['/usr/bin/codesign', '--force', '--sign', '-', str(bundle)], check=True)
        bundle.rename(app)
    image = output / 'notepad-matrix.dmg'
    with tempfile.TemporaryDirectory(prefix='.dmg-', dir=output) as temporary:
        stage = Path(temporary)
        shutil.copytree(app, stage / app.name)
        (stage / 'Applications').symlink_to('/Applications', target_is_directory=True)
        subprocess.run(['/usr/bin/hdiutil', 'create', '-volname', 'notepad-matrix',
                        '-srcfolder', str(stage), '-format', 'UDZO', str(image)], check=True)
    print(image)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    package(args.binary, args.output)
