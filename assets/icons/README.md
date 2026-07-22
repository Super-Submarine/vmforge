# VMForge app icon set (wave-1)

Production brand/app icons for the Linux wave-1 release (.deb + .AppImage). The mark is a
git-style snapshot-branch glyph — VMForge's snapshot-tree identity — in
`color.accent.emphasis` `#0F62FE` from Visual Design System v1 (company doc
`019f8a53-2ce6-7c9a-9587-e6a7f250b962`), designed per Design Spec v2 §5
(doc `019f8ab2-4df2-79c2-ab4c-928041f5ad2f`).

## Files

| File | Purpose |
|---|---|
| `vmforge.svg` | **SVG master** — single source of truth; 512×512 canvas, ~8% padding, edit this only |
| `vmforge-symbolic.svg` | Single-color (`#FFFFFF`) symbolic variant for GNOME dark panels/trays |
| `png/vmforge-{16,32,48,64,128,256,512}.png` | Renders from the master (`rsvg-convert -w N -h N vmforge.svg`) — never scale one PNG to another size |

Regenerate PNGs after editing the master:

```sh
for s in 16 32 48 64 128 256 512; do
  rsvg-convert -w $s -h $s vmforge.svg -o png/vmforge-${s}.png
done
```

## Usage — .desktop entry (deb / system install)

Per the freedesktop icon theme spec (https://specifications.freedesktop.org/icon-theme-spec/latest/)
install into the **hicolor** theme and reference by bare name from the `.desktop` file
(desktop entry spec: https://specifications.freedesktop.org/desktop-entry-spec/latest/):

```
/usr/share/icons/hicolor/16x16/apps/vmforge.png    <- png/vmforge-16.png
/usr/share/icons/hicolor/32x32/apps/vmforge.png    <- png/vmforge-32.png
/usr/share/icons/hicolor/48x48/apps/vmforge.png    <- png/vmforge-48.png   (mandatory minimum size)
/usr/share/icons/hicolor/64x64/apps/vmforge.png    <- png/vmforge-64.png
/usr/share/icons/hicolor/128x128/apps/vmforge.png  <- png/vmforge-128.png
/usr/share/icons/hicolor/256x256/apps/vmforge.png  <- png/vmforge-256.png
/usr/share/icons/hicolor/512x512/apps/vmforge.png  <- png/vmforge-512.png
/usr/share/icons/hicolor/scalable/apps/vmforge.svg <- vmforge.svg
```

In `vmforge.desktop`: `Icon=vmforge` — bare name, **no path, no extension**.
Deb postinst should run `gtk-update-icon-cache /usr/share/icons/hicolor || true`.
Validate with `desktop-file-validate vmforge.desktop`
(https://www.freedesktop.org/wiki/Software/desktop-file-utils/).

## Usage — AppImage embedding

Per the AppDir reference (https://docs.appimage.org/reference/appdir.html), the AppDir needs:

1. `vmforge.png` at the AppDir **root** (use `png/vmforge-256.png` or larger);
2. `.DirIcon` at the AppDir root (copy of the same PNG);
3. the same hicolor tree as above under `AppDir/usr/share/icons/hicolor/…` so
   appimaged/thumbnailers resolve the icon;
4. `vmforge.desktop` at the AppDir root with `Icon=vmforge` (bare name).

## Usage — Tauri bundle

List the rendered PNGs (32, 128, 128@2x) in `tauri.conf.json > bundle > icon`
(https://tauri.app/develop/icons/); keep this directory as the source, do not
hand-edit copies under `gui/src-tauri/icons/`.

## Design constraints (from Design Spec v2 §5.2)

- No text in the mark (illegible ≤48 px, untranslatable); silhouette-first, ≤2 shape layers.
- ~8% padding on the square canvas (GNOME HIG app-icon grid:
  https://developer.gnome.org/hig/guidelines/app-icons.html).
- Holds on both light `#FFFFFF` and dark `#0D1117` desktop themes.
- One identity everywhere — never restyle per-distro.
