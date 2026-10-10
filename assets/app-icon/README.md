# ArtCraft Launcher app icon

A dark rounded tile holding a 3×3 grid of smaller tiles in the Craft apps' colours: the suite of
apps. The middle tile is paper-coloured with an upward launch arrow. It is a plain geometric mark
rather than an engraved mascot (`../craftrules/standards/icon-design.md`), because the launcher
stands for the whole suite rather than one app.

## Palette

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` → `#26282e` | the tile (vertical gradient) and the arrow |
| Paper | `#efe9dc` | the centre tile |
| PhotoCraft, VectorCraft, FilmCraft, EffectCraft | `#2f7bf5`, `#e8573f`, `#8b5cf6`, `#e0368f` | top row, middle left |
| LightCraft, DesignCraft, GridCraft, PdfCraft | `#f2a516`, `#7bb51c`, `#1f9d55`, `#12a58a` | middle right, bottom row |

## Geometry

A 512-unit tile (`viewBox="0 0 512 512"`), rounded square with `rx=112`. Inner tiles are 104
units with 24-unit gaps (`rx=26`). The macOS renders pad it onto Apple's 824/1024 icon grid; the
Windows and Linux renders keep the whole tile with a hairline margin.

## Files

- `artcraft-launcher.svg`: canonical master; also the hicolor scalable icon.
- `artcraft-launcher-1024.png`: 1024 px render on the macOS grid.
- `artcraft-launcher.icns`: macOS bundle icon (`CFBundleIconFile`).
- `artcraft-launcher.ico`: Windows icon, 16–256 px, embedded in the `.exe` by
  `apps/artcraft-launcher/build.rs` and used by the MSI.
- `hicolor/<size>/apps/ai.storyteller.artcraft-launcher.png` (16–512) and
  `hicolor/scalable/apps/ai.storyteller.artcraft-launcher.svg`: Linux icon theme.
- `tray/tray-template.svg` → `tray/tray-template.png` (44×44, black on transparent): the macOS
  menu-bar template image (22 pt @2x). Three squares and an arrow.
- `tray/tray-color.svg` → `tray/tray-color.png` (64×64): the tray icon on Windows, Linux and FreeBSD.

## Regenerate

Edit the SVGs, then run `packaging/icons.sh`. It needs `resvg`, plus `iconutil` on macOS; the
`.ico` is packed by `cargo xtask ico`.
