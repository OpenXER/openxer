"""OpenXER icon drawing library.

Original icons drawn from vector primitives on a 16-unit grid. They follow
the usual meaning of each scheduling command (a bar for activities, a
folder for projects) in OpenXER's own palette and shapes.

Coordinates are in icon units: (0,0) is the top-left of a 16x16 icon, and
(16,16) is the bottom-right. An integer coordinate is a pixel edge, so
rect(2,3,14,9) fills pixel columns 2..13 and rows 3..8 exactly.
For a 1-unit stroke centered on a pixel, use half-unit coordinates (x+0.5).

Rendering is supersampled (SS x the output size) then box-downsampled, so
axis-aligned edges on integer coordinates stay crisp and diagonals get
anti-aliased.

Usage in an icon module:

    from iconlib import icon, P, Icon

    @icon("ADD16")
    def _(c: Icon):
        c.doc(3, 1, 13, 15)
"""
from __future__ import annotations

import math
from PIL import Image, ImageDraw

SS = 8  # supersample factor per output pixel

# ---------------------------------------------------------------- palette
# OpenXER palette: common colour *roles* (blue = planned bar, green =
# in progress, red = critical/complete, yellow = notes/folders) but our own
# hues. Every colour has a dark variant used for its outline.
P = {
    "ink": (38, 44, 52),         # main outline / black-ish
    "ink2": (92, 100, 112),      # secondary outline
    "gray": (150, 156, 166),
    "gray_l": (204, 208, 214),
    "gray_xl": (232, 234, 238),
    "paper": (255, 255, 255),
    "paper2": (246, 247, 249),

    "blue": (52, 110, 200),
    "blue_d": (28, 62, 130),
    "blue_l": (150, 186, 236),
    "navy": (30, 48, 110),

    "green": (58, 160, 82),
    "green_d": (28, 96, 46),
    "green_l": (160, 214, 170),

    "red": (210, 58, 52),
    "red_d": (130, 28, 26),
    "red_l": (240, 160, 152),

    "orange": (232, 120, 50),
    "orange_d": (150, 68, 22),
    "orange_l": (248, 196, 160),

    "yellow": (246, 206, 72),
    "yellow_d": (150, 116, 20),
    "yellow_l": (252, 236, 164),

    "teal": (36, 160, 170),
    "teal_d": (16, 96, 104),
    "teal_l": (150, 220, 226),

    "purple": (124, 72, 168),
    "purple_d": (70, 36, 104),
    "purple_l": (196, 170, 224),

    "tan": (226, 204, 150),
    "tan_d": (140, 112, 56),
}

# Colour family helper: fam("blue") -> (fill, dark, light)
def fam(name: str):
    return P[name], P.get(name + "_d", P["ink"]), P.get(name + "_l", P["paper"])


def _c(col):
    if col is None:
        return None
    if isinstance(col, str):
        col = P[col]
    if len(col) == 3:
        return tuple(col) + (255,)
    if col[3] != 255:
        # ImageDraw replaces pixels rather than blending, so a translucent
        # fill would punch a semi-transparent hole. Use a solid colour.
        raise ValueError(f"translucent colour {col} not supported; use a solid palette colour")
    return tuple(col)


class Icon:
    """A drawing canvas in 16-unit icon coordinates."""

    def __init__(self, w: int = 16, h: int = 16, scale: int = 1):
        self.w, self.h = w, h
        self.scale = scale
        self.k = SS * scale  # device px per icon unit
        self.img = Image.new("RGBA", (w * self.k, h * self.k), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.img)

    # ---- coordinate helpers
    def _p(self, x, y):
        return (x * self.k, y * self.k)

    def _box(self, x0, y0, x1, y1):
        k = self.k
        # Pillow treats the far edge inclusively; subtract one device px.
        return (x0 * k, y0 * k, x1 * k - 1, y1 * k - 1)

    def _lw(self, w):
        return max(1, int(round(w * self.k)))

    # ---- primitives
    def rect(self, x0, y0, x1, y1, fill=None, outline=None, width=1):
        """Filled rectangle on pixel edges; outline drawn *inside* the box."""
        if fill is not None:
            self.d.rectangle(self._box(x0, y0, x1, y1), fill=_c(fill))
        if outline is not None:
            w = width
            o = outline
            self.rect(x0, y0, x1, y0 + w, fill=o)
            self.rect(x0, y1 - w, x1, y1, fill=o)
            self.rect(x0, y0, x0 + w, y1, fill=o)
            self.rect(x1 - w, y0, x1, y1, fill=o)

    def rrect(self, x0, y0, x1, y1, r=1.5, fill=None, outline=None, width=1):
        k = self.k
        box = self._box(x0, y0, x1, y1)
        self.d.rounded_rectangle(box, radius=r * k, fill=_c(fill),
                                 outline=_c(outline),
                                 width=self._lw(width) if outline else 0)

    def ellipse(self, x0, y0, x1, y1, fill=None, outline=None, width=1):
        self.d.ellipse(self._box(x0, y0, x1, y1), fill=_c(fill),
                       outline=_c(outline),
                       width=self._lw(width) if outline else 0)

    def circle(self, cx, cy, r, fill=None, outline=None, width=1):
        self.ellipse(cx - r, cy - r, cx + r, cy + r, fill, outline, width)

    def poly(self, pts, fill=None, outline=None, width=1):
        pp = [self._p(x, y) for x, y in pts]
        if fill is not None:
            self.d.polygon(pp, fill=_c(fill))
        if outline is not None:
            self.line(list(pts) + [pts[0]], outline, width)

    def line(self, pts, color="ink", width=1, cap=True):
        """Polyline through icon-unit points (use .5 coords for 1px lines)."""
        pp = [self._p(x, y) for x, y in pts]
        lw = self._lw(width)
        self.d.line(pp, fill=_c(color), width=lw, joint="curve")
        if cap:
            r = lw / 2
            for (x, y) in (pp[0], pp[-1]):
                self.d.rectangle((x - r, y - r, x + r - 1, y + r - 1),
                                 fill=_c(color))

    def arc(self, x0, y0, x1, y1, start, end, color="ink", width=1):
        self.d.arc(self._box(x0, y0, x1, y1), start, end, fill=_c(color),
                   width=self._lw(width))

    def pie(self, x0, y0, x1, y1, start, end, fill):
        self.d.pieslice(self._box(x0, y0, x1, y1), start, end, fill=_c(fill))

    def hline(self, x0, x1, y, color="ink", t=1):
        """Horizontal pixel row(s): covers columns x0..x1-1, rows y..y+t-1."""
        self.rect(x0, y, x1, y + t, fill=color)

    def vline(self, x, y0, y1, color="ink", t=1):
        self.rect(x, y0, x + t, y1, fill=color)

    def px(self, x, y, color="ink"):
        self.rect(x, y, x + 1, y + 1, fill=color)

    def clear(self, x0, y0, x1, y1):
        """Punch a transparent hole (used under badges for separation)."""
        self.d.rectangle(self._box(x0, y0, x1, y1), fill=(0, 0, 0, 0))

    # ---- composites (shared vocabulary so the set reads as one family)
    def bar(self, x0, y0, x1, y1, color="blue"):
        """Gantt/activity bar: filled, dark outline, light top highlight."""
        f, dk, lt = fam(color)
        self.rect(x0, y0, x1, y1, fill=f, outline=dk)
        if y1 - y0 >= 4:
            self.hline(x0 + 1, x1 - 1, y0 + 1, lt)

    def doc(self, x0, y0, x1, y1, fold=3, fill="paper", outline="ink2",
            lines=None, line_color="gray"):
        """Page with a folded top-right corner. lines=n draws text lines."""
        f = fold
        pts = [(x0 + .5, y0 + .5), (x1 - f - .5, y0 + .5), (x1 - .5, y0 + f + .5),
               (x1 - .5, y1 - .5), (x0 + .5, y1 - .5)]
        self.poly(pts, fill=fill)
        self.poly([(x1 - f - .5, y0 + .5), (x1 - f - .5, y0 + f + .5),
                   (x1 - .5, y0 + f + .5)], fill="gray_l")
        self.line(pts + [pts[0]], outline)
        self.line([(x1 - f - .5, y0 + .5), (x1 - f - .5, y0 + f + .5),
                   (x1 - .5, y0 + f + .5)], outline)
        if lines:
            y = y0 + f + 2
            for i in range(lines):
                if y >= y1 - 1:
                    break
                right = x1 - 2 if i % 3 != 2 else x1 - 4
                self.hline(x0 + 2, right, y, line_color)
                y += 2

    def folder(self, x0, y0, x1, y1, color="yellow", open_=False):
        f, dk, lt = fam(color)
        tab_w = max(3, (x1 - x0) // 3)
        self.rect(x0, y0, x0 + tab_w, y0 + 2, fill=f, outline=None)
        self.line([(x0 + .5, y0 + 2), (x0 + .5, y0 + .5),
                   (x0 + tab_w - .5, y0 + .5), (x0 + tab_w + .5, y0 + 1.5)], dk)
        if not open_:
            self.rect(x0, y0 + 2, x1, y1, fill=f, outline=dk)
            self.hline(x0 + 1, x1 - 1, y0 + 3, lt)
        else:
            self.rect(x0, y0 + 2, x1 - 1, y1, fill=dk)
            self.rect(x0 + 1, y0 + 3, x1 - 2, y1 - 1, fill=lt)
            front = [(x0 + 2.5, y0 + 5.5), (x1 - .5, y0 + 5.5),
                     (x1 - 2.5, y1 - .5), (x0 + .5, y1 - .5)]
            self.poly(front, fill=f, outline=dk)

    def window(self, x0, y0, x1, y1, title="blue", body="paper"):
        """Small application window: title strip + body."""
        self.rect(x0, y0, x1, y1, fill=body, outline="ink2")
        self.rect(x0, y0, x1, y0 + 3, fill=title)
        self.rect(x0, y0, x1, y1, outline="ink2")

    def grid(self, x0, y0, x1, y1, cols=3, rows=3, head="gray_l",
             body="paper", lines="gray", first_col=None):
        """Table/spreadsheet: header row, optional shaded first column."""
        self.rect(x0, y0, x1, y1, fill=body)
        ch = (y1 - y0) / rows
        cw = (x1 - x0) / cols
        self.rect(x0, y0, x1, y0 + round(ch), fill=head)
        if first_col:
            self.rect(x0, y0, x0 + round(cw), y1, fill=first_col)
        for i in range(1, rows):
            self.hline(x0, x1, y0 + round(ch * i), lines)
        for j in range(1, cols):
            self.vline(x0 + round(cw * j), y0, y1, lines)
        self.rect(x0, y0, x1, y1, outline="ink2")

    def person(self, cx, top, color="blue", s=1.0):
        """Head-and-shoulders user glyph, ~8 units wide, 10 tall at s=1."""
        f, dk, lt = fam(color)
        hr = 2.2 * s
        self.circle(cx, top + hr + .3, hr, fill="tan", outline="tan_d")
        body = [(cx - 4 * s, top + 10 * s), (cx - 4 * s, top + 7.5 * s),
                (cx - 2 * s, top + 5.3 * s), (cx + 2 * s, top + 5.3 * s),
                (cx + 4 * s, top + 7.5 * s), (cx + 4 * s, top + 10 * s)]
        self.poly(body, fill=f, outline=dk)

    def arrow(self, x0, y0, x1, y1, color="ink", width=1.4, head=3.0):
        """Straight arrow from (x0,y0) to (x1,y1) with a filled head."""
        ang = math.atan2(y1 - y0, x1 - x0)
        bx, by = x1 - head * math.cos(ang), y1 - head * math.sin(ang)
        self.line([(x0, y0), (bx, by)], color, width, cap=False)
        nx, ny = -math.sin(ang) * head * .62, math.cos(ang) * head * .62
        self.poly([(x1, y1), (bx + nx, by + ny), (bx - nx, by - ny)], fill=color)

    def tri(self, cx, cy, size, direction="right", fill="ink", outline=None):
        """Solid triangle pointing up/down/left/right, centered at cx,cy."""
        s = size / 2
        pts = {
            "right": [(cx - s * .8, cy - s), (cx + s * .8, cy), (cx - s * .8, cy + s)],
            "left": [(cx + s * .8, cy - s), (cx - s * .8, cy), (cx + s * .8, cy + s)],
            "up": [(cx - s, cy + s * .8), (cx, cy - s * .8), (cx + s, cy + s * .8)],
            "down": [(cx - s, cy - s * .8), (cx, cy + s * .8), (cx + s, cy - s * .8)],
        }[direction]
        self.poly(pts, fill=fill, outline=outline)

    def plus(self, cx, cy, r=3, color="green", t=2, outline=None):
        """Plus sign. With outline, draws a 1px dark rim (badge style)."""
        if outline:
            self.rect(cx - r - 1, cy - t / 2 - 1, cx + r + 1, cy + t / 2 + 1, fill=outline)
            self.rect(cx - t / 2 - 1, cy - r - 1, cx + t / 2 + 1, cy + r + 1, fill=outline)
        self.rect(cx - r, cy - t / 2, cx + r, cy + t / 2, fill=color)
        self.rect(cx - t / 2, cy - r, cx + t / 2, cy + r, fill=color)

    def minus(self, cx, cy, r=3, color="red", t=2, outline=None):
        if outline:
            self.rect(cx - r - 1, cy - t / 2 - 1, cx + r + 1, cy + t / 2 + 1, fill=outline)
        self.rect(cx - r, cy - t / 2, cx + r, cy + t / 2, fill=color)

    def check(self, x0, y0, s=1.0, color="green", width=2.0):
        """Check mark inside a box (x0,y0)..(x0+8s,y0+7s)."""
        self.line([(x0 + 1 * s, y0 + 3.5 * s), (x0 + 3.2 * s, y0 + 6 * s),
                   (x0 + 7.5 * s, y0 + .8 * s)], color, width, cap=False)

    def cross(self, cx, cy, r=3, color="red", width=1.8):
        self.line([(cx - r, cy - r), (cx + r, cy + r)], color, width, cap=False)
        self.line([(cx - r, cy + r), (cx + r, cy - r)], color, width, cap=False)

    # ---- badges (bottom-right overlays that modify a base icon)
    def badge_current(self, x0=9, y0=9):
        """'Current/selected' overlay: small framed tile with a pointer.
        A blue rounded tile with a white pointer arrow."""
        self.clear(x0 - 1, y0 - 1, 16, 16)
        self.rrect(x0, y0, 16, 16, r=1.5, fill="blue", outline="blue_d")
        s = (16 - x0) / 7
        self.poly([(x0 + 1.8 * s, y0 + 1.6 * s), (x0 + 5.6 * s, y0 + 3.6 * s),
                   (x0 + 3.9 * s, y0 + 4.1 * s), (x0 + 3.4 * s, y0 + 5.8 * s)],
                  fill="paper")

    def badge_plus(self, cx=12, cy=12, r=3, color="green"):
        self.plus(cx, cy, r=r, color=color, outline=P[color + "_d"])

    def badge_minus(self, cx=12, cy=12, r=3, color="red"):
        self.minus(cx, cy, r=r, color=color, outline=P[color + "_d"])

    def badge_check(self, x0=8, y0=8):
        self.clear(x0, y0, 16, 16)
        self.rrect(x0, y0, 16, 16, r=1.5, fill="green", outline="green_d")
        s = (16 - x0) / 8
        self.check(x0 + .3 * s, y0 + .8 * s, s=s, color="paper", width=1.4)

    def badge_whatif(self, x0=9, y0=9):
        """'What-if' marker: amber diamond with a question tick."""
        cx, cy = (x0 + 16) / 2, (y0 + 16) / 2
        r = (16 - x0) / 2
        self.poly([(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)],
                  fill="orange", outline="orange_d")
        self.rect(cx - .5, cy - r * .5, cx + .5, cy + r * .15, fill="paper")
        self.rect(cx - .5, cy + r * .35, cx + .5, cy + r * .6, fill="paper")

    def badge_lock(self, x0=9, y0=8):
        self.arc(x0 + 1.2, y0, x0 + 5.8, y0 + 5, 180, 360, "ink2", 1.1)
        self.rect(x0, y0 + 3, x0 + 7, y0 + 8, fill="yellow", outline="yellow_d")

    # ---- output
    def render(self) -> Image.Image:
        out_w, out_h = self.w * self.scale, self.h * self.scale
        # Premultiplied box downsample so transparent edges do not darken.
        from PIL import ImageChops
        r, g, b, a = self.img.split()
        prem = [ImageChops.multiply(ch, a).resize((out_w, out_h), Image.BOX)
                for ch in (r, g, b)]
        al = a.resize((out_w, out_h), Image.BOX)
        pr, pg, pb, pa = (ch.load() for ch in (*prem, al))
        out = Image.new("RGBA", (out_w, out_h))
        po = out.load()
        for y in range(out_h):
            for x in range(out_w):
                v = pa[x, y]
                po[x, y] = (0, 0, 0, 0) if v == 0 else (
                    min(255, pr[x, y] * 255 // v), min(255, pg[x, y] * 255 // v),
                    min(255, pb[x, y] * 255 // v), v)
        return out


# ---------------------------------------------------------------- registry
REGISTRY: dict[str, tuple] = {}


def icon(name: str, w: int = 16, h: int = 16):
    """Register a drawing function under its icon name, e.g. 'ADD16'.

    The name is also the PNG file name under assets/icons/16 and /32.
    """
    def deco(fn):
        if name in REGISTRY:
            raise ValueError(f"duplicate icon {name}")
        REGISTRY[name] = (fn, w, h)
        return fn
    return deco


def draw(name: str, scale: int = 1) -> Image.Image:
    fn, w, h = REGISTRY[name]
    c = Icon(w, h, scale)
    fn(c)
    return c.render()
