# OpenXER icons

These are original icons for OpenXER, drawn in code from vector
primitives. The build reads no bitmap from another program: every PNG
under `assets/icons/` is written by `build.py` from the drawings in this
folder.

- `iconlib.py` holds the shared palette, the drawing primitives and the
  composite shapes (bar, doc, folder, badges). It draws on a 16-unit grid,
  supersampled, so the same drawing renders at any scale.
- `set_a.py`, `set_b.py` and `set_c.py` hold the 271 small 16 px command,
  tree and grid icons.
- `set_d.py` holds main-window toolbar icons that had no drawing yet
  (line numbers, progress spotlight, progress line, tab groups, what's
  new, online help) and the Link and Unlink Activities commands of the
  table context menu. Their names start with `OX`.
- `set_x.py` holds the extras: the dialog/message icons, the 14 module tab
  banners (100x29), UNKNOWNFILE, and the OpenXER app icon (MAINICON).
- Each icon is registered under a short name that is also its file name,
  for example `SAVE16` writes `SAVE16.png`.

Build with Python 3 and Pillow:

    python build.py            # every set
    python build.py set_b      # one set

The build writes to `assets/icons/`:

- `16/` holds 1x PNGs.
- `32/` holds 2x PNGs for HiDPI.
- `openxer.ico` is the multi-size app icon.
- `openxer_256.png` is a 256 px version of the app icon.

It also writes a review sheet for each set to `_review/` (each icon
enlarged and at 1x).

Not drawn: control internals (grid, toolbar and chart widgets), docking
guides, skins, cursors and splash or wizard art. egui draws its own
controls.
