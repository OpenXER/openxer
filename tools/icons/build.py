"""Render every registered OpenXER icon to assets/icons/{16,32}/<NAME>.png
(16 = 1x native size, 32 = 2x for HiDPI; non-16px icons keep their own size)
and write a review sheet per icon module (each icon enlarged and at 1x).

    python build.py            # all modules
    python build.py set_a      # one module (also writes its review sheet)
"""
import importlib, os, sys, pathlib
from PIL import Image, ImageDraw, ImageFont
import iconlib

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]                       # openxer/
OUT = ROOT / "assets" / "icons"
SHEETS = pathlib.Path(os.environ.get("ICON_SHEETS", HERE / "_review"))

def modules(args):
    if args:
        return args
    return sorted(p.stem for p in HERE.glob("set_*.py"))

def sheet(mod, names):
    S, cols = 5, 6
    cw, ch = 16 * S + 40 + 32, 16 * S + 22
    rows = (len(names) + cols - 1) // cols
    sh = Image.new("RGB", (cw * cols, ch * rows), (236, 236, 236))
    d = ImageDraw.Draw(sh)
    f = ImageFont.truetype("arial.ttf", 11)
    for i, n in enumerate(names):
        x, y = (i % cols) * cw + 4, (i // cols) * ch + 4
        ours = iconlib.draw(n)
        k = max(1, (16 * S) // max(ours.width, ours.height))
        o = ours.resize((ours.width * k, ours.height * k), Image.NEAREST)
        sh.paste(o, (x, y), o)
        if max(ours.size) <= 32:
            sh.paste(ours, (x + 16 * S + 8, y), ours)
        d.text((x, y + 16 * S + 3), n[:30], fill=0, font=f)
    SHEETS.mkdir(parents=True, exist_ok=True)
    p = SHEETS / f"{mod}.png"
    sh.save(p)
    return p

def main():
    mods = modules(sys.argv[1:])
    for d in ("16", "32"):
        (OUT / d).mkdir(parents=True, exist_ok=True)
    for m in mods:
        before = set(iconlib.REGISTRY)
        importlib.import_module(m)
        names = [n for n in iconlib.REGISTRY if n not in before]
        for n in names:
            iconlib.draw(n, 1).save(OUT / "16" / f"{n}.png")
            iconlib.draw(n, 2).save(OUT / "32" / f"{n}.png")
            if n == "MAINICON":  # Windows app icon, multi-resolution
                big = iconlib.draw(n, 8)
                big.save(OUT / "openxer.ico",
                         sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (256, 256)])
                big.save(OUT / "openxer_256.png")
        p = sheet(m, names)
        print(f"{m}: {len(names)} icons -> review sheet {p}")

if __name__ == "__main__":
    main()
