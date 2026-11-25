"""Build portable Windows desktop and CLI/MCP executables with bundled runtimes,
and optionally the Setup program (Inno Setup 6) from the same files."""
from pathlib import Path
import argparse
import json
import os
import re
import shutil
import subprocess
import tomllib
ROOT = Path(__file__).resolve().parents[1]
# Shipped beside the executables: the licenses and the third-party notices
# that the bundled crates, fonts and word lists require to travel with them.
LICENSE_FILES = ('LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md')
# A placeholder, not a path: the package must not carry the build machine's
# folders, and MCP clients need the full path of the installed file. Setup
# writes its own copy with the installed path filled in.
MCP_COMMAND = '<full path to openxer-cli.exe>'
EXECUTABLES = ('OpenXER.exe', 'openxer-cli.exe')
ISS = ROOT / 'tools/installer/openxer.iss'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=Path('dist/OpenXER.exe'),
                    help='Destination executable, relative to the workspace root')
parser.add_argument('--package-only', action='store_true',
                    help='Skip the build and the executables; write only the '
                         'documents, licenses and openxer.mcp.json')
parser.add_argument('--installer', action='store_true',
                    help='Also build OpenXER-<version>-setup.exe from the packaged '
                         'files (needs Inno Setup 6)')
parser.add_argument('--installer-output', type=Path, default=Path('target/installer'),
                    help='Folder for the Setup program, relative to the workspace root')
parser.add_argument('--iscc', type=Path,
                    help='ISCC.exe to use (default: ISCC environment variable, PATH, '
                         'then the usual Inno Setup 6 folders)')
parser.add_argument('--sign-command',
                    help='Command that signs one file, with $f where the quoted file '
                         'path goes (for example: signtool sign /fd sha256 /a $f). '
                         'Signs both executables, Setup and the uninstaller. '
                         'Without it nothing is signed.')
args = parser.parse_args()


def absolute(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def version() -> str:
    """The workspace version, the one the executables carry."""
    with open(ROOT / 'Cargo.toml', 'rb') as f:
        return tomllib.load(f)['workspace']['package']['version']


def sign(path: Path) -> None:
    command = args.sign_command.replace('$f', f'"{path}"')
    subprocess.run(command, shell=True, cwd=ROOT, check=True)


def find_iscc() -> Path:
    candidates = [args.iscc, os.environ.get('ISCC'), shutil.which('ISCC')]
    for base in ('LOCALAPPDATA', 'ProgramFiles(x86)', 'ProgramFiles'):
        folder = os.environ.get(base)
        if folder:
            programs = Path(folder) / 'Programs' if base == 'LOCALAPPDATA' else Path(folder)
            candidates.append(programs / 'Inno Setup 6' / 'ISCC.exe')
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise SystemExit('Inno Setup 6 (ISCC.exe) was not found; install it or pass --iscc')


output = absolute(args.output)
if not args.package_only:
    subprocess.run([
        'cargo.exe', 'build', '-p', 'openxer-desktop', '-p', 'openxer-automation', '--release',
        '--target', 'x86_64-pc-windows-msvc',
        '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-Ctarget-feature=+crt-static"]',
    ], cwd=ROOT, check=True)
output.parent.mkdir(parents=True, exist_ok=True)
if not args.package_only:
    for name, destination in [(EXECUTABLES[0], output),
                              (EXECUTABLES[1], output.parent / EXECUTABLES[1])]:
        temporary = destination.with_suffix('.exe.tmp')
        try:
            shutil.copy2(ROOT / 'target/x86_64-pc-windows-msvc/release' / name, temporary)
            if args.sign_command:
                sign(temporary)
            temporary.replace(destination)
        finally:
            temporary.unlink(missing_ok=True)
        print(destination)
shutil.copy2(ROOT / 'automation/README.md', output.parent / 'CLI-MCP.md')
for name in LICENSE_FILES:
    shutil.copy2(ROOT / name, output.parent / name)
(output.parent / 'openxer.mcp.json').write_text(json.dumps({
    'mcpServers': {'openxer': {
        'command': MCP_COMMAND,
        'args': ['mcp'],
    }},
}, indent=2) + '\n', encoding='utf-8', newline='\n')
if args.installer:
    if output.name != EXECUTABLES[0]:
        raise SystemExit('--installer needs --output to end in OpenXER.exe')
    missing = [n for n in EXECUTABLES if not (output.parent / n).is_file()]
    if missing:
        raise SystemExit(f'--installer needs {", ".join(missing)} in {output.parent}')
    app_version = version()
    # Windows version resources take numbers only: 1.2.3-beta.1 -> 1.2.3.
    numbers = re.match(r'\d+(\.\d+){0,3}', app_version).group(0)
    installer_output = absolute(args.installer_output)
    installer_output.mkdir(parents=True, exist_ok=True)
    command = [str(find_iscc()), '/Q',
               f'/DAppVersion={app_version}', f'/DVersionNumbers={numbers}',
               f'/DSourceDir={output.parent}', f'/DOutputDir={installer_output}']
    if args.sign_command:
        command += ['/DSignTool=openxersign', f'/Sopenxersign={args.sign_command}']
    subprocess.run(command + [str(ISS)], cwd=ROOT, check=True)
    print(installer_output / f'OpenXER-{app_version}-setup.exe')
