"""Regenerate the Rust crate and SQLite notices in THIRD_PARTY.md.

Usage, from the repository root (Windows or Linux; the output is the same
on both):

    python tools/licenses/third_party.py          # rewrite THIRD_PARTY.md
    python tools/licenses/third_party.py --check  # fail if it is out of date

The crates are the ones the shipped executables are built from, on
Windows (OpenXER.exe and openxer-cli.exe) and on Linux (OpenXER and
openxer-cli): `cargo tree -e normal --target <triple>` for openxer-desktop
and openxer-automation, once for each triple in TARGETS (normal
dependencies only, not build or dev dependencies), with license and
location details from `cargo metadata`. The two sets are listed together;
a crate that only one system's executables contain is marked with that
system. Procedural macro crates are included, which lists slightly more
than strictly ships. Crates missing from the local cargo registry are
downloaded by cargo when it is online.

For each crate the license expression comes from its Cargo.toml and the
copyright lines and notice files from its package in the local cargo
registry. Crates are grouped by license expression; each standard license
text is printed once; license files that are not plain MIT or Apache-2.0
texts, and every NOTICE file, are reproduced per crate. Any license outside
PERMISSIVE is printed to the console for review; nothing is decided here.

Only the part of THIRD_PARTY.md between the BEGIN and END markers is
written; the hand-written word list and font notices above it stay.
"""
from collections import Counter, defaultdict
from pathlib import Path
import argparse
import json
import re
import subprocess
import sys
import textwrap

ROOT = Path(__file__).resolve().parents[2]
# (triple, system name) for each system the executables are released for.
TARGETS = (('x86_64-pc-windows-msvc', 'Windows'), ('x86_64-unknown-linux-gnu', 'Linux'))
SHIPPED = ('openxer-desktop', 'openxer-automation')
BEGIN = '<!-- BEGIN generated crate notices: tools/licenses/third_party.py -->'
END = '<!-- END generated crate notices -->'
PERMISSIVE = {
    'MIT', 'MIT-0', 'Apache-2.0', 'Apache-2.0 WITH LLVM-exception', 'BSD-2-Clause',
    'BSD-3-Clause', '0BSD', 'ISC', 'Zlib', 'BSL-1.0', 'CC0-1.0', 'Unlicense',
    'Unicode-3.0', 'Unicode-DFS-2016', 'MPL-2.0',
}
NOTICE_NAMES = ('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'COPYRIGHT', 'UNLICENSE')
# A copyright line: "Copyright (c) 2018 Name", "Copyright 2018 Name",
# "(c) 2018 Name", "Copyright The X Developers"; not license prose such as
# "copyright license to reproduce" or "(c) You must retain".
COPYRIGHT = re.compile(
    r'^\s*(copyright\s*(\(c\)|©|\d{4})|(\(c\)|©)\s*\d{4}|'
    r'copyright\s+(?!(license|licence|assignment|notice|owner|holder|statement|law|'
    r'and|or|is|to|of|in|by|the\s+above)\b)[A-Z])',
    re.IGNORECASE)
TEMPLATE = re.compile(r'\[yyyy\]|\{yyyy\}|<year>|<copyright|\[name of|\{name of|'
                      r'copyright notice|copyright holders? (and|or|be)|copyright owner',
                      re.IGNORECASE)


def run(*command):
    return subprocess.run(command, cwd=ROOT, check=True, capture_output=True,
                          text=True, encoding='utf-8').stdout


def metadata():
    base = ['cargo', 'metadata', '--format-version', '1', '--locked']
    for triple, _ in TARGETS:
        base += ['--filter-platform', triple]
    try:
        return json.loads(run(*base, '--offline'))
    except subprocess.CalledProcessError:
        return json.loads(run(*base))


def tree(triple):
    """(name, version) of every package `cargo tree -e normal` lists for one target.

    cargo tree resolves features for the shipped packages alone, as the
    release build does; the metadata graph unifies features across the
    whole workspace (tests included) and would list crates that do not ship.
    """
    command = ['cargo', 'tree', '-e', 'normal', '--target', triple, '--prefix', 'none',
               '-f', '{p}', '--locked']
    for name in SHIPPED:
        command += ['-p', name]
    try:
        listing = run(*command, '--offline')
    except subprocess.CalledProcessError:
        listing = run(*command)
    wanted = set()
    for line in listing.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1].startswith('v'):
            wanted.add((parts[0], parts[1][1:]))
    return wanted


def shipped_packages(meta):
    """Registry packages built into any system's executables, each with the
    names of the systems (in TARGETS order) whose executables contain it."""
    systems = defaultdict(list)
    for triple, system in TARGETS:
        for key in tree(triple):
            systems[key].append(system)
    out = [(p, tuple(systems[(p['name'], p['version'])])) for p in meta['packages']
           if p['source'] is not None and (p['name'], p['version']) in systems]
    return sorted(out, key=lambda e: (e[0]['name'], e[0]['version']))


def license_files(package):
    folder = Path(package['manifest_path']).parent
    files = {f for f in folder.iterdir()
             if f.is_file() and f.name.upper().startswith(NOTICE_NAMES)}
    if package.get('license_file'):
        named = folder / package['license_file']
        if named.is_file():
            files.add(named)
    return sorted(files, key=lambda f: f.name.upper())


def read(path):
    return path.read_bytes().decode('utf-8', 'replace').replace('\r\n', '\n').replace('\r', '\n')


def kind(text):
    """The SPDX id a license file's text is, or None when it is not one plain text."""
    t = ' '.join(text.split()).lower()
    if 'unicode' in t and 'data files' in t:
        return 'Unicode-3.0' if 'unicode license v3' in t else None
    found = []
    if 'apache license' in t and 'version 2.0' in t and 'terms and conditions' in t:
        found.append('Apache-2.0')
    if 'permission is hereby granted, free of charge' in t:
        found.append('MIT')
    return found[0] if len(found) == 1 else None


def body(text):
    """A license text without its copyright lines."""
    lines = [l for l in text.split('\n') if not (COPYRIGHT.match(l) and not TEMPLATE.search(l))]
    return '\n'.join(lines).strip('\n')


def norm(text):
    return ' '.join(text.split())


def copyrights(package, files):
    lines = []
    for f in files:
        for line in read(f).split('\n'):
            line = line.strip()
            if COPYRIGHT.match(line) and not TEMPLATE.search(line) and line not in lines:
                lines.append(line)
    return lines


def ids(expression):
    parts = re.split(r'\s+(?:OR|AND)\s+|[()/]', expression or '')
    out = []
    for p in (p.strip() for p in parts):
        if p:
            out.append(p)
    # Keep "X WITH exception" together.
    joined = []
    for p in out:
        if joined and p.startswith('WITH '):
            joined[-1] += ' ' + p
        else:
            joined.append(p)
    return [re.sub(r'\s+', ' ', p) for p in joined]


def sqlite_section(packages):
    sys_crate = next((p for p in packages if p['name'] == 'libsqlite3-sys'), None)
    if sys_crate is None:
        return []
    header = Path(sys_crate['manifest_path']).parent / 'sqlite3' / 'sqlite3.h'
    version = 'unknown'
    if header.is_file():
        m = re.search(r'#define SQLITE_VERSION\s+"([^"]+)"', read(header))
        if m:
            version = m.group(1)
    return [
        '## SQLite',
        '',
        f'OpenXER and openxer-cli include SQLite {version}, compiled from the',
        f'amalgamation bundled in the `libsqlite3-sys` {sys_crate["version"]} crate',
        '(listed below with its own license). SQLite is in the public domain; its',
        'source carries this blessing in place of a license:',
        '',
        '```text',
        'The author disclaims copyright to this source code.  In place of',
        'a legal notice, here is a blessing:',
        '',
        '   May you do good and not evil.',
        '   May you find forgiveness for yourself and forgive others.',
        '   May you share freely, never taking more than you give.',
        '```',
        '',
    ]


def generate():
    meta = metadata()
    shipped = shipped_packages(meta)
    packages = [p for p, _ in shipped]
    # The one system whose executables contain a crate, for crates not in all.
    only = {(p['name'], p['version']): systems[0]
            for p, systems in shipped if len(systems) < len(TARGETS)}
    only_counts = Counter(only.values())
    groups = defaultdict(list)
    texts = defaultdict(Counter)
    originals = {}
    verbatim = []
    review = []
    for p in packages:
        expression = p.get('license') or 'NOASSERTION (see the license file)'
        files = license_files(p)
        groups[expression].append((p, copyrights(p, files),
                                   only.get((p['name'], p['version']))))
        for i in ids(p.get('license')):
            if i not in PERMISSIVE:
                review.append(f'{p["name"]} {p["version"]}: {expression}')
                break
        if not p.get('license'):
            review.append(f'{p["name"]} {p["version"]}: no license expression')
        for f in files:
            text = read(f)
            k = kind(text)
            if k and not f.name.upper().startswith('NOTICE'):
                b = body(text)
                texts[k][norm(b)] += 1
                originals.setdefault(norm(b), b)
            else:
                verbatim.append((p, f.name, text.strip('\n')))
    out = [
        BEGIN,
        '',
        '## Rust crates',
        '',
        *textwrap.wrap(
            'OpenXER and openxer-cli (OpenXER.exe and openxer-cli.exe on Windows) '
            f'are built from the {len(packages)} crates below: the normal dependencies '
            'of `openxer-desktop` and `openxer-automation` for '
            + ' and '.join(f'`{triple}` ({system})' for triple, system in TARGETS)
            + ', from `Cargo.lock`. A crate that only one system\'s executables '
            'contain is marked with that system ('
            + ', '.join(f'{only_counts[system]} marked "{system} only"'
                        for _, system in TARGETS)
            + '); the rest are in both. Each is listed under the license expression '
            'its `Cargo.toml` declares, with the copyright lines of its license '
            'files. The full license texts follow the list; files that are not a '
            'plain MIT or Apache-2.0 text, and every NOTICE file, are reproduced '
            'per crate after that. En and em dashes in reproduced texts are written '
            'as hyphens.', width=76, break_long_words=False, break_on_hyphens=False),
        'Regenerate this part with `python tools/licenses/third_party.py`.',
        '',
    ]
    out += sqlite_section(packages)
    out += ['### Crates by license', '']
    for expression in sorted(groups, key=lambda e: (-len(groups[e]), e)):
        members = groups[expression]
        out += [f'#### {expression} ({len(members)} crate{"s" if len(members) != 1 else ""})', '']
        for p, lines, system in members:
            authors = ', '.join(p.get('authors') or []) or f'the {p["name"]} developers'
            note = '; '.join(lines) if lines else f'no copyright line in the package; authors: {authors}'
            mark = f' ({system} only)' if system else ''
            out.append(f'- `{p["name"]}` {p["version"]}{mark}: {note}')
        out.append('')
    out += ['### License texts', '']
    for k in sorted(texts):
        common = texts[k].most_common(1)[0][0]
        out += [f'#### {k}', '',
                'Each crate under this license is covered by this text with its own',
                'copyright lines above.', '', '```text', originals[common], '```', '']
    if verbatim:
        out += ['### License and notice files reproduced per crate', '']
        for p, name, text in verbatim:
            out += [f'#### `{p["name"]}` {p["version"]}: `{name}`', '', '```text', text, '```', '']
    out += [END, '']
    # The project writes no en or em dashes (AGENTS.md); in reproduced
    # texts they become hyphens, which changes no meaning.
    text = '\n'.join(out).replace('\u2013', '-').replace('\u2014', '-')
    return text, review, len(packages)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--check', action='store_true',
                        help='Exit 1 when THIRD_PARTY.md differs from what would be written')
    args = parser.parse_args()
    path = ROOT / 'THIRD_PARTY.md'
    current = read(path)
    generated, review, count = generate()
    if BEGIN in current and END in current:
        head = current[:current.index(BEGIN)]
        tail = current[current.index(END) + len(END):].lstrip('\n')
    else:
        head, tail = current.rstrip('\n') + '\n\n', ''
    new = head + generated + tail
    print(f'{count} crates')
    for line in review:
        print(f'REVIEW (not a listed permissive license): {line}')
    if args.check:
        sys.exit(0 if new == current else 1)
    path.write_bytes(new.encode('utf-8'))


if __name__ == '__main__':
    main()
