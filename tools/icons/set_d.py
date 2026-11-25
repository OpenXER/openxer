"""Main-window chrome extras: toolbar commands that had no OpenXER icon yet.

Drawn for OpenXER from the shared primitives. Their names start with OX.
"""
from iconlib import icon, Icon


def _pane(c: Icon, x0, y0, x1, y1):
    c.rect(x0, y0, x1, y1, fill="paper", outline="blue_d")
    c.rect(x0 + 1, y0 + 1, x1 - 1, y0 + 3, fill="blue_l")


@icon("OXLINENO16")
def _(c: Icon):
    # A hash sign for row numbers, drawn as two slanted and two flat strokes.
    for x in (6, 10):
        c.line([(x + 1.5, 2.5), (x - 1.5, 13.5)], "blue", 1.6)
    for y in (6, 10):
        c.line([(2.5, y + .5), (13.5, y + .5)], "blue", 1.6)


@icon("OXSPOTLT16")
def _(c: Icon):
    # A lamp casting a pale band of light across two bars.
    c.rect(1, 9, 15, 15, fill="yellow_l")
    c.bar(2, 10, 9, 12, "blue")
    c.bar(6, 13, 14, 15, "green")
    c.poly([(3, 1), (8, 1), (13, 9), (1, 9)], fill="yellow_l", outline="yellow_d")
    c.rrect(3, 0, 8, 3, r=1, fill="gray", outline="ink2")


@icon("OXPRGLIN16")
def _(c: Icon):
    # Bars with a red line zigzagging through their progress points.
    c.bar(1, 2, 12, 5, "blue")
    c.bar(4, 7, 15, 10, "blue")
    c.bar(2, 12, 10, 15, "blue")
    c.line([(8.5, 0.5), (6.5, 3.5), (10.5, 8.5), (5.5, 13.5), (8.5, 15.5)], "red", 1.4)


@icon("OXTABHZ16")
def _(c: Icon):
    # Two panes stacked: a new horizontal tab group.
    _pane(c, 1, 1, 15, 8)
    _pane(c, 1, 9, 15, 16)


@icon("OXTABVT16")
def _(c: Icon):
    # Two panes side by side: a new vertical tab group.
    _pane(c, 1, 1, 8, 15)
    _pane(c, 9, 1, 16, 15)


@icon("OXTABMG16")
def _(c: Icon):
    # Two pane outlines folding into one: merge tab groups.
    c.rect(1, 1, 10, 10, fill="gray_xl", outline="gray")
    _pane(c, 5, 5, 15, 15)
    c.arrow(2, 13, 6, 9, "green_d", 1.2, 2.5)


@icon("OXWHTNW16")
def _(c: Icon):
    # A small star burst over a page: what's new.
    c.doc(2, 3, 12, 15)
    c.hline(4, 10, 8, "gray")
    c.hline(4, 10, 10, "gray")
    c.hline(4, 9, 12, "gray")
    c.poly([(12, 0), (13.2, 3), (16, 3.6), (13.6, 5.4), (14.4, 8.5), (12, 6.8),
            (9.6, 8.5), (10.4, 5.4), (8, 3.6), (10.8, 3)], fill="orange", outline="orange_d")


@icon("OXONLHLP16")
def _(c: Icon):
    # A globe with a question mark badge: help on the web.
    c.circle(7, 7, 6.5, fill="blue_l", outline="blue_d")
    c.arc(3, 0.5, 11, 13.5, 0, 360, "blue_d", 1)
    c.hline(1, 13, 7, "blue_d")
    c.circle(12, 12, 4, fill="yellow", outline="yellow_d")
    c.arc(10.4, 9.4, 13.6, 12.4, 180, 30, "ink", 1.3)
    c.vline(12, 12, 13, "ink")
    c.px(12, 14, "ink")


def _linked_bars(c: Icon):
    # Two activity bars, one finishing before the next starts.
    c.bar(0, 1, 9, 6, "blue")
    c.bar(6, 10, 16, 15, "blue")


@icon("OXLINK16")
def _(c: Icon):
    # Two bars tied finish to start: link the selected activities.
    _linked_bars(c)
    c.line([(9, 3.5), (11.5, 3.5), (11.5, 7)], "ink", 1)
    c.tri(11.5, 8.2, 3.4, "down", fill="ink")


@icon("OXUNLINK16")
def _(c: Icon):
    # The same two bars with the tie cut by a red cross.
    _linked_bars(c)
    c.line([(9, 3.5), (11.5, 3.5)], "ink", 1)
    c.cross(3.5, 11.5, r=2.5, color="red", width=1.8)


def _path_bars(c: Icon, color="red"):
    # Three bars stepping down: a chain of activities on one path.
    c.bar(0, 1, 6, 4, color)
    c.bar(4, 6, 10, 9, color)
    c.bar(8, 11, 14, 14, color)
    c.line([(5.5, 4), (5.5, 6)], "ink", 1)
    c.line([(9.5, 9), (9.5, 11)], "ink", 1)


@icon("OXACPSHOW16")
def _(c: Icon):
    # The critical chain through an activity: red bars, the middle one dark.
    _path_bars(c)
    c.bar(4, 6, 10, 9, "red_d")


@icon("OXACPMORE16")
def _(c: Icon):
    # The chain with a plus: show one more path.
    _path_bars(c)
    c.badge_plus(12, 4, 3)


@icon("OXACPFEWER16")
def _(c: Icon):
    # The chain with a minus: show one path less.
    _path_bars(c)
    c.badge_minus(12, 4, 3)


@icon("OXACPSELECT16")
def _(c: Icon):
    # The chain with a check mark: choose the paths to show.
    _path_bars(c, "blue")
    c.check(9, 1, 0.6, "green_d", 1.6)


@icon("OXACPGROUP16")
def _(c: Icon):
    # Two band rows, each holding a short red chain: group by path.
    c.rect(0, 1, 16, 4, fill="yellow_l", outline="yellow_d")
    c.bar(3, 5, 9, 7, "red")
    c.rect(0, 9, 16, 12, fill="yellow_l", outline="yellow_d")
    c.bar(5, 13, 13, 15, "red")


@icon("OXACPBACK16")
def _(c: Icon):
    # The chain with an arrow pointing back to where it starts.
    _path_bars(c)
    c.arrow(15, 2.5, 9, 2.5, "green_d", 1.4, 3)


@icon("OXACPFWD16")
def _(c: Icon):
    # The chain with an arrow pointing on to where it leads.
    _path_bars(c)
    c.arrow(9, 2.5, 15, 2.5, "green_d", 1.4, 3)


@icon("OXACPRES16")
def _(c: Icon):
    # The chain with a person: paths through resource use.
    _path_bars(c)
    c.person(12, 0, "blue", 0.7)


# ------------------------------------------------- the Reports view

@icon("OXRUNBAT16")
def _(c: Icon):
    # A stack of report pages with a green play triangle: run a batch.
    c.doc(3, 0, 13, 11, fold=3)
    c.doc(1, 3, 11, 14, fold=3)
    c.hline(3, 8, 8, "gray")
    c.hline(3, 8, 10, "gray")
    c.circle(11.5, 11.5, 4.2, fill="green", outline="green_d")
    c.tri(12, 11.5, 4.2, "right", fill="paper")


@icon("OXRPTWIZ16")
def _(c: Icon):
    # A report page with a small yellow spark: made with the wizard.
    c.doc(1, 1, 12, 15, fold=3, lines=4)
    c.poly([(12, 5), (13, 8), (16, 9), (13, 10), (12, 13), (11, 10), (8, 9), (11, 8)],
           fill="yellow", outline="orange_d")


@icon("OXRPTEDT16")
def _(c: Icon):
    # A report page with a pencil across its corner: made in the editor.
    c.doc(1, 1, 12, 15, fold=3, lines=4)
    c.poly([(8.5, 12.5), (13.5, 7.5), (15.5, 9.5), (10.5, 14.5), (8, 15)],
           fill="orange_l", outline="orange_d")
    c.line([(13.5, 7.5), (15.5, 9.5)], "orange_d", 1.2)


# ------------------------------------------------- the Resources window

@icon("OXMATRSRC16")
def _(c: Icon):
    # Three stacked bricks: a material resource.
    c.rect(0, 9, 8, 15, fill="orange", outline="orange_d")
    c.rect(8, 9, 16, 15, fill="orange", outline="orange_d")
    c.rect(4, 3, 12, 9, fill="orange", outline="orange_d")
    c.hline(1, 7, 10, "orange_l")
    c.hline(9, 15, 10, "orange_l")
    c.hline(5, 11, 4, "orange_l")

# ------------------------------------------------- the notebook text toolbar

@icon("OXBOLD16")
def _(c: Icon):
    # A heavy capital B drawn as a stem and two rounded bowls.
    c.line([(4.5, 2.5), (4.5, 13.5)], "ink", 2.6)
    c.line([(4.5, 2.5), (9, 2.5), (11, 4), (11, 6), (9, 7.5), (4.5, 7.5)], "ink", 2.4)
    c.line([(4.5, 7.5), (9.5, 7.5), (12, 9.3), (12, 11.7), (9.5, 13.5), (4.5, 13.5)], "ink", 2.4)


@icon("OXITALIC16")
def _(c: Icon):
    # A slanted capital I with short serifs.
    c.line([(9.5, 2.5), (6.5, 13.5)], "ink", 1.6)
    c.hline(7, 13, 2, "ink")
    c.hline(4, 10, 13, "ink")


@icon("OXUNDERL16")
def _(c: Icon):
    # A capital U over a blue rule.
    c.line([(4.5, 1.5), (4.5, 8.5), (6, 11), (10, 11), (11.5, 8.5), (11.5, 1.5)], "ink", 1.6)
    c.hline(3, 13, 14, "blue_d", 1)


def _text_lines(c: Icon):
    for y in (3, 8, 13):
        c.hline(7, 15, y, "ink2")


@icon("OXBULLET16")
def _(c: Icon):
    # Three short lines each led by a round blue dot.
    _text_lines(c)
    for y in (3, 8, 13):
        c.circle(3, y + .5, 1.6, fill="blue", outline="blue_d")


@icon("OXNUMBER16")
def _(c: Icon):
    # Three short lines led by the figures one, two and three.
    _text_lines(c)
    c.line([(3, 1.5), (3, 5)], "blue_d", 1.2)
    c.line([(1.5, 6.6), (3.5, 6.4), (3.6, 8), (1.5, 10), (4, 10)], "blue_d", 1.1)
    c.line([(1.5, 11.6), (3.8, 11.6), (2.6, 13), (3.9, 14), (3, 15.4), (1.5, 15.2)], "blue_d", 1.1)


@icon("OXCLRFMT16")
def _(c: Icon):
    # A plain letter A with a red stroke through its corner: formatting off.
    c.line([(2.5, 13.5), (6.5, 2.5), (10.5, 13.5)], "ink", 1.6)
    c.hline(4, 9, 9, "ink")
    c.cross(12, 12, r=3, color="red", width=1.8)