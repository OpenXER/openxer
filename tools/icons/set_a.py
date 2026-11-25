"""OpenXER icons, set A (ACCUNT16 .. HLPCNT16).

Original drawings for OpenXER's commands in OpenXER's palette and shape vocabulary (see iconlib.py).
"""
from iconlib import icon, Icon, P, fam


# ---------------------------------------------------------------- local helpers
def flag(c: Icon, x=11, y=3):
    """Progress pennant used on activity-type icons (pole + yellow pennant)."""
    c.clear(x - 1, y - 1, 16, 16)
    c.vline(x, y, 16, "ink2")
    c.poly([(x + 1, y), (16, y + 3), (x + 1, y + 6)], fill="yellow")
    c.line([(x + 1, y + .5), (15.5, y + 3), (x + 1, y + 5.5)], "yellow_d")


def oline(c: Icon, pts, color="ink"):
    """Orthogonal polyline snapped to whole pixels (no anti-aliasing).
    A .5 coordinate names the pixel it sits in; an integer is a pixel edge."""
    import math
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        xa, xb = sorted((x0, x1)); ya, yb = sorted((y0, y1))
        c.rect(math.floor(xa), math.floor(ya),
               max(math.floor(xa) + 1, math.ceil(xb)),
               max(math.floor(ya) + 1, math.ceil(yb)), fill=color)


def head_down(c: Icon, col, y, n=3, color="ink"):
    """Pixel-stepped arrowhead pointing down; tip in column `col`, top row y."""
    y = int(y)
    for i in range(n):
        c.hline(col - (n - 1 - i), col + (n - i), y + i, color)


def head_right(c: Icon, x, row, n=3, color="ink"):
    x = int(x)
    for i in range(n):
        c.vline(x + i, row - (n - 1 - i), row + (n - i), color)


def node(c: Icon, x0, y0, s=5, color="yellow"):
    f, dk, lt = fam(color)
    c.rrect(x0, y0, x0 + s, y0 + s, r=1, fill=lt, outline=dk)


def qmark(c: Icon, cx, top, color="blue_d"):
    """Bold pixel question mark, 6 wide x 8 tall, centred on column cx."""
    c.rect(cx - 2, top, cx + 2, top + 1, fill=color)
    c.rect(cx - 3, top + 1, cx - 1, top + 3, fill=color)
    c.rect(cx + 1, top + 1, cx + 3, top + 3, fill=color)
    c.rect(cx, top + 3, cx + 2, top + 4, fill=color)
    c.rect(cx - 1, top + 4, cx + 1, top + 6, fill=color)
    c.rect(cx - 1, top + 7, cx + 1, top + 8, fill=color)


def bubble(c: Icon, x0, y0, x1, y1, tail_x):
    """Speech bubble with a tail at the bottom-left."""
    c.poly([(tail_x, y1 - 1), (tail_x + 4, y1 - 1), (tail_x, y1 + 3)],
           fill="yellow", outline="yellow_d")
    c.rrect(x0, y0, x1, y1, r=2, fill="yellow", outline="yellow_d")
    c.rect(tail_x + 1, y1 - 1, tail_x + 3, y1, fill="yellow")


def help_disc(c: Icon):
    c.circle(8, 8, 7.5, fill="yellow", outline="yellow_d")
    c.arc(2, 2, 14, 14, 200, 290, "yellow_l", 1)
    qmark(c, 8, 4)


def two_cards(c: Icon):
    c.rrect(0, 0, 10, 10, r=1.5, fill="yellow_l", outline="yellow_d")
    c.hline(2, 8, 2, "yellow")
    c.rrect(3, 5, 15, 14, r=1.5, fill="gray_l", outline="ink2")
    c.hline(4, 14, 6, "paper")
    c.hline(5, 13, 9, "gray")
    c.hline(5, 11, 11, "gray")


def org(c: Icon, root_x):
    """Root node plus three children; root_x is the root's left edge."""
    node(c, root_x, 1)
    rc = root_x + 2.5
    oline(c, [(rc, 6), (rc, 8.5)], "ink2")
    oline(c, [(3.5, 10), (3.5, 8.5), (13.5, 8.5), (13.5, 10)], "ink2")
    oline(c, [(8.5, 8.5), (8.5, 10)], "ink2")
    for x in (1, 6, 11):
        node(c, x, 10, s=4)


def scroll(c: Icon, x0, y0, x1, y1, w=4.5):
    c.line([(x0, y1), (x1, y0)], "tan_d", w + 1.4, cap=False)
    c.line([(x0, y1), (x1, y0)], "tan", w, cap=False)
    c.circle(x0, y1, w / 2 + .3, fill="tan", outline="tan_d")
    c.circle(x1, y0, w / 2 + .3, fill="tan", outline="tan_d")
    c.circle(x1, y0, .9, fill="tan_d")
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    c.line([(mx - 2.2, my - 2.2), (mx + 2.2, my + 2.2)], "red", 2.4, cap=False)
    c.poly([(mx + .5, my + .5), (mx + 4.5, my + 1.5), (mx + 3, my + 3)], fill="red")
    c.poly([(mx + .5, my + .5), (mx + 1.5, my + 4.5), (mx + 3, my + 3)], fill="red_d")


def drive(c: Icon):
    c.rrect(0, 1, 16, 11, r=1.5, fill="gray_l", outline="ink2")
    c.hline(1, 15, 2, "paper")


def panel_boxes(c: Icon, x0, y0, x1, y1, vertical):
    c.rrect(x0, y0, x1, y1, r=1, fill="gray_l", outline="ink2")
    if vertical:
        c.rect(x0 + 2, y0 + 2, x1 - 2, y0 + 6, fill="yellow", outline="yellow_d")
        c.rect(x0 + 2, y1 - 6, x1 - 2, y1 - 2, fill="yellow", outline="yellow_d")
    else:
        c.rect(x0 + 2, y0 + 2, x0 + 6, y1 - 2, fill="yellow", outline="yellow_d")
        c.rect(x1 - 6, y0 + 2, x1 - 2, y1 - 2, fill="yellow", outline="yellow_d")


# ---------------------------------------------------------------- activities
@icon("ACCUNT16")
def _(c: Icon):
    two_cards(c)


@icon("ACTCUR16")
def _(c: Icon):
    two_cards(c)
    c.badge_current()


@icon("ATV16")
def _(c: Icon):
    c.bar(1, 4, 15, 11, "green")


@icon("ATVCPT16")
def _(c: Icon):
    c.bar(1, 4, 15, 11, "blue")


@icon("ACTCTP16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "blue")
    flag(c, 10, 6)


@icon("ACTINP16")
def _(c: Icon):
    c.bar(1, 4, 15, 11, "blue")
    c.rect(8, 5, 14, 10, fill="green")
    c.hline(8, 14, 5, "green_l")
    c.vline(8, 4, 11, "blue_d")


@icon("ACTIPP16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "blue")
    c.rect(7, 3, 13, 8, fill="green")
    c.hline(7, 13, 3, "green_l")
    c.vline(7, 2, 9, "blue_d")
    flag(c, 10, 6)


@icon("ACTPRM16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "green")
    flag(c, 10, 6)


@icon("ACTRJ16")
def _(c: Icon):
    c.bar(1, 4, 15, 11, "red")


@icon("ACTRJP16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "red")
    flag(c, 10, 6)


@icon("ATVCDE16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "blue")
    c.clear(7, 7, 16, 16)
    tag = [(9, 8.5), (13, 8.5), (15.5, 11), (12, 15.5), (8.5, 12)]
    c.poly(tag, fill="yellow_l", outline="yellow_d")
    c.px(10, 10, "yellow_d")


@icon("ATVDET16")
def _(c: Icon):
    c.rrect(1, 2, 15, 14, r=1.5, fill="gray_xl", outline="ink2")
    c.rect(8, 3, 14, 6, fill="gray")
    c.hline(3, 7, 8, "gray")
    c.hline(3, 12, 10, "gray")


@icon("ATVGRD16")
def _(c: Icon):
    c.grid(1, 1, 15, 15, cols=4, rows=5, head="blue_l", first_col="blue_l")


@icon("ATVHST16")
def _(c: Icon):
    for x0, top in ((1, 1), (5, 5), (10, 8)):
        c.rect(x0, top, x0 + 3, 13, fill="blue_l", outline="blue_d")
    c.bar(3, 9, 16, 15, "green")


@icon("ATVLKC16")
def _(c: Icon):
    c.rect(1, 4, 15, 11, fill="orange_l", outline="orange_d")
    c.hline(2, 14, 5, "paper")


@icon("ATVLKR16")
def _(c: Icon):
    c.bar(0, 5, 13, 12, "orange")
    c.clear(8, 0, 16, 8)
    c.rrect(9, 0, 16, 7, r=1.5, fill="paper", outline="ink2")
    for i, w in enumerate((3, 2, 1)):
        c.hline(11, 11 + w, 2 + i, "red")
    c.px(12, 3, "red")
    c.px(13, 4, "red")


@icon("ATVNET16")
def _(c: Icon):
    oline(c, [(6, 3.5), (10, 3.5)], "ink2")
    oline(c, [(8.5, 3.5), (8.5, 12.5), (6, 12.5)], "ink2")
    for x, y in ((1, 1), (10, 1), (1, 10)):
        c.rrect(x, y, x + 5, y + 5, r=1, fill="yellow", outline="yellow_d")


@icon("ATVNWT16")
def _(c: Icon):
    oline(c, [(1, 1.5), (6.5, 1.5), (6.5, 3)], "ink")
    head_down(c, 6, 2)
    c.rrect(3, 5, 13, 11, r=1, fill="yellow", outline="yellow_d")
    oline(c, [(9.5, 11), (9.5, 14.5), (13, 14.5)], "ink")
    head_right(c, 13, 14)


@icon("ATVPRD16")
def _(c: Icon):
    c.bar(0, 2, 14, 9, "blue")
    oline(c, [(7.5, 9), (7.5, 13.5), (12, 13.5)], "ink")
    head_right(c, 12, 13)


@icon("ATVSUC16")
def _(c: Icon):
    oline(c, [(3, 1.5), (9.5, 1.5), (9.5, 4)], "ink")
    head_down(c, 9, 4)
    c.bar(1, 7, 15, 13, "blue")


@icon("ATVSPD16")
def _(c: Icon):
    c.grid(1, 1, 13, 13, cols=4, rows=4, head="blue_l", first_col="blue_l")
    c.clear(3, 8, 16, 16)
    c.bar(4, 9, 16, 15, "green")


@icon("ASSIGN16")
def _(c: Icon):
    c.grid(0, 1, 13, 12, cols=3, rows=4)
    c.clear(7, 7, 16, 16)
    c.badge_plus(12, 12, r=3)


@icon("ADD16")
def _(c: Icon):
    c.doc(2, 1, 14, 15, fold=4)
    c.badge_plus(11, 11, r=3)


@icon("APPLY16")
def _(c: Icon):
    c.grid(3, 5, 15, 15, cols=3, rows=4)
    c.bar(1, 1, 12, 6, "green")
    c.vline(8, 7, 10, "ink", t=2)
    c.poly([(5, 10), (12, 10), (8.5, 13.5)], fill="ink")


# ---------------------------------------------------------------- org / tree
@icon("ALGNCT16")
def _(c: Icon):
    org(c, 6)


@icon("ALGNLT16")
def _(c: Icon):
    org(c, 1)


@icon("ALGNRT16")
def _(c: Icon):
    org(c, 11)


@icon("ARHORZ16")
def _(c: Icon):
    node(c, 6, 1, color="blue")
    oline(c, [(8.5, 6), (8.5, 8.5)], "ink2")
    oline(c, [(3.5, 10), (3.5, 8.5), (13.5, 8.5), (13.5, 10)], "ink2")
    oline(c, [(8.5, 8.5), (8.5, 10)], "ink2")
    for x in (1, 6, 11):
        node(c, x, 10, s=4)


@icon("ARVERT16")
def _(c: Icon):
    node(c, 1, 0, color="blue")
    oline(c, [(3.5, 5), (3.5, 12.5), (10, 12.5)], "ink2")
    oline(c, [(3.5, 7.5), (10, 7.5)], "ink2")
    node(c, 10, 5)
    node(c, 10, 10)


@icon("CHRTVW16")
def _(c: Icon):
    c.rrect(4, 1, 12, 6, r=1, fill="yellow_l", outline="yellow_d")
    oline(c, [(8.5, 6), (8.5, 8.5)], "ink2")
    oline(c, [(4.5, 10), (4.5, 8.5), (11.5, 8.5), (11.5, 10)], "ink2")
    c.rrect(1, 10, 8, 15, r=1, fill="yellow_l", outline="yellow_d")
    c.rrect(9, 10, 16, 15, r=1, fill="yellow_l", outline="yellow_d")


@icon("CTREE")
def _(c: Icon):
    oline(c, [(3.5, 4), (3.5, 13.5), (9, 13.5)], "gray")
    oline(c, [(3.5, 7.5), (9, 7.5)], "gray")
    for x, y in ((1, 0), (9, 5), (9, 11)):
        c.rrect(x, y, x + 5, y + 5, r=1, fill="blue", outline="blue_d")
        c.px(x + 1, y + 1, "blue_l")


@icon("CLEAF")
def _(c: Icon):
    c.doc(2, 1, 14, 15, fold=4, lines=5, line_color="ink2")


# ---------------------------------------------------------------- folders / drives
@icon("CBRANCH")
def _(c: Icon):
    c.folder(1, 2, 15, 14, "yellow")


@icon("CLOSEDFOLDER", w=16, h=15)
def _(c: Icon):
    c.folder(1, 2, 15, 14, "yellow")


@icon("CURRENTFOLDER", w=16, h=15)
def _(c: Icon):
    c.folder(1, 2, 16, 14, "yellow", open_=True)


@icon("CDROM", w=16, h=11)
def _(c: Icon):
    drive(c)
    c.rect(2, 4, 12, 6, fill="ink2")
    c.circle(7, 5, 1.5, fill="gray_l")
    c.rect(13, 7, 15, 9, fill="green")


@icon("FLOPPY", w=16, h=11)
def _(c: Icon):
    drive(c)
    c.hline(2, 14, 5, "ink2")
    c.rect(6, 4, 10, 7, fill="ink")
    c.rect(12, 8, 14, 9, fill="green")


@icon("HARD", w=16, h=11)
def _(c: Icon):
    drive(c)
    c.hline(2, 14, 4, "gray")
    c.hline(2, 14, 6, "gray")
    c.rect(2, 8, 4, 9, fill="red")


@icon("EXECUTABLE", w=16, h=15)
def _(c: Icon):
    c.window(0, 1, 16, 14, title="teal", body="gray_xl")
    c.rect(2, 6, 9, 8, fill="gray_l")
    c.rect(2, 9, 12, 11, fill="gray_l")


# ---------------------------------------------------------------- arrows / nav
@icon("BACK16")
def _(c: Icon):
    c.poly([(2, 8), (12, 1.5), (12, 14.5)], fill="blue", outline="blue_d")


@icon("BACK3D16")
def _(c: Icon):
    c.poly([(2, 8), (12, 1.5), (12, 14.5)], fill="blue", outline="blue_d")
    c.poly([(4, 8), (11, 3.5), (11, 8)], fill="blue_l")


@icon("DLTARW16")
def _(c: Icon):
    c.poly([(1, 8), (7.5, 2), (7.5, 14)], fill="ink")
    c.poly([(8, 8), (14.5, 2), (14.5, 14)], fill="ink")


@icon("DRTARW16")
def _(c: Icon):
    c.poly([(15, 8), (8.5, 2), (8.5, 14)], fill="ink")
    c.poly([(8, 8), (1.5, 2), (1.5, 14)], fill="ink")


@icon("DWNARW16")
def _(c: Icon):
    c.poly([(2.5, 5), (13.5, 5), (8, 11)], fill="ink")


@icon("EXPALL16")
def _(c: Icon):
    c.plus(5, 5, r=3, color="navy", t=2)
    c.plus(11, 11, r=3, color="navy", t=2)


@icon("COLALL16")
def _(c: Icon):
    c.minus(5, 5, r=3, color="navy", t=2)
    c.minus(11, 11, r=3, color="navy", t=2)


@icon("FILDWN16")
def _(c: Icon):
    for y in (2, 7, 12):
        c.rrect(1, y, 9, y + 2, r=.5, fill="navy")
    c.vline(12, 1, 11, "ink")
    head_down(c, 12, 10, n=4)


@icon("EXPORT16")
def _(c: Icon):
    c.poly([(1, 9), (4, 8), (12, 8), (15, 9), (15, 15), (1, 15)], fill="gray_l")
    oline(c, [(1.5, 9), (1.5, 14.5), (14.5, 14.5), (14.5, 9)], "ink2")
    c.hline(2, 14, 11, "gray")
    c.rect(7, 5, 10, 13, fill="red", outline="red_d")
    c.poly([(3, 6), (8.5, 0.5), (14, 6)], fill="red", outline="red_d")
    c.rect(8, 5, 9, 7, fill="red")


# ---------------------------------------------------------------- tables / layouts
@icon("CHRTLY16")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="yellow_l", outline="yellow_d")
    c.hline(2, 14, 5, "yellow_d")
    c.hline(2, 14, 9, "yellow_d")
    c.vline(7, 9, 14, "yellow_d")


@icon("COLLAY16")
def _(c: Icon):
    c.grid(0, 3, 16, 15, cols=4, rows=4)
    c.rect(6, 0, 11, 16, fill="yellow_l", outline="yellow_d")
    c.rect(7, 1, 10, 3, fill="teal")


@icon("DATRNG16")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="paper", outline="ink2")
    c.rect(2, 2, 14, 5, fill="yellow")
    for y in (8, 12):
        c.hline(2, 14, y, "gray_l")
    for x in (5, 8, 11):
        c.vline(x, 6, 14, "gray_l")
    c.rrect(3, 8, 13, 12, r=1, fill="blue_l", outline="blue_d")


@icon("GRPSRT16")
def _(c: Icon):
    c.rect(1, 1, 15, 15, fill="paper", outline="ink")
    c.rect(2, 2, 14, 5, fill="teal")
    c.rect(2, 2, 5, 14, fill="teal")
    c.rect(5, 5, 14, 8, fill="green")
    c.rect(5, 5, 8, 14, fill="green")
    c.hline(5, 14, 5, "green_d")
    c.vline(5, 5, 14, "green_d")
    c.hline(9, 14, 10, "gray")
    c.hline(9, 14, 12, "gray")


@icon("CLNDR16")
def _(c: Icon):
    c.rect(0, 0, 13, 13, fill="paper", outline="ink2")
    c.rect(1, 1, 12, 3, fill="green")
    for x in (4, 8):
        c.vline(x, 3, 12, "gray_l")
    for y in (6, 9):
        c.hline(1, 12, y, "gray_l")
    c.clear(6, 6, 16, 16)
    c.circle(11.5, 11.5, 4.5, fill="yellow_l", outline="yellow_d")
    oline(c, [(11.5, 8.5), (11.5, 11.5), (13.5, 11.5)], "ink")


@icon("GANTT16")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="paper", outline="ink2")
    c.rect(2, 2, 14, 4, fill="gray_l")
    c.vline(6, 2, 4, "gray")
    c.vline(10, 2, 4, "gray")
    c.rect(3, 6, 9, 8, fill="green")
    c.rect(6, 9, 11, 11, fill="blue")
    c.rect(10, 12, 13, 14, fill="red")


# ---------------------------------------------------------------- windows / dialogs
@icon("CLOSE16")
def _(c: Icon):
    c.window(0, 4, 12, 15, title="blue", body="gray_l")
    c.clear(6, 0, 16, 10)
    c.rrect(7, 0, 16, 9, r=1.5, fill="paper", outline="ink2")
    c.cross(11.5, 4.5, r=2.2, color="ink", width=1.6)


@icon("DIALOG16")
def _(c: Icon):
    c.window(1, 1, 15, 13, title="blue", body="gray_l")
    c.poly([(4, 15.5), (8, 10.5), (12, 15.5)], fill="yellow_l", outline="yellow_d")


@icon("FACTVL16")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="paper", outline="ink2")
    c.vline(3, 3, 13, "gray")
    c.rect(6, 3, 12, 9, fill="blue", outline="blue_d")
    c.rect(7, 4, 9, 6, fill="blue_l")
    c.hline(6, 13, 11, "gray")


@icon("ESTMAT16")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="paper", outline="ink2")
    c.rect(3, 9, 7, 14, fill="blue", outline="blue_d")
    c.rect(9, 5, 13, 14, fill="green", outline="green_d")
    c.line([(3, 7), (7, 5), (12.5, 3)], "ink", 1, cap=False)


@icon("EXPNSE16")
def _(c: Icon):
    c.rrect(2, 0, 14, 16, r=1.5, fill="gray_l", outline="ink2")
    c.rect(4, 2, 12, 5, fill="green_l", outline="green_d")
    for y in (7, 10, 13):
        for x in (4, 7, 10):
            c.rect(x, y, x + 2, y + 2, fill="ink2")


@icon("FACTOR16")
def _(c: Icon):
    c.line([(5, 11.5), (8, 11.5), (11, 5)], "ink", cap=False)
    c.rrect(0, 8, 6, 14, r=1, fill="green", outline="green_d")
    c.rect(1, 9, 3, 11, fill="green_l")
    c.rrect(10, 1, 16, 7, r=1, fill="blue", outline="blue_d")
    c.rect(11, 2, 13, 4, fill="blue_l")


# ---------------------------------------------------------------- misc glyphs
@icon("BASLNE16")
def _(c: Icon):
    c.rect(6, 0, 10, 16, fill="yellow", outline="yellow_d")
    c.rect(0, 5, 16, 10, fill="yellow_l", outline="yellow_d")
    for x in (3, 7, 12):
        c.vline(x, 6, 8, "yellow_d")


@icon("BLANK16")
def _(c: Icon):
    pass


@icon("BRGRPH16")
def _(c: Icon):
    c.vline(1, 0, 16, "ink2")
    c.bar(2, 1, 15, 5, "blue")
    c.bar(2, 6, 11, 10, "green")
    c.bar(2, 11, 7, 15, "yellow")


@icon("CANCEL16")
def _(c: Icon):
    c.circle(8, 8, 6.6, outline="red", width=2.2)
    c.line([(3.6, 12.4), (12.4, 3.6)], "red", 2.2, cap=False)


@icon("CHKBX016")
def _(c: Icon):
    c.rrect(2, 2, 14, 14, r=1.5, fill="paper", outline="ink2")


@icon("CHKBX116")
def _(c: Icon):
    c.rrect(2, 2, 14, 14, r=1.5, fill="paper", outline="ink2")
    c.check(4, 4.5, s=1.0, color="ink", width=2.0)


@icon("CHKBX216")
def _(c: Icon):
    c.rrect(2, 2, 14, 14, r=1.5, fill="gray_xl", outline="gray")
    c.check(4, 4.5, s=1.0, color="gray", width=2.0)


@icon("COPY16")
def _(c: Icon):
    c.doc(0, 0, 10, 12, fold=3, lines=3, line_color="ink2")
    c.doc(6, 4, 16, 16, fold=3, outline="navy", lines=3, line_color="navy")


@icon("CUT16")
def _(c: Icon):
    c.line([(5.5, 10), (11, 1)], "ink", 1.3, cap=False)
    c.line([(10.5, 10), (5, 1)], "ink", 1.3, cap=False)
    c.circle(4.5, 12.5, 2.4, outline="ink", width=1.3)
    c.circle(11.5, 12.5, 2.4, outline="ink", width=1.3)
    c.px(8, 6, "gray")


@icon("DELETE16")
def _(c: Icon):
    c.cross(8, 8, r=5.6, color="ink", width=2.2)


@icon("DELIV16")
def _(c: Icon):
    scroll(c, 3.5, 3, 12.5, 12.5)


@icon("DLVCUR16")
def _(c: Icon):
    scroll(c, 3, 2.5, 10.5, 10.5, w=4)
    c.badge_current()


@icon("DOCMNT16")
def _(c: Icon):
    c.doc(2, 1, 14, 15, fold=3, fill="yellow_l", outline="yellow_d",
          lines=5, line_color="ink2")


@icon("DOCCUR16")
def _(c: Icon):
    c.doc(1, 0, 13, 14, fold=3, fill="yellow_l", outline="yellow_d",
          lines=5, line_color="ink2")
    c.badge_current()


@icon("DIRCTY16")
def _(c: Icon):
    c.rect(6, 1, 9, 16, fill="blue", outline="blue_d")
    c.poly([(0.5, 4.5), (2.5, 2.5), (8, 2.5), (8, 6.5), (2.5, 6.5)],
           fill="gray_l", outline="ink2")
    c.poly([(7, 8.5), (13.5, 8.5), (15.5, 10.5), (13.5, 12.5), (7, 12.5)],
           fill="gray_l", outline="ink2")


@icon("EDITRC16")
def _(c: Icon):
    for x in (1, 3, 5):
        c.px(x, 14, "ink")
    c.line([(7.5, 13), (14, 4)], "yellow_d", 3.4, cap=False)
    c.line([(7.5, 13), (14, 4)], "yellow", 2, cap=False)
    c.line([(13.2, 5.1), (15, 2.6)], "orange", 3.2, cap=False)
    c.poly([(6.3, 12.2), (8.7, 13.9), (6.2, 15)], fill="ink")


@icon("EPS16")
def _(c: Icon):
    c.poly([(8, 1), (1, 11), (9, 15)], fill="yellow_l")
    c.poly([(8, 1), (9, 15), (15, 10)], fill="yellow")
    c.line([(8, 1), (1, 11), (9, 15), (15, 10), (8, 1)], "yellow_d")
    c.line([(8, 1), (9, 15)], "yellow_d")


@icon("FILTER16")
def _(c: Icon):
    pts = [(1, 1.5), (15, 1.5), (9.5, 8), (9.5, 14.5), (6.5, 13), (6.5, 8)]
    c.poly(pts, fill="gray_l", outline="ink2")
    c.poly([(9.5, 2.5), (14, 2.5), (9.5, 7.5)], fill="gray")


@icon("FIND16")
def _(c: Icon):
    c.rrect(2, 1, 6, 5, r=.8, fill="ink")
    c.rrect(10, 1, 14, 5, r=.8, fill="ink")
    c.rrect(1, 4, 7, 15, r=1.5, fill="ink")
    c.rrect(9, 4, 15, 15, r=1.5, fill="ink")
    c.rect(6, 6, 10, 10, fill="ink2")
    c.circle(4, 11.5, 1.8, fill="blue_l")
    c.circle(12, 11.5, 1.8, fill="blue_l")


@icon("FINISH16")
def _(c: Icon):
    c.vline(1, 0, 16, "ink")
    c.rect(2, 1, 15, 11, fill="paper", outline="ink")
    for r in range(3):
        for k in range(4):
            if (r + k) % 2 == 0:
                x = 3 + k * 3
                y = 2 + r * 3
                c.rect(x, y, min(x + 3, 14), y + 3, fill="ink")


@icon("FTCLRS16")
def _(c: Icon):
    c.rrect(5, 5, 16, 16, r=1, fill="paper", outline="ink2")
    for (x, y, col) in ((7, 8, "yellow"), (10, 11, "red"), (13, 8, "blue"),
                        (7, 13, "green"), (13, 13, "yellow")):
        c.rect(x, y, x + 2, y + 2, fill=col)
    c.poly([(1, 4), (5, 0.5), (9.5, 5), (6, 8.5)], fill="gray_l", outline="ink2")
    c.line([(8.5, 6), (10, 8)], "blue", 2, cap=False)


@icon("GLOBAL16")
def _(c: Icon):
    c.circle(8, 8, 7.5, fill="blue", outline="blue_d")
    c.poly([(4, 3), (8, 2.5), (8, 6), (5.5, 7.5), (6, 10), (3.5, 9), (2.5, 6)],
           fill="green")
    c.poly([(10, 6), (13.5, 5), (14, 9), (11.5, 13), (10, 10)], fill="green")
    c.arc(1.5, 1.5, 14.5, 14.5, 200, 260, "blue_l", 1)


@icon("GLOBVW16")
def _(c: Icon):
    c.circle(9.5, 8, 6.5, fill="blue", outline="blue_d")
    c.poly([(11, 4), (14, 5), (14.5, 9), (12, 12), (11, 8)], fill="green")
    c.circle(5.5, 5.5, 4.3, fill="paper", outline="ink", width=1.3)
    c.line([(8.5, 8.5), (13, 13.5)], "ink", 2.4, cap=False)
    c.line([(4, 5.5), (5.3, 7), (7.5, 4)], "green", 1.1, cap=False)


@icon("HDETHZ16")
def _(c: Icon):
    c.poly([(0.5, 4), (5, 8), (0.5, 12)], fill="yellow_l", outline="yellow_d")
    panel_boxes(c, 7, 0, 15, 16, vertical=True)


@icon("HDETVT16")
def _(c: Icon):
    panel_boxes(c, 0, 1, 16, 9, vertical=False)
    c.poly([(4, 15.5), (8, 11), (12, 15.5)], fill="yellow_l", outline="yellow_d")


@icon("HIDEDT16")
def _(c: Icon):
    c.poly([(4, 0.5), (12, 0.5), (8, 5)], fill="yellow_l", outline="yellow_d")
    panel_boxes(c, 0, 7, 16, 15, vertical=False)


@icon("HELP16")
def _(c: Icon):
    help_disc(c)


@icon("HINT3D16")
def _(c: Icon):
    help_disc(c)


@icon("HELP3D16")
def _(c: Icon):
    bubble(c, 1, 1, 16, 11, 3)
    c.hline(4, 10, 4, "blue")
    c.hline(4, 12, 7, "blue")
    c.rect(12, 3, 14, 5, fill="yellow_d")


@icon("HINT16")
def _(c: Icon):
    bubble(c, 0, 2, 15, 12, 3)
    c.hline(3, 11, 5, "blue")
    c.hline(3, 9, 8, "blue")


@icon("HLPCNT16")
def _(c: Icon):
    c.rrect(3, 11, 15, 16, r=1, fill="paper", outline="ink2")
    c.hline(5, 14, 13, "gray_l")
    c.rrect(2, 0, 14, 14, r=1.5, fill="purple", outline="purple_d")
    c.rect(3, 1, 5, 13, fill="purple_d")
    qmark(c, 10, 3, "yellow")
