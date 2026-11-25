//! Software previews of the production UI. No window, GL context, or desktop
//! input is created. This rasterizes egui's actual tessellated output; it does
//! not prove native compositor, driver, DPI negotiation, or window chrome.
use super::*;
use egui::epaint::{ImageData, ImageDelta, Primitive, Vertex};
use std::collections::HashMap;

struct Texture {
    size: [usize; 2],
    pixels: Vec<[f32; 4]>,
    options: egui::TextureOptions,
}

fn rgba(color: Color32) -> [f32; 4] {
    color.to_array().map(|value| value as f32 / 255.)
}

impl Texture {
    fn texel(&self, x: isize, y: isize) -> [f32; 4] {
        let address = |value: isize, length: usize| -> usize {
            let length = length as isize;
            match self.options.wrap_mode {
                egui::TextureWrapMode::ClampToEdge => value.clamp(0, length - 1) as usize,
                egui::TextureWrapMode::Repeat => value.rem_euclid(length) as usize,
                egui::TextureWrapMode::MirroredRepeat => {
                    let value = value.rem_euclid(2 * length);
                    (if value < length {
                        value
                    } else {
                        2 * length - 1 - value
                    }) as usize
                }
            }
        };
        self.pixels[address(y, self.size[1]) * self.size[0] + address(x, self.size[0])]
    }

    fn sample(&self, uv: Pos2, minifying: bool) -> [f32; 4] {
        let x = uv.x * self.size[0] as f32;
        let y = uv.y * self.size[1] as f32;
        let filter = if minifying {
            self.options.minification
        } else {
            self.options.magnification
        };
        if filter == egui::TextureFilter::Nearest {
            return self.texel(x.floor() as isize, y.floor() as isize);
        }
        // UVs address texel centers, including the atlas's single white texel.
        let x = x - 0.5;
        let y = y - 0.5;
        let ix = x.floor() as isize;
        let iy = y.floor() as isize;
        let tx = x - x.floor();
        let ty = y - y.floor();
        let samples = [
            self.texel(ix, iy),
            self.texel(ix + 1, iy),
            self.texel(ix, iy + 1),
            self.texel(ix + 1, iy + 1),
        ];
        std::array::from_fn(|channel| {
            let top = samples[0][channel] * (1. - tx) + samples[1][channel] * tx;
            let bottom = samples[2][channel] * (1. - tx) + samples[3][channel] * tx;
            top * (1. - ty) + bottom * ty
        })
    }
}

#[derive(Default)]
pub(crate) struct SoftwarePainter {
    textures: HashMap<egui::TextureId, Texture>,
    /// Logical screen size; `None` keeps the 1500 x 900 preview size.
    pub(crate) screen: Option<Vec2>,
    /// Textures freed by the last ingested frame.
    pending_free: Vec<egui::TextureId>,
}

fn edge(a: Pos2, b: Pos2, p: Pos2) -> f64 {
    (b.x as f64 - a.x as f64) * (p.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (p.x as f64 - a.x as f64)
}

fn top_left(a: Pos2, b: Pos2) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}

impl SoftwarePainter {
    pub(crate) fn click_action(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        time: &mut f64,
        action: &str,
    ) {
        let [left, top, right, bottom] = *app
            .action_rects
            .get(action)
            .unwrap_or_else(|| panic!("No rendered control {action:?}"));
        let pos = Pos2::new((left + right) / 2., (top + bottom) / 2.);
        for pressed in [true, false] {
            let _ = self.frame(
                app,
                ctx,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                false,
            );
        }
        let _ = self.frame(app, ctx, time, vec![egui::Event::PointerGone], false);
        let _ = self.frame(app, ctx, time, vec![], false);
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        while app.busy_active() {
            assert!(
                Instant::now() < deadline,
                "background scheduling/leveling worker timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
            let _ = self.frame(app, ctx, time, vec![], false);
        }
    }

    /// Move the pointer onto the last painted copy of `text` (the topmost
    /// one) and let the UI settle; false when the text is not on screen.
    pub(crate) fn hover_text(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        time: &mut f64,
        text: &str,
    ) -> bool {
        *time += 0.1;
        let screen = self.screen.unwrap_or(Vec2::new(1500., 900.));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
                time: Some(*time),
                focused: true,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        for (id, delta) in &output.textures_delta.set {
            self.update_texture(*id, delta);
        }
        for id in &output.textures_delta.free {
            self.textures.remove(id);
        }
        let found = output.shapes.iter().rev().find_map(|clipped| {
            let egui::Shape::Text(shape) = &clipped.shape else {
                return None;
            };
            let rect = shape.visual_bounding_rect().intersect(clipped.clip_rect);
            (shape.galley.text() == text && rect.is_positive()).then_some(rect)
        });
        let Some(rect) = found else {
            return false;
        };
        for _ in 0..3 {
            let _ = self.frame(
                app,
                ctx,
                time,
                vec![egui::Event::PointerMoved(rect.center())],
                false,
            );
        }
        true
    }

    fn update_texture(&mut self, id: egui::TextureId, delta: &ImageDelta) {
        assert!(
            delta.options.mipmap_mode.is_none(),
            "Software preview does not implement mipmaps"
        );
        let ImageData::Color(image) = &delta.image;
        if let Some([x, y]) = delta.pos {
            let texture = self
                .textures
                .get_mut(&id)
                .expect("Texture patch needs its preceding full image");
            assert!(x + image.size[0] <= texture.size[0] && y + image.size[1] <= texture.size[1]);
            for row in 0..image.size[1] {
                for column in 0..image.size[0] {
                    texture.pixels[(y + row) * texture.size[0] + x + column] =
                        rgba(image.pixels[row * image.size[0] + column]);
                }
            }
            texture.options = delta.options;
        } else {
            assert!(image.size.into_iter().all(|length| length > 0));
            self.textures.insert(
                id,
                Texture {
                    size: image.size,
                    pixels: image.pixels.iter().copied().map(rgba).collect(),
                    options: delta.options,
                },
            );
        }
    }

    pub(crate) fn frame(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        time: &mut f64,
        events: Vec<egui::Event>,
        paint: bool,
    ) -> Option<(image::RgbaImage, usize)> {
        *time += 0.1;
        let screen = self.screen.unwrap_or(Vec2::new(1500., 900.));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
                time: Some(*time),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        for (id, delta) in &output.textures_delta.set {
            self.update_texture(*id, delta);
        }
        let rendered = if paint {
            Some(self.rasterize(ctx, output.shapes, output.pixels_per_point, screen))
        } else {
            None
        };
        for id in output.textures_delta.free {
            self.textures.remove(&id);
        }
        rendered
    }

    /// Uploads the textures of a frame run outside this painter. Frees from
    /// the previous call are applied first, so a frame kept for
    /// [`Self::rasterize`] still finds its textures.
    pub(crate) fn ingest(&mut self, output: &egui::FullOutput) {
        for id in std::mem::take(&mut self.pending_free) {
            self.textures.remove(&id);
        }
        for (id, delta) in &output.textures_delta.set {
            self.update_texture(*id, delta);
        }
        self.pending_free = output.textures_delta.free.clone();
    }

    /// Draws tessellated shapes to an image of `screen` logical size.
    pub(crate) fn rasterize(
        &mut self,
        ctx: &egui::Context,
        shapes: Vec<egui::epaint::ClippedShape>,
        ppp: f32,
        screen: Vec2,
    ) -> (image::RgbaImage, usize) {
        let width = (screen.x * ppp).round() as usize;
        let height = (screen.y * ppp).round() as usize;
        let mut canvas = vec![rgba(CHROME); width * height];
        let primitives = ctx.tessellate(shapes, ppp);
        let mut triangles = 0;
        for primitive in primitives {
            let clip =
                Rect::from_min_max(primitive.clip_rect.min * ppp, primitive.clip_rect.max * ppp)
                    .intersect(Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(width as f32, height as f32),
                    ));
            let Primitive::Mesh(mesh) = primitive.primitive else {
                panic!("Native paint callbacks cannot be rendered by this software preview");
            };
            let texture = self
                .textures
                .get(&mesh.texture_id)
                .expect("Paint mesh references an uploaded texture");
            assert!(mesh.indices.len().is_multiple_of(3));
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let mut vertices = [triangle[0], triangle[1], triangle[2]]
                    .map(|index| mesh.vertices[index as usize]);
                for vertex in &mut vertices {
                    vertex.pos *= ppp;
                }
                Self::triangle(&mut canvas, width, height, clip, vertices, texture);
                triangles += 1;
            }
        }
        let pixels: Vec<u8> = canvas
            .into_iter()
            .flat_map(|pixel| pixel.map(|channel| (channel.clamp(0., 1.) * 255.).round() as u8))
            .collect();
        (
            image::RgbaImage::from_raw(width as u32, height as u32, pixels).unwrap(),
            triangles,
        )
    }

    fn triangle(
        canvas: &mut [[f32; 4]],
        width: usize,
        height: usize,
        clip: Rect,
        mut vertices: [Vertex; 3],
        texture: &Texture,
    ) {
        let mut area = edge(vertices[0].pos, vertices[1].pos, vertices[2].pos);
        if area.abs() < 1e-9 || !clip.is_positive() {
            return;
        }
        if area < 0. {
            vertices.swap(1, 2);
            area = -area;
        }
        let [a, b, c] = vertices;
        let min_x = a
            .pos
            .x
            .min(b.pos.x)
            .min(c.pos.x)
            .max(clip.left())
            .floor()
            .max(0.) as usize;
        let min_y = a
            .pos
            .y
            .min(b.pos.y)
            .min(c.pos.y)
            .max(clip.top())
            .floor()
            .max(0.) as usize;
        let max_x = (a
            .pos
            .x
            .max(b.pos.x)
            .max(c.pos.x)
            .min(clip.right())
            .ceil()
            .max(0.) as usize)
            .min(width);
        let max_y = (a
            .pos
            .y
            .max(b.pos.y)
            .max(c.pos.y)
            .min(clip.bottom())
            .ceil()
            .max(0.) as usize)
            .min(height);
        let includes = [
            top_left(b.pos, c.pos),
            top_left(c.pos, a.pos),
            top_left(a.pos, b.pos),
        ];
        let colors = [rgba(a.color), rgba(b.color), rgba(c.color)];
        let minifying =
            edge(a.uv, b.uv, c.uv).abs() * (texture.size[0] * texture.size[1]) as f64 > area;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                if point.x < clip.left()
                    || point.x >= clip.right()
                    || point.y < clip.top()
                    || point.y >= clip.bottom()
                {
                    continue;
                }
                let edges = [
                    edge(b.pos, c.pos, point),
                    edge(c.pos, a.pos, point),
                    edge(a.pos, b.pos, point),
                ];
                if (0..3).any(|i| edges[i] < 0. || (edges[i] == 0. && !includes[i])) {
                    continue;
                }
                let weights = edges.map(|value| (value / area) as f32);
                let uv = Pos2::new(
                    a.uv.x * weights[0] + b.uv.x * weights[1] + c.uv.x * weights[2],
                    a.uv.y * weights[0] + b.uv.y * weights[1] + c.uv.y * weights[2],
                );
                let sample = texture.sample(uv, minifying);
                // egui_glow multiplies vertex and texture colors in gamma space.
                // Both inputs are premultiplied; use ONE, ONE_MINUS_SRC_ALPHA.
                let source: [f32; 4] = std::array::from_fn(|channel| {
                    sample[channel]
                        * (colors[0][channel] * weights[0]
                            + colors[1][channel] * weights[1]
                            + colors[2][channel] * weights[2])
                });
                let destination = &mut canvas[y * width + x];
                for channel in 0..4 {
                    destination[channel] =
                        source[channel] + destination[channel] * (1. - source[3]);
                }
            }
        }
    }
}

#[test]
fn render_production_ui_to_png_without_a_window_or_pointer() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let database = std::env::temp_dir().join(format!(
        "openxer-software-preview-{}-{unique}.db",
        std::process::id()
    ));
    let mut report = vec![];
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let overview_origin = app.timeline_origin;
        let overview_scale = app.pixels_per_day;
        for (name, zoom, dialog) in [
            ("headless-activities.png", None, None),
            ("headless-day.png", Some(timescale::Unit::Day), None),
            ("headless-hour.png", Some(timescale::Unit::Hour), None),
            ("headless-options.png", None, Some("options")),
            ("headless-timescale.png", None, Some("timescale")),
        ] {
            if app.show_schedule {
                painter.click_action(&mut app, &ctx, &mut time, "options_cancel");
                assert!(!app.show_schedule);
            }
            app.timeline_origin = overview_origin;
            app.pixels_per_day = overview_scale;
            app.update_timescale_units();
            if let Some(unit) = zoom {
                let preset = timescale::ZOOM_PRESETS
                    .iter()
                    .find(|preset| preset.unit == unit)
                    .unwrap();
                app.zoom_at(preset.pixels_per_week, app.gantt_left);
                // Show the data-date boundary and first work period at fine zoom.
                app.timeline_origin = app.project.model.project.data_date.unwrap() - 120;
                app.view.upper = preset.major;
                app.view.lower = preset.minor;
            }
            if let Some(action) = dialog {
                if action == "options" {
                    // Schedule Options opens from the Schedule dialog.
                    app.open_options();
                    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
                } else {
                    painter.click_action(&mut app, &ctx, &mut time, action);
                }
                assert!(if action == "options" {
                    app.show_schedule
                } else {
                    app.gantt_ui.timescale.is_some()
                });
            }
            let (image, triangles) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            assert_eq!(image.dimensions(), (1500, 900));
            assert!(
                triangles > 1000,
                "The preview must contain the actual dense UI"
            );
            assert!(
                painter.textures.len() > 10,
                "Font atlas and authored icons must be uploaded"
            );
            let count = |hex: u32| {
                let [r, g, b] = crate::theme::bytes(hex);
                image.pixels().filter(|p| p.0 == [r, g, b, 255]).count()
            };
            // The data date line is dark blue and shows even where no blue bar does.
            let blue = count(crate::theme::BLUE)
                + count(crate::theme::TEAL)
                + count(crate::theme::DATA_DATE);
            let red = count(crate::theme::CRITICAL);
            assert!(
                blue > 10 && red > 10,
                "Scheduled activity bars must reach the raster output ({name}: {blue} blue, {red} red)"
            );
            let path = artifacts.join(name);
            image.save(&path).unwrap();
            report.push(serde_json::json!({"file":name,"width":1500,"height":900,"triangles":triangles,"textures":painter.textures.len()}));
            println!("Software-rendered production UI: {}", path.display());
        }
    }
    std::fs::remove_file(database).unwrap();
    let report = serde_json::json!({"renderer":"egui tessellation with CPU texture/triangle rasterization","native_window_created":false,"desktop_input_used":false,"limitations":"No native compositor, GPU driver, platform chrome, or DPI negotiation is verified. Linear/nearest texture sampling and premultiplied gamma-space blending are implemented; GPU dithering is omitted.","images":report});
    std::fs::write(
        artifacts.join("headless-render.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

/// Renders each scheduling dialog alone to the ignored `artifacts/review/`
/// folder, for review.
#[test]
fn render_schedule_dialogs_for_review() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/openxer");
    std::fs::create_dir_all(&out).unwrap();
    let database =
        std::env::temp_dir().join(format!("openxer-dialog-preview-{}.db", std::process::id()));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let store = Store::open(database.clone()).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        let dialogs: [(&str, &str, Vec2); 6] = [
            ("schedule.png", "schedule", Vec2::new(533., 273.)),
            ("options-general.png", "options", Vec2::new(617., 630.)),
            ("options-advanced.png", "options", Vec2::new(617., 630.)),
            ("check-schedule.png", "check", Vec2::new(869., 678.)),
            ("out-of-sequence.png", "sequence", Vec2::new(496., 118.)),
            (
                "out-of-sequence-detail.png",
                "sequence_detail",
                Vec2::new(638., 452.),
            ),
        ];
        for (name, salt, client) in dialogs {
            app.sched.schedule_open = false;
            app.show_schedule = false;
            app.sched.check_open = false;
            app.sched.sequence = None;
            app.sched.sequence_detail = false;
            app.sched.options_advanced = false;
            match name {
                "schedule.png" => {
                    app.sched.log_to_file = true;
                    app.open_schedule_dialog();
                }
                "options-general.png" => app.open_options(),
                "options-advanced.png" => {
                    app.open_options();
                    app.sched.options_advanced = true;
                }
                "check-schedule.png" => app.open_check_schedule(),
                _ => {
                    app.sched.sequence = Some(schedule_dialogs::SequencePrompt {
                        activity: app.project.model.links[0].succ,
                        link: 0,
                        previous: None,
                        status_revert: None,
                    });
                    app.sched.sequence_detail = name == "out-of-sequence-detail.png";
                }
            }
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            let close = app.action_rects[&format!("{salt}_title_close")];
            let size = client + Vec2::new(2., 33.);
            let left = (close[2] + 1. - size.x).round() as u32;
            let top = (close[1] - 1.).round() as u32;
            let cropped =
                image::imageops::crop_imm(&image, left, top, size.x as u32, size.y as u32)
                    .to_image();
            cropped.save(out.join(name)).unwrap();
        }
    }
    let _ = std::fs::remove_file(database);
}

#[test]
fn render_wbs_projects_and_resources_views_to_png() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-views-preview-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let project = views::resources::tests::with_resource();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        for (name, action) in [
            ("headless-wbs.png", "directory_wbs"),
            ("headless-projects.png", "directory_projects"),
            ("headless-resources.png", "directory_resources"),
        ] {
            painter.click_action(&mut app, &ctx, &mut time, action);
            let (image, triangles) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            assert!(triangles > 500, "{name} draws the dense view");
            let selected = image.pixels().filter(|p| p.0 == SELECT.to_array()).count();
            assert!(selected > 100, "{name} shows a selected row");
            image.save(artifacts.join(name)).unwrap();
            println!("Software-rendered view: {}", artifacts.join(name).display());
        }
        assert!(app.error.is_none());
    }
    let _ = std::fs::remove_file(database);
}

/// The Resources table with all three resources selected (Ctrl+A).
#[test]
fn render_resources_multi_selection_to_png() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-multiselect-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let project = views::resources::tests::with_resource();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        painter.click_action(&mut app, &ctx, &mut time, "directory_resources");
        app.views.resources.assigned_only = false;
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        app.grid_view_select_all();
        assert_eq!(app.views.resources.picked(), [7, 8, 9]);
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        // Three 18 px rows across the table's columns.
        let selected = image.pixels().filter(|p| p.0 == SELECT.to_array()).count();
        assert!(selected > 3 * 16 * 400, "{selected} selected pixels");
        let name = "headless-resources-multiselect.png";
        image.save(artifacts.join(name)).unwrap();
        println!("Software-rendered view: {}", artifacts.join(name).display());
        assert!(app.error.is_none());
    }
    let _ = std::fs::remove_file(database);
}

/// Renders the main window at 1920 x 1009 (96 DPI, a maximized window on a
/// 1080p monitor) with its menus and toolbars, for review.
#[test]
fn render_main_window_chrome_for_review() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/review/chrome");
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-chrome-review-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter {
            screen: Some(Vec2::new(1920., 1009.)),
            ..Default::default()
        };
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        assert_eq!(image.dimensions(), (1920, 1009));
        image.save(output.join("openxer-main.png")).unwrap();
        // Each main menu dropdown, opened with a real click on its title.
        for name in [
            "file",
            "edit",
            "view",
            "project",
            "enterprise",
            "tools",
            "admin",
            "help",
        ] {
            painter.click_action(&mut app, &ctx, &mut time, &format!("menu_bar_{name}"));
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            image
                .save(output.join(format!("openxer-menu-{name}.png")))
                .unwrap();
            painter.click_action(&mut app, &ctx, &mut time, &format!("menu_bar_{name}"));
        }
        // View > Zoom: dropdown and submenu geometry: the submenu window
        // overlaps the dropdown by 2 px with its top 2 px above the item,
        // and both cast a soft shadow offset 4 px right and down.
        let title = app.action_rects["menu_bar_view"];
        painter.click_action(&mut app, &ctx, &mut time, "menu_bar_view");
        assert!(painter.hover_text(&mut app, &ctx, &mut time, "Zoom"));
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image
            .save(output.join("openxer-menu-view-zoom.png"))
            .unwrap();
        let base = image::open(output.join("openxer-main.png"))
            .unwrap()
            .into_rgba8();
        check_submenu_geometry(&image, &base, title[0] as u32);
        painter.click_action(&mut app, &ctx, &mut time, "menu_bar_view");
        // The layout options popup behind the `Layout:` button.
        painter.click_action(&mut app, &ctx, &mut time, "layout_button");
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("openxer-layout-menu.png")).unwrap();
        painter.click_action(&mut app, &ctx, &mut time, "layout_button");
        println!(
            "Chrome review render: {}",
            output.join("openxer-main.png").display()
        );
    }
    let _ = std::fs::remove_file(database);
}

/// Dropdown and submenu geometry in a render with View > Zoom open
/// (`left` is the dropdown's left edge); `base` is the same window with no
/// menu open, to see the shadows against.
fn check_submenu_geometry(image: &image::RgbaImage, base: &image::RgbaImage, left: u32) {
    let px = |x: u32, y: u32| {
        let p = image.get_pixel(x, y).0;
        Color32::from_rgb(p[0], p[1], p[2])
    };
    let border = chrome::pal::SHADOW;
    // The dropdown: its top border runs along y 23 from the title's left.
    let top = 23;
    assert_eq!(px(left, top), border, "dropdown top-left corner");
    let mut right = left;
    while px(right + 1, top) == border {
        right += 1;
    }
    let mut bottom = top;
    while px(left, bottom + 1) == border {
        bottom += 1;
    }
    // Its widest item, "Bottom Layout Options...", plus the 25 px before and
    // 26 px after an item's text. To the pixel only with Arial itself: a
    // metric-compatible substitute (Liberation Sans on Linux) draws that
    // text 1 px wider, egui's own font 3 px.
    if chrome::arial_is_real() {
        assert_eq!(right - left + 1, 186, "View dropdown width");
    } else {
        let width = right - left + 1;
        assert!(width.abs_diff(186) <= 3, "View dropdown width {width}");
    }
    // The submenu's left border is the dropdown's second-last column; the
    // dropdown's own right border is under the submenu.
    let column = right - 1;
    let probe = (top..bottom).find(|&y| px(right - 20, y) == chrome::pal::MENU_HOT);
    let item_top = probe.expect("the Zoom item is hot");
    let mut sub_top = item_top + 5;
    assert_eq!(
        px(column, sub_top),
        border,
        "submenu left border at x {column}"
    );
    assert_ne!(
        px(column - 1, sub_top),
        border,
        "nothing left of the submenu"
    );
    while px(column, sub_top - 1) == border {
        sub_top -= 1;
    }
    assert_eq!(sub_top + 2, item_top, "submenu top 2 px above the item");
    assert_eq!(px(column + 10, sub_top), border, "submenu top border");
    let mut sub_right = column;
    while px(sub_right + 1, sub_top) == border {
        sub_right += 1;
    }
    // Shadows: darker than the window under them, 2 px past the right and
    // bottom edges, and nothing at the top-left offset.
    let darker = |x: u32, y: u32| {
        let (a, b) = (image.get_pixel(x, y).0, base.get_pixel(x, y).0);
        (0..3).all(|c| a[c] < b[c])
    };
    assert!(darker(left + 40, bottom + 2), "dropdown shadow under it");
    assert!(
        darker(sub_right + 2, sub_top + 40),
        "submenu shadow at its right"
    );
    assert!(
        !darker(left - 2, top + 40),
        "no shadow left of the dropdown"
    );
}

/// Renders every main view and every Activities pane in a one-group window,
/// so a change to the window structure can be compared pixel for pixel
/// (`OPENXER_RENDER_DIR` picks the folder; default `artifacts/views`).
#[test]
fn render_every_view_in_one_tab_group() {
    let output = std::env::var_os("OPENXER_RENDER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("artifacts/views")
        });
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-every-view-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let project = views::resources::tests::with_resource();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        type Setup = Box<dyn Fn(&mut App)>;
        let mut shots: Vec<(String, Setup)> = vec![];
        for view in [
            views::MainView::Activities,
            views::MainView::Wbs,
            views::MainView::Projects,
            views::MainView::Resources,
            views::MainView::Assignments,
            views::MainView::Expenses,
            views::MainView::Documents,
            views::MainView::Risks,
            views::MainView::Issues,
            views::MainView::Thresholds,
        ] {
            shots.push((
                format!("view-{view:?}"),
                Box::new(move |app: &mut App| app.show_view(view)),
            ));
        }
        for bottom in panes::BottomView::MENU {
            shots.push((
                format!("bottom-{}", bottom.key()),
                Box::new(move |app: &mut App| {
                    app.show_view(views::MainView::Activities);
                    app.panes.bottom = bottom;
                    app.show_details = true;
                }),
            ));
        }
        shots.push((
            "top-network".into(),
            Box::new(|app: &mut App| {
                app.panes.bottom = panes::BottomView::Details;
                app.panes.top = panes::TopView::Network;
            }),
        ));
        shots.push((
            "no-bottom".into(),
            Box::new(|app: &mut App| {
                app.panes.top = panes::TopView::Gantt;
                app.show_details = false;
            }),
        ));
        for (name, setup) in shots {
            setup(&mut app);
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            image.save(output.join(format!("{name}.png"))).unwrap();
        }
        assert!(app.error.is_none(), "{:?}", app.error);
    }
    let _ = std::fs::remove_file(database);
}

/// Renders a toolbar chevron's menu: open on the Display bar, its Add or
/// Remove Buttons list with a button switched off, the bar without that
/// button, and the menu of a left-dock bar.
#[test]
fn render_toolbar_chevron_menus() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/toolbars");
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-chevron-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        let shot = |painter: &mut SoftwarePainter, app: &mut App, time: &mut f64, name: &str| {
            for _ in 0..2 {
                let _ = painter.frame(app, &ctx, time, vec![], false);
            }
            let (image, _) = painter.frame(app, &ctx, time, vec![], true).unwrap();
            let path = output.join(name);
            image.save(&path).unwrap();
            println!("Software-rendered chevron: {}", path.display());
        };
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        painter.click_action(&mut app, &ctx, &mut time, "bar_chevron_Display");
        assert!(app.action_rects.contains_key("chevron_customize"));
        shot(&mut painter, &mut app, &mut time, "chevron-menu.png");
        assert!(painter.hover_text(&mut app, &ctx, &mut time, "Add or Remove Buttons"));
        painter.click_action(&mut app, &ctx, &mut time, "chevron_button_tool_tab_merge");
        // Back over the list, as the pointer would be after the click.
        assert!(painter.hover_text(&mut app, &ctx, &mut time, "Zoom In"));
        shot(&mut painter, &mut app, &mut time, "chevron-submenu.png");
        assert!(!app.button_visible("tool_tab_merge"));
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            false,
        );
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerGone],
            false,
        );
        // The bar without the button, the chevron hot under the pointer.
        let [l, t, r, b] = app.action_rects["bar_chevron_Display"];
        let at = Pos2::new((l + r) / 2., (t + b) / 2.);
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerMoved(at)],
            false,
        );
        assert!(!app.action_rects.contains_key("chevron_customize"));
        shot(
            &mut painter,
            &mut app,
            &mut time,
            "toolbar-button-hidden.png",
        );
        // A left-dock bar's chevron opens its menu to the right.
        let _ = painter.frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerGone],
            false,
        );
        painter.click_action(&mut app, &ctx, &mut time, "bar_chevron_Project");
        assert!(painter.hover_text(&mut app, &ctx, &mut time, "Add or Remove Buttons"));
        shot(&mut painter, &mut app, &mut time, "chevron-left-dock.png");
        assert!(app.error.is_none(), "{:?}", app.error);
    }
    let _ = std::fs::remove_file(database);
}

/// Renders two-group windows: the Activities view beside the Additional
/// Activities View (vertical split), and Activities over WBS (horizontal).
#[test]
fn render_two_tab_groups() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/views");
    std::fs::create_dir_all(&output).unwrap();
    let database = std::env::temp_dir().join(format!(
        "openxer-two-groups-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let mut store = Store::open(database.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        painter.click_action(&mut app, &ctx, &mut time, "dock_additional_activities");
        painter.click_action(&mut app, &ctx, &mut time, "tool_tab_vertical");
        assert_eq!(app.views.groups.groups.len(), 2);
        // A different look in the additional view: no bottom pane, collapsed.
        app.show_details = false;
        app.collapse_all_bands();
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("two-groups-vertical.png")).unwrap();
        painter.click_action(&mut app, &ctx, &mut time, "tool_tab_merge");
        painter.click_action(&mut app, &ctx, &mut time, "directory_wbs");
        painter.click_action(&mut app, &ctx, &mut time, "tool_tab_horizontal");
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image
            .save(output.join("two-groups-horizontal.png"))
            .unwrap();
        // Every optional toolbar switched on: they fill the second row.
        for bar in menu::OPTIONAL_TOOLBARS {
            app.toggle_toolbar(bar);
        }
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("optional-toolbars.png")).unwrap();
        // Toolbars > Customize... with a hidden Display button, toolbars locked.
        app.open_customize_toolbars();
        app.views.customize = Some(toolbars::Customize { bar: 5 });
        app.views.hidden_buttons.insert("tool_whats_new".into());
        app.views.toolbars_locked = true;
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(output.join("customize-toolbars.png")).unwrap();
        assert!(app.error.is_none(), "{:?}", app.error);
    }
    let _ = std::fs::remove_file(database);
}
