"""OpenXER icon set B (HOME16 .. RPIMP16).

Original drawings in OpenXER's palette, rounded shapes and own badges.
"""
import math
from iconlib import icon, Icon, P, fam


# ------------------------------------------------------------ local helpers
def globe(c: Icon, x0, y0, x1, y1, sea="blue", land="green"):
    """Earth: sea disc with two land masses and a dark rim."""
    c.ellipse(x0, y0, x1, y1, fill=sea)
    w, h = x1 - x0, y1 - y0
    m = lambda pts: [(x0 + u * w, y0 + v * h) for u, v in pts]
    c.poly(m([(.22, .14), (.48, .10), (.52, .30), (.40, .40), (.34, .62),
              (.22, .56), (.16, .36)]), fill=land)
    c.poly(m([(.60, .42), (.86, .36), (.90, .58), (.76, .86), (.62, .72)]),
           fill=land)
    c.ellipse(x0, y0, x1, y1, outline=fam(sea)[1], width=1)


def gear(c: Icon, cx, cy, r, color="yellow"):
    f, dk, lt = fam(color)
    pts = []
    n = 8
    for i in range(n * 4):
        a = (i / (n * 4)) * 2 * math.pi
        rr = r if (i % 4) in (0, 1) else r * .7
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    c.poly(pts, fill=f, outline=dk)
    c.circle(cx, cy, r * .3, fill=dk)


def pencil(c: Icon, x0, y0, x1, y1):
    """Pencil from tip (x0,y0) up to end (x1,y1)."""
    c.line([(x0, y0), (x1, y1)], "yellow_d", 3.4, cap=False)
    c.line([(x0, y0), (x1, y1)], "yellow", 1.8, cap=False)
    ang = math.atan2(y1 - y0, x1 - x0)
    tx, ty = x0 + 1.6 * math.cos(ang), y0 + 1.6 * math.sin(ang)
    c.line([(x0, y0), (tx, ty)], "green_d", 2.4, cap=False)


def proj(c: Icon, open_=False, tone="gray"):
    """Project glyph: a briefcase-like folder. tone gray (tree) or
    blue (toolbar)."""
    if tone == "gray":
        f, o, hi, back = "gray_l", "ink2", "paper", "gray"
    else:
        f, o, hi, back = "blue_l", "blue_d", "paper", "blue"
    c.rrect(3, 2, 9, 6, r=1, fill=f, outline=o)
    if not open_:
        c.rrect(1, 4, 16, 15, r=1.5, fill=f, outline=o)
        c.hline(2, 15, 5, hi)
        c.hline(2, 15, 13, back)
    else:
        c.rrect(2, 4, 16, 15, r=1.5, fill=back, outline=o)
        c.hline(3, 15, 5, hi)
        front = [(.5, 8.5), (12.5, 8.5), (15.5, 14.5), (2.5, 14.5)]
        c.poly(front, fill=f, outline=o)
        c.hline(2, 13, 9, hi)


def red_check(c: Icon, x0=7.5, y0=0):
    c.check(x0, y0, s=1.05, color="paper", width=3.6)
    c.check(x0, y0, s=1.05, color="red", width=1.9)


def role_person(c: Icon, cx, top, s=1.0):
    c.person(cx, top, color="green", s=s)


def nlr(c: Icon, dx=0, two=False):
    gear(c, 11.5 + dx, 4.5, 4.2)
    if two:
        c.person(4.5 + dx, 2.5, color="gray", s=.95)
    c.person(7 + dx, 5.5, color="blue", s=1.05)


def magnifier(c: Icon, cx, cy, r, lens="teal_l"):
    c.line([(cx + r * .6, cy + r * .6), (cx + r + 2.4, cy + r + 2.4)],
           "ink", 2.2, cap=False)
    c.circle(cx, cy, r, fill=lens, outline="ink", width=1.2)
    c.px(round(cx - r * .5), round(cy - r * .5), "paper")


def key(c: Icon, x0, y0):
    """Horizontal key, 7-unit bow at (x0,y0), shaft to the right edge."""
    c.rect(10, y0 + 4, 12, y0 + 7, fill="yellow", outline="yellow_d")
    c.rect(13, y0 + 4, 15, y0 + 6, fill="yellow", outline="yellow_d")
    c.rect(x0 + 5, y0 + 2, 16, y0 + 5, fill="yellow", outline="yellow_d")
    c.hline(x0 + 6, 15, y0 + 3, "yellow_l")
    c.rrect(x0, y0, x0 + 7, y0 + 7, r=2.5, fill="yellow", outline="yellow_d")
    c.rect(x0 + 2, y0 + 3, x0 + 4, y0 + 4, fill="yellow_d")


def harrow(c: Icon, x0, x1, y, t=1, head=3.5, color="blue", left=False):
    """Crisp horizontal arrow: shaft t px thick centred on y."""
    h = head * .75
    if t % 2 == 1:
        h = math.floor(h) + .5
    else:
        h = round(h)
    if not left:
        c.rect(x0, y - t / 2, x1 - head + .5, y + t / 2, fill=color)
        c.poly([(x1 - head, y - h), (x1, y), (x1 - head, y + h)], fill=color)
    else:
        c.rect(x0 + head - .5, y - t / 2, x1, y + t / 2, fill=color)
        c.poly([(x0 + head, y - h), (x0, y), (x0 + head, y + h)], fill=color)


def book_closed(c: Icon, x0, y0, x1, y1):
    c.rect(x0 + 2, y1 - 3, x1, y1, fill="paper", outline="ink2")
    c.rect(x1 - 3, y0 + 2, x1, y1, fill="paper", outline="ink2")
    c.rrect(x0, y0, x1 - 2, y1 - 2, r=1.5, fill="blue", outline="blue_d")
    c.vline(x0 + 2, y0 + 1, y1 - 3, "blue_l")


def book_open(c: Icon, x0, y0, x1, y1):
    mid = (x0 + x1) // 2
    c.rrect(x0, y0 + 2, x1, y1, r=1, fill="blue", outline="blue_d")
    c.rect(x0 + 1, y0, mid, y1 - 2, fill="paper", outline="ink2")
    c.rect(mid, y0, x1 - 1, y1 - 2, fill="paper", outline="ink2")
    for yy in range(y0 + 2, y1 - 3, 2):
        c.hline(x0 + 3, mid - 1, yy, "gray")
        c.px(mid + 2, yy, "yellow_d")
        if mid + 4 < x1 - 1:
            c.px(mid + 4, yy, "yellow_d")


# --------------------------------------------------------------- general
@icon("HOME16")
def _(c):
    c.rect(11, 2, 13, 6, fill="red", outline="red_d")
    c.poly([(.5, 8.5), (8, 1), (15.5, 8.5)], fill="blue", outline="blue_d")
    c.rect(3, 8, 13, 15, fill="gray_xl", outline="ink2")
    c.rrect(6, 10, 10, 15, r=1, fill="yellow", outline="yellow_d")


@icon("HSTGRM16")
def _(c):
    for x0, top, red in ((1, 2, 4), (6, 5, 2), (11, 9, 0)):
        c.rect(x0, top, x0 + 4, 15, fill="green")
        if red:
            c.rect(x0, top, x0 + 4, top + red, fill="red")
        c.rect(x0, top, x0 + 4, 15, outline="ink2")
        c.vline(x0 + 1, top + 1, 14, "paper" if not red else "red_l")
        c.vline(x0 + 1, top + 1 + red, 14, "green_l")
    c.hline(0, 16, 15, "ink")


@icon("ID_1001")
def _(c):
    # warning dialog glyph
    c.rrect(1, 1, 15, 15, r=3.5, fill="yellow", outline="yellow_d")
    c.rrect(7, 3, 9, 10, r=.8, fill="ink")
    c.rect(7, 11, 9, 13, fill="ink")


@icon("ID_1002")
def _(c):
    globe(c, 1, 1, 15, 15)


@icon("ID_1005")
def _(c):
    # ID_1005 is a bare solid mask: a solid tile with the silhouette of
    # ID_1001, so it works as a mask.
    c.rrect(1, 1, 15, 15, r=3.5, fill="ink")


# ------------------------------------------------------------ EPS / projects
def eps(c):
    globe(c, 0, 0, 16, 16)
    c.rrect(2, 4, 15, 13, r=1, fill="yellow", outline="yellow_d")
    c.hline(4, 13, 6, "paper")
    c.hline(4, 13, 8, "paper")
    c.hline(4, 10, 10, "paper")


@icon("IMG_ICO_EPS")
def _(c):
    eps(c)


@icon("IMG_ICO_EPS_CHECKED")
def _(c):
    eps(c)
    red_check(c)


@icon("IMG_ICO_PROJ_CLOSED")
def _(c):
    proj(c)


@icon("IMG_ICO_PROJ_CLOSED_CHECKED")
def _(c):
    proj(c); red_check(c)


@icon("IMG_ICO_PROJ_CLOSED_CHECKED_WHATIF")
def _(c):
    proj(c); red_check(c); c.badge_whatif(8, 8)


@icon("IMG_ICO_PROJ_CLOSED_WHATIF")
def _(c):
    proj(c); c.badge_whatif(8, 8)


@icon("IMG_ICO_PROJ_OPEN")
def _(c):
    proj(c, True)


@icon("IMG_ICO_PROJ_OPEN_CHECKED")
def _(c):
    proj(c, True); red_check(c)


@icon("IMG_ICO_PROJ_OPEN_CHECKED_WHATIF")
def _(c):
    proj(c, True); red_check(c); c.badge_whatif(8, 8)


@icon("IMG_ICO_PROJ_OPEN_WHATIF")
def _(c):
    proj(c, True); c.badge_whatif(8, 8)


@icon("PROJCT16")
def _(c):
    proj(c, tone="blue")


@icon("PRJOPN16")
def _(c):
    proj(c, True, tone="blue")


@icon("PRJCKO16")
def _(c):
    proj(c, tone="blue"); red_check(c)


@icon("PRJWCO16")
def _(c):
    proj(c, tone="blue"); red_check(c); c.badge_whatif(8, 8)


@icon("PRJWIF16")
def _(c):
    proj(c, tone="blue"); c.badge_whatif(8, 8)


@icon("PJOCKO16")
def _(c):
    proj(c, True, tone="blue"); red_check(c)


@icon("PJOWCO16")
def _(c):
    proj(c, True, tone="blue"); red_check(c); c.badge_whatif(8, 8)


@icon("PJOWIF16")
def _(c):
    proj(c, True, tone="blue"); c.badge_whatif(8, 8)


@icon("PROJCD16")
def _(c):
    proj(c, tone="blue")
    c.clear(7, 8, 16, 16)
    tag = [(8.5, 9.5), (12.5, 9.5), (15.5, 12.5), (12.5, 15.5), (8.5, 15.5)]
    c.poly(tag, fill="yellow", outline="yellow_d")
    c.px(12, 12, "yellow_d")


# ------------------------------------------------------------ file / nav
@icon("IMPORT16")
def _(c):
    c.rrect(1, 9, 15, 15, r=1.5, fill="gray_l", outline="ink2")
    c.hline(4, 12, 11, "ink2")
    c.rect(6, 0, 10, 6, fill="green", outline="green_d")
    c.poly([(3, 5.5), (13, 5.5), (8, 11.5)], fill="green", outline="green_d")
    c.rect(7, 1, 9, 7, fill="green")
    c.vline(7, 1, 6, "green_l")


@icon("ISSUE16")
def _(c):
    c.line([(3.5, 3.5), (1.5, 3.5), (1.5, 12.5), (3.5, 12.5)], "ink2")
    c.line([(12.5, 3.5), (14.5, 3.5), (14.5, 12.5), (12.5, 12.5)], "ink2")
    c.circle(8, 8, 3.6, fill="red", outline="red_d")
    c.px(7, 6, "red_l")


@icon("JUMP16")
def _(c):
    for x, y in ((2, 7), (2, 5), (3, 3), (5, 2), (7, 1), (9, 1), (11, 2),
                 (12, 4)):
        c.px(x, y, "ink2")
    c.tri(12.5, 7.5, 5, "down", fill="navy")
    c.rrect(1, 9, 8, 16, r=1.5, fill="blue", outline="blue_d")
    c.rect(3, 11, 5, 13, fill="blue_l")


@icon("KNOWNFILE", w=16, h=15)
def _(c):
    c.doc(2, 0, 14, 15, fold=3)
    c.hline(4, 9, 4, "gray")
    for y in (7, 9, 11):
        c.hline(4, 12, y, "navy")


@icon("NEW16")
def _(c):
    c.doc(2, 0, 14, 16, fold=4)


@icon("OPEN16")
def _(c):
    c.folder(1, 5, 15, 15, open_=True)
    c.arc(6, 1, 14, 8, 190, 330, "ink", 1.3)
    c.tri(13.5, 4.8, 3.6, "down", fill="ink")


@icon("OPENFOLDER", w=16, h=15)
def _(c):
    c.folder(0, 1, 16, 15, open_=True)


@icon("PASTE16")
def _(c):
    c.rrect(1, 2, 12, 15, r=1.5, fill="tan", outline="tan_d")
    c.rrect(4, 0, 9, 4, r=1, fill="gray_l", outline="ink2")
    c.doc(7, 6, 16, 16, fold=3)
    c.hline(9, 14, 11, "navy")
    c.hline(9, 13, 13, "navy")


@icon("PRINT16")
def _(c):
    c.rect(4, 1, 12, 6, fill="paper", outline="ink2")
    c.rrect(1, 5, 15, 12, r=1.5, fill="gray_l", outline="ink")
    c.hline(2, 14, 6, "paper")
    c.hline(3, 13, 9, "ink2")
    c.px(12, 7, "green")
    c.rect(3, 11, 13, 15, fill="paper", outline="ink2")
    c.hline(5, 11, 13, "gray")


@icon("PREVIEWGLYPH")
def _(c):
    c.doc(1, 0, 13, 16, fold=4)
    for y in (6, 8):
        c.hline(3, 7, y, "gray")
    magnifier(c, 9.5, 8.5, 3)


@icon("PRVIEW16")
def _(c):
    c.doc(0, 0, 12, 16, fold=3)
    for y in (5, 7, 9, 11):
        c.hline(2, 4, y, "navy")
        c.hline(5, 9, y, "gray")
    magnifier(c, 10, 10, 3.2)


@icon("PGSETP16")
def _(c):
    c.rect(3, 3, 14, 14, fill="paper", outline="ink")
    for v in (6, 11):
        for y in range(4, 13, 2):
            c.px(v, y, "blue_l")
    for h in (6, 11):
        for x in range(4, 13, 2):
            c.px(x, h, "blue_l")
    for v in (6, 11):
        c.vline(v, 0, 2, "ink2")
        c.vline(v, 15, 16, "ink2")
    for h in (6, 11):
        c.hline(0, 2, h, "ink2")
        c.hline(15, 16, h, "ink2")


@icon("NETWORK", w=16, h=11)
def _(c):
    c.rrect(0, 0, 16, 11, r=1.5, fill="gray_xl", outline="ink2")
    c.hline(4, 13, 3, "ink2")
    for x in (4, 8, 12):
        c.vline(x, 3, 6, "ink2")
        c.rect(x - 1, 6, x + 2, 9, fill="blue", outline="blue_d")


@icon("RAM", w=16, h=11)
def _(c):
    c.rrect(0, 1, 16, 10, r=1, fill="green", outline="green_d")
    for x in (2, 6, 10):
        c.rect(x, 3, x + 4 - (1 if x == 10 else 0) + (0), 7, fill="ink")
    for x in range(2, 15, 2):
        c.px(x, 8, "yellow")


@icon("REFRSH16")
def _(c):
    c.poly([(1.5, 9.5), (1.5, 2.5), (9.5, 2.5), (9.5, .5), (13.5, 4),
            (9.5, 7.5), (9.5, 5.5), (4.5, 5.5), (4.5, 9.5)],
           fill="blue", outline="blue_d")
    c.hline(2, 10, 3, "blue_l")
    c.poly([(14.5, 6.5), (14.5, 13.5), (6.5, 13.5), (6.5, 15.5), (2.5, 12),
            (6.5, 8.5), (6.5, 10.5), (11.5, 10.5), (11.5, 6.5)],
           fill="green", outline="green_d")
    c.vline(13, 7, 13, "green_l")


@icon("REMOVE16")
def _(c):
    c.grid(0, 1, 14, 13, cols=3, rows=4)
    c.cross(11, 11.5, 3.2, "red_d", 3.6)
    c.cross(11, 11.5, 3.2, "red", 2.0)


@icon("REPLAC16")
def _(c):
    c.rrect(0, 0, 7, 7, r=1.5, fill="green", outline="green_d")
    c.rrect(8, 8, 16, 16, r=1.5, fill="paper", outline="ink2")
    c.arrow(8.5, 2, 13.5, 6.8, "ink", 1.2, 3)
    c.arrow(2, 8.5, 6.8, 13.5, "ink", 1.2, 3)


@icon("REPORT16")
def _(c):
    c.rrect(1, 1, 15, 15, r=1.5, fill="gray_l", outline="ink2")
    c.rect(3, 3, 13, 13, fill="paper")
    for y in (4, 6, 8, 10):
        c.px(4, y, "ink")
        c.hline(6, 12, y, "ink2")


@icon("PRIVLG16")
def _(c):
    key(c, 0, 4)


@icon("LOCK16")
def _(c):
    c.rrect(4, 1, 12, 12, r=3.5, outline="ink2", width=1.6)
    c.rrect(2, 7, 14, 15, r=1.5, fill="yellow", outline="yellow_d")
    c.hline(3, 13, 8, "yellow_l")
    c.circle(8, 10.5, 1.2, fill="ink")
    c.rect(7.5, 11, 8.5, 13, fill="ink")


# ------------------------------------------------------------ arrows
@icon("LFTARW16")
def _(c):
    c.poly([(11.5, 3), (4, 8), (11.5, 13)], fill="ink")


@icon("RGTARW16")
def _(c):
    c.poly([(4.5, 3), (12, 8), (4.5, 13)], fill="ink")


@icon("NEXT16")
def _(c):
    c.poly([(5.5, 1.5), (13, 8), (5.5, 14.5)], fill="blue", outline="blue_d")


@icon("NEXT3D16")
def _(c):
    c.poly([(5.5, 1.5), (13, 8), (5.5, 14.5)], fill="blue", outline="navy")
    c.line([(6.5, 3.5), (6.5, 12.5)], "blue_l", 1)
    c.line([(7, 3.2), (11.2, 7.2)], "blue_l", 1)


@icon("OPTBAR16")
def _(c):
    c.poly([(4.5, 6.5), (11.5, 6.5), (8, 10.5)], fill="paper", outline="ink2")


@icon("OK16")
def _(c):
    c.check(1, 2, s=1.75, color="green_d", width=3.6)
    c.check(1, 2, s=1.75, color="green", width=2.0)


@icon("RADBX016")
def _(c):
    c.circle(8, 8, 6, fill="paper", outline="ink2", width=1.2)
    c.arc(3, 3, 13, 13, 135, 315, "gray_l", 1)


@icon("RADBX116")
def _(c):
    c.circle(8, 8, 6, fill="paper", outline="ink2", width=1.2)
    c.arc(3, 3, 13, 13, 135, 315, "gray_l", 1)
    c.circle(8, 8, 2.7, fill="navy")


# ------------------------------------------------------------ memos
def memo(c, fill, lines):
    pts = [(1.5, 2.5), (14.5, 2.5), (14.5, 10.5), (10.5, 14.5), (1.5, 14.5)]
    c.poly(pts, fill=fill, outline="yellow_d" if fill != "paper" else "ink2")
    c.poly([(10.5, 14.5), (10.5, 10.5), (14.5, 10.5)], fill="gray_l",
           outline="ink2")
    for i in range(lines):
        y = 6 + 2 * i
        c.hline(7, 13 if y < 10 else 10, y, "ink2")
    c.rrect(3, 0, 7, 10, r=2, outline="gray", width=1)
    c.vline(5, 2, 8, "gray")


@icon("MEMO16")
def _(c):
    memo(c, "yellow_l", 2)


@icon("MEMOEM16")
def _(c):
    memo(c, "paper", 0)


@icon("MEMOFL16")
def _(c):
    memo(c, "yellow_l", 4)


# ------------------------------------------------------------ resources
@icon("NLRSRC16")
def _(c):
    nlr(c)


@icon("NLRCUR16")
def _(c):
    nlr(c, dx=-1)
    c.badge_current(9, 9)


@icon("NLRSU16")
def _(c):
    nlr(c, dx=0, two=True)


@icon("NLRSUC16")
def _(c):
    nlr(c, dx=0, two=True)
    c.badge_current(9, 9)


@icon("OBS16")
def _(c):
    c.rrect(0, 1, 11, 9, r=1.5, fill="blue", outline="blue_d")
    c.hline(1, 10, 2, "blue_l")
    c.clear(6, 4, 15, 16)
    c.person(10.5, 5, color="orange", s=1.05)


@icon("OBSROT16")
def _(c):
    c.rrect(0, 1, 11, 9, r=1.5, fill="gray_l", outline="ink2")
    c.hline(1, 10, 2, "paper")
    c.clear(6, 4, 15, 16)
    c.person(10.5, 5, color="orange", s=1.05)


@icon("PEGACT16")
def _(c):
    c.rrect(0, 1, 16, 8, r=1.5, fill="blue", outline="blue_d")
    c.hline(1, 15, 2, "blue_l")
    c.clear(2, 4, 13, 16)
    c.person(7.5, 5, color="green", s=1.1)


@icon("PEGTS16")
def _(c):
    c.grid(0, 1, 13, 13, cols=3, rows=4, head="blue", first_col="blue_l")
    c.clear(6, 6, 16, 16)
    c.circle(11, 11, 4.4, fill="yellow_l", outline="ink2", width=1)
    c.vline(11, 8, 12, "ink")
    c.hline(11, 14, 11, "ink")


@icon("RESHST16")
def _(c):
    c.rect(1, 1, 5, 14, fill="blue_l")
    c.rect(1, 1, 5, 4, fill="red")
    c.rect(1, 1, 5, 14, outline="ink2")
    c.person(11, 4, color="blue", s=1.15)


@icon("RES_RL16")
def _(c):
    c.person(5, 0, color="blue", s=1.0)
    pencil(c, 10.5, 7.5, 15, 1)
    gear(c, 10.5, 11.5, 4.2)


@icon("ROLE16")
def _(c):
    pencil(c, 10.5, 8, 15.2, 1)
    role_person(c, 6, 4, 1.15)


@icon("ROLCUR16")
def _(c):
    pencil(c, 10, 6.5, 14.5, .5)
    role_person(c, 5, 4, 1.1)
    c.badge_current(9, 9)


@icon("ROLEOP16")
def _(c):
    c.rrect(0, 1, 9, 11, r=1.5, fill="gray_l", outline="ink2")
    c.hline(1, 8, 2, "paper")
    c.clear(5, 4, 16, 16)
    role_person(c, 10.5, 5, 1.05)


@icon("RLATSK16")
def _(c):
    c.bar(0, 3, 11, 9, "green")
    c.clear(6, 4, 16, 16)
    role_person(c, 11, 5, 1.05)


@icon("PROFLE16")
def _(c):
    c.person(11, .5, color="gray", s=.95)
    c.clear(0, 8, 16, 16)
    key(c, 0, 9)


# ------------------------------------------------------------ portfolios
@icon("PRTFLO16")
def _(c):
    book_closed(c, 1, 1, 16, 15)


@icon("PRTFOP16")
def _(c):
    book_open(c, 0, 2, 16, 15)


@icon("PRTFGL16")
def _(c):
    globe(c, 0, 0, 14, 14)
    c.clear(5, 5, 16, 16)
    book_closed(c, 6, 6, 16, 16)


@icon("PRTOGB16")
def _(c):
    globe(c, 0, 0, 16, 16)
    book_open(c, 2, 5, 14, 16)


# ------------------------------------------------------------ activities
@icon("OVRHD16")
def _(c):
    c.rrect(1, 5, 15, 11, r=1.5, fill="gray_l", outline="ink2")
    c.hline(2, 14, 6, "paper")


def steps(c):
    pts = [(.5, 14.5), (.5, 10.5), (5.5, 10.5), (5.5, 6.5), (10.5, 6.5),
           (10.5, 2.5), (15.5, 2.5), (15.5, 14.5)]
    c.poly(pts, fill="gray_l", outline="ink2")
    c.hline(1, 5, 11, "paper")
    c.hline(6, 10, 7, "paper")
    c.hline(11, 15, 3, "paper")


@icon("PRCDUR16")
def _(c):
    steps(c)


@icon("PCDCUR16")
def _(c):
    steps(c)
    c.badge_current(9, 9)


def quality(c):
    c.window(0, 0, 16, 15, title="gray_l")
    c.rect(2, 5, 11, 7, fill="green")
    c.rect(2, 8, 6, 10, fill="red")
    c.rect(2, 11, 8, 13, fill="red")


@icon("QALITY16")
def _(c):
    quality(c)


@icon("QULCUR16")
def _(c):
    quality(c)
    c.badge_current(9, 9)


@icon("LEVELR16")
def _(c):
    for x0, top, red in ((1, 1, 2), (6, 2, 2), (11, 5, 0)):
        c.rect(x0, top, x0 + 4, 7, fill="blue_l")
        if red:
            c.rect(x0, top, x0 + 4, top + red, fill="red")
        c.rect(x0, top, x0 + 4, 7, outline="ink2")
    c.tri(8, 8.5, 3, "down", fill="ink")
    for x0 in (1, 6, 11):
        c.rect(x0, 10, x0 + 4, 15, fill="blue_l", outline="ink2")


@icon("RISK16")
def _(c):
    c.rrect(11, 1, 16, 15, r=1, fill="red", outline="red_d")
    c.vline(12, 2, 14, "red_l")
    harrow(c, 0, 10.5, 8, t=2, head=5, color="blue")


@icon("REORGZ16")
def _(c):
    c.line([(6, 3.5), (8.5, 3.5), (8.5, 12.5), (11, 12.5)], "ink2")
    c.hline(8, 11, 3, "ink2")
    c.rrect(1, 1, 6, 6, r=1, fill="yellow", outline="yellow_d")
    c.rrect(11, 1, 16, 6, r=1, fill="yellow", outline="yellow_d")
    c.rrect(11, 10, 16, 15, r=1, fill="yellow", outline="yellow_d")
    harrow(c, 0, 8.5, 12.5, t=1, head=3.5)


@icon("RORGAL16")
def _(c):
    for x0, y in ((0, 2.5), (2, 7.5), (0, 12.5)):
        harrow(c, x0, 10, y, t=1, head=4)
    for y in (0, 5, 10):
        c.rrect(11, y + .5, 16, y + 5.5, r=1, fill="yellow", outline="yellow_d")


# ------------------------------------------------------------ report editor
def rwin(c, x0, y0, x1, y1):
    c.rect(x0, y0, x1, y1, fill="paper", outline="ink")
    c.rect(x0 + 1, y0 + 1, x1 - 1, y0 + 2, fill="gray_l")


@icon("RPADDG16")
def _(c):
    rwin(c, 3, 0, 16, 16)
    c.rrect(0, 4, 14, 12, r=1, fill="green", outline="green_d")
    c.vline(3, 5, 11, "green_d")
    for y in (6, 8, 10):
        c.hline(5, 13, y, "green_l")


@icon("RPADDI16")
def _(c):
    c.rect(1, 2, 15, 14, fill="teal_l")
    c.circle(4.5, 5.5, 1.6, fill="yellow")
    c.poly([(1, 11), (6, 6.5), (9, 9.5), (11, 8), (15, 11.5), (15, 14), (1, 14)],
           fill="green")
    c.rect(1, 12, 15, 14, fill="blue")
    c.rect(1, 2, 15, 14, outline="ink")


@icon("RPADDL16")
def _(c):
    c.rrect(1, 3, 15, 13, r=1.5, fill="paper", outline="ink")
    for x in range(3, 13, 2):
        c.px(x, 6, "ink")
    c.hline(3, 13, 9, "ink")


@icon("RPADDR16")
def _(c):
    rwin(c, 3, 0, 16, 16)
    c.rect(0, 5, 16, 11, fill="blue", outline="blue_d")
    c.rect(2, 7, 14, 9, fill="paper")


@icon("RPADDT16")
def _(c):
    c.rrect(0, 3, 16, 13, r=1.5, fill="paper", outline="ink")
    # A
    c.line([(2.5, 11), (5, 5.2), (7.5, 11)], "ink", 1.3, cap=False)
    c.hline(4, 7, 9, "ink")
    # b
    c.vline(9, 5, 11, "ink")
    c.rrect(9, 7, 14, 11, r=1.5, outline="ink", width=1)


@icon("RPEDIT16")
def _(c):
    rwin(c, 3, 0, 16, 16)
    c.rrect(0, 3, 13, 7, r=1, fill="green", outline="green_d")
    c.rrect(0, 7, 13, 11, r=1, fill="blue", outline="blue_d")
    c.rrect(0, 11, 13, 15, r=1, fill="green", outline="green_d")


@icon("RPEXP16")
def _(c):
    rwin(c, 0, 0, 10, 15)
    c.rrect(8, 6, 16, 15, r=1, fill="blue_l", outline="ink")
    c.rect(10, 7, 14, 10, fill="paper")
    c.rect(11, 12, 13, 15, fill="ink2")
    c.poly([(4.5, 7), (8.5, 10.5), (4.5, 14)], fill="green", outline="green_d")


@icon("RPIMP16")
def _(c):
    rwin(c, 0, 0, 11, 16)
    c.poly([(15.5, 8.5), (11.5, 8.5), (11.5, 5.5), (6.5, 10), (11.5, 14.5),
            (11.5, 11.5), (15.5, 11.5)], fill="green", outline="green_d")
