"""Extras: message-box icons, module-tab banners, OpenXER app icon.

Dialog icons are redrawn as a flat family (round error/info/question,
triangle warning). Module tabs (100x29) are small scenes in the same
vocabulary as the 16 px set. The app icon is OpenXER's own mark.
"""
from iconlib import icon, Icon, P


# ------------------------------------------------------------- dialog icons
def _disc(c: Icon, n, fill, dark):
    m = n / 16
    c.circle(n / 2, n / 2, n / 2 - m * .6, fill=dark)
    c.circle(n / 2, n / 2, n / 2 - m * 1.4, fill=fill)


def _glyph_i(c: Icon, n, col="paper"):
    c.circle(n / 2, n * .29, n * .07, fill=col)
    c.rrect(n * .44, n * .41, n * .56, n * .78, r=n * .03, fill=col)


def _glyph_q(c: Icon, n, col="paper"):
    w = n * .1
    c.arc(n * .33, n * .2, n * .67, n * .52, 190, 410, col, w)
    c.line([(n * .62, n * .46), (n * .5, n * .55), (n * .5, n * .63)], col, w, cap=False)
    c.circle(n / 2, n * .76, n * .065, fill=col)


def _glyph_x(c: Icon, n, col="paper"):
    c.cross(n / 2, n / 2, r=n * .2, color=col, width=n * .11)


def _warn(c: Icon, n):
    m = n / 16
    pts = [(n / 2, m * 1), (n - m * .6, n - m * 1.4), (m * .6, n - m * 1.4)]
    c.poly(pts, fill="yellow_d")
    inner = [(n / 2, m * 2.6), (n - m * 2.0, n - m * 2.3), (m * 2.0, n - m * 2.3)]
    c.poly(inner, fill="yellow")
    c.rrect(n * .455, n * .36, n * .545, n * .68, r=n * .02, fill="ink")
    c.circle(n / 2, n * .77, n * .055, fill="ink")


def _make_dialog_set(n, prefix_map):
    for name, kind in prefix_map.items():
        def fn(c, kind=kind):
            if kind == "error":
                _disc(c, n, "red", "red_d"); _glyph_x(c, n)
            elif kind == "info":
                _disc(c, n, "blue", "blue_d"); _glyph_i(c, n)
            elif kind == "question":
                _disc(c, n, "blue", "blue_d"); _glyph_q(c, n)
            else:
                _warn(c, n)
        icon(name, n, n)(fn)


# 50x50 classic message-box bitmaps (error has its own red disc).
_make_dialog_set(50, {"ERRORDG": "error", "INFODG": "info",
                      "QUESTNDG": "question", "WARNDG": "warning"})
# 64x64 modern message icons
_make_dialog_set(64, {"MSG_ERROR": "error", "MSG_INFO": "info",
                      "MSG_WARNING": "warning"})
# 12x12 in-cell validation glyphs
_make_dialog_set(12, {"CXEDIT_GLYPH_ERROR": "error", "CXEDIT_GLYPH_INFO": "info",
                      "CXEDIT_GLYPH_WARNING": "warning"})


# ------------------------------------------------------------- module tabs
W, H = 100, 29


def tab(name):
    return icon(name, W, H)


def _people(c, xs, colors, y=9):
    for x, col in zip(xs, colors):
        c.circle(x, y + 2, 2, fill="tan", outline="tan_d")
        c.rrect(x - 3, y + 4.5, x + 3, y + 13, r=1.5, fill=col, outline=P.get(col + "_d", P["ink2"]))
        c.vline(int(x), y + 10, y + 15, P.get(col + "_d", P["ink2"]))


def _org(c, cx, top, kids, node, gap=13, s=6):
    c.rect(cx - s / 2, top, cx + s / 2, top + s, fill=node[0], outline=node[1])
    c.vline(cx, top + s, top + s + 3, "ink2")
    xs = [cx + (i - (kids - 1) / 2) * gap for i in range(kids)]
    c.hline(xs[0], xs[-1] + 1, top + s + 3, "ink2")
    for x in xs:
        c.vline(x, top + s + 3, top + s + 6, "ink2")
        c.rect(x - s / 2, top + s + 6, x + s / 2, top + 2 * s + 6, fill=node[0], outline=node[1])


def _axes(c, x0=6, y0=3, x1=94, y1=26):
    c.vline(x0, y0, y1, "ink2")
    c.hline(x0, x1, y1 - 1, "ink2")
    for x in range(x0 + 12, x1, 14):
        c.vline(x, y1 - 3, y1 - 1, "ink2")


@tab("ATVMM")  # Activities: Gantt in a table frame
def _(c):
    c.rrect(2, 2, 98, 27, r=2, fill="paper", outline="ink2")
    c.rect(2, 2, 14, 27, fill="gray_xl"); c.vline(14, 2, 27, "gray")
    for y in (8, 14, 20):
        c.hline(3, 13, y, "gray_l")
    c.vline(42, 3, 26, "gray_l")
    c.bar(18, 5, 42, 10, "blue")
    c.bar(36, 12, 66, 17, "green")
    c.line([(66, 14.5), (72.5, 14.5), (72.5, 19)], "ink2", 1, cap=False)
    c.tri(72.5, 19.5, 3, "down", "ink2")
    c.bar(68, 20, 94, 25, "red")


@tab("DOCMNTMM")  # Work products & documents
def _(c):
    for i, x in enumerate((26, 22, 18)):
        c.doc(x, 4 + 2 * (2 - i), x + 18, 24 + 2 * (2 - i) - 2, fill="yellow_l", outline="yellow_d", lines=3, line_color="yellow_d")
    for i, x in enumerate((62, 56, 50)):
        c.doc(x, 3 + 2 * i, x + 22, 22 + 2 * i, lines=4)


@tab("EXPENSMM")  # Expenses: calculator + receipt + currency
def _(c):
    c.rrect(8, 4, 26, 26, r=2, fill="gray_l", outline="ink2")
    c.rect(11, 7, 23, 11, fill="green_l", outline="green_d")
    for r in range(3):
        for k in range(3):
            c.rect(11 + k * 4.3, 13 + r * 4, 13.5 + k * 4.3, 15 + r * 4, fill="ink2")
    c.rrect(30, 9, 60, 24, r=1.5, fill="green_l", outline="green_d")
    for y in (14, 18):
        c.hline(34, 56, y, "green_d")
    c.rrect(64, 6, 94, 26, r=2, fill="yellow_l", outline="yellow_d")
    # coin stack instead of currency glyphs
    for i, x in enumerate((72, 80, 88)):
        c.ellipse(x - 4, 12, x + 4, 22, fill="yellow", outline="yellow_d")
        c.vline(x, 14, 20, "yellow_d")


@tab("ISSUESMM")  # Issues: monitored threshold breach
def _(c):
    _axes(c, 6, 3, 52, 26)
    c.line([(8, 22), (22, 22), (32, 11), (50, 11)], "green", 2, cap=False)
    c.line([(8, 14.5), (50, 14.5)], "red", 1, cap=False)
    c.tri(58, 14, 6, "right", "ink2")
    for x, h in zip(range(66, 96, 5), (12, 18, 9, 20, 14, 16)):
        c.vline(x, 26 - h, 26, "gray")
    c.vline(80, 10, 26, "ink2")
    c.circle(80.5, 9, 3, fill="red", outline="red_d")


@tab("MTHLNKMM")  # Method linking: folder plugged into folders
def _(c):
    c.folder(6, 4, 36, 26, color="gray_l")
    c.line([(36, 13.5), (52, 13.5), (52, 9.5), (60, 9.5)], "green_d", 1.5, cap=False)
    c.rect(43, 11, 49, 16, fill="green", outline="green_d")
    c.line([(36, 20.5), (60, 20.5)], "green_d", 1.5, cap=False)
    c.rect(50, 18, 56, 23, fill="green", outline="green_d")
    c.folder(60, 3, 80, 15, color="gray_l")
    c.folder(66, 14, 92, 27, color="gray_l")


@tab("OBSMM")  # OBS: org chart of people
def _(c):
    c.circle(50, 5, 2.4, fill="tan", outline="tan_d")
    c.rrect(46, 7, 54, 11, r=1.5, fill="orange", outline="orange_d")
    c.vline(50, 11, 14, "ink2")
    c.hline(26, 75, 14, "ink2")
    for x, col in ((26, "blue"), (50, "orange"), (74, "blue")):
        c.vline(x, 14, 17, "ink2")
        c.circle(x, 19, 2.4, fill="tan", outline="tan_d")
        c.rrect(x - 4, 21, x + 4, 26, r=1.5, fill=col, outline=P.get(col + "_d", P["ink2"]))


@tab("PROJMM")  # Projects / EPS tree
def _(c):
    c.poly([(50, 2), (55, 6.5), (50, 11), (45, 6.5)], fill="yellow", outline="yellow_d")
    c.vline(50, 11, 15, "ink2")
    c.hline(28, 73, 15, "ink2")
    for x in (28, 50, 72):
        c.vline(x, 15, 18, "ink2")
        c.folder(x - 7, 18, x + 7, 27, color="gray_l")


@tab("REPRTSMM")  # Reports: layout feeding printed pages
def _(c):
    c.rrect(6, 4, 40, 26, r=2, fill="paper", outline="ink2")
    c.rect(6, 4, 14, 26, fill="gray_xl"); c.vline(14, 4, 26, "gray")
    c.bar(17, 9, 30, 13, "blue"); c.bar(24, 17, 37, 21, "green")
    for y in (11, 19):
        c.arrow(44, y, 54, y, "ink", 1.3, 3)
    for i, x in enumerate((70, 64, 58)):
        c.doc(x, 3 + 2 * i, x + 24, 22 + 2 * i, lines=4)


@tab("RISKSMM")  # Risks: arrow into a heat scale
def _(c):
    c.rrect(4, 5, 96, 25, r=2, fill="gray_xl", outline="ink2")
    for x in range(14, 60, 10):
        c.vline(x, 6, 24, "gray_l")
    c.arrow(8, 15, 60, 15, "blue", 4, 7)
    for i, col in enumerate(("yellow", "orange", "red")):
        c.rect(64 + i * 10, 7, 72 + i * 10, 23, fill=col, outline=P.get(col + "_d", P["ink2"]))


@tab("ROLESMM")  # Roles: crew row + pen
def _(c):
    _people(c, (22, 33, 44, 55), ("orange", "yellow", "blue", "gray"), y=7)
    c.line([(62, 22), (78, 6)], "green_d", 4, cap=False)
    c.line([(62, 22), (78, 6)], "green", 2.4, cap=False)
    c.poly([(59, 25), (61, 20.5), (63.5, 23)], fill="ink2")


@tab("RSRCPLMM")  # Resource planning: gears + crew
def _(c):
    import math
    for cx, cy, r, col in ((32, 11, 5.5, "yellow"), (26, 20, 4.5, "gray_l"), (38, 21, 4.5, "yellow")):
        teeth = []
        for k in range(16):
            a = k * math.pi / 8
            rr = r + (1.4 if k % 2 == 0 else 0)
            teeth.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
        c.poly(teeth, fill=col, outline=P.get(col + "_d", P["ink2"]) if col + "_d" in P else "ink2")
        c.circle(cx, cy, r * .35, fill="paper")
    _people(c, (56, 66, 76), ("blue", "blue", "blue"), y=7)


@tab("THRHLDMM")  # Thresholds: limit line breached
def _(c):
    _axes(c)
    c.line([(8, 14.5), (92, 14.5)], "red", 1, cap=False)
    for x in range(10, 92, 4):
        c.px(x, 14, "paper")
    c.line([(8, 22), (44, 22), (60, 9), (92, 9)], "green_d", 3, cap=False)
    c.line([(8, 22), (44, 22), (60, 9), (92, 9)], "green", 1.4, cap=False)


@tab("TRACKMM")  # Tracking: bar chart with trend line
def _(c):
    _axes(c, 6, 2, 92, 27)
    import random
    rnd = random.Random(7)
    cols = ("blue", "green", "teal")
    for i, x in enumerate(range(10, 86, 6)):
        h = 5 + i * 1.2 + rnd.randint(0, 5)
        col = cols[i % 3]
        c.rect(x, 26 - h, x + 4, 26, fill=col, outline=P.get(col + "_d", P["ink2"]))
    c.line([(10, 21), (30, 17), (44, 20), (60, 9), (72, 16), (88, 5)], "red_d", 2.4, cap=False)
    c.line([(10, 21), (30, 17), (44, 20), (60, 9), (72, 16), (88, 5)], "red", 1.2, cap=False)


@tab("WBSMM")  # WBS tree
def _(c):
    _org(c, 50, 2, 3, ("blue", "blue_d"), gap=22, s=8)


# ------------------------------------------------------------- app icon
@icon("MAINICON", 32, 32)
def _(c):
    # OpenXER mark: navy tile with a stepped Gantt and a link elbow.
    c.rrect(1, 1, 31, 31, r=6, fill="navy", outline="blue_d")
    c.rrect(6, 7, 18, 12, r=1.5, fill="blue_l")
    c.rrect(12, 14, 25, 19, r=1.5, fill="green")
    c.rrect(19, 21, 27, 26, r=1.5, fill="orange")
    c.line([(18, 9.5), (20.5, 9.5), (20.5, 13)], "paper", 1, cap=False)
    c.line([(25, 16.5), (26.5, 16.5), (26.5, 20)], "paper", 1, cap=False)
    c.vline(4, 6, 27, "blue_l")


# ------------------------------------------------------------- file glyph
@icon("UNKNOWNFILE", 16, 15)  # file of unknown type: blank page + '?'
def _(c):
    c.doc(3, 0, 13, 15, fold=3)
    c.arc(5.6, 3.6, 10.4, 8.4, 200, 400, "blue", 1.4)
    c.line([(9.6, 7.6), (8, 8.8), (8, 10.2)], "blue", 1.4, cap=False)
    c.rect(7.4, 11.4, 8.8, 12.8, fill="blue")
