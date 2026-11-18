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
SANDBOX_ENTITLEMENTS = {
    'com.apple.security.app-sandbox': True,
    'com.apple.security.files.user-selected.read-write': True,
    'com.apple.security.print': True,
}


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


def sandbox_entitlements(sandbox, entitlements):
    if entitlements is not None and not sandbox:
        raise ValueError('--entitlements requires --sandbox')
    if not sandbox:
        return None
    values = dict(SANDBOX_ENTITLEMENTS)
    if entitlements is not None:
        with entitlements.open('rb') as stream:
            supplied = plistlib.load(stream)
        if not isinstance(supplied, dict):
            raise ValueError('Entitlements must be a plist dictionary')
        values.update(supplied)
    if any(values.get(key) is not True for key in SANDBOX_ENTITLEMENTS):
        raise ValueError('Sandbox, user-selected read/write and print entitlements must remain enabled')
    return values


def disk_image(app, image, output):
    icon = ROOT / 'assets/notepad-matrix.icns'
    with tempfile.TemporaryDirectory(prefix='.dmg-', dir=output) as temporary:
        work = Path(temporary)
        stage = work / 'stage'
        stage.mkdir()
        shutil.copytree(app, stage / app.name)
        (stage / 'Applications').symlink_to('/Applications', target_is_directory=True)
        writable = work / 'writable.dmg'
        subprocess.run(['/usr/sbin/diskutil', 'image', 'create', 'from', '--volumeName',
                        'notepad-matrix', '--format', 'RAW', str(stage), str(writable)], check=True)
        mount = Path(tempfile.mkdtemp(prefix='.mount-', dir=output))
        try:
            subprocess.run(['/usr/bin/hdiutil', 'attach', '-nobrowse', '-mountpoint',
                            str(mount), str(writable)], check=True)
            shutil.copy2(icon, mount / '.VolumeIcon.icns')
            subprocess.run(['/usr/bin/xcrun', 'SetFile', '-a', 'C', str(mount)], check=True)
        finally:
            if os.path.ismount(mount):
                result = subprocess.run(['/usr/bin/hdiutil', 'detach', str(mount)])
                if result.returncode:
                    subprocess.run(['/usr/bin/hdiutil', 'detach', '-force', str(mount)], check=True)
            mount.rmdir()
        subprocess.run(['/usr/bin/hdiutil', 'convert', str(writable), '-format', 'UDZO',
                        '-o', str(image)], check=True)
        script = work / 'disk-icon.swift'
        script.write_text('''import AppKit
let arguments = CommandLine.arguments
guard let icon = NSImage(contentsOfFile: arguments[1]),
      NSWorkspace.shared.setIcon(icon, forFile: arguments[2], options: []) else {
    fputs("Cannot set disk image icon\\n", stderr)
    exit(1)
}
''', encoding='utf-8')
        subprocess.run(['/usr/bin/swift', str(script), str(icon), str(image)], check=True)


def package(binary, output, sign_identity='-', sandbox=False, entitlements=None):
    if sys.platform != 'darwin':
        raise SystemExit('Packaging requires macOS')
    if not sign_identity.strip():
        raise ValueError('Signing identity must not be empty; use - for ad-hoc signing')
    signing_entitlements = sandbox_entitlements(sandbox, entitlements)
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
    reported = subprocess.check_output([str(binary), '--version'], text=True).strip()
    version = re.fullmatch(r'notepad-matrix (\d+\.\d+\.\d+)\+(?:[0-9a-f]{7}|unknown)', reported)
    if version is None:
        raise ValueError(f'Unexpected executable version: {reported}')
    version = version[1]
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
        signing = ['/usr/bin/codesign', '--force', '--sign', sign_identity]
        if signing_entitlements is not None:
            entitlement_file = Path(temporary) / 'sandbox.entitlements'
            entitlement_file.write_bytes(plistlib.dumps(signing_entitlements))
            signing.extend(['--entitlements', str(entitlement_file), '--generate-entitlement-der'])
        subprocess.run([*signing, str(bundle)], check=True)
        subprocess.run(['/usr/bin/codesign', '--verify', '--strict', '--verbose=2', str(bundle)], check=True)
        bundle.rename(app)
    image = output / 'notepad-matrix.dmg'
    disk_image(app, image, output)
    print(image)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--sign-identity', default='-', help='codesign identity; default: ad-hoc (-)')
    parser.add_argument('--sandbox', action='store_true', help='Create an App Sandbox development package')
    parser.add_argument('--entitlements', type=Path, help='Explicit entitlement overrides/additions; requires --sandbox')
    args = parser.parse_args()
    output = args.output if args.output is not None else ROOT / ('dist-sandbox' if args.sandbox else 'dist')
    package(args.binary, output, args.sign_identity, args.sandbox, args.entitlements)
