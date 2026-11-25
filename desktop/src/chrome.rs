//! Main-window chrome shared by every view: fonts and visuals, the palette,
//! the original OpenXER icons, and the drawing primitives for toolbars, bar menus, status
//! panels and dialog button columns. Sizes are logical pixels at 96 DPI.
use eframe::egui::{
    self, containers::menu::MenuConfig, Align2, Color32, FontId, Pos2, Rect, Response, Sense,
    Stroke, Ui, Vec2,
};

/// Arial 8 pt at 96 DPI: tables, timescale, bar labels, dialogs.
pub const FONT_SIZE: f32 = 32. / 3.;
/// Segoe UI 9 pt at 96 DPI: menus and the status bar.
pub const UI_FONT_SIZE: f32 = 12.;
/// Tahoma 10 pt bold at 96 DPI: the view caption and the dock tabs.
pub const CAPTION_FONT_SIZE: f32 = 40. / 3.;
pub const INK: Color32 = Color32::BLACK;
pub const CHROME: Color32 = Color32::from_rgb(240, 240, 240);
pub const SELECT: Color32 = Color32::from_rgb(0, 120, 215);
pub const GROUP: Color32 = Color32::from_rgb(226, 226, 226);
pub const BLUE: Color32 = crate::theme::color(crate::theme::DATA_DATE);
pub const CRITICAL: Color32 = crate::theme::color(crate::theme::CRITICAL);

/// Named chrome colors.
pub mod pal {
    use eframe::egui::Color32;
    const fn hex(value: u32) -> Color32 {
        Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
    pub const WHITE: Color32 = Color32::WHITE;
    pub const FACE: Color32 = hex(0xF0F0F0);
    pub const SHADOW: Color32 = hex(0xA0A0A0);
    pub const LIGHT: Color32 = hex(0xE3E3E3);
    pub const GRIP: Color32 = hex(0xC3C3C3);
    pub const GRIP_LIGHT: Color32 = hex(0xE4E4E4);
    pub const HOT_FILL: Color32 = hex(0xCCE8FF);
    pub const HOT_BORDER: Color32 = hex(0x99D1FF);
    pub const PRESSED_FILL: Color32 = hex(0x99D1FF);
    pub const PRESSED_BORDER: Color32 = hex(0x66B5F5);
    pub const CAPTION_BG: Color32 = hex(0xF9F9F9);
    pub const CAPTION_LINE: Color32 = hex(0xDCDCDC);
    pub const TAB_INACTIVE: Color32 = hex(0xF3F3F3);
    pub const TAB_BORDER: Color32 = hex(0xE5E5E5);
    pub const FOCUS_OUTER: Color32 = hex(0xCCE8FF);
    pub const FOCUS_INNER: Color32 = hex(0xE5F3FF);
    pub const VIEW_MARGIN: Color32 = hex(0xD8D8D8);
    pub const CLOSE_RED: Color32 = hex(0xC75050);
    pub const MENU_SEPARATOR: Color32 = hex(0x848484);
    pub const MENU_SEPARATOR_LIGHT: Color32 = hex(0xF5F5F5);
    pub const MENU_HOT: Color32 = hex(0xADD6FF);
    pub const MENU_ICON_HOT: Color32 = hex(0xE5F3FF);
    pub const MENU_DISABLED: Color32 = hex(0x6D6D6D);
    pub const POPUP_BG: Color32 = hex(0xF9F9F9);
    pub const POPUP_SEPARATOR: Color32 = hex(0xD7D7D7);
    pub const POPUP_HOT: Color32 = hex(0x95CDFB);
    pub const POPUP_HOT_DISABLED: Color32 = hex(0xECECEC);
    pub const DIALOG_STRIP: Color32 = hex(0xD0D0D0);
    pub const TEXT_DISABLED: Color32 = hex(crate::theme::DISABLED_TEXT);
    /// Inner dark edge of a sunken field and of the radio glyph.
    pub const DARK: Color32 = hex(0x696969);
    /// Outer lines of vertical tabs and their page.
    pub const TAB_LINE: Color32 = hex(0x646464);
    /// A read-only field's fill (the Activities page's).
    pub const READONLY: Color32 = hex(0xF8F8F8);
    /// Dialog title bar, window border and the hot close box.
    pub const TITLE_BG: Color32 = Color32::from_rgb(249, 241, 237);
    pub const WINDOW_BORDER: Color32 = Color32::from_rgb(130, 135, 144);
    pub const CLOSE_HOT: Color32 = Color32::from_rgb(196, 43, 28);
    /// A selected list row.
    pub const SELECTED: Color32 = hex(0x0078D7);
}

const ARIAL: &str = "Arial";
const ARIAL_BOLD: &str = "Arial Bold";
const SEGOE: &str = "Segoe UI";
const SEGOE_BOLD: &str = "Segoe UI Bold";
const TAHOMA: &str = "Tahoma";
const TAHOMA_BOLD: &str = "TahomaBold";

#[cfg(windows)]
fn fonts_dir() -> std::path::PathBuf {
    std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:/Windows".into())
        .join("Fonts")
}

/// Where the faces come from off Windows: a Windows font folder when
/// `WINDIR` names one (for example `/mnt/c/Windows` under WSL), then the
/// standard font folders, where the same faces may be installed (the
/// msttcorefonts package) or metric-compatible substitutes stand in.
#[cfg(not(windows))]
struct FontsDir(Option<std::path::PathBuf>);

#[cfg(not(windows))]
fn fonts_dir() -> FontsDir {
    FontsDir(std::env::var_os("WINDIR").map(|dir| std::path::PathBuf::from(dir).join("Fonts")))
}

#[cfg(not(windows))]
impl FontsDir {
    /// The face `file` names (a Windows file name), or the first of its
    /// substitutes found. Segoe UI and Tahoma have none of their own: their
    /// families fall back to the Arial face, which may be a substitute.
    fn join(&self, file: &str) -> std::path::PathBuf {
        let substitutes: &[&str] = match file {
            "arial.ttf" => &["LiberationSans-Regular.ttf", "DejaVuSans.ttf"],
            "arialbd.ttf" => &[
                "Arial_Bold.ttf",
                "LiberationSans-Bold.ttf",
                "DejaVuSans-Bold.ttf",
            ],
            _ => &[],
        };
        if let Some(dir) = &self.0 {
            if dir.join(file).is_file() {
                return dir.join(file);
            }
        }
        let found = installed_fonts();
        std::iter::once(&file)
            .chain(substitutes)
            .find_map(|name| found.get(&name.to_ascii_lowercase()))
            .cloned()
            // A path that does not exist: the face is left out.
            .unwrap_or_default()
    }
}

/// Every `.ttf` file in the standard font folders, by lower-case file name
/// (the first found wins, folders in the order listed and names sorted).
/// Searched once; the walk is bounded in depth and in entries read.
#[cfg(not(windows))]
fn installed_fonts() -> &'static std::collections::HashMap<String, std::path::PathBuf> {
    use std::path::PathBuf;
    static FOUND: std::sync::OnceLock<std::collections::HashMap<String, PathBuf>> =
        std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        const MAX_DEPTH: usize = 6;
        const MAX_ENTRIES: usize = 50_000;
        let mut roots = vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ];
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join(".local/share/fonts"));
            roots.push(home.join(".fonts"));
        }
        let mut found = std::collections::HashMap::new();
        let mut budget = MAX_ENTRIES;
        fn walk(
            dir: &std::path::Path,
            depth: usize,
            budget: &mut usize,
            found: &mut std::collections::HashMap<String, PathBuf>,
        ) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                if *budget == 0 {
                    return;
                }
                *budget -= 1;
                let path = entry.path();
                // Folders that are links are not followed, so a loop of
                // links cannot hold the walk; linked font files are read.
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    if depth < MAX_DEPTH {
                        walk(&path, depth + 1, budget, found);
                    }
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.ends_with(".ttf") && path.is_file() {
                    found.entry(name).or_insert(path);
                }
            }
        }
        for root in roots {
            walk(&root, 0, &mut budget, &mut found);
        }
        found
    })
}

/// Fonts: the Windows system faces are read from the system font folder at
/// run time (nothing is bundled), cached once per process so startup can load
/// them before the OS window is created.
fn fonts() -> egui::FontDefinitions {
    static CACHE: std::sync::OnceLock<egui::FontDefinitions> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut fonts = egui::FontDefinitions::default();
            let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
            let dir = fonts_dir();
            let mut loaded = std::collections::BTreeSet::new();
            for (name, file) in [
                (ARIAL, "arial.ttf"),
                (ARIAL_BOLD, "arialbd.ttf"),
                (SEGOE, "segoeui.ttf"),
                (SEGOE_BOLD, "segoeuib.ttf"),
                (TAHOMA, "tahoma.ttf"),
                (TAHOMA_BOLD, "tahomabd.ttf"),
            ] {
                if let Ok(bytes) = std::fs::read(dir.join(file)) {
                    fonts
                        .font_data
                        .insert(name.into(), egui::FontData::from_owned(bytes).into());
                    loaded.insert(name);
                }
            }
            let chain = |names: &[&str]| -> Vec<String> {
                names
                    .iter()
                    .filter(|name| loaded.contains(*name))
                    .map(|name| name.to_string())
                    .chain(fallback.iter().cloned())
                    .collect()
            };
            fonts
                .families
                .insert(egui::FontFamily::Proportional, chain(&[ARIAL, TAHOMA]));
            for (family, faces) in [
                (ARIAL, &[ARIAL, TAHOMA][..]),
                (ARIAL_BOLD, &[ARIAL_BOLD, TAHOMA_BOLD][..]),
                (SEGOE, &[SEGOE, ARIAL][..]),
                (SEGOE_BOLD, &[SEGOE_BOLD, ARIAL_BOLD][..]),
                (TAHOMA, &[TAHOMA, ARIAL][..]),
                (TAHOMA_BOLD, &[TAHOMA_BOLD, ARIAL_BOLD][..]),
            ] {
                fonts
                    .families
                    .insert(egui::FontFamily::Name(family.into()), chain(faces));
            }
            fonts
        })
        .clone()
}

/// Read system fonts into memory ahead of window creation so the window never
/// stalls while loading TTF files on first paint.
pub(crate) fn preload_fonts() {
    let _ = fonts();
}

/// Whether the Arial face is Arial itself rather than a substitute, so text
/// measures as on Windows. Tests that pin a width to the pixel ask this.
#[cfg(all(test, windows))]
pub(crate) fn arial_is_real() -> bool {
    true
}

#[cfg(all(test, not(windows)))]
pub(crate) fn arial_is_real() -> bool {
    let path = fonts_dir().join("arial.ttf");
    path.is_file()
        && path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("arial.ttf"))
}

pub fn configure(ctx: &egui::Context) {
    ctx.options_mut(|o| {
        if let Some(passes) = std::num::NonZeroUsize::new(3) {
            o.max_passes = passes;
        }
    });
    ctx.set_theme(egui::Theme::Light);
    ctx.set_fonts(fonts());
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = CHROME;
    style.visuals.window_fill = CHROME;
    style.visuals.override_text_color = None;
    style.visuals.weak_text_color = Some(Color32::GRAY);
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.fg_stroke.color = INK;
    }
    style.visuals.hyperlink_color = BLUE;
    // Scroll-bar tracks use the extreme color; edit fields stay white.
    style.visuals.extreme_bg_color = pal::FACE;
    style.visuals.text_edit_bg_color = Some(Color32::WHITE);
    style.visuals.selection.bg_fill = SELECT;
    style.visuals.selection.stroke = Stroke::new(1_f32, Color32::WHITE);
    style.visuals.widgets.noninteractive.corner_radius = 0.into();
    style.visuals.widgets.inactive.corner_radius = 0.into();
    style.visuals.widgets.hovered.corner_radius = 0.into();
    style.visuals.widgets.active.corner_radius = 0.into();
    style.visuals.widgets.open.corner_radius = 0.into();
    style.visuals.window_corner_radius = 0.into();
    style.visuals.menu_corner_radius = 0.into();
    style.visuals.popup_shadow = egui::Shadow::NONE;
    style.spacing.item_spacing = Vec2::new(4., 3.);
    style.spacing.button_padding = Vec2::new(5., 2.);
    style.spacing.interact_size = Vec2::new(20., 18.);
    // Solid 16 px scroll bars on a face-colored track with full persistent
    // handle visibility across all ScrollAreas.
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 16.,
        bar_inner_margin: 0.,
        bar_outer_margin: 0.,
        handle_min_length: 24.,
        dormant_background_opacity: 1.0,
        active_background_opacity: 1.0,
        interact_background_opacity: 1.0,
        dormant_handle_opacity: 0.75,
        active_handle_opacity: 0.9,
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::solid()
    };
    for kind in [
        egui::TextStyle::Body,
        egui::TextStyle::Button,
        egui::TextStyle::Small,
        egui::TextStyle::Heading,
    ] {
        style
            .text_styles
            .insert(kind, FontId::proportional(FONT_SIZE));
    }
    ctx.set_style(style);
}

/// Arial 8 pt bold: band rows, dialog section headings.
pub fn bold_font() -> FontId {
    FontId::new(FONT_SIZE, egui::FontFamily::Name(ARIAL_BOLD.into()))
}
/// Arial 8 pt: table cells, timescale, labels.
pub fn table_font() -> FontId {
    FontId::new(FONT_SIZE, egui::FontFamily::Name(ARIAL.into()))
}
/// A typeface the table and chart fonts can use: the faces the app loads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Face {
    #[default]
    Arial,
    #[serde(rename = "Segoe UI")]
    SegoeUi,
    Tahoma,
}

impl Face {
    pub const ALL: [Face; 3] = [Face::Arial, Face::SegoeUi, Face::Tahoma];
    pub fn name(self) -> &'static str {
        match self {
            Face::Arial => ARIAL,
            Face::SegoeUi => SEGOE,
            Face::Tahoma => TAHOMA,
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }
    /// The egui family of the face, bold or regular. Italics are drawn by
    /// slanting the regular or bold glyphs.
    pub fn family(self, bold: bool) -> egui::FontFamily {
        let name = match (self, bold) {
            (Face::Arial, false) => ARIAL,
            (Face::Arial, true) => ARIAL_BOLD,
            (Face::SegoeUi, false) => SEGOE,
            (Face::SegoeUi, true) => SEGOE_BOLD,
            (Face::Tahoma, false) => TAHOMA,
            (Face::Tahoma, true) => TAHOMA_BOLD,
        };
        egui::FontFamily::Name(name.into())
    }
    /// The font at `points` (96 DPI).
    pub fn font(self, points: f32, bold: bool) -> FontId {
        FontId::new(points * 4. / 3., self.family(bold))
    }
}

/// Segoe UI 9 pt: menus and the status bar.
pub fn ui_font() -> FontId {
    FontId::new(UI_FONT_SIZE, egui::FontFamily::Name(SEGOE.into()))
}
/// Tahoma 10 pt bold: view caption and dock tabs.
pub fn caption_font() -> FontId {
    FontId::new(
        CAPTION_FONT_SIZE,
        egui::FontFamily::Name(TAHOMA_BOLD.into()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Open,
    Save,
    Export,
    Undo,
    Redo,
    Schedule,
    Options,
    Add,
    ZoomIn,
    ZoomOut,
    Fit,
    Details,
    Trace,
    Links,
    Activities,
    Wbs,
    Projects,
    Resources,
    Expenses,
    Risks,
    Reports,
    Issues,
    Thresholds,
    WorkProducts,
    Cut,
    Copy,
    Paste,
    Delete,
    ShiftUp,
    ShiftDown,
    ShiftLeft,
    ShiftRight,
    ProjectOpen,
    ProjectClosed,
    Eps,
    Labor,
    Nonlabor,
    Material,
    // Main-window toolbars and menus.
    New,
    CloseAll,
    Print,
    PrintPreview,
    PageSetup,
    Import,
    Refresh,
    Table,
    Gantt,
    Network,
    ActivityUsageSheet,
    ActivityUsageProfile,
    ResourceUsageSheet,
    ResourceUsageProfile,
    Bars,
    Columns,
    Timescale,
    Filter,
    GroupSort,
    LineNumbers,
    CheckSchedule,
    Level,
    Spotlight,
    ProgressLine,
    UpdateProgress,
    GlobalChange,
    TabHorizontal,
    TabVertical,
    TabMerge,
    Hint,
    Help,
    WhatsNew,
    ResourceCodes,
    ProjectCodes,
    ActivityCodes,
    RoleCodes,
    AssignmentCodes,
    Udf,
    Calendars,
    StepTemplates,
    CostAccounts,
    FundingSources,
    Roles,
    Obs,
    AssignResource,
    AssignByRole,
    AssignRole,
    AssignCode,
    Predecessors,
    Successors,
    Steps,
    Find,
    Replace,
    FillDown,
    SpellCheck,
    Preferences,
    ExpandAll,
    CollapseAll,
    CollapseTo,
    Lock,
    Reorganize,
    Baselines,
    Ok,
    Cancel,
    Apply,
    // Table context menu and dialog command buttons.
    Link,
    Unlink,
    TableFont,
    Modify,
    Run,
    Global,
    Back,
    Next,
    Finish,
    AcpShow,
    AcpMore,
    AcpFewer,
    AcpSelect,
    AcpGroup,
    AcpBack,
    AcpForward,
    // The optional toolbars (Action, Administrator, Find, Publish,
    // Reports, Check Schedule).
    ApplyActuals,
    StorePeriod,
    RecalcCosts,
    AdminPreferences,
    AdminCategories,
    Currencies,
    FindNext,
    ReportGroups,
    BatchReports,
    ReportWizard,
    RunBatch,
    WizardReport,
    EditorReport,
    CheckFilters,
    // The notebook text toolbar.
    Bold,
    Italic,
    Underline,
    BulletList,
    NumberedList,
    ClearFormatting,
}

struct IconAsset {
    name: &'static str,
    one: &'static [u8],
    two: &'static [u8],
}

macro_rules! asset {
    ($name:literal) => {
        IconAsset {
            name: $name,
            one: include_bytes!(concat!("../../assets/icons/16/", $name, ".png")),
            two: include_bytes!(concat!("../../assets/icons/32/", $name, ".png")),
        }
    };
}

impl Icon {
    /// The name of the drawing the icon shows.
    #[cfg(test)]
    pub fn asset_name(self) -> &'static str {
        self.asset().name
    }

    // Only original OpenXER drawings belong here: every PNG under assets/icons
    // is written by tools/icons/build.py. Never embed art from another program.
    fn asset(self) -> IconAsset {
        match self {
            Self::Open => asset!("OPEN16"),
            Self::Save => asset!("SAVE16"),
            Self::Export => asset!("EXPORT16"),
            Self::Undo | Self::Redo => asset!("UNDO16"),
            Self::Schedule => asset!("SCHDUL16"),
            Self::Options => asset!("CLNDR16"),
            Self::Add => asset!("ADD16"),
            Self::ZoomIn => asset!("ZOOMIN16"),
            Self::ZoomOut => asset!("ZOOMOT16"),
            Self::Fit => asset!("ZOOMTF16"),
            Self::Details => asset!("ATVDET16"),
            Self::Trace => asset!("ATVNWT16"),
            Self::Links => asset!("SHOWRELBTN16"),
            Self::Activities => asset!("ATV16"),
            Self::Wbs => asset!("WBS16"),
            Self::Projects => asset!("PROJCT16"),
            Self::Resources | Self::Labor => asset!("RSRC16"),
            Self::Expenses => asset!("EXPNSE16"),
            Self::Risks => asset!("RISK16"),
            Self::Reports => asset!("REPORT16"),
            Self::Issues => asset!("ISSUE16"),
            Self::Thresholds => asset!("THSHLD16"),
            Self::WorkProducts => asset!("DOCMNT16"),
            Self::Cut => asset!("CUT16"),
            Self::Copy => asset!("COPY16"),
            Self::Paste => asset!("PASTE16"),
            Self::Delete => asset!("DELETE16"),
            Self::ShiftUp => asset!("UPARW16"),
            Self::ShiftDown => asset!("DWNARW16"),
            Self::ShiftLeft => asset!("LFTARW16"),
            Self::ShiftRight => asset!("RGTARW16"),
            Self::ProjectOpen => asset!("IMG_ICO_PROJ_OPEN"),
            Self::ProjectClosed => asset!("IMG_ICO_PROJ_CLOSED"),
            Self::Eps => asset!("EPS16"),
            Self::Nonlabor => asset!("NLRSRC16"),
            Self::Material => asset!("OXMATRSRC16"),
            Self::New => asset!("NEW16"),
            Self::CloseAll => asset!("CLOSE16"),
            Self::Print => asset!("PRINT16"),
            Self::PrintPreview => asset!("PRVIEW16"),
            Self::PageSetup => asset!("PGSETP16"),
            Self::Import => asset!("IMPORT16"),
            Self::Refresh => asset!("REFRSH16"),
            Self::Table => asset!("ATVGRD16"),
            Self::Gantt => asset!("GANTT16"),
            Self::Network => asset!("ATVNET16"),
            Self::ActivityUsageSheet => asset!("ATVSPD16"),
            Self::ActivityUsageProfile => asset!("ATVHST16"),
            Self::ResourceUsageSheet => asset!("RSCSPD16"),
            Self::ResourceUsageProfile => asset!("RESHST16"),
            Self::Bars => asset!("VMENU_ICO_CHART_LAYOUT"),
            Self::Columns => asset!("VMENU_ICO_COLUMNS"),
            Self::Timescale => asset!("DATRNG16"),
            Self::Filter => asset!("FILTER16"),
            Self::GroupSort => asset!("GRPSRT16"),
            Self::LineNumbers => asset!("OXLINENO16"),
            Self::CheckSchedule => asset!("QALITY16"),
            Self::Level => asset!("LEVELR16"),
            Self::Spotlight => asset!("OXSPOTLT16"),
            Self::ProgressLine => asset!("OXPRGLIN16"),
            Self::UpdateProgress => asset!("ACTPRM16"),
            Self::GlobalChange => asset!("GLOBAL16"),
            Self::TabHorizontal => asset!("OXTABHZ16"),
            Self::TabVertical => asset!("OXTABVT16"),
            Self::TabMerge => asset!("OXTABMG16"),
            Self::Hint => asset!("HINT16"),
            Self::Help => asset!("HELP16"),
            Self::WhatsNew => asset!("OXWHTNW16"),
            Self::ResourceCodes => asset!("RSRCCD16"),
            Self::ProjectCodes => asset!("PROJCD16"),
            Self::ActivityCodes => asset!("ATVCDE16"),
            Self::RoleCodes => asset!("ROLEOP16"),
            Self::AssignmentCodes => asset!("ASSIGN16"),
            Self::Udf => asset!("FACTVL16"),
            Self::Calendars => asset!("CLNDR16"),
            Self::StepTemplates => asset!("TMPLAT16"),
            Self::CostAccounts => asset!("ACCUNT16"),
            Self::FundingSources => asset!("ESTMAT16"),
            Self::Roles => asset!("ROLE16"),
            Self::Obs => asset!("OBS16"),
            Self::AssignResource => asset!("RSRCUR16"),
            Self::AssignByRole => asset!("RSRCUC16"),
            Self::AssignRole => asset!("ROLCUR16"),
            Self::AssignCode => asset!("ACTCUR16"),
            Self::Predecessors => asset!("ATVPRD16"),
            Self::Successors => asset!("ATVSUC16"),
            Self::Steps => asset!("STEPD16"),
            Self::Find => asset!("FIND16"),
            Self::Replace => asset!("REPLAC16"),
            Self::FillDown => asset!("FILDWN16"),
            Self::SpellCheck => asset!("SPLCHK16"),
            Self::Preferences => asset!("USER16"),
            Self::ExpandAll => asset!("EXPALL16"),
            Self::CollapseAll => asset!("COLALL16"),
            Self::CollapseTo => asset!("VMENU_ICO_COLLAPSE_TO"),
            Self::Lock => asset!("LOCK16"),
            Self::Reorganize => asset!("REORGZ16"),
            Self::Baselines => asset!("BASLNE16"),
            Self::Ok => asset!("OK16"),
            Self::Cancel => asset!("CANCEL16"),
            Self::Apply => asset!("APPLY16"),
            Self::Link => asset!("OXLINK16"),
            Self::Unlink => asset!("OXUNLINK16"),
            Self::TableFont => asset!("VMENU_ICO_GRID_FONT_COLOR"),
            Self::Modify => asset!("DIALOG16"),
            Self::Run => asset!("RUN16"),
            Self::Global => asset!("GLOBAL16"),
            Self::Back => asset!("BACK16"),
            Self::Next => asset!("NEXT16"),
            Self::Finish => asset!("FINISH16"),
            Self::AcpShow => asset!("OXACPSHOW16"),
            Self::AcpMore => asset!("OXACPMORE16"),
            Self::AcpFewer => asset!("OXACPFEWER16"),
            Self::AcpSelect => asset!("OXACPSELECT16"),
            Self::AcpGroup => asset!("OXACPGROUP16"),
            Self::AcpBack => asset!("OXACPBACK16"),
            Self::AcpForward => asset!("OXACPFWD16"),
            Self::ApplyActuals => asset!("TMAPVL16"),
            Self::StorePeriod => asset!("TMSHT16"),
            Self::RecalcCosts => asset!("RSRCPR16"),
            Self::AdminPreferences => asset!("PROFLE16"),
            Self::AdminCategories => asset!("STNDRD16"),
            Self::Currencies => asset!("PRTOGB16"),
            Self::FindNext => asset!("JUMP16"),
            Self::ReportGroups => asset!("RPTGRP16"),
            Self::BatchReports => asset!("RUNBAT16"),
            Self::ReportWizard => asset!("RPTWZD16"),
            Self::RunBatch => asset!("OXRUNBAT16"),
            Self::WizardReport => asset!("OXRPTWIZ16"),
            Self::EditorReport => asset!("OXRPTEDT16"),
            Self::CheckFilters => asset!("SELBX116"),
            Self::Bold => asset!("OXBOLD16"),
            Self::Italic => asset!("OXITALIC16"),
            Self::Underline => asset!("OXUNDERL16"),
            Self::BulletList => asset!("OXBULLET16"),
            Self::NumberedList => asset!("OXNUMBER16"),
            Self::ClearFormatting => asset!("OXCLRFMT16"),
        }
    }
}

fn texture(ctx: &egui::Context, icon: Icon, enabled: bool) -> egui::TextureHandle {
    let asset = icon.asset();
    // Use the authored 2x image whenever 1x would be enlarged. Both resolutions
    // remain cached when a window moves between monitors with different DPI.
    let scale = if ctx.pixels_per_point() > 1. { 2 } else { 1 };
    let key = egui::Id::new(("openxer_original_icon", asset.name, scale, enabled));
    if let Some(texture) = ctx.data_mut(|data| data.get_temp::<egui::TextureHandle>(key)) {
        return texture;
    }
    let bytes = if scale == 2 { asset.two } else { asset.one };
    let mut rgba = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("Embedded OpenXER icon must be a valid PNG")
        .into_rgba8();
    if !enabled {
        for pixel in rgba.pixels_mut() {
            let gray = (54 * pixel[0] as u16 + 183 * pixel[1] as u16 + 19 * pixel[2] as u16) / 256;
            let faded = ((gray + 240) / 2) as u8;
            pixel[0] = faded;
            pixel[1] = faded;
            pixel[2] = faded;
            pixel[3] = (pixel[3] as u16 * 3 / 4) as u8;
        }
    }
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let texture = ctx.load_texture(
        format!(
            "OpenXER/{}@{scale}x/{}",
            asset.name,
            if enabled { "enabled" } else { "disabled" }
        ),
        pixels,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    texture
}

/// The original OpenXER mark for the native window/taskbar. The executable's
/// Explorer icon is embedded separately by build.rs from the same authored set.
pub fn app_icon() -> egui::IconData {
    let rgba = image::load_from_memory_with_format(
        include_bytes!("../../assets/icons/openxer_256.png"),
        image::ImageFormat::Png,
    )
    .expect("Embedded OpenXER application icon must be a valid PNG")
    .into_rgba8();
    egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}

fn snap(ctx: &egui::Context, pos: Pos2) -> Pos2 {
    let ppp = ctx.pixels_per_point();
    Pos2::new((pos.x * ppp).round() / ppp, (pos.y * ppp).round() / ppp)
}

/// Paints an authored 16x16 icon with its top-left corner at `min`.
pub fn paint_icon_at(painter: &egui::Painter, icon: Icon, min: Pos2, enabled: bool) {
    let ctx = painter.ctx();
    let texture = texture(ctx, icon, enabled);
    let uv = if matches!(icon, Icon::Redo) {
        // The pack has one undo arrow; mirror that original texture for redo.
        Rect::from_min_max(Pos2::new(1., 0.), Pos2::new(0., 1.))
    } else {
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.))
    };
    painter.image(
        texture.id(),
        Rect::from_min_size(snap(ctx, min), Vec2::splat(16.)),
        uv,
        Color32::WHITE,
    );
}

/// Paints an authored 16x16 icon centered in `rect`, pixel-aligned like the toolbar.
pub fn paint_icon(painter: &egui::Painter, icon: Icon, rect: Rect, enabled: bool) {
    paint_icon_at(painter, icon, rect.center() - Vec2::splat(8.), enabled);
}

/// Where `paint_pointer_icon` puts the icon's top-left corner, from the
/// pointer: just below and to the right, clear of the hot spot cross.
pub const POINTER_ICON_OFFSET: Vec2 = Vec2::new(5., 5.);

/// Shows `icon` as the mouse pointer for this frame. egui has no image
/// cursors, so the system cursor is hidden and the icon is painted on the
/// top layer, unclipped, just below and right of the pointer. A small cross
/// marks the hot spot, the exact point that decides what is hit. Call it
/// every frame the icon should show; the normal cursor returns on the first
/// frame it is not called, or when the pointer leaves the window.
pub fn paint_pointer_icon(ctx: &egui::Context, icon: Icon) {
    let Some(pos) = ctx.pointer_hover_pos() else {
        return;
    };
    ctx.set_cursor_icon(egui::CursorIcon::None);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("openxer_pointer_icon"),
    ));
    let at = snap(ctx, pos);
    // A 5 px cross centered on the hot spot pixel: dark on a white halo,
    // so it reads over bars, lines and the chart background alike.
    let (x, y) = (at.x, at.y);
    fill(
        &painter,
        Rect::from_min_max(Pos2::new(x - 3., y - 1.), Pos2::new(x + 4., y + 2.)),
        Color32::WHITE,
    );
    fill(
        &painter,
        Rect::from_min_max(Pos2::new(x - 1., y - 3.), Pos2::new(x + 2., y + 4.)),
        Color32::WHITE,
    );
    hline(&painter, x - 2., x + 3., y, Color32::BLACK);
    vline(&painter, x, y - 2., y + 3., Color32::BLACK);
    paint_icon_at(&painter, icon, at + POINTER_ICON_OFFSET, true);
    // Follow the pointer.
    ctx.request_repaint();
}

// ------------------------------------------------------------------ pixels

/// Fills a rectangle as a plain mesh: no anti-aliasing feather, so pixel
/// rows, columns and single dots at integer coordinates come out exact.
pub fn fill(painter: &egui::Painter, rect: Rect, color: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(rect, color);
    painter.add(egui::Shape::mesh(mesh));
}
pub fn hline(painter: &egui::Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    fill(
        painter,
        Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 1.)),
        color,
    );
}
pub fn vline(painter: &egui::Painter, x: f32, y0: f32, y1: f32, color: Color32) {
    fill(
        painter,
        Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x + 1., y1)),
        color,
    );
}
fn dot(painter: &egui::Painter, x: f32, y: f32, color: Color32) {
    fill(
        painter,
        Rect::from_min_size(Pos2::new(x, y), Vec2::splat(1.)),
        color,
    );
}

/// The light-blue frame of a hot or checked button: 1 px border with
/// softened corners around a flat fill.
pub fn hot_frame(painter: &egui::Painter, rect: Rect, fill_color: Color32, border: Color32) {
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    fill(painter, rect.shrink(1.), fill_color);
    hline(painter, l + 2., r - 2., t, border);
    hline(painter, l + 2., r - 2., b - 1., border);
    vline(painter, l, t + 2., b - 2., border);
    vline(painter, r - 1., t + 2., b - 2., border);
    let soft = border.lerp_to_gamma(Color32::WHITE, 0.38);
    let inner = border.lerp_to_gamma(fill_color, 0.33);
    for (x, y, color) in [
        (l + 1., t, soft),
        (l, t + 1., soft),
        (l + 1., t + 1., inner),
        (r - 2., t, soft),
        (r - 1., t + 1., soft),
        (r - 2., t + 1., inner),
        (l + 1., b - 1., soft),
        (l, b - 2., soft),
        (l + 1., b - 2., inner),
        (r - 2., b - 1., soft),
        (r - 1., b - 2., soft),
        (r - 2., b - 2., inner),
    ] {
        dot(painter, x, y, color);
    }
}

/// Dotted bar grip: 2x2 dots on a 4 px pitch, along x (`horizontal`) or y.
pub fn grip(painter: &egui::Painter, start: Pos2, length: f32, horizontal: bool) {
    let mut offset = 0.;
    while offset + 2. <= length {
        let (x, y) = if horizontal {
            (start.x + offset, start.y)
        } else {
            (start.x, start.y + offset)
        };
        dot(painter, x, y, pal::GRIP_LIGHT);
        dot(painter, x + 1., y, pal::GRIP);
        dot(painter, x, y + 1., pal::GRIP);
        dot(painter, x + 1., y + 1., pal::GRIP);
        offset += 4.;
    }
}

/// Small solid triangle (5 x 3 down, or 3 x 5 left) for bar chevrons and
/// dropdown buttons, `min` at the top-left pixel.
pub fn chevron(painter: &egui::Painter, min: Pos2, down: bool, color: Color32) {
    for step in 0..3 {
        let s = step as f32;
        if down {
            hline(painter, min.x + s, min.x + 5. - s, min.y + s, color);
        } else {
            vline(
                painter,
                min.x + 2. - s,
                min.y + 2. - s,
                min.y + 3. + s,
                color,
            );
        }
    }
}

// ---------------------------------------------------------------- toolbars

pub const TOOL_PITCH: f32 = 24.;
pub const TOOL_HEIGHT: f32 = 23.;
pub const TOOL_ARROW: f32 = 10.;
/// Grip area before the first button and chevron area after the last.
pub const BAR_LEAD: f32 = 9.;
pub const BAR_TAIL: f32 = 13.;
pub const DOCK_BAR_TAIL: f32 = 14.;

/// An icon-only toolbar button: 24 x 23 (plus 10 px for a dropdown arrow),
/// light-blue frame when hot or checked, darker while pressed, tooltip text
/// on hover. Disabled buttons show a faded icon and ignore clicks.
pub fn tool_button(
    ui: &mut Ui,
    icon: Icon,
    tip: &str,
    enabled: bool,
    checked: bool,
    dropdown: bool,
) -> Response {
    let width = TOOL_PITCH + if dropdown { TOOL_ARROW } else { 0. };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, TOOL_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rect = Rect::from_min_size(snap(ui.ctx(), rect.min), rect.size());
        if enabled && response.is_pointer_button_down_on() {
            hot_frame(painter, rect, pal::PRESSED_FILL, pal::PRESSED_BORDER);
        } else if checked || (enabled && response.hovered()) {
            hot_frame(painter, rect, pal::HOT_FILL, pal::HOT_BORDER);
        }
        paint_icon_at(painter, icon, rect.min + Vec2::splat(4.), enabled);
        if dropdown {
            chevron(
                painter,
                rect.min + Vec2::new(23., 10.),
                true,
                if enabled { INK } else { pal::TEXT_DISABLED },
            );
        }
    }
    if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_disabled_hover_text(tip)
    }
}

// ------------------------------------------------------------- bar menus

/// Splits `&File` into ("File", Some(0)): the character after `&` is the
/// mnemonic, which the reference always underlines.
pub fn mnemonic(label: &str) -> (String, Option<usize>) {
    let mut text = String::with_capacity(label.len());
    let mut index = None;
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                text.push('&');
                chars.next();
            } else if index.is_none() {
                index = Some(text.chars().count());
            }
        } else {
            text.push(c);
        }
    }
    (text, index)
}

fn underline(painter: &egui::Painter, galley: &egui::Galley, origin: Pos2, index: Option<usize>) {
    let Some(index) = index else { return };
    let start = galley.pos_from_cursor(egui::text::CCursor::new(index));
    let end = galley.pos_from_cursor(egui::text::CCursor::new(index + 1));
    let y = (origin.y + galley.rows.first().map_or(12., |row| row.row.height()) - 2.).round();
    let x0 = (origin.x + start.min.x).round();
    let x1 = (origin.x + end.min.x).round().max(x0 + 1.);
    hline(painter, x0, x1, y, galley.job.sections[0].format.color);
}

/// The soft shadow behind every dropdown and submenu: the reference draws a
/// separate shadow window offset 4 px right and down, the same size as the
/// menu, darkest along the menu's right and bottom edges.
pub const MENU_SHADOW: egui::Shadow = egui::Shadow {
    offset: [4, 4],
    blur: 4,
    spread: 0,
    color: Color32::from_black_alpha(56),
};

/// Style for bar-menu dropdowns: 1 px #A0A0A0 border, #F0F0F0 body, no
/// rounding, a 4 px offset soft shadow, items packed at 22 px. Plain
/// `ui.button` items added to these menus also come out at the reference
/// height with text at 27 px.
pub fn bar_menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.menu_margin = egui::Margin::same(1);
    style.spacing.item_spacing = Vec2::ZERO;
    style.spacing.button_padding = Vec2::new(25., 3.);
    style.spacing.interact_size.y = 22.;
    style.visuals.window_fill = pal::FACE;
    style.visuals.window_stroke = Stroke::new(1_f32, pal::SHADOW);
    style.visuals.menu_corner_radius = 0.into();
    style.visuals.popup_shadow = MENU_SHADOW;
    style.visuals.widgets.hovered.weak_bg_fill = pal::MENU_HOT;
    style.visuals.widgets.hovered.bg_fill = pal::MENU_HOT;
    style.visuals.widgets.hovered.expansion = 0.;
    style.visuals.widgets.active.expansion = 0.;
    for kind in [egui::TextStyle::Body, egui::TextStyle::Button] {
        style.text_styles.insert(kind, ui_font());
    }
}

/// Style for the native popup menus opened from view controls (the
/// `Layout:` button, context menus): 1 px #A0A0A0 border, 2 px #F0F0F0
/// frame, #F9F9F9 items, full-width #95CDFB hot item, text at x 31.
pub fn popup_menu_style(style: &mut egui::Style) {
    bar_menu_style(style);
    style.spacing.menu_margin = egui::Margin::same(2);
    style.visuals.widgets.hovered.weak_bg_fill = pal::POPUP_HOT;
    style.visuals.widgets.hovered.bg_fill = pal::POPUP_HOT;
}

fn is_popup_style(ui: &Ui) -> bool {
    ui.visuals().widgets.hovered.weak_bg_fill == pal::POPUP_HOT
}

fn bar_menu_config() -> MenuConfig {
    MenuConfig::new().style(bar_menu_style)
}

/// How a main-menu title sits in its bar: which way its dropdown opens
/// and, in a column of titles, the one width they all share.
#[derive(Clone, Copy)]
pub struct TitleStyle {
    pub align: egui::RectAlign,
    pub width: Option<f32>,
}

impl Default for TitleStyle {
    fn default() -> Self {
        Self {
            align: egui::RectAlign::BOTTOM_START,
            width: None,
        }
    }
}

/// Width of a title in a column of titles (the widest caption).
pub const TITLE_COLUMN_WIDTH: f32 = 64.;

/// A main-menu title. Opens its dropdown on click and follows the pointer to
/// the neighbouring title while any menu-bar dropdown is open.
pub fn menu_bar_title<R>(
    ui: &mut Ui,
    label: &str,
    style: TitleStyle,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    let (text, index) = mnemonic(label);
    let galley = ui.painter().layout_no_wrap(text, ui_font(), INK);
    // Hit rectangle: caption plus about 6 px each side, 21 px tall, no
    // gaps; the reference widths are used for the standard titles.
    let measured = match galley.text() {
        "File" => 30.,
        "Edit" => 32.,
        "View" => 37.,
        "Project" => 49.,
        "Enterprise" => 64.,
        "Tools" => 40.,
        "Admin" => 48.,
        "Help" => 37.,
        _ => (galley.size().x + 12.).ceil(),
    };
    let size = Vec2::new(style.width.unwrap_or(measured), 21.);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ctx = ui.ctx().clone();
    let popup_id = egui::Popup::default_response_id(&response);
    crate::menu_keys::note_title(&ctx, popup_id, label);
    let titles_id = egui::Id::new("openxer_menu_bar_titles");
    let mut titles: Vec<egui::Id> = ctx.data(|d| d.get_temp(titles_id)).unwrap_or_default();
    if !titles.contains(&popup_id) {
        titles.push(popup_id);
        ctx.data_mut(|d| d.insert_temp(titles_id, titles.clone()));
    }
    let open = egui::Popup::is_id_open(&ctx, popup_id);
    let other_open = titles
        .iter()
        .any(|id| *id != popup_id && egui::Popup::is_id_open(&ctx, *id));
    if response.hovered() && other_open && !open {
        egui::Popup::open_id(&ctx, popup_id);
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if open || egui::Popup::is_id_open(&ctx, popup_id) {
            hot_frame(painter, rect, pal::HOT_FILL, pal::HOT_BORDER);
        } else if response.hovered() || crate::menu_keys::bar_focus(&ctx, popup_id) {
            hot_frame(painter, rect, pal::MENU_ICON_HOT, pal::HOT_FILL);
        }
        let origin = Pos2::new(
            rect.left() + 6.,
            (rect.center().y - galley.size().y / 2.).floor(),
        );
        underline(painter, &galley, origin, index);
        painter.galley(origin, galley, INK);
    }
    let inner = egui::Popup::menu(&response)
        .align(style.align)
        .style(bar_menu_style)
        .info(
            egui::UiStackInfo::new(egui::UiKind::Menu)
                .with_tag_value(MenuConfig::MENU_CONFIG_TAG, bar_menu_config()),
        )
        .show(|ui| {
            let noted = crate::menu_keys::enter_level(ui, None);
            let inner = content(ui);
            if noted {
                crate::menu_keys::leave_level(ui.ctx());
            }
            inner
        })
        .map(|r| r.inner);
    (response, inner)
}

/// One dropdown entry.
#[derive(Clone, Copy)]
pub struct Item<'a> {
    pub label: &'a str,
    pub shortcut: &'a str,
    pub icon: Option<Icon>,
    pub enabled: bool,
    pub checked: bool,
}

impl<'a> Item<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            shortcut: "",
            icon: None,
            enabled: true,
            checked: false,
        }
    }
    pub fn key(mut self, shortcut: &'a str) -> Self {
        self.shortcut = shortcut;
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

fn paint_item(ui: &Ui, rect: Rect, response: &Response, item: &Item, hot: bool, submenu: bool) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let color = if item.enabled {
        INK
    } else if is_popup_style(ui) {
        pal::TEXT_DISABLED
    } else {
        pal::MENU_DISABLED
    };
    let native = is_popup_style(ui);
    let cell = Rect::from_min_size(rect.min, Vec2::new(22., 22.));
    if native {
        fill(
            painter,
            rect,
            if hot { pal::POPUP_HOT } else { pal::POPUP_BG },
        );
        if hot && !item.enabled {
            fill(painter, rect, pal::POPUP_HOT_DISABLED);
        }
    } else if hot && item.enabled {
        hot_frame(painter, cell, pal::MENU_ICON_HOT, pal::HOT_FILL);
        fill(
            painter,
            Rect::from_min_max(Pos2::new(rect.left() + 23., rect.top()), rect.max),
            pal::MENU_HOT,
        );
    }
    if item.checked {
        let cell = if native {
            cell.translate(Vec2::new(1., 0.))
        } else {
            cell
        };
        hot_frame(painter, cell, pal::HOT_FILL, pal::HOT_BORDER);
        if item.icon.is_none() {
            let c = cell.center();
            painter.line(
                vec![
                    c + Vec2::new(-4., 0.),
                    c + Vec2::new(-1., 3.),
                    c + Vec2::new(4., -3.),
                ],
                Stroke::new(1.5_f32, color),
            );
        }
    }
    let shift = if native { 3. } else { 0. };
    if let Some(icon) = item.icon {
        paint_icon_at(painter, icon, rect.min + Vec2::new(2., 3.), item.enabled);
    }
    let (text, index) = mnemonic(item.label);
    let galley = painter.layout_no_wrap(text, ui_font(), color);
    // Native popups put the text a pixel higher (cap rows 6-16 of the item)
    // and end the shortcut 23 px before the item's right edge.
    let (lift, key_inset) = if native { (1., 23.) } else { (0., 28.) };
    let origin = Pos2::new(
        rect.left() + 25. + shift,
        (rect.center().y - galley.size().y / 2.).round() - lift,
    );
    underline(painter, &galley, origin, index);
    painter.galley(origin, galley, color);
    if !item.shortcut.is_empty() {
        painter.text(
            Pos2::new(rect.right() - key_inset, rect.center().y - lift),
            Align2::RIGHT_CENTER,
            item.shortcut,
            ui_font(),
            color,
        );
    }
    if submenu {
        let x = rect.right() - 11.;
        let y = rect.top() + 8.;
        for step in 0..3 {
            let s = step as f32;
            vline(painter, x + s, y + s, y + 5. - s, color);
        }
    }
    let _ = response;
}

fn item_width(ui: &Ui, item: &Item) -> f32 {
    let text = mnemonic(item.label).0;
    let width = ui.fonts_mut(|f| f.layout_no_wrap(text, ui_font(), INK).size().x);
    // Text starts 25 px into the item and ends 26 px before its right
    // edge; a shortcut ends 28 px before the edge, at least 12 px after the
    // text.
    let shortcut = if item.shortcut.is_empty() {
        26.
    } else {
        12. + ui.fonts_mut(|f| {
            f.layout_no_wrap(item.shortcut.into(), ui_font(), INK)
                .size()
                .x
        }) + 28.
    };
    (25. + width + shortcut).ceil()
}

/// A 22 px dropdown item: icon cell, text with mnemonic, right-aligned
/// shortcut. Returns the response; callers close the menu on click.
pub fn menu_item(ui: &mut Ui, item: Item) -> Response {
    let width = item_width(ui, &item);
    let (rect, response) = ui.allocate_at_least(
        Vec2::new(width, 22.),
        if item.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let label = mnemonic(item.label).0;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, item.enabled, label.clone())
    });
    let keyed = crate::menu_keys::note_item(ui.ctx(), response.id, item.label, item.enabled, false);
    paint_item(
        ui,
        rect,
        &response,
        &item,
        response.hovered() || keyed,
        false,
    );
    response
}

/// A check item that shows an icon as well: the check mark in the first
/// cell, the icon in a second, then the caption. Returns the response; the
/// caller decides whether a click closes the menu.
pub fn check_icon_item(ui: &mut Ui, label: &str, icon: Option<Icon>, checked: bool) -> Response {
    let item = Item::new(label).checked(checked);
    let width = item_width(ui, &item) + ICON_CELL;
    let (rect, response) = ui.allocate_at_least(Vec2::new(width, 22.), Sense::click());
    let text = mnemonic(label).0;
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, checked, &text)
    });
    let keyed = crate::menu_keys::note_item(ui.ctx(), response.id, label, true, false);
    let hot = response.hovered() || keyed;
    // The row and the check cell as a plain check item draws them.
    paint_item(
        ui,
        rect,
        &response,
        &Item::new("").checked(checked),
        hot,
        false,
    );
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let native = is_popup_style(ui);
    let shift = if native { 3. } else { 0. };
    let lift = if native { 1. } else { 0. };
    let painter = ui.painter();
    if let Some(icon) = icon {
        paint_icon_at(
            painter,
            icon,
            rect.min + Vec2::new(2. + shift + ICON_CELL, 3.),
            true,
        );
    }
    let (text, index) = mnemonic(label);
    let galley = painter.layout_no_wrap(text, ui_font(), INK);
    let origin = Pos2::new(
        rect.left() + 25. + shift + ICON_CELL,
        (rect.center().y - galley.size().y / 2.).round() - lift,
    );
    underline(painter, &galley, origin, index);
    painter.galley(origin, galley, INK);
    response
}

/// Width of the extra icon cell of [`check_icon_item`].
const ICON_CELL: f32 = 22.;

/// A dropdown item that opens a submenu on hover.
pub fn submenu<R>(
    ui: &mut Ui,
    item: Item,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    submenu_with(ui, item, egui::PopupCloseBehavior::CloseOnClick, content)
}

/// [`submenu`] with its own close behavior: `CloseOnClickOutside` keeps the
/// submenu open while its items are clicked (a list of check items).
pub fn submenu_with<R>(
    ui: &mut Ui,
    item: Item,
    close: egui::PopupCloseBehavior,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    let id = ui.next_auto_id();
    let open = egui::containers::menu::MenuState::from_ui(ui, |state, _| {
        state.open_item == Some(egui::containers::menu::SubMenu::id_from_widget_id(id))
    });
    let width = item_width(ui, &item);
    let (rect, response) = ui.allocate_at_least(
        Vec2::new(width, 22.),
        if item.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let keyed = crate::menu_keys::note_item(ui.ctx(), response.id, item.label, item.enabled, true);
    paint_item(
        ui,
        rect,
        &response,
        &item,
        open || response.hovered() || keyed,
        true,
    );
    if !item.enabled {
        return (response, None);
    }
    // A submenu of a native popup is a native popup itself.
    let config = if is_popup_style(ui) {
        MenuConfig::new().style(popup_menu_style)
    } else {
        bar_menu_config()
    }
    .close_behavior(close);
    // The submenu window overlaps the parent menu by 2 px, its top 2 px
    // above the item (its first item level with this one). egui places it a
    // gap of half the frame margins plus 2 to the right of the anchor, so
    // the anchor ends that far inside the item's right edge, which lies
    // the parent frame's margin (2 px for the bar menus) inside the window.
    let gap = egui::Frame::menu(ui.style()).total_margin().sum().x / 2. + 2.;
    let parent_margin = egui::Frame::menu(ui.style()).total_margin().right;
    let mut anchor = response.clone();
    anchor.interact_rect.max.x = rect.right() + parent_margin - 2. - gap;
    // A submenu too tall for the space below its item slides up until its
    // bottom edge meets the screen edge (never past the top), rather than
    // egui's flip to end level with the item. The anchor moves so that the
    // window's usual place, its top level with the item, already fits.
    let popup_id = egui::containers::menu::SubMenu::id_from_widget_id(response.id);
    if let Some(size) = egui::containers::AreaState::load(ui.ctx(), popup_id).and_then(|s| s.size) {
        let screen = ui.ctx().content_rect();
        let lift = egui::Frame::menu(ui.style()).total_margin().sum().y / 2.;
        let top = anchor.interact_rect.top() - lift;
        if top + size.y > screen.bottom() {
            let slid = (screen.bottom() - size.y).floor().max(screen.top());
            anchor.interact_rect = anchor.interact_rect.translate(Vec2::new(0., slid - top));
        }
    }
    let item_id = response.id;
    let inner = egui::containers::menu::SubMenu::new()
        .config(config)
        .show(ui, &anchor, |ui| {
            let noted = crate::menu_keys::enter_level(ui, Some(item_id));
            let inner = content(ui);
            if noted {
                crate::menu_keys::leave_level(ui.ctx());
            }
            inner
        })
        .map(|r| r.inner);
    (response, inner)
}

/// A 6 px separator slot with a dark line and a light line under it.
pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_at_least(Vec2::new(8., 6.), Sense::hover());
    let painter = ui.painter();
    if is_popup_style(ui) {
        // The line starts under the item text (window x 32), not under
        // the icon column.
        fill(painter, rect, pal::POPUP_BG);
        hline(
            painter,
            rect.left() + 29.,
            rect.right(),
            rect.top() + 2.,
            pal::POPUP_SEPARATOR,
        );
        return;
    }
    hline(
        painter,
        rect.left() + 2.,
        rect.right() - 2.,
        rect.top() + 2.,
        pal::MENU_SEPARATOR,
    );
    hline(
        painter,
        rect.left() + 2.,
        rect.right() - 2.,
        rect.top() + 3.,
        pal::MENU_SEPARATOR_LIGHT,
    );
}

// ------------------------------------------------------------ status bar

/// A sunken status panel: #A0A0A0 top/left, white bottom/right, Segoe UI
/// text 3 px in from the left, clipped to the panel.
pub fn status_panel(painter: &egui::Painter, rect: Rect, text: &str, sunken: bool) {
    if sunken {
        hline(painter, rect.left(), rect.right(), rect.top(), pal::SHADOW);
        vline(painter, rect.left(), rect.top(), rect.bottom(), pal::SHADOW);
        hline(
            painter,
            rect.left(),
            rect.right(),
            rect.bottom() - 1.,
            pal::WHITE,
        );
        vline(
            painter,
            rect.right() - 1.,
            rect.top(),
            rect.bottom(),
            pal::WHITE,
        );
    }
    if !text.is_empty() {
        // Baseline 4 px above the panel bottom, as the reference.
        painter.with_clip_rect(rect.shrink(1.)).text(
            Pos2::new(rect.left() + 3., rect.top() - 1.),
            Align2::LEFT_TOP,
            text,
            ui_font(),
            INK,
        );
    }
}

/// Width of a status panel that fits `text` (minimum 76 px).
pub fn status_width(ctx: &egui::Context, text: &str) -> f32 {
    let width = ctx.fonts_mut(|f| f.layout_no_wrap(text.into(), ui_font(), INK).size().x);
    (width + 12.).ceil().max(76.)
}

// ---------------------------------------------------------------- dialogs

/// One button of a dialog's right-hand button column.
#[derive(Clone, Copy)]
pub struct DialogButton<'a> {
    pub label: &'a str,
    pub icon: Option<Icon>,
    pub enabled: bool,
    /// Extra space before this button (Apply and Help start new groups).
    pub group: bool,
}

// The standard buttons of the shared dialog layout.
impl<'a> DialogButton<'a> {
    pub fn new(label: &'a str, icon: Option<Icon>) -> Self {
        Self {
            label,
            icon,
            enabled: true,
            group: false,
        }
    }
    pub fn ok() -> Self {
        Self::new("OK", Some(Icon::Ok))
    }
    pub fn cancel() -> Self {
        Self::new("Cancel", Some(Icon::Cancel))
    }
    pub fn apply() -> Self {
        Self::new("Apply", Some(Icon::Apply)).grouped()
    }
    pub fn help() -> Self {
        Self::new("Help", Some(Icon::Help)).grouped()
    }
    pub fn grouped(mut self) -> Self {
        self.group = true;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

pub const DIALOG_STRIP_WIDTH: f32 = 112.;
pub const DIALOG_BUTTON: Vec2 = Vec2::new(108., 25.);

/// The icon the reference shows on a dialog command button with this
/// caption, and whether it sits right of the caption (Next). Message box
/// buttons (Yes, No) and captions not listed carry none.
pub fn command_icon(label: &str) -> Option<(Icon, bool)> {
    let (text, _) = mnemonic(label);
    let key = text.trim().trim_end_matches("...").trim().to_lowercase();
    let icon = match key.as_str() {
        "ok" => Icon::Ok,
        "cancel" => Icon::Cancel,
        "help" => Icon::Help,
        "close" => Icon::CloseAll,
        "add" | "new" => Icon::Add,
        "delete" | "remove" | "nonwork" => Icon::Delete,
        "apply" | "apply change" => Icon::Apply,
        "modify" | "used by" | "options" | "edit column" | "sort" | "workweek" | "time periods"
        | "details" | "update" | "select resources" | "change" => Icon::Modify,
        "copy" | "copy from" => Icon::Copy,
        "print" => Icon::Print,
        "assign" => Icon::AssignCode,
        "paste" => Icon::Paste,
        "cut" => Icon::Cut,
        "import" => Icon::Import,
        "export" => Icon::Export,
        "default" | "schedule" | "restore" | "standard" | "level" | "recalculate" | "store now" => {
            Icon::Run
        }
        "view log" => Icon::Open,
        "to global" | "make global" => Icon::Global,
        "shift up" | "move up" => Icon::ShiftUp,
        "shift down" | "move down" => Icon::ShiftDown,
        "work" => Icon::Ok,
        "prev" | "back" => Icon::Back,
        "finish" => Icon::Finish,
        "next" => return Some((Icon::Next, true)),
        _ => return None,
    };
    Some((icon, false))
}

/// Paints a command button's icon and caption (with its mnemonic
/// underline) on a face already drawn: the icon 6 px in from the left edge
/// (from the right for Next) and vertically centred, the caption centred in
/// the rest of the face. Without an icon the caption is centred on the
/// face. `shift` moves both for a pressed button.
#[allow(clippy::too_many_arguments)]
pub fn paint_command_face(
    painter: &egui::Painter,
    rect: Rect,
    label: &str,
    icon: Option<(Icon, bool)>,
    font: FontId,
    color: Color32,
    enabled: bool,
    shift: f32,
) {
    let mut text_rect = rect;
    if let Some((icon, right)) = icon {
        let y = rect.top() + ((rect.height() - 16.) / 2.).floor() + shift;
        let x = if right {
            text_rect.max.x = rect.right() - 22.;
            rect.right() - 22. + shift
        } else {
            text_rect.min.x = rect.left() + 22.;
            rect.left() + 6. + shift
        };
        paint_icon_at(painter, icon, Pos2::new(x, y), enabled);
    }
    let (text, index) = mnemonic(label);
    let galley = painter.layout_no_wrap(text, font, color);
    let origin = Pos2::new(
        ((text_rect.left() + text_rect.right() - galley.size().x) / 2.).round() + shift,
        (rect.center().y - galley.size().y / 2.).round() + shift,
    );
    underline(painter, &galley, origin, index);
    painter.galley(origin, galley, color);
}

/// Smallest button that carries a command icon; narrower buttons (arrows,
/// `...`) stay text only.
pub const ICON_BUTTON_MIN_WIDTH: f32 = 60.;

/// A dialog command button in a flow layout, `size` (wider when its
/// caption needs it), drawn by the dialog kit's push button painter with the
/// icon for its caption ([`command_icon`]) when it has one.
pub fn command_button(ui: &mut Ui, caption: &str, size: Vec2, enabled: bool) -> Response {
    let icon = command_icon(caption).filter(|_| size.x >= ICON_BUTTON_MIN_WIDTH);
    let label = mnemonic(caption).0;
    let text = ui
        .painter()
        .layout_no_wrap(label.clone(), crate::dialog_kit::button_font(), INK)
        .size()
        .x;
    let need = text.ceil() + 12. + if icon.is_some() { 22. } else { 0. };
    let size = Vec2::new(size.x.max(need), size.y);
    let (_, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    crate::dialog_keys::note_button(ui, &response, caption, enabled);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
    if ui.is_rect_visible(response.rect) {
        let default = crate::dialog_keys::is_default(ui, response.id);
        crate::dialog_kit::paint_push_button(
            ui.painter(),
            response.rect,
            caption,
            icon,
            crate::dialog_kit::Push::of(&response, enabled, default),
        );
    }
    response
}

/// A raised 108 x 25 dialog button, drawn by the dialog kit's push button
/// painter. A button without an explicit icon gets its caption's
/// [`command_icon`].
pub fn dialog_button(ui: &mut Ui, button: &DialogButton) -> Response {
    dialog_button_sized(ui, button, DIALOG_BUTTON)
}

/// `dialog_button` at another size. Deprecated: every dialog button is
/// [`DIALOG_BUTTON`]; Page Setup's 100 x 25 strip moves to it in a later
/// batch.
pub fn dialog_button_sized(ui: &mut Ui, button: &DialogButton, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if button.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, button.enabled, button.label)
    });
    crate::dialog_keys::note_button(ui, &response, button.label, button.enabled);
    if ui.is_rect_visible(rect) {
        let icon = match button.icon {
            Some(icon) => Some((icon, false)),
            None => command_icon(button.label),
        };
        let default = crate::dialog_keys::is_default(ui, response.id);
        crate::dialog_kit::paint_push_button(
            ui.painter(),
            rect,
            button.label,
            icon,
            crate::dialog_kit::Push::of(&response, button.enabled, default),
        );
    }
    response
}

/// The flow-layout adapter of `dialog_kit::button_column`. Lays out a
/// dialog body with the standard right-hand button column:
/// a 112 px #D0D0D0 strip running the full height, buttons 108 x 25 at its
/// left edge on a 30 px pitch, the first 6 px from the top, 10 px more
/// before each grouped button. Returns the body's value and the index of the
/// clicked button. Use it inside a window whose frame has no inner margin.
pub fn dialog_with_buttons<R>(
    ui: &mut Ui,
    buttons: &[DialogButton],
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, Option<usize>, Vec<Response>) {
    let (inner, clicked, responses, _) =
        dialog_with_buttons_from_and_pad(ui, None, body, |_| buttons.to_vec());
    (inner, clicked, responses)
}

/// [`dialog_with_buttons`] with the buttons made from the body's value
/// after the body has drawn, so a button's state follows what the body
/// changed this frame (Find Next is live in the frame the first letter is
/// typed, and Enter's default button is read from that frame's notes).
pub fn dialog_with_buttons_from<'a, R>(
    ui: &mut Ui,
    body: impl FnOnce(&mut Ui) -> R,
    buttons: impl FnOnce(&R) -> Vec<DialogButton<'a>>,
) -> (R, Option<usize>, Vec<Response>) {
    let (inner, clicked, responses, _) = dialog_with_buttons_from_and_pad(ui, None, body, buttons);
    (inner, clicked, responses)
}

/// What the move pad under a flow dialog's buttons reported: the arrow
/// clicked and the control rectangles the pad recorded.
pub(crate) type PadOutput = (Option<crate::dialog_kit::Move>, Vec<(String, Rect)>);

/// [`dialog_with_buttons`] with the shared four-arrow move pad
/// (`dialog_kit::move_pad`) under the buttons, as a column entry places it:
/// 10 px more space before it. `pad` is its action prefix and which arrows
/// are enabled (left, up, down, right).
pub(crate) fn dialog_with_buttons_and_pad<R>(
    ui: &mut Ui,
    buttons: &[DialogButton],
    pad: Option<(&'static str, [bool; 4])>,
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, Option<usize>, Vec<Response>, PadOutput) {
    dialog_with_buttons_from_and_pad(ui, pad, body, |_| buttons.to_vec())
}

fn dialog_with_buttons_from_and_pad<'a, R>(
    ui: &mut Ui,
    pad: Option<(&'static str, [bool; 4])>,
    body: impl FnOnce(&mut Ui) -> R,
    buttons: impl FnOnce(&R) -> Vec<DialogButton<'a>>,
) -> (R, Option<usize>, Vec<Response>, PadOutput) {
    let mut clicked = None;
    let mut responses = vec![];
    let mut moved = (None, vec![]);
    let inner = ui
        .horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let body = ui
                .vertical(|ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4., 3.);
                            body(ui)
                        })
                        .inner
                })
                .inner;
            let buttons = buttons(&body);
            let column = buttons
                .iter()
                .fold(6., |height, b| height + if b.group { 40. } else { 30. })
                + 6.
                + if pad.is_some() { 10. + 43. } else { 0. };
            let height = ui.min_rect().height().max(column);
            let (strip, _) =
                ui.allocate_exact_size(Vec2::new(DIALOG_STRIP_WIDTH, height), Sense::hover());
            fill(ui.painter(), strip, pal::DIALOG_STRIP);
            let mut y = strip.top() + 6.;
            for (index, button) in buttons.iter().enumerate() {
                if button.group && index > 0 {
                    y += 10.;
                }
                let rect = Rect::from_min_size(Pos2::new(strip.left(), y), DIALOG_BUTTON);
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        dialog_button(ui, button)
                    })
                    .inner;
                if response.clicked() {
                    clicked = Some(index);
                }
                responses.push(response);
                y += 30.;
            }
            if let Some((prefix, enabled)) = pad {
                let area = Rect::from_min_size(
                    Pos2::new(strip.left(), y + 10.),
                    Vec2::new(DIALOG_BUTTON.x, 43.),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
                let (dir, rects) =
                    crate::dialog_kit::flow_canvas(&mut child, prefix, area.size(), |c| {
                        crate::dialog_kit::move_pad(c, prefix, 0., 0., enabled)
                    });
                moved = (dir, rects);
            }
            body
        })
        .inner;
    (inner, clicked, responses, moved)
}
/// A dialog window in the dialog kit's frame (see [`crate::kit_window`]):
/// face body, 1 px border, 31 px title bar with a close box, and no inner
/// margin so the button column reaches the edge.
pub fn dialog_window<'a>(title: impl AsRef<str>) -> crate::kit_window::KitWindow<'a> {
    crate::kit_window::KitWindow::flush(title)
}

/// The same frame with an 8 px margin around the body and square push
/// buttons, for forms and lists that egui lays out.
pub fn plain_window<'a>(title: impl AsRef<str>) -> crate::kit_window::KitWindow<'a> {
    crate::kit_window::KitWindow::padded(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_uses_original_textures_and_switches_resolution_without_resizing_controls() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut ids = vec![];
        for scale in [1., 2., 1.] {
            ctx.set_pixels_per_point(scale);
            let mut rect = Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = tool_button(ui, Icon::Schedule, "Schedule", true, false, false).rect;
                    tool_button(ui, Icon::Undo, "Undo", false, false, false);
                    tool_button(ui, Icon::Redo, "Redo", true, true, false);
                });
            });
            assert_eq!(rect.size(), Vec2::new(TOOL_PITCH, TOOL_HEIGHT));
            let handle = texture(&ctx, Icon::Schedule, true);
            assert_eq!(handle.size(), [(16. * scale) as usize; 2]);
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == handle.id())));
            ids.push(handle.id());
        }
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            ids[0], ids[2],
            "Changing DPI should reuse the original texture"
        );
        let enabled = texture(&ctx, Icon::Undo, true);
        let disabled = texture(&ctx, Icon::Undo, false);
        assert_ne!(enabled.id(), disabled.id());
        let icon = app_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert!(icon
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 0));
    }

    #[test]
    fn mnemonics_strip_the_marker_and_keep_literal_ampersands() {
        assert_eq!(mnemonic("&File"), ("File".into(), Some(0)));
        assert_eq!(mnemonic("Ent&erprise"), ("Enterprise".into(), Some(3)));
        assert_eq!(mnemonic("WPs && &Docs"), ("WPs & Docs".into(), Some(6)));
        assert_eq!(mnemonic("Plain"), ("Plain".into(), None));
    }

    #[test]
    fn system_fonts_back_every_family_and_fall_back_to_egui() {
        let fonts = fonts();
        for family in [ARIAL, ARIAL_BOLD, SEGOE, TAHOMA, TAHOMA_BOLD] {
            let chain = &fonts.families[&egui::FontFamily::Name(family.into())];
            assert!(
                chain.iter().any(|name| name == "Ubuntu-Light"),
                "{family} ends with egui's default font"
            );
        }
        if fonts_dir().join("arial.ttf").exists() {
            assert_eq!(
                fonts.families[&egui::FontFamily::Proportional][0],
                ARIAL,
                "Tables default to Arial"
            );
        }
    }

    #[test]
    fn dialog_column_orders_ok_cancel_apply_and_help_on_the_reference_pitch() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut rects = vec![];
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let buttons = [
                    DialogButton::ok(),
                    DialogButton::cancel(),
                    DialogButton::apply(),
                    DialogButton::new("Delete", Some(Icon::Delete)),
                    DialogButton::help(),
                ];
                let (_, _, responses) = dialog_with_buttons(ui, &buttons, |ui| {
                    ui.label("Body");
                });
                rects = responses.iter().map(|r| r.rect).collect();
            });
        });
        assert_eq!(rects.len(), 5);
        assert!(rects.iter().all(|r| r.size() == DIALOG_BUTTON));
        let tops: Vec<f32> = rects.iter().map(|r| r.top() - rects[0].top()).collect();
        assert_eq!(tops, vec![0., 30., 70., 100., 140.]);
    }

    #[test]
    fn command_buttons_carry_the_reference_icons_and_message_buttons_none() {
        assert_eq!(command_icon("OK"), Some((Icon::Ok, false)));
        assert_eq!(command_icon("&Help"), Some((Icon::Help, false)));
        assert_eq!(command_icon("Modify..."), Some((Icon::Modify, false)));
        assert_eq!(command_icon("Wo&rkweek..."), Some((Icon::Modify, false)));
        assert_eq!(command_icon("Apply Change"), Some((Icon::Apply, false)));
        assert_eq!(command_icon("Next"), Some((Icon::Next, true)));
        for plain in [
            "Yes",
            "&No",
            "To Shared",
            "To Personal",
            "Copy As Layout",
            "...",
        ] {
            assert_eq!(command_icon(plain), None, "{plain}");
        }
        let ctx = egui::Context::default();
        configure(&ctx);
        let textures = |caption: &'static str, width: f32| {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    command_button(ui, caption, Vec2::new(width, 25.), true);
                    dialog_button(ui, &DialogButton::new(caption, None));
                });
            });
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let cancel = texture(&ctx, Icon::Cancel, true).id();
        let found = textures("Cancel", 108.);
        assert_eq!(found.iter().filter(|id| **id == cancel).count(), 2);
        let found = textures("Yes", 108.);
        assert!(!found.contains(&cancel));
        // A narrow button stays text only; the 108 px dialog button does not.
        let ok = texture(&ctx, Icon::Ok, true).id();
        let found = textures("OK", 40.);
        assert_eq!(found.iter().filter(|id| **id == ok).count(), 1);
    }

    /// A submenu too tall for the space below its item slides up until its
    /// bottom edge meets the screen edge; one that fits keeps its top level
    /// with its item.
    #[test]
    fn a_tall_submenu_slides_up_to_the_screen_edge_instead_of_flipping() {
        let mut d = crate::qa_sweep::Driver::new(
            "submenu-slide",
            crate::reports::test_support::scheduled_starter(),
        );
        d.home();
        d.click("menu_bar_view");
        assert!(d.hover_text("Toolbars"));
        d.frames(3);
        let item = d
            .texts()
            .into_iter()
            .rev()
            .find(|(t, _)| t == "Toolbars")
            .map(|(_, r)| r)
            .expect("the Toolbars item is drawn");
        let tops: Vec<(String, Rect)> = d
            .app
            .action_rects
            .iter()
            .filter(|(k, _)| {
                k.starts_with("menu_toolbar_")
                    || *k == "menu_customize_toolbars"
                    || *k == "menu_reset_toolbar_layout"
            })
            .map(|(k, r)| {
                (
                    k.clone(),
                    Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3])),
                )
            })
            .collect();
        assert!(tops.len() > 10, "the submenu is open: {tops:?}");
        let first = tops.iter().map(|(_, r)| r.top()).fold(f32::MAX, f32::min);
        let last = tops.iter().map(|(_, r)| r.bottom()).fold(0., f32::max);
        // The window ends at the screen edge: its last item is within the
        // frame margin of it, and the window starts above its parent item.
        assert!(
            (892. ..=900.).contains(&last),
            "last item ends at {last}, the screen at 900"
        );
        assert!(
            first < item.top() - 20.,
            "slid up: {first} vs {}",
            item.top()
        );
        assert!(first >= 0., "never past the top");
        // A submenu that fits is where it always was: View > Attachments'
        // first item sits level with its parent.
        d.key(egui::Key::Escape);
        d.frames(3);
        d.click("menu_bar_view");
        assert!(d.hover_text("Attachments"));
        d.frames(3);
        let parent = d
            .texts()
            .into_iter()
            .rev()
            .find(|(t, _)| t == "Attachments")
            .map(|(_, r)| r)
            .unwrap();
        let first = d
            .texts()
            .into_iter()
            .rev()
            .find(|(t, _)| t == "Curtain")
            .map(|(_, r)| r)
            .expect("the submenu's first item is drawn");
        assert!(
            (first.top() - parent.top()).abs() < 2.,
            "first item {} vs its parent {}",
            first.top(),
            parent.top()
        );
    }
}
