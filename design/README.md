# App icon

The penguin control-panel icon is the current application mark.

- `app-icon.svg` — editable vector source (rounded tile)
- `app-icon.png` — 1024 px master raster
- `app-mark.svg` — transparent penguin only, for the sidebar header

## Regenerate

```bash
# Platform icons (src-tauri/icons: PNG, ICO, ICNS, iOS, Android)
npx tauri icon design/app-icon.png

# Browser / PWA icons used by public/index.html and the sidebar
for size in 32 64 128 256 512; do
  magick design/app-icon.png -resize "${size}x${size}" -depth 8 -strip "public/icons/icon-${size}.png"
done
magick design/app-icon.png -resize 512x512 -depth 8 -strip public/app-icon.png
magick design/app-icon.png -resize 64x64 -depth 8 -strip public/favicon.png
# Sidebar mark (no tile: the accent gradient behind it comes from CSS)
magick -background none design/app-mark.svg -resize 512x512 -depth 8 -strip public/app-icon-mark.png
```
