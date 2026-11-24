# App icon

The penguin control-panel icon is the current application mark.

- `app-icon.svg` — editable vector source (rounded tile)
- `app-icon.png` — 1024 px master raster
- `app-mark.svg` — transparent penguin only, for the sidebar header

## Regenerate

```bash
# Platform icons (src-tauri/icons: PNG, ICO, ICNS)
# `tauri icon` also writes the mobile/Windows sets; this Linux build does not
# track them — regenerate on demand if you ever target those platforms.
npx tauri icon design/app-icon.png

# Browser favicon (public/index.html)
magick design/app-icon.png -resize 64x64 -depth 8 -strip public/favicon.png
# Sidebar mark (no tile: the accent gradient behind it comes from CSS)
magick -background none design/app-mark.svg -resize 512x512 -depth 8 -strip public/app-icon-mark.png
```
