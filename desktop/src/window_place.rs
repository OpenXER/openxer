//! The main window's size, position and maximized state, kept in the
//! workspace preferences between sessions.
//!
//! At start the window opens where it last was (`viewport`); with nothing
//! stored it opens at a size that fits a 1280 x 720 display. Every frame
//! notes where the window is (`track`); a restored window larger than its
//! monitor, or on no monitor at all, is brought back (`fit`). `save` writes
//! the last normal (not maximized, not minimized) placement on exit.
use super::*;
use serde::{Deserialize, Serialize};
use std::cell::Cell;

const KEY: &str = "window_placement";
/// Opening size with nothing stored: fits a 1280 x 720 logical display
/// with its taskbar and the window's own frame.
pub(crate) const DEFAULT_SIZE: Vec2 = Vec2::new(1200., 620.);
/// The smallest the window can be made.
pub(crate) const MIN_SIZE: Vec2 = Vec2::new(900., 560.);

/// Where the window was: its inner size, outer position, and whether it was
/// maximized.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

thread_local! {
    static LAST: Cell<Option<Placement>> = const { Cell::new(None) };
    static FITTED: Cell<bool> = const { Cell::new(false) };
}

/// The stored placement, when there is a readable one.
pub(crate) fn load(store: &Store) -> Option<Placement> {
    let text = store.preference(KEY).ok().flatten()?;
    let p: Placement = serde_json::from_str(&text).ok()?;
    let sane = [p.x, p.y, p.width, p.height].iter().all(|v| v.is_finite())
        && p.width >= MIN_SIZE.x / 2.
        && p.height >= MIN_SIZE.y / 2.;
    sane.then_some(p)
}

/// The main window's viewport: title, icon and the stored placement.
pub(crate) fn viewport(placement: Option<Placement>) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title("OpenXER")
        .with_icon(chrome::app_icon())
        .with_min_inner_size(MIN_SIZE);
    match placement {
        Some(p) => builder
            .with_inner_size([p.width.max(MIN_SIZE.x), p.height.max(MIN_SIZE.y)])
            .with_position([p.x, p.y])
            .with_maximized(p.maximized),
        None => builder.with_inner_size(DEFAULT_SIZE),
    }
}

/// What to change so a window of `inner` size at `outer` fits on a monitor
/// of `monitor` size (`None`: the window is on no monitor): a smaller inner
/// size and/or a new outer position.
pub(crate) fn fit(
    outer: Option<Rect>,
    inner: Option<Rect>,
    monitor: Option<Vec2>,
) -> (Option<Vec2>, Option<Pos2>) {
    let Some(inner) = inner else {
        return (None, None);
    };
    match monitor {
        // Not on any monitor (one that was unplugged): back to the primary.
        None if outer.is_some() => (None, Some(Pos2::new(40., 40.))),
        None => (None, None),
        Some(m) => {
            // The frame around the client area, and room for the taskbar.
            let frame = outer.map_or(Vec2::new(16., 39.), |o| o.size() - inner.size());
            // Only shrink if the window genuinely exceeds the monitor; avoid
            // snapping/shrinking already-fitting windows on frame 3.
            let fits_monitor =
                inner.width() + frame.x <= m.x + 2. && inner.height() + frame.y <= m.y + 2.;
            if fits_monitor {
                return (None, None);
            }
            let room = (m - frame - Vec2::new(0., 48.)).max(MIN_SIZE);
            let size = inner.size().min(room);
            let resize = (size != inner.size()).then_some(size);
            (resize, None)
        }
    }
}

/// Per frame: note where the window is, and on the first frames that know
/// the monitor, bring a window that does not fit back onto it.
pub(crate) fn track(ctx: &egui::Context) {
    let (outer, inner, monitor, maximized, minimized) = ctx.input(|i| {
        let v = i.viewport();
        (
            v.outer_rect,
            v.inner_rect,
            v.monitor_size,
            v.maximized.unwrap_or(false),
            v.minimized.unwrap_or(false),
        )
    });
    if !FITTED.get() && inner.is_some() && ctx.cumulative_pass_nr() > 2 {
        FITTED.set(true);
        if !maximized {
            let (size, pos) = fit(outer, inner, monitor);
            if let Some(size) = size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            }
            if let Some(pos) = pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }
        }
    }
    if minimized {
        return;
    }
    let last = LAST.get();
    let placement = match (maximized, outer, inner) {
        // Maximized: keep the normal size and position it came from.
        (true, _, _) => last.map(|p| Placement {
            maximized: true,
            ..p
        }),
        (false, Some(o), Some(i)) => Some(Placement {
            x: o.left(),
            y: o.top(),
            width: i.width(),
            height: i.height(),
            maximized: false,
        }),
        _ => last,
    };
    LAST.set(placement);
}

/// Seed `LAST` with the loaded placement so closing while still maximized
/// preserves the normal (unmaximized) position and size.
pub(crate) fn seed(placement: Option<Placement>) {
    LAST.set(placement);
}

/// On exit: store the last placement noted.
pub(crate) fn save(store: &Store) {
    if let Some(p) = LAST.get() {
        if let Ok(text) = serde_json::to_string(&p) {
            let _ = store.set_preference(KEY, &text);
        }
    }
}

/// Keeps the newly created Windows HWND cloaked in DWM until the first real
/// OpenGL frame has been laid out, maximized (if requested), and buffer-swapped.
/// Works around winit 0.30 calling `ShowWindow(SW_MAXIMIZE)` -> `ShowWindow(SW_HIDE)`
/// during creation and eframe calling `set_visible(true)` before `swap_buffers()`.
#[derive(Default)]
pub(crate) struct StartupCloak {
    hwnd: isize,
    cloaked: bool,
    want_maximized: bool,
    sent_maximize: bool,
    frames: u8,
}

impl StartupCloak {
    /// Locate this thread's hidden OpenXER window and cloak it in DWM.
    pub(crate) fn engage(want_maximized: bool) -> Self {
        #[cfg(target_os = "windows")]
        {
            let hwnd = win32::find_openxer_hwnd();
            let cloaked = hwnd != 0 && win32::set_cloak(hwnd, true);
            Self {
                hwnd,
                cloaked,
                want_maximized,
                sent_maximize: false,
                frames: 0,
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = want_maximized;
            Self::default()
        }
    }

    /// True while startup is still settling behind the cloak (suppresses `fit()`
    /// until the final maximized/normal dimensions are in place).
    pub(crate) fn active(&self) -> bool {
        self.cloaked || (self.want_maximized && !self.sent_maximize)
    }

    /// Advance one eframe `update()` call (called once per painted pass).
    /// Uncloaks at the start of the frame *after* the settled, maximized OpenGL
    /// frame has already called `swap_buffers()`.
    pub(crate) fn step(&mut self, ctx: &egui::Context, settled_frame: bool) {
        if !self.cloaked && !self.want_maximized {
            return;
        }
        if !settled_frame {
            return;
        }
        self.frames = self.frames.saturating_add(1);
        if self.want_maximized && !self.sent_maximize {
            self.sent_maximize = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            ctx.request_repaint();
            return;
        }
        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let min_frames = if self.want_maximized { 3 } else { 2 };
        let ready = (self.frames >= min_frames && (!self.want_maximized || is_maximized))
            || self.frames >= 8;
        if ready {
            self.uncloak();
            self.want_maximized = false;
        } else {
            ctx.request_repaint();
        }
    }

    pub(crate) fn uncloak(&mut self) {
        if self.cloaked && self.hwnd != 0 {
            #[cfg(target_os = "windows")]
            win32::set_cloak(self.hwnd, false);
            self.cloaked = false;
        }
    }
}

impl Drop for StartupCloak {
    fn drop(&mut self) {
        self.uncloak();
    }
}

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;

    const DWMWA_CLOAK: u32 = 13;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumThreadWindows(
            thread_id: u32,
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowTextW(hwnd: isize, buf: *mut u16, max_count: i32) -> i32;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, val: *const c_void, size: u32) -> i32;
    }

    unsafe extern "system" fn match_openxer(hwnd: isize, lparam: isize) -> i32 {
        let mut buf = [0u16; 64];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title.starts_with("OpenXER") {
                *(lparam as *mut isize) = hwnd;
                return 0; // stop enumeration
            }
        }
        1
    }

    pub(super) fn find_openxer_hwnd() -> isize {
        let mut found: isize = 0;
        unsafe {
            let tid = GetCurrentThreadId();
            EnumThreadWindows(tid, match_openxer, &mut found as *mut isize as isize);
        }
        found
    }

    pub(super) fn set_cloak(hwnd: isize, cloak: bool) -> bool {
        let val: u32 = u32::from(cloak);
        let hr = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_CLOAK,
                &val as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        hr == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_size_fits_a_1280_by_720_display() {
        // Window frame about 16 x 39, taskbar 48.
        let (default, least) = (DEFAULT_SIZE, MIN_SIZE);
        assert!(default.x + 16. <= 1280.);
        assert!(default.y + 39. + 48. <= 720.);
        assert!(least.x <= default.x && least.y <= default.y);
    }

    #[test]
    fn a_placement_is_stored_and_read_back() {
        let path =
            std::env::temp_dir().join(format!("openxer-window-place-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        assert_eq!(load(&store), None, "nothing stored");
        let p = Placement {
            x: 100.,
            y: 50.,
            width: 1000.,
            height: 700.,
            maximized: true,
        };
        LAST.set(Some(p));
        save(&store);
        assert_eq!(load(&store), Some(p));
        // Junk is ignored.
        store.set_preference(KEY, "{\"x\": 1}").unwrap();
        assert_eq!(load(&store), None);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_window_larger_than_its_monitor_shrinks_and_a_lost_one_comes_back() {
        let inner = Rect::from_min_size(Pos2::new(10., 40.), Vec2::new(1500., 900.));
        let outer = Rect::from_min_max(Pos2::new(2., 9.), inner.max + Vec2::new(8., 8.));
        let (size, pos) = fit(Some(outer), Some(inner), Some(Vec2::new(1280., 720.)));
        let size = size.expect("shrunk");
        assert!(
            size.x + 16. <= 1280. && size.y + 39. + 48. <= 720.,
            "{size:?}"
        );
        assert_eq!(pos, None);
        // It fits: nothing changes.
        let small = Rect::from_min_size(inner.min, Vec2::new(1000., 600.));
        let outer = Rect::from_min_max(Pos2::new(2., 9.), small.max + Vec2::new(8., 8.));
        assert_eq!(
            fit(Some(outer), Some(small), Some(Vec2::new(1920., 1080.))),
            (None, None)
        );
        // On no monitor: it moves back.
        assert_eq!(
            fit(Some(outer), Some(small), None).1,
            Some(Pos2::new(40., 40.))
        );
        // A restored placement opens there, maximized as it was.
        let v = viewport(Some(Placement {
            x: 30.,
            y: 20.,
            width: 1000.,
            height: 600.,
            maximized: true,
        }));
        assert_eq!(v.position, Some(Pos2::new(30., 20.)));
        assert_eq!(v.inner_size, Some(Vec2::new(1000., 600.)));
        assert_eq!(v.maximized, Some(true));
        assert_eq!(viewport(None).inner_size, Some(DEFAULT_SIZE));
    }
}
