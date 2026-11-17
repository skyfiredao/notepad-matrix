import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'aarch64-apple-darwin'


def document_types(output):
    source = (ROOT / 'src/ui/preferences_files_surface.rs').read_text(encoding='utf-8')
    start = source.index('pub(super) const ASSOCIATION_GROUPS:')
    end = source.index('\n];', start)
    extensions = [extension for _, values in re.findall(r'\("([^\"]+)",\s*&\[(.*?)\]\)', source[start:end], re.S)
                  for extension in re.findall(r'"([^\"]+)"', values)]
    if not extensions:
        raise ValueError('No source-supported document extensions found')
    program = '''import Foundation
import UniformTypeIdentifiers
var types: [String: String] = [:]
for ext in CommandLine.arguments.dropFirst() {
    if let type = UTType(filenameExtension: ext) { types[ext] = type.identifier }
}
let data = try JSONSerialization.data(withJSONObject: types)
FileHandle.standardOutput.write(data)
'''
    with tempfile.TemporaryDirectory(prefix='.document-types-', dir=output) as temporary:
        script = Path(temporary) / 'document-types.swift'
        script.write_text(program, encoding='utf-8')
        resolved = json.loads(subprocess.check_output(['/usr/bin/swift', str(script), *extensions]))
    types = []
    grouped = {}
    for extension in extensions:
        identifier = resolved.get(extension)
        if not identifier:
            raise ValueError(f'macOS cannot resolve the content type for .{extension}')
        if identifier.startswith('dyn.'):
            types.append({'CFBundleTypeName': f'{extension.upper()} document',
                          'CFBundleTypeRole': 'Editor', 'LSHandlerRank': 'Alternate',
                          'CFBundleTypeExtensions': [extension]})
        else:
            grouped.setdefault(identifier, []).append(extension)
    for identifier, members in grouped.items():
        types.append({'CFBundleTypeName': identifier, 'CFBundleTypeRole': 'Editor',
                      'LSHandlerRank': 'Alternate', 'LSItemContentTypes': [identifier],
                      'CFBundleTypeExtensions': members})
    return types


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
                       if any(kind['kind'] is None for kind in dependency['dep_kinds']))
    return [package for package in metadata['packages'] if package['id'] in selected]


def install_licenses(resources, packages):
    notices = (ROOT / 'THIRD_PARTY_NOTICES.md').read_text(encoding='utf-8')
    recorded = set(re.findall(r'^\| `([^`]+)` \| `([^`]+)` \| `([^`]+)` \|', notices, re.M))
    expected = {(package['name'], package['version'], package['license'])
                for package in packages if package['name'] != 'notepad-matrix'}
    if recorded != expected:
        raise ValueError(f'Third-party notices do not match release dependencies: '
                         f'missing={sorted(expected - recorded)}, stale={sorted(recorded - expected)}')
    for name in ['LICENSE', 'THIRD_PARTY_NOTICES.md']:
        shutil.copy2(ROOT / name, resources / name)


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
    types = document_types(output)
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
        shutil.copy2(ROOT / 'assets/notepad-matrix.icns', resources / 'notepad-matrix.icns')
        shutil.copytree(ROOT / 'assets/themes', resources / 'themes')
        identity = {
            'CFBundleName': 'notepad-matrix',
            'CFBundleDisplayName': 'notepad-matrix',
            'CFBundleIdentifier': 'org.notepad-matrix.app',
            'CFBundleExecutable': 'notepad-matrix',
            'CFBundleIconFile': 'notepad-matrix.icns',
            'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': version,
            'CFBundleVersion': version,
            'LSMinimumSystemVersion': os.environ['MACOSX_DEPLOYMENT_TARGET'],
            'NSHighResolutionCapable': True,
            'NSSupportsAutomaticGraphicsSwitching': True,
            'CFBundleDocumentTypes': types,
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
