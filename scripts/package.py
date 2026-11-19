#!/usr/bin/env python3
"""Package the mod as a source-only `.hnmod`, the file Horizon's launcher installs and builds.

The archive holds `horizon-native.json`, the root MIT licence and `mixins/voice_chat/` (its `.rs` and `.toml` files, the
engine crate and the vendored pure-Rust `opus-rs` included), with fixed dates and modes, so the
same sources always give the same bytes. `--install` also copies it into the launcher's native
mods folder.

Opus is pure Rust (vendored under `engine/vendor/opus-rs/`), so there is nothing to download and
no per-platform libopus binary to bundle.
"""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tomllib
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pack_resources  # type: ignore  # noqa: E402  (same scripts/ dir)

ROOT = Path(__file__).resolve().parents[1]
INSTALLED = Path.home() / '.local/share/horizon/horizon-native-mods'

# Files kept in the bundle besides `.rs`/`.toml` (e.g. the vendored Opus licence).
EXTRA_NAMES = {'COPYING', 'LICENSE', 'LICENSE.md'}


def package():
    compat = json.loads((ROOT / 'compatibility.json').read_text())
    bundle = ROOT / 'mixins' / 'voice_chat'
    mixin = tomllib.loads((bundle / 'horizon-mixin.toml').read_text())
    mod = mixin['mod']
    if mixin.get('compatibility', {}).get('horizon_revision') != compat['horizon_revision']:
        raise SystemExit('horizon-mixin.toml and compatibility.json pin different Horizon revisions')
    if (bundle / 'engine' / 'Cargo.lock').exists():
        raise SystemExit('remove mixins/voice_chat/engine/Cargo.lock: a mod crate builds on Horizon\'s lock')
    # Rebuild the bundled icon pack from assets/ first, so one command packages
    # everything (code + icons).
    resources = pack_resources.build()
    # No `hooks`: the mod reads game fields beyond the native hook points, so it
    # is pinned to the Horizon commit it was built and tested on.
    manifest = dict(schema=1, id=mod['id'], version=mod['version'], game=compat['game'],
                    horizon=compat['horizon_revision'], mixins=compat['horizon_mixins_revision'])
    files = {'horizon-native.json': (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()}
    files[f"mixins/{mod['id']}/LICENSE"] = (ROOT / 'LICENSE').read_bytes()
    for path in sorted(bundle.rglob('*')):
        if not path.is_file():
            continue
        parts = path.relative_to(bundle).parts
        # Skip Cargo's build output if it ever lands under the bundle.
        if 'target' in parts:
            continue
        # `.rs`/`.toml`, licence/readme text, and anything in a `data/` folder
        # (the bundled icon pack) — what a `.hnmod` is allowed to carry.
        in_data = 'data' in parts
        if path.suffix in ('.rs', '.toml') or path.name in EXTRA_NAMES or in_data:
            if path.is_symlink():
                raise SystemExit(f'{path.relative_to(ROOT)}: symlinks are not allowed in the bundle')
            files[path.relative_to(ROOT).as_posix()] = path.read_bytes()
    out = ROOT / 'target' / f"{mod['id']}-{mod['version']}.hnmod"
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)
    size = out.stat().st_size
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    print(f"Voice Chat {mod['version']} packaged")
    print(f"  Textures: {resources['assets']}")
    print(f"  Files:    {len(files)}")
    print(f"  Size:     {size / 1024:.1f} KiB")
    print(f"  Package:  {out.relative_to(ROOT)}")
    print(f"  SHA-256:  {digest}")
    if '--install' in sys.argv[1:]:
        INSTALLED.mkdir(parents=True, exist_ok=True)
        installed = INSTALLED / f"{mod['id']}.hnmod"
        shutil.copyfile(out, installed)
        print(f"  Installed: ~/{installed.relative_to(Path.home())}")


if __name__ == '__main__':
    package()
