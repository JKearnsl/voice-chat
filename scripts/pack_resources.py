#!/usr/bin/env python3
"""Build the icon resource pack.

A native `.hnmod` can only carry `.rs`/`.toml`/data, so the mod's GUI textures
(`voice_chat:gui/icon_*`, drawn with `native_sprite`) ship as an ordinary
Minecraft resource pack: `assets/voice_chat/textures/gui/*.png` plus a
`pack.mcmeta`.

The pack is written into the bundle's `data/` folder so it travels inside the
`.hnmod`; the mod's post-build step (`install_icons`) then installs and turns it
on automatically — one image to add, nothing else to handle.

The pack's own `pack.png` (the icon shown in the resource-pack list) is the
mod's branding logo from `design/marketplace/`.
"""
import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'assets'
BUNDLE_DATA = ROOT / 'mixins' / 'voice_chat' / 'data'
BRAND_ICON = ROOT / 'design' / 'marketplace' / 'icon_512.png'
# Horizon's RESOURCE_PACK_FORMAT for 26.3 is (97, 1). Format 97 is past the
# last pre-minor version (64), so the pack MUST declare min_format/max_format,
# not just pack_format, or the game marks it incompatible. min 97 -> (97, 0) and
# max 97 -> (97, i32::MAX) span the current (97, 1).
PACK_FORMAT = 97

MCMETA = {
    'pack': {
        'description': 'Voice Chat — proximity voice chat icons',
        'pack_format': PACK_FORMAT,
        'min_format': PACK_FORMAT,
        'max_format': PACK_FORMAT,
    }
}


def pack_icon():
    """The branding logo as a 128x128 `pack.png`, pixel-art-crisp. Falls back
    to the original bytes if Pillow isn't available."""
    if not BRAND_ICON.is_file():
        return None
    try:
        from PIL import Image
        import io
        nearest = getattr(getattr(Image, 'Resampling', Image), 'NEAREST')
        img = Image.open(BRAND_ICON).convert('RGBA').resize((128, 128), nearest)
        buf = io.BytesIO()
        img.save(buf, format='PNG')
        return buf.getvalue()
    except Exception:
        return BRAND_ICON.read_bytes()


def build():
    BUNDLE_DATA.mkdir(parents=True, exist_ok=True)
    bundled = BUNDLE_DATA / 'voice_chat-icons.zip'
    pngs = sorted(ASSETS.rglob('*.png'))
    if not pngs:
        raise SystemExit('no PNGs under assets/ to pack')
    icon = pack_icon()
    with zipfile.ZipFile(bundled, 'w', compression=zipfile.ZIP_DEFLATED) as z:
        info = zipfile.ZipInfo('pack.mcmeta', (2026, 1, 1, 0, 0, 0))
        info.external_attr = 0o100644 << 16
        z.writestr(info, json.dumps(MCMETA, indent=2) + '\n')
        if icon is not None:
            info = zipfile.ZipInfo('pack.png', (2026, 1, 1, 0, 0, 0))
            info.external_attr = 0o100644 << 16
            z.writestr(info, icon)
        for png in pngs:
            rel = png.relative_to(ROOT).as_posix()  # assets/voice_chat/textures/gui/*.png
            info = zipfile.ZipInfo(rel, (2026, 1, 1, 0, 0, 0))
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, png.read_bytes())
    return {'bundled': bundled, 'assets': len(pngs), 'size': bundled.stat().st_size}


if __name__ == '__main__':
    result = build()
    print('Resource pack built')
    print(f"  Textures: {result['assets']}")
    print(f"  File:     {result['bundled'].relative_to(ROOT)} ({result['size'] / 1024:.1f} KiB)")
