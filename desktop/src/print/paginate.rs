//! Pagination math: scale, timescale stretch and page tiles. Lengths are
//! canvas pixels (96 per inch at 100%) unless named otherwise.
use super::setup::Scaling;

/// Pixels per inch of the canvas and the page layout.
pub const DPI: f32 = 96.;
/// Most pages one print job may produce.
pub const MAX_PAGES: usize = 2000;

fn too_many(pages: f64) -> String {
    format!(
        "This print needs about {pages:.0} pages; the limit is {MAX_PAGES}. Use Fit to, a larger paper size or a shorter timescale."
    )
}

/// Refuses a job whose estimated page count passes `MAX_PAGES`, before
/// anything is painted.
pub fn check_pages(
    canvas_width: f32,
    header_height: f32,
    body_height: f32,
    scale: f32,
    area: Area,
) -> Result<(), String> {
    let scale = scale.max(0.01) as f64;
    let across = (canvas_width as f64 * scale / area.width as f64)
        .ceil()
        .max(1.);
    let room = (area.height as f64 / scale - header_height as f64).max(1.);
    let down = (body_height as f64 / room).ceil().max(1.);
    if !(across * down).is_finite() || across * down > MAX_PAGES as f64 {
        return Err(too_many(across * down));
    }
    Ok(())
}

/// The printable body of a page (after margins, header and footer), in
/// page pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub width: f32,
    pub height: f32,
}

/// The chosen print scale and Gantt pixels per day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub scale: f32,
    pub pixels_per_day: f32,
    /// Body y (from the body start) from which pages print without the
    /// column header: the Trace Logic page, which has no table columns.
    pub headerless_from: Option<f32>,
    /// Timescale span in days: the extent's, or longer when the finish was
    /// extended to fill the page at the screen's zoom.
    pub span_days: f64,
    pub extended: bool,
}

/// What the canvas holds before it is scaled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    /// Width of the table part (0 when the table is not printed).
    pub table_width: f32,
    /// Timescale span in days (0 when the Gantt is not printed).
    pub span_days: f64,
    /// Column header height, repeated on every page.
    pub header_height: f32,
    /// Height of all rows.
    pub body_height: f32,
}

/// How the pages may flow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// One page across whatever the scaling says: Adjust is a ceiling, and
    /// Fit and Fit timescale use one page wide.
    pub one_wide: bool,
    /// A narrow timescale is extended (a later finish at the screen's zoom)
    /// to fill the page width instead of stretching the zoom.
    pub extend: bool,
    /// Width of other content that has to fit across (a printed bottom
    /// pane), which the table and Gantt do not account for.
    pub min_width: f32,
}

/// Picks the scale and the Gantt's pixels per day for a scaling choice.
/// Fit never enlarges text past 100%: when the content is narrower than the
/// pages, the timescale stretches to fill them instead.
#[cfg(test)]
pub fn plan(scaling: Scaling, area: Area, extent: Extent, screen_ppd: f32) -> Plan {
    plan_with(scaling, area, extent, screen_ppd, Flow::default())
}

/// `plan` with the page flow choices.
pub fn plan_with(
    scaling: Scaling,
    area: Area,
    extent: Extent,
    screen_ppd: f32,
    flow: Flow,
) -> Plan {
    let gantt = extent.span_days > 0.;
    let ppd = screen_ppd.max(1e-3);
    let stretch = |width: f32| -> f32 {
        if gantt {
            ((width - extent.table_width) / extent.span_days as f32).max(screen_ppd)
        } else {
            screen_ppd
        }
    };
    let wide_of = |wide: u32| if flow.one_wide { 1 } else { wide };
    // The span that makes the table and chart exactly `pages` pages wide
    // at `scale` and the screen's zoom, when that is longer than the extent.
    let fill = |scale: f32, pages: u32| -> (f64, bool) {
        if flow.extend && gantt {
            let room = pages as f32 * area.width / scale.max(1e-3) - extent.table_width;
            let span = f64::from(room / ppd).floor();
            if span > extent.span_days + 1e-6 {
                return (span, true);
            }
        }
        (extent.span_days, false)
    };
    let plain = |scale: f32, pixels_per_day: f32, (span_days, extended): (f64, bool)| Plan {
        scale,
        pixels_per_day,
        headerless_from: None,
        span_days,
        extended,
    };
    match scaling {
        Scaling::Adjust { percent } => {
            let mut scale = percent as f32 / 100.;
            if flow.one_wide {
                let width = (extent.table_width + extent.span_days as f32 * screen_ppd)
                    .max(flow.min_width)
                    .max(1.);
                scale = scale.min(area.width / width);
                return plain(scale, screen_ppd, fill(scale, 1));
            }
            plain(scale, screen_ppd, (extent.span_days, false))
        }
        Scaling::FitTimescale { wide } => {
            // On one page the table gives up size before the chart gets
            // less than a quarter of the width.
            let scale = if flow.one_wide {
                let table = extent.table_width.max(flow.min_width * 0.75);
                (0.75 * area.width / table.max(1.)).min(1.)
            } else {
                1.
            };
            let target = wide_of(wide) as f32 * area.width / scale;
            let room = (target - extent.table_width).max(area.width * 0.25 / scale);
            plain(
                scale,
                if gantt {
                    room / extent.span_days as f32
                } else {
                    screen_ppd
                },
                (extent.span_days, false),
            )
        }
        Scaling::Fit { wide, tall } => {
            let wide = wide_of(wide);
            let width =
                (extent.table_width + extent.span_days as f32 * screen_ppd).max(flow.min_width);
            let by_width = wide as f32 * area.width / width.max(1.);
            let by_height = if tall > 0 {
                tall as f32 * area.height
                    / (extent.body_height + tall as f32 * extent.header_height).max(1.)
            } else {
                f32::INFINITY
            };
            let scale = by_width.min(by_height);
            if scale <= 1. || !gantt {
                let scale = scale.min(1.);
                let span = if scale < by_width.min(1.) - 1e-6 {
                    fill(scale, wide)
                } else {
                    (extent.span_days, false)
                };
                plain(scale, screen_ppd, span)
            } else {
                let scale = by_height.min(1.);
                if flow.extend {
                    plain(scale, screen_ppd, fill(scale, wide))
                } else {
                    plain(
                        scale,
                        stretch(wide as f32 * area.width / scale),
                        (extent.span_days, false),
                    )
                }
            }
        }
    }
}

/// Fit to `tall` pages: the scale the height allows assumes the rows split
/// anywhere, but pages break between rows, so the last rows can spill onto
/// one more page. Shrinks `scale` in small steps until `blocks_at(scale)`
/// (the page rows at a scale) is at most `tall`.
pub fn fit_tall(tall: u32, scale: f32, blocks_at: &dyn Fn(f32) -> usize) -> f32 {
    let mut scale = scale;
    if tall == 0 {
        return scale;
    }
    for _ in 0..200 {
        if blocks_at(scale) <= tall as usize || scale <= 0.011 {
            break;
        }
        scale = (scale * 0.98).max(0.01);
    }
    scale
}

/// One row of the body: top (from the body start), height, and whether a
/// page break is forced before it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowSlot {
    pub top: f32,
    pub height: f32,
    pub break_before: bool,
    /// The page never breaks between this row and the next one (a band
    /// header stays with its first row).
    pub keep_with_next: bool,
    /// Top and height of the band row to repeat above this row when it
    /// starts a page (not the first).
    pub repeat: Option<(f32, f32)>,
}

impl RowSlot {
    pub fn new(top: f32, height: f32) -> Self {
        Self {
            top,
            height,
            ..Self::default()
        }
    }
}

/// One printed page's piece of the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    /// Page column (across) and row block (down), from 0.
    pub column: usize,
    pub block: usize,
    /// Canvas x range of the slice.
    pub x0: f32,
    pub x1: f32,
    /// Body y range (from the body start) of the slice.
    pub y0: f32,
    pub y1: f32,
    /// Band row (top, height) printed above the slice.
    pub repeat: Option<(f32, f32)>,
}

/// A page row of the body: its y range and the band row repeated above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub y0: f32,
    pub y1: f32,
    pub repeat: Option<(f32, f32)>,
}

/// Number of pages across for a canvas width at a scale.
pub fn columns(canvas_width: f32, scale: f32, area: Area) -> usize {
    ((canvas_width * scale / area.width) - 1e-3).ceil().max(1.) as usize
}

/// Splits the rows into page blocks. A block never cuts a row unless the row
/// alone is taller than a page.
#[cfg(test)]
pub fn blocks(rows: &[RowSlot], body_height: f32, available: f32) -> Vec<(f32, f32)> {
    blocks_with(rows, body_height, &|_, _| available)
        .into_iter()
        .map(|b| (b.y0, b.y1))
        .collect()
}

/// `blocks` where the room of a block depends on where it falls:
/// `room(k, last)` is the height available to block `k` (from 0) when it is
/// the last block (or not). A block never ends on a row that has to stay
/// with the next one (unless nothing else fits) and starts with the band row
/// of the group it continues when the row asks for it.
pub fn blocks_with(
    rows: &[RowSlot],
    body_height: f32,
    room: &dyn Fn(usize, bool) -> f32,
) -> Vec<Block> {
    let end = body_height.max(rows.last().map_or(0., |r| r.top + r.height));
    let last_break = rows
        .iter()
        .filter(|r| r.break_before)
        .map(|r| r.top)
        .fold(f32::NEG_INFINITY, f32::max);
    // The room of block `k` starting at `top` with row `first`, and its
    // repeated band row.
    let start = |k: usize, top: f32, first: usize| -> (f32, Option<(f32, f32)>) {
        let repeat = if k > 0 {
            rows.get(first).and_then(|r| r.repeat)
        } else {
            None
        };
        let taken = repeat.map_or(0., |r| r.1);
        let last_room = (room(k, true) - taken).max(1.);
        let fits_last = last_break <= top && end - top <= last_room + 0.01;
        let other = (room(k, false) - taken).max(1.);
        let avail = if fits_last {
            last_room
        } else if end - top <= other + 0.01 && last_break <= top {
            // The rest would fill a page that is not the last one, but it
            // is the last: it must leave room for what the last page holds.
            last_room.min(other)
        } else {
            other
        };
        (avail, repeat)
    };
    let mut out: Vec<Block> = vec![];
    let mut top = 0.;
    let mut first = 0;
    let (mut avail, mut repeat) = start(0, 0., 0);
    let mut i = 0;
    while i < rows.len() {
        if out.len() > MAX_PAGES {
            break;
        }
        let row = rows[i];
        let bottom = row.top + row.height;
        let overflow = bottom - top > avail + 0.01;
        if row.top > top && (overflow || row.break_before) {
            let mut at = i;
            if overflow && !row.break_before {
                while at > first && rows[at - 1].keep_with_next {
                    at -= 1;
                }
                // A kept chain that is the whole block cannot move.
                if at == first {
                    at = i;
                }
            }
            let cut = rows[at].top;
            out.push(Block {
                y0: top,
                y1: cut,
                repeat,
            });
            top = cut;
            first = at;
            (avail, repeat) = start(out.len(), top, first);
            i = at;
            continue;
        }
        // A row taller than a page is cut into page-high pieces.
        while bottom - top > avail + 0.01 && out.len() <= MAX_PAGES {
            out.push(Block {
                y0: top,
                y1: top + avail,
                repeat,
            });
            top += avail;
            repeat = None;
            avail = start(out.len(), top, usize::MAX).0;
        }
        i += 1;
    }
    if end > top || out.is_empty() {
        out.push(Block {
            y0: top,
            y1: end.max(top),
            repeat,
        });
    }
    out
}

/// Tiles the canvas: blocks down, columns across. Pages run across first,
/// then down (row block 1 pages 1..n, then row block 2).
#[cfg(test)]
pub fn tiles(
    canvas_width: f32,
    header_height: f32,
    rows: &[RowSlot],
    body_height: f32,
    scale: f32,
    area: Area,
) -> Result<Vec<Tile>, String> {
    let scale = scale.max(0.01);
    let height = area.height / scale - header_height;
    tiles_with(
        canvas_width,
        rows,
        body_height,
        scale,
        area.width,
        &|_, _| height,
    )
}

/// `tiles` where `room(k, last)` is the body height (canvas pixels, after
/// the column header) of page row `k`.
pub fn tiles_with(
    canvas_width: f32,
    rows: &[RowSlot],
    body_height: f32,
    scale: f32,
    area_width: f32,
    room: &dyn Fn(usize, bool) -> f32,
) -> Result<Vec<Tile>, String> {
    let scale = scale.max(0.01);
    let across = columns(
        canvas_width,
        scale,
        Area {
            width: area_width,
            height: 0.,
        },
    );
    let slice = area_width / scale;
    let blocks = blocks_with(rows, body_height, room);
    if across.saturating_mul(blocks.len()) > MAX_PAGES {
        return Err(too_many(across as f64 * blocks.len() as f64));
    }
    let mut out = vec![];
    for (block, b) in blocks.into_iter().enumerate() {
        for column in 0..across {
            let x0 = column as f32 * slice;
            out.push(Tile {
                column,
                block,
                x0,
                x1: (x0 + slice).min(canvas_width.max(x0 + 1.)),
                y0: b.y0,
                y1: b.y1,
                repeat: b.repeat,
            });
        }
    }
    Ok(out)
}
