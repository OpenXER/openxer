"""OpenXER icon set C: report pages, resources, save/run, steps, templates,
timesheets, WBS, zoom and the VMENU_ICO_* view-menu commands.

Original drawings in the shared iconlib style.
"""
import math

from iconlib import icon, Icon, P, fam


# ------------------------------------------------------------ local helpers
def page_frame(c: Icon):
    """Report page viewer frame: rounded page with a grey title strip."""
    c.rrect(1, 1, 15, 15, r=1.5, fill="paper", outline="ink2")
    c.rect(2, 2, 14, 4, fill="gray_l")
    c.hline(2, 14, 4, "gray")


def resource(c: Icon, cx=8, top=3, color="blue", s=1.0):
    c.person(cx, top, color=color, s=s)


def magnifier(c: Icon, cx=7, cy=7, r=5.5, handle="blue", lens="paper"):
    """Lens with a thick diagonal handle to the bottom-right."""
    f, dk, _ = fam(handle)
    a = math.radians(45)
    hx0, hy0 = cx + r * math.cos(a) - .3, cy + r * math.sin(a) - .3
    c.line([(hx0, hy0), (15, 15)], dk, 3.2, cap=False)
    c.line([(hx0 + .3, hy0 + .3), (14.6, 14.6)], f, 1.8, cap=False)
    c.circle(cx, cy, r, fill=lens, outline="ink", width=1.4)


def floppy(c: Icon, x0=1, y0=1, x1=15, y1=15, color="blue"):
    f, dk, lt = fam(color)
    c.rrect(x0, y0, x1, y1, r=1.2, fill=f, outline=dk)
    w = x1 - x0
    # label
    c.rect(x0 + 2, y0 + 1, x1 - 2, y0 + int(w * .45), fill="paper")
    c.hline(x0 + 3, x1 - 3, y0 + 2, "gray_l")
    c.hline(x0 + 3, x1 - 3, y0 + 4, "gray_l")
    # shutter
    sh = max(4, round(w * .33))
    c.rect(x0 + 3, y1 - sh, x1 - 3, y1, fill="gray_l")
    c.rect(x1 - 6, y1 - sh + 1, x1 - 4, y1 - 1, fill=dk)


def sparkle(c: Icon, cx, cy, color="yellow"):
    """Four-point twinkle, 5x5 px, centred on pixel (cx,cy)."""
    c.rect(cx - 2, cy, cx + 3, cy + 1, fill=color)
    c.rect(cx, cy - 2, cx + 1, cy + 3, fill=color)
    c.px(cx, cy, "orange_d")


def wand(c: Icon, x0, y0, x1, y1):
    c.line([(x0, y0), (x1, y1)], "ink", 2.2, cap=False)
    c.line([(x0, y0), (x1, y1)], "gray_l", 1.0, cap=False)
    c.circle(x1, y1, 1.1, fill="paper", outline="ink", width=.8)


def clock(c: Icon, cx, cy, r, face="yellow_l", rim="yellow_d"):
    c.circle(cx, cy, r, fill=face, outline=rim, width=1.1)
    c.line([(cx, cy - r * .62), (cx, cy), (cx + r * .5, cy + r * .2)], "ink", 1.0, cap=False)


def tile(c: Icon, x0, y0, x1, y1, color="blue"):
    """WBS-style square node: filled, dark rim, light inner bevel."""
    f, dk, lt = fam(color)
    c.rrect(x0, y0, x1, y1, r=1.2, fill=f, outline=dk)
    c.hline(x0 + 1, x1 - 2, y0 + 1, lt)
    c.vline(x0 + 1, y0 + 1, y1 - 2, lt)


def node(c: Icon, x0, y0, s=4, color="yellow"):
    """Org-chart box."""
    f, dk, _ = fam(color)
    c.rrect(x0, y0, x0 + s, y0 + s, r=.8, fill=f, outline="ink")


def abc_check(c: Icon, color="navy"):
    # A
    c.vline(1, 2, 7); c.vline(4, 2, 7); c.hline(2, 4, 1); c.hline(2, 4, 4)
    # B
    c.vline(6, 1, 7); c.hline(7, 9, 1); c.hline(7, 9, 4); c.hline(7, 9, 6)
    c.vline(9, 2, 4); c.vline(9, 5, 6)
    # C
    c.vline(11, 2, 6); c.hline(12, 14, 1); c.hline(12, 14, 6)
    c.line([(2, 10.5), (5.5, 14), (14.5, 7)], color, 2.0, cap=False)


def bucket(c: Icon, x0=0, y0=0):
    """Paint can tipped 45 degrees, spilling colour from its mouth."""
    pts = [(x0 + 1, y0 + 5), (x0 + 5, y0 + 1), (x0 + 10, y0 + 6), (x0 + 6, y0 + 10)]
    c.poly(pts, fill="gray_l")
    c.line(pts + [pts[0]], "ink", 1.0, cap=False)
    # handle loop on the base
    c.arc(x0 - 1, y0 - 1, x0 + 6, y0 + 6, 180, 270, "ink2", 1.2)
    # paint at the mouth and a drip
    c.line([(x0 + 10, y0 + 6), (x0 + 6, y0 + 10)], "blue", 1.6, cap=False)
    c.rect(x0 + 10, y0 + 7, x0 + 12, y0 + 11, fill="blue")
    c.px(x0 + 11, y0 + 11, "blue_d")


def tag(c: Icon, x0, y0, color="yellow"):
    """Code tag, pointing right, 7x6."""
    f, dk, _ = fam(color)
    pts = [(x0 + .5, y0 + .5), (x0 + 4.5, y0 + .5), (x0 + 6.8, y0 + 3),
           (x0 + 4.5, y0 + 5.5), (x0 + .5, y0 + 5.5)]
    c.poly(pts, fill=f, outline=dk)
    c.px(x0 + 4, y0 + 2, dk)


def checkbox(c: Icon):
    c.rrect(1, 1, 15, 15, r=1.5, fill="gray_l", outline="ink2")
    c.rect(3, 3, 13, 13, fill="paper", outline="gray")


def stairs(c: Icon):
    pts = [(.5, 14.5), (.5, 10.5), (5.5, 10.5), (5.5, 6.5), (10.5, 6.5),
           (10.5, 2.5), (15.5, 2.5), (15.5, 14.5)]
    c.poly(pts, fill="gray_l", outline="ink2")
    c.line([(1.5, 11.5), (6.5, 11.5)], "paper")
    c.line([(6.5, 7.5), (11.5, 7.5)], "paper")
    c.line([(11.5, 3.5), (14.5, 3.5)], "paper")


def certificate(c: Icon):
    c.rrect(2, 0, 15, 12, r=1.2, fill="yellow_l", outline="yellow_d")
    c.hline(5, 13, 3, "ink2")
    c.hline(5, 12, 5, "gray")
    c.hline(8, 13, 7, "gray")
    # rosette
    c.poly([(3, 11), (1.5, 15.5), (3.6, 14.2), (4.6, 16)], fill="blue_d")
    c.poly([(6, 11), (7.5, 15.5), (5.4, 14.2), (4.4, 16)], fill="blue_d")
    c.circle(4.5, 10, 2.6, fill="blue", outline="blue_d", width=1)
    c.circle(4.5, 10, 1, fill="blue_l")


def org3(c: Icon, top_x):
    """Org chart: one node on top, three underneath."""
    node(c, top_x, 1)
    cx = top_x + 2
    c.vline(cx, 5, 8, "ink")
    c.hline(2, 15, 8, "ink")
    for x in (2, 7, 13):
        c.vline(x, 8, 10, "ink")
    node(c, 0, 10); node(c, 5, 10); node(c, 11, 10)


def folder_gray(c: Icon, x0, y0, x1, y1):
    """Template folder: cool grey, rounded body, short tab."""
    tw = max(3, (x1 - x0) // 3)
    c.rrect(x0, y0, x0 + tw + 1, y0 + 4, r=1, fill="gray", outline="ink2")
    c.rrect(x0, y0 + 2, x1, y1, r=1, fill="gray_l", outline="ink2")
    c.hline(x0 + 1, x1 - 1, y0 + 3, "paper")


def ptri(c: Icon, x, y, n, direction, color="ink"):
    """Pixel-exact triangle, 2n px tall/wide. (x,y) is the top-left of its
    bounding box; the box is n x 2n (left/right) or 2n x n (up/down)."""
    for k in range(2 * n):
        d = min(k, 2 * n - 1 - k) + 1
        if direction == "right":
            c.hline(x, x + d, y + k, color)
        elif direction == "left":
            c.hline(x + n - d, x + n, y + k, color)
        elif direction == "down":
            c.vline(x + k, y, y + d, color)
        else:
            c.vline(x + k, y + n - d, y + n, color)


# ------------------------------------------------------------ report pages
@icon("RPPGF16")
def _(c: Icon):
    page_frame(c)
    c.vline(3, 6, 13, "navy", 2)
    ptri(c, 6, 6, 3, "left", "navy")
    ptri(c, 9, 6, 3, "left", "navy")
    c.hline(9, 12, 9, "navy")


@icon("RPPGL16")
def _(c: Icon):
    page_frame(c)
    c.hline(4, 7, 9, "navy")
    ptri(c, 4, 6, 3, "right", "navy")
    ptri(c, 7, 6, 3, "right", "navy")
    c.vline(11, 6, 13, "navy", 2)


@icon("RPPGN16")
def _(c: Icon):
    page_frame(c)
    ptri(c, 6, 6, 4, "right", "navy")
    c.hline(4, 6, 9, "navy", 2)


@icon("RPPGP16")
def _(c: Icon):
    page_frame(c)
    ptri(c, 6, 6, 4, "left", "navy")
    c.hline(10, 12, 9, "navy", 2)


@icon("RPTGLB16")
def _(c: Icon):
    c.circle(6.5, 6.5, 6, fill="blue_l", outline="blue_d", width=1)
    c.poly([(2.5, 3.5), (6, 1.5), (9.5, 3.5), (8, 6), (4.5, 5.5), (3, 8.5), (1.5, 6)],
           fill="green")
    c.clear(6, 6, 16, 16)
    c.doc(7, 7, 16, 16, fold=2, lines=3)


@icon("RPTGRP16")
def _(c: Icon):
    c.doc(1, 0, 11, 11, fold=3, lines=3)
    c.clear(5, 4, 16, 16)
    c.doc(6, 5, 16, 16, fold=3, lines=4)


@icon("RPTWZD16")
def _(c: Icon):
    c.doc(7, 3, 16, 16, fold=3, lines=4)
    wand(c, 1, 15, 8, 8)
    sparkle(c, 3, 3)
    sparkle(c, 9, 1)


# ------------------------------------------------------------ resources
@icon("RSATSK16")
def _(c: Icon):
    c.bar(0, 3, 9, 9, "green")
    resource(c, 11.5, 5)


@icon("RSCNWK16")
def _(c: Icon):
    resource(c, 6, 5)
    # yellow key, top right
    c.rrect(11, 0, 16, 5, r=1.5, fill="yellow", outline="yellow_d")
    c.px(13, 2, "paper")
    c.rect(6, 2, 11, 3, fill="yellow_d")
    c.rect(7, 3, 8, 5, fill="yellow_d")
    c.rect(9, 3, 10, 4, fill="yellow_d")


@icon("RSCSPD16")
def _(c: Icon):
    c.grid(0, 1, 10, 15, cols=2, rows=4, first_col="blue_l")
    c.clear(7, 4, 16, 16)
    resource(c, 11.5, 5)


@icon("RSRC16")
def _(c: Icon):
    resource(c, 8, 3, s=1.15)


@icon("RSRCCD16")
def _(c: Icon):
    resource(c, 6, 3)
    tag(c, 9, 6)


@icon("RSRCOP16")
def _(c: Icon):
    c.rrect(1, 2, 10, 12, r=1.5, fill="gray_l", outline="ink2")
    c.hline(2, 9, 3, "paper")
    c.clear(7, 4, 16, 16)
    resource(c, 11.5, 5)


@icon("RSRCPR16")
def _(c: Icon):
    resource(c, 7, 5)
    c.vline(12, 0, 7, "ink")
    c.poly([(13, .5), (15.8, 2.2), (13, 4)], fill="yellow", outline="yellow_d")


@icon("RSRCU16")
def _(c: Icon):
    c.person(6, 1, color="gray")
    resource(c, 9.5, 5.5)


@icon("RSRCUC16")
def _(c: Icon):
    c.person(5, 1, color="gray")
    resource(c, 7.5, 5)
    c.badge_current()


@icon("RSRCUR16")
def _(c: Icon):
    resource(c, 7, 3)
    c.badge_current()


# ------------------------------------------------------------ run / save
@icon("RUN16")
def _(c: Icon):
    c.poly([(4.5, 1.5), (13, 8), (4.5, 14.5)], fill="green", outline="green_d")
    c.line([(5.5, 3.5), (5.5, 8)], "green_l")


@icon("RUNBAT16")
def _(c: Icon):
    c.poly([(1.5, 2.5), (8.5, 8), (1.5, 13.5)], fill="green", outline="green_d")
    c.poly([(7.5, 2.5), (14.5, 8), (7.5, 13.5)], fill="green", outline="green_d")


@icon("SAVE16")
def _(c: Icon):
    floppy(c)


@icon("SAVEAS16")
def _(c: Icon):
    c.doc(0, 0, 9, 10, fold=3)
    c.clear(3, 3, 16, 16)
    floppy(c, 4, 4, 16, 16)


@icon("SCHDUL16")
def _(c: Icon):
    # F9 Schedule: pale clock with a rose rim, green "run" triangle at the
    # lower right.
    # Clock: 12 px disc centred on the pixel edge (7,7) -> pixels 1..12.
    c.circle(7.4, 7.4, 6, fill="red")                  # lower-right shade
    c.circle(7, 7, 6, fill=(232, 136, 140))            # rose rim
    c.circle(7, 7, 5, fill=(222, 236, 252))            # pale blue face
    c.circle(6.5, 6.5, 3.5, fill=(240, 247, 255))      # face highlight
    for x, y in ((6, 3), (10, 6), (6, 10), (3, 6)):    # 12 / 3 / 6 / 9 ticks
        c.px(x, y, "gray")
    c.vline(6, 3, 7, "ink")                            # hour hand to 12
    c.px(7, 4, "ink2")                                 # thicken hand tip side
    for x, y in ((5, 7), (4, 8), (3, 9)):              # minute hand ~7:35
        c.px(x, y, "ink2")
    c.px(6, 7, "ink")                                  # hub
    # Run triangle, lower right, overlapping the rim.
    c.poly([(10, 6), (16, 11), (10, 16)], fill=(70, 150, 84))
    c.poly([(11, 7.8), (14.8, 11), (11, 14.2)], fill=(132, 214, 142))
    c.vline(11, 8, 14, (176, 232, 182))                # light leading edge


@icon("SEARCH16")
def _(c: Icon):
    c.rect(0, 9, 7, 16, fill="yellow")
    # highlighter marker
    c.line([(5, 11), (14, 2)], "ink", 4.0, cap=False)
    c.line([(5.4, 10.6), (13.6, 2.4)], "purple_l", 2.2, cap=False)
    c.line([(3.5, 12.5), (6, 10)], "red", 2.6, cap=False)


@icon("SELBX016")
def _(c: Icon):
    checkbox(c)


@icon("SELBX116")
def _(c: Icon):
    checkbox(c)
    c.check(4, 4.5, s=1.05, color="red", width=2.0)


@icon("SELECT16")
def _(c: Icon):
    c.window(0, 1, 12, 9, title="gray_l")
    c.clear(5, 5, 16, 16)
    c.plus(11, 11, r=4, t=3, color="green", outline=P["green_d"])


@icon("SHOWRELBTN16")
def _(c: Icon):
    c.line([(3.5, 1), (3.5, 9.5), (12.5, 9.5), (12.5, 11)], "purple_d", 2.0, cap=False)
    ptri(c, 10, 11, 3, "down", "purple_d")


@icon("SHOWRESCURVES16")
def _(c: Icon):
    c.rrect(0, 0, 16, 16, r=1.5, fill="paper", outline="gray")
    for x, h in ((2, 5), (5, 9), (8, 10), (11, 6)):
        c.rect(x, 15 - h, x + 2, 15, fill="yellow", outline=None)
        c.vline(x + 2, 15 - h, 15, "yellow_d")
    c.line([(1.5, 9), (5, 4), (8.5, 3), (13, 6), (14.5, 9)], "blue", 1.0, cap=False)


@icon("SPLCHK16")
def _(c: Icon):
    abc_check(c, "navy")


@icon("SPNDWN16")
def _(c: Icon):
    ptri(c, 4, 6, 4, "down", "ink")


@icon("SPNUP16")
def _(c: Icon):
    ptri(c, 4, 6, 4, "up", "ink")


# ------------------------------------------------------------ standards, steps
@icon("STDCUR16")
def _(c: Icon):
    certificate(c)
    c.badge_current()


@icon("STEPD16")
def _(c: Icon):
    stairs(c)
    c.check(5, 5, s=1.1, color="yellow_d", width=2.6)
    c.check(5, 5, s=1.1, color="yellow", width=1.4)


@icon("STEPND16")
def _(c: Icon):
    stairs(c)


@icon("STNDRD16")
def _(c: Icon):
    certificate(c)


@icon("SUBMIT16")
def _(c: Icon):
    c.grid(0, 0, 13, 14, cols=3, rows=4, head="blue", first_col="blue_l")
    c.clear(10, 8, 16, 16)
    c.poly([(10.5, 9.5), (15.5, 12.5), (10.5, 15.5)], fill="yellow", outline="yellow_d")


@icon("THSHLD16")
def _(c: Icon):
    c.rrect(4, 0, 12, 6, r=1, fill="red", outline="red_d")
    c.hline(5, 11, 1, "red_l")
    for x in (0, 4, 8, 12):
        c.rect(x, 7, x + 3, 9, fill="red")
    c.rrect(4, 10, 12, 16, r=1, fill="green", outline="green_d")
    c.hline(5, 11, 11, "green_l")


# ------------------------------------------------------------ timesheets, templates
@icon("TMAPVL16")
def _(c: Icon):
    clock(c, 5, 5, 4.5)
    c.line([(6, 5.5), (8.5, 8), (14.5, 1.5)], "green", 2.0, cap=False)
    c.line([(6, 11.5), (8.5, 14.5), (14.5, 8)], "red", 2.0, cap=False)


@icon("TMPLAT16")
def _(c: Icon):
    folder_gray(c, 1, 2, 15, 14)


@icon("TMSHT16")
def _(c: Icon):
    c.grid(0, 0, 15, 14, cols=3, rows=4, head="blue", first_col="blue_l")
    c.clear(6, 6, 16, 16)
    clock(c, 11.5, 11.5, 4.3)


@icon("TPLGIN16")
def _(c: Icon):
    c.rrect(4, 6, 16, 16, r=1.2, fill="gray_xl", outline="gray")
    c.line([(7.5, 8.5), (12, 13)], "ink", 1.0, cap=False)
    c.hline(9, 14, 13, "ink", 1)
    c.vline(13, 9, 14, "ink", 1)
    folder_gray(c, 0, 0, 10, 8)


@icon("TPOOL16")
def _(c: Icon):
    folder_gray(c, 0, 0, 10, 7)
    folder_gray(c, 3, 4, 13, 11)
    folder_gray(c, 6, 8, 16, 15)


@icon("TRACK16")
def _(c: Icon):
    c.rrect(0, 0, 16, 16, r=1.5, fill="paper", outline="ink2")
    c.rect(1, 1, 15, 3, fill="gray_l")
    c.rect(3, 11, 5, 15, fill="blue")
    c.rect(6, 10, 8, 15, fill="green")
    c.rect(9, 12, 11, 15, fill="blue")
    c.rect(12, 8, 14, 15, fill="green")
    c.arrow(2, 9, 13.5, 4, "red", width=1.1, head=3)


@icon("UNDO16")
def _(c: Icon):
    # hooked arrow: up from the bottom right, over, back to the left
    c.arc(4, 4, 15, 15, 270, 360, "blue_d", 3.6)
    c.arc(5, 5, 14, 14, 270, 360, "blue", 1.6)
    c.rect(11, 9, 15, 15, fill="blue_d")
    c.rect(12, 9, 14, 14, fill="blue")
    c.rect(5, 4, 10, 8, fill="blue_d")
    c.rect(5, 5, 10, 7, fill="blue")
    ptri(c, 1, 2, 4, "left", "blue_d")
    ptri(c, 2, 3, 3, "left", "blue")


@icon("UNLOCK16")
def _(c: Icon):
    # shackle lifted and swung open: left leg free
    c.rrect(1, 0, 9, 12, r=3.5, outline="ink2", width=2)
    c.clear(0, 5, 4, 12)
    c.rrect(5, 7, 16, 16, r=1.2, fill="yellow", outline="yellow_d")
    c.hline(6, 15, 8, "yellow_l")
    c.rect(10, 10, 11, 14, fill="yellow_d")
    c.rect(9, 10, 12, 12, fill="yellow_d")


@icon("UNSEL16")
def _(c: Icon):
    c.grid(0, 0, 13, 12, cols=3, rows=4)
    c.clear(7, 7, 16, 16)
    c.cross(11.5, 11.5, r=3, color="red", width=2.4)


@icon("UPARW16")
def _(c: Icon):
    ptri(c, 2, 5, 6, "up", "ink")


# ------------------------------------------------------------ users, views
@icon("USER16")
def _(c: Icon):
    c.person(8, 3, color="gray", s=1.15)


@icon("USRCUR16")
def _(c: Icon):
    c.person(6, 2, color="gray")
    c.badge_current()


@icon("VIEWS16")
def _(c: Icon):
    c.rrect(1, 0, 15, 15, r=1.5, fill="paper", outline="ink2")
    c.rect(2, 1, 14, 3, fill="gray_l")
    c.line([(3, 6), (6, 9), (9, 7), (13, 4)], "green", 1.2, cap=False)
    c.hline(3, 9, 12, "gray")
    c.clear(8, 8, 16, 16)
    c.circle(11, 11, 2.6, fill="blue_l", outline="ink", width=1)
    c.line([(13, 13), (15.5, 15.5)], "blue_d", 2.0, cap=False)


# ------------------------------------------------------------ VMENU: chart arrangement
@icon("VMENU_ICO_ALIGN_CENTER")
def _(c: Icon):
    org3(c, 6)


@icon("VMENU_ICO_ALIGN_LEFT")
def _(c: Icon):
    node(c, 0, 1)
    c.vline(2, 5, 10, "ink")
    c.hline(2, 15, 8, "ink")
    for x in (7, 13):
        c.vline(x, 8, 10, "ink")
    node(c, 0, 10); node(c, 5, 10); node(c, 11, 10)


@icon("VMENU_ICO_ALIGN_RIGHT")
def _(c: Icon):
    node(c, 11, 1)
    c.vline(13, 5, 10, "ink")
    c.hline(2, 13, 8, "ink")
    for x in (2, 7):
        c.vline(x, 8, 10, "ink")
    node(c, 0, 10); node(c, 5, 10); node(c, 11, 10)


@icon("VMENU_ICO_ARRANGE_HORZ")
def _(c: Icon):
    org3(c, 6)
    c.hline(0, 16, 15, "gray")


@icon("VMENU_ICO_ARRANGE_VERT")
def _(c: Icon):
    node(c, 1, 0)
    c.vline(3, 4, 13, "ink")
    c.hline(3, 10, 8, "ink")
    c.hline(3, 10, 13, "ink")
    node(c, 10, 6)
    node(c, 10, 11)


@icon("VMENU_ICO_CHART_FONT_COLOR")
def _(c: Icon):
    c.doc(4, 3, 16, 16, fold=3)
    c.rect(6, 11, 14, 14, fill="yellow", outline="yellow_d")
    bucket(c, 0, 0)


@icon("VMENU_ICO_CHART_LAYOUT")
def _(c: Icon):
    c.rrect(0, 1, 16, 5, r=.8, fill="yellow", outline="ink")
    c.rrect(0, 6, 16, 10, r=.8, fill="yellow", outline="ink")
    c.rrect(0, 11, 8, 15, r=.8, fill="yellow", outline="ink")
    c.rrect(9, 11, 16, 15, r=.8, fill="yellow", outline="ink")


@icon("VMENU_ICO_CHART_VIEW")
def _(c: Icon):
    c.rrect(4, 1, 12, 6, r=.8, fill="yellow", outline="ink")
    c.vline(8, 6, 8, "ink")
    c.hline(3, 14, 8, "ink")
    c.vline(3, 8, 10, "ink")
    c.vline(13, 8, 10, "ink")
    c.rrect(0, 10, 7, 15, r=.8, fill="yellow", outline="ink")
    c.rrect(9, 10, 16, 15, r=.8, fill="yellow", outline="ink")


@icon("VMENU_ICO_COLLAPSE_ALL")
def _(c: Icon):
    c.plus(4, 4, r=3, t=2, color="navy")
    c.plus(12, 12, r=3, t=2, color="navy")


@icon("VMENU_ICO_COLLAPSE_TO")
def _(c: Icon):
    c.minus(4, 4, r=3, t=2, color="blue")
    c.plus(12, 12, r=3, t=2, color="blue")


@icon("VMENU_ICO_EXPAND_ALL")
def _(c: Icon):
    c.minus(4, 4, r=3, t=2, color="navy")
    c.minus(12, 12, r=3, t=2, color="navy")


# ------------------------------------------------------------ VMENU: grid
@icon("VMENU_ICO_COLUMNS")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="paper", outline="ink")
    c.rect(2, 2, 14, 4, fill="gray_l")
    c.vline(5, 2, 14, "ink2")
    c.vline(10, 2, 14, "ink2")
    for y in (7, 10, 13):
        c.hline(2, 14, y, "gray_l")


@icon("VMENU_ICO_FILLDOWN")
def _(c: Icon):
    c.rect(1, 2, 9, 4, fill="navy")
    c.rect(2, 7, 9, 9, fill="navy")
    c.rect(3, 12, 9, 14, fill="navy")
    c.arrow(13, 1, 13, 15, "ink", width=1.4, head=4)


@icon("VMENU_ICO_GROUPING")
def _(c: Icon):
    c.rrect(1, 1, 15, 15, r=1, fill="teal", outline="ink")
    c.rect(4, 4, 14, 14, fill="green")
    c.rect(6, 7, 14, 14, fill="paper")
    c.hline(6, 14, 10, "gray")
    c.rrect(1, 1, 15, 15, r=1, outline="ink")


@icon("VMENU_ICO_GRID_FONT_COLOR")
def _(c: Icon):
    c.grid(4, 5, 16, 16, cols=3, rows=3)
    c.rect(5, 9, 15, 12, fill="yellow")
    bucket(c, 0, 0)


# ------------------------------------------------------------ VMENU: edit
@icon("VMENU_ICO_COPY")
def _(c: Icon):
    c.doc(0, 0, 9, 11, fold=3, lines=3, line_color="ink2")
    c.clear(5, 4, 16, 16)
    c.doc(6, 5, 16, 16, fold=3, lines=3, outline="navy", line_color="navy")


@icon("VMENU_ICO_CUT")
def _(c: Icon):
    c.line([(5, 1), (10, 10)], "ink2", 1.4, cap=False)
    c.line([(11, 1), (6, 10)], "ink2", 1.4, cap=False)
    c.circle(5, 12.5, 2.4, fill=None, outline="ink", width=1.3)
    c.circle(11, 12.5, 2.4, fill=None, outline="ink", width=1.3)
    c.px(7, 7, "ink")


@icon("VMENU_ICO_DELETE")
def _(c: Icon):
    c.line([(2.5, 2.5), (13.5, 13.5)], "ink", 2.4, cap=False)
    c.line([(13.5, 2.5), (2.5, 13.5)], "ink", 2.4, cap=False)


@icon("VMENU_ICO_INSERT")
def _(c: Icon):
    c.doc(2, 1, 14, 15, fold=4, outline="ink")
    c.plus(11, 12, r=3, t=2, color="green", outline=P["green_d"])


@icon("VMENU_ICO_NEW")
def _(c: Icon):
    c.doc(2, 1, 14, 15, fold=4, outline="ink")
    sparkle(c, 12, 3)


@icon("VMENU_ICO_OPEN")
def _(c: Icon):
    c.folder(0, 5, 15, 15, color="yellow", open_=True)
    c.arc(6, 0, 14, 7, 200, 340, "ink", 1.2)
    c.tri(13.5, 4.5, 3.4, "down", fill="ink")


@icon("VMENU_ICO_PASTE")
def _(c: Icon):
    c.rrect(0, 1, 12, 15, r=1.2, fill="tan", outline="tan_d")
    c.rrect(3, 0, 9, 3, r=.6, fill="gray_l", outline="ink2")
    c.clear(5, 6, 16, 16)
    c.doc(6, 7, 16, 16, fold=3, lines=2, outline="navy", line_color="navy")


@icon("VMENU_ICO_PRINT")
def _(c: Icon):
    c.rect(4, 1, 12, 6, fill="paper", outline="ink")
    c.rrect(0, 5, 16, 13, r=1.5, fill="gray_l", outline="ink")
    c.hline(1, 15, 6, "paper")
    c.rect(11, 8, 13, 9, fill="green")
    c.rect(3, 11, 13, 15, fill="paper", outline="ink")
    c.hline(5, 11, 13, "gray")


@icon("VMENU_ICO_PRINT_PREVIEW")
def _(c: Icon):
    c.doc(1, 0, 12, 15, fold=3, lines=4, outline="ink")
    c.clear(6, 6, 16, 16)
    c.circle(10, 10, 3.2, fill="teal_l", outline="ink", width=1.1)
    c.line([(12.5, 12.5), (15.5, 15.5)], "ink", 1.8, cap=False)


@icon("VMENU_ICO_SAVE")
def _(c: Icon):
    floppy(c, 1, 1, 15, 15, color="navy")


@icon("VMENU_ICO_SPELL_CHECK")
def _(c: Icon):
    abc_check(c, "blue")


@icon("VMENU_ICO_FILTER")
def _(c: Icon):
    c.poly([(.5, 1.5), (15.5, 1.5), (9.5, 8), (9.5, 14.5), (6.5, 14.5), (6.5, 8)],
           fill="gray_l", outline="ink")
    c.poly([(9, 2), (15, 2), (9, 8)], fill="gray")
    c.poly([(.5, 1.5), (15.5, 1.5), (9.5, 8), (9.5, 14.5), (6.5, 14.5), (6.5, 8)],
           outline="ink")


@icon("VMENU_ICO_FIND")
def _(c: Icon):
    for x in (1, 9):
        c.rrect(x + 1, 1, x + 5, 5, r=.6, fill="ink")
        c.rrect(x, 4, x + 6, 15, r=1.6, fill="ink")
        c.rect(x + 1, 11, x + 3, 13, fill="blue_l")
    c.rect(6, 6, 10, 10, fill="ink2")


# ------------------------------------------------------------ zoom
def zoom_plus(c: Icon):
    magnifier(c)
    c.plus(7, 7, r=3, t=2, color="ink")


def zoom_minus(c: Icon):
    magnifier(c)
    c.minus(7, 7, r=3, t=2, color="ink")


@icon("VMENU_ICO_ZOOM_BEST_FIT")
def _(c: Icon):
    magnifier(c)
    c.circle(7, 7, 3.5, fill="blue", outline="navy", width=.8)
    c.poly([(5.5, 4.5), (8.5, 5), (7.5, 7.5), (9, 9.5), (6, 9), (5, 6.5)], fill="green")


@icon("VMENU_ICO_ZOOM_IN")
def _(c: Icon):
    zoom_plus(c)


@icon("VMENU_ICO_ZOOM_OUT")
def _(c: Icon):
    zoom_minus(c)


@icon("ZOOMIN16")
def _(c: Icon):
    magnifier(c, handle="teal")
    c.plus(7, 7, r=3, t=2, color="ink")


@icon("ZOOMOT16")
def _(c: Icon):
    magnifier(c, handle="teal")
    c.minus(7, 7, r=3, t=2, color="ink")


@icon("ZOOMTF16")
def _(c: Icon):
    magnifier(c, handle="teal")
    c.rect(3, 4, 9, 9, fill="yellow_l", outline="yellow_d")
    c.rect(11, 2, 16, 6, fill="yellow", outline="yellow_d")
    c.hline(9, 11, 5, "gray")
    c.rect(4, 13, 8, 16, fill="yellow", outline="yellow_d")


# ------------------------------------------------------------ WBS
@icon("WBS16")
def _(c: Icon):
    tile(c, 0, 0, 10, 10, "blue")
    tile(c, 5, 5, 16, 16, "blue")


@icon("WBSLKC16")
def _(c: Icon):
    tile(c, 0, 0, 10, 10, "orange")
    tile(c, 5, 5, 16, 16, "orange")
    c.rect(7, 7, 14, 14, fill="orange_l")


@icon("WBSLKR16")
def _(c: Icon):
    tile(c, 0, 1, 10, 11, "orange")
    tile(c, 4, 6, 14, 16, "orange")
    c.clear(7, 0, 16, 7)
    c.rrect(8, 0, 16, 7, r=1, fill="paper", outline="ink2")
    c.rect(10, 2, 13, 3, fill="red")
    c.rect(10, 2, 11, 5, fill="red")
    for i in range(3):
        c.px(11 + i, 3 + i, "red")


@icon("WBSROT16")
def _(c: Icon):
    folder_gray(c, 0, 1, 13, 12)
    c.clear(5, 6, 16, 16)
    tile(c, 6, 7, 16, 16, "blue")


@icon("WIZARD16")
def _(c: Icon):
    wand(c, 2, 15, 10, 7)
    sparkle(c, 3, 3)
    sparkle(c, 12, 3)
    sparkle(c, 13, 12)
