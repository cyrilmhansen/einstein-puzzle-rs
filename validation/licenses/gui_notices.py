#!/usr/bin/env python3
"""Assemble and verify exact-license text for the locked Linux GUI dependency graph."""
import argparse
import csv
from hashlib import sha256
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "licenses/gui"
TARGET = "x86_64-unknown-linux-gnu"
NOTICE = re.compile(r"^(license|licence|licenses|copying|notice|copyright|unlicense)([-_.].*)?$", re.I)
FONT = {"OFL.txt", "UFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt", "egui-icons.txt"}
# The registry commit returns 404; exact release tag resolved and corroborated
# by byte-identical src/lib.rs for every affected crate. See licenses/gui/amont/SOURCES.tsv.
PUBLIC_RELEASE = {
    ("emilk/egui", "7c0b40402271e8d40cda69f87cf3c2617c2e4382", "0.37.0"):
        "841a328ba2dd8f6996beb7189ebb7fc00e668a2a",
}
MAX_TEXT = 2 << 20


def registry_graph():
    command = ["cargo", "tree", "--locked", "--offline", "-p", "einstein-gui", "-e", "normal",
               "--target", TARGET, "--prefix", "none", "--format", "{p}|{l}"]
    text = subprocess.check_output(command, cwd=ROOT, text=True)
    found = {}
    for line in text.splitlines():
        package, _ = line.split("|", 1)
        name, version, *annotations = package.split()
        if annotations and annotations[0].startswith("(/"):
            continue  # local workspace packages
        key = (name, version.removeprefix("v"))
        found[key] = "(proc-macro)" in annotations
    return found


def digest(data):
    return sha256(data).hexdigest()


def safe_path(name):
    p = PurePosixPath(name)
    if p.is_absolute() or not p.parts or '..' in p.parts or '\\' in name:
        raise ValueError(f"unsafe path: {name}")
    return p


def write_exact(target, data):
    if target.exists():
        if target.read_bytes() != data:
            raise ValueError(f"refusing to replace different bytes: {target}")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as out:
            out.write(data)


def collect(cargo_home):
    graph = registry_graph()
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    registry = {(p['name'], p['version']): p for p in lock['package'] if p.get('source', '').startswith('registry+')}
    records = []
    for (name, version), proc_macro in sorted(graph.items()):
        stem = f"{name}-{version}"
        locked = registry[(name, version)]
        expected = locked['checksum']
        archives = sorted((cargo_home / 'registry/cache').glob(f'*/{stem}.crate'))
        archive = next((p for p in archives if digest(p.read_bytes()) == expected), None)
        if archive is None:
            raise ValueError(f"missing archive with expected lock digest: {stem}")
        record = {"crate": name, "version": version, "proc_macro": proc_macro, "archive_sha256": expected,
                  "archive_url": f"https://static.crates.io/crates/{name}/{stem}.crate", "archive_notices": []}
        with tarfile.open(archive, 'r:gz') as tar:
            members = {}
            for src in tar:
                if not src.isfile():
                    continue
                path = safe_path(src.name)
                if path.parts[0] != stem or len(path.parts) < 2:
                    raise ValueError(f"archive paths disagree with package name: {src.name}")
                rel = PurePosixPath(*path.parts[1:]).as_posix()
                if rel in members:
                    raise ValueError(f"duplicate path in {stem}: {rel}")
                members[rel] = src

            def read(rel):
                src = members[rel]
                if src.size > MAX_TEXT:
                    raise ValueError(f"text over bound: {stem}/{rel}")
                with tar.extractfile(src) as reader:
                    return reader.read()

            package = tomllib.loads(read('Cargo.toml').decode())['package']
            if (name, version) != (package['name'], package['version']):
                raise ValueError(f"archive manifest disagrees with lock: {stem}")
            record['license_expression'] = package.get('license')
            record['license_file'] = package.get('license-file')
            record['repository'] = package.get('repository')
            vcs = json.loads(read('.cargo_vcs_info.json')) if '.cargo_vcs_info.json' in members else {}
            record['commit'] = vcs.get('git', {}).get('sha1')
            record['path_in_vcs'] = vcs.get('path_in_vcs')
            paths = {p for p in members if NOTICE.fullmatch(PurePosixPath(p).name) or
                     any(segment.lower() in {'license', 'licenses', 'licences'} for segment in PurePosixPath(p).parts[:-1]) or
                     (name == 'epaint_default_fonts' and p.startswith('fonts/') and PurePosixPath(p).name in FONT)}
            if package.get('license-file') in members:
                paths.add(package['license-file'])
            for rel in sorted(paths):
                data = read(rel)
                if b'\0' in data:
                    raise ValueError(f"non-text notice: {stem}/{rel}")
                dest = DEST / 'archives' / stem / rel
                write_exact(dest, data)
                record['archive_notices'].append({"path": dest.relative_to(DEST).as_posix(), "sha256": digest(data)})
        records.append(record)
    return {"target": TARGET, "cargo_lock_sha256": digest((ROOT / 'Cargo.lock').read_bytes()), "crates": records}


def verify(inventory):
    if inventory['target'] != TARGET or inventory['cargo_lock_sha256'] != digest((ROOT / 'Cargo.lock').read_bytes()):
        raise ValueError('inventory and lock disagree')
    graph = registry_graph()
    by_key = {(r['crate'], r['version']): r['proc_macro'] for r in inventory['crates']}
    if by_key != graph or len(by_key) != len(inventory['crates']):
        raise ValueError('normal Cargo graph differs from inventory')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    checksums = {(p['name'], p['version']): p['checksum'] for p in lock['package'] if 'checksum' in p}
    archived_paths = set()
    for record in inventory['crates']:
        key = (record['crate'], record['version'])
        if checksums.get(key) != record['archive_sha256']:
            raise ValueError(f'archive digest disagreement in lock: {key}')
        for part in record['archive_notices']:
            relative = safe_path(part['path'])
            if relative in archived_paths or relative.parts[:2] != ('archives', '-'.join(key)):
                raise ValueError(f'bad or duplicate archive notice path: {relative}')
            archived_paths.add(relative)
            dest = DEST / relative
            if digest(dest.read_bytes()) != part['sha256']:
                raise ValueError(f'notice digest mismatch: {dest}')
    on_disk = {PurePosixPath(p.relative_to(DEST)) for p in (DEST / 'archives').rglob('*') if p.is_file()}
    if on_disk != archived_paths:
        raise ValueError('notices folder contains unindexed files')
    supplemental = {(p['crate'], p['version']): p for p in inventory['crates']
                    if not p['archive_notices'] or p['crate'] == 'epaint_default_fonts'}
    covered = set()
    verified_sources = set()
    sources = DEST / 'amont' / 'SOURCES.tsv'
    if sources.exists():
        with sources.open(newline='') as stream:
            for part in csv.DictReader(stream, delimiter='\t'):
                key = (part['crate'], part['version'])
                record = supplemental[key]
                repository = '/'.join(record['repository'].removeprefix('https://github.com/').split('/')[:2])
                rev = PUBLIC_RELEASE.get((repository, record['commit'], record['version']), record['commit'])
                prefix = f"https://raw.githubusercontent.com/{repository}/{rev}/"
                if not part['url'].lower().startswith(prefix.lower()):
                    raise ValueError(f"upstream source does not match its audited revision: {part['url']}")
                asset = DEST / 'amont' / safe_path(part['path relative to amont/'])
                if digest(asset.read_bytes()) != part['sha256']:
                    raise ValueError(f'upstream notice digest mismatch: {asset}')
                covered.add(key)
                verified_sources.add(part['url'])
    missing = supplemental.keys() - covered
    print(f"{len(graph)} registry crates, {sum(len(r['archive_notices']) for r in inventory['crates'])} archive notices, "
          f"{len(verified_sources)} pinned upstream references for {len(covered)} crates")
    if missing:
        raise ValueError(f"missing pinned code notices for: {sorted(missing)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['collect', 'verify'])
    parser.add_argument('--cargo-home', type=Path, default=Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')))
    parser.add_argument('--inventory', type=Path, default=DEST / 'inventory-wayland.json',
                        help='inventory file to create (collect) or verify; existing files are never overwritten')
    args = parser.parse_args()
    path = args.inventory if args.inventory.is_absolute() else ROOT / args.inventory
    if args.mode == 'collect':
        inventory = collect(args.cargo_home)
        write_exact(path, (json.dumps(inventory, indent=2, ensure_ascii=False) + '\n').encode())
    else:
        inventory = json.loads(path.read_text())
    verify(inventory)


if __name__ == '__main__':
    main()
