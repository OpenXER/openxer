//! Page settings of a report: paper, orientation, margins and the three-part
//! header and footer bands, held in the nested payload (`PageSettingsList`).
//! Nodes and words this code does not know stay in the tree and are written
//! back as they were.
use openxer_xer::tree::{self, Tree};

pub const PAYLOAD_PROP: &str = "PageSettingsList.Strings";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// A Windows paper code and its size in inches (portrait).
pub const PAPERS: [(i32, &str, f32, f32); 6] = [
    (1, "Letter", 8.5, 11.),
    (5, "Legal", 8.5, 14.),
    (3, "Tabloid", 11., 17.),
    (8, "A3", 11.69, 16.54),
    (9, "A4", 8.27, 11.69),
    (11, "A5", 5.83, 8.27),
];

/// One of the left, middle or right parts of a header or footer.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub text: String,
    pub font_name: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Margins {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub header: f32,
    pub footer: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub orientation: Orientation,
    pub paper: i32,
    /// Page size in inches as it prints (width is the horizontal side).
    pub width: f32,
    pub length: f32,
    pub scale: f32,
    pub fit_to_page: bool,
    pub margins: Margins,
    /// Header and footer parts: left, middle, right.
    pub header: [Part; 3],
    pub footer: [Part; 3],
}

fn part(t: Option<&Tree>) -> Part {
    let font = t.and_then(|t| t.child("font"));
    let get = |k: &str| font.and_then(|f| f.get(k));
    let style = get("style").unwrap_or("Regular");
    Part {
        text: t.and_then(|t| t.text("text")).unwrap_or_default(),
        font_name: get("font_name").unwrap_or("Arial").to_string(),
        size: get("size").and_then(|s| s.parse().ok()).unwrap_or(10.),
        bold: style.contains("Bold"),
        italic: style.contains("Italic"),
        color: get("color").and_then(|s| s.parse().ok()).unwrap_or(0),
    }
}

impl Default for Page {
    fn default() -> Self {
        Page::from_tree(&Tree::new(""))
    }
}

impl Page {
    pub fn from_tree(root: &Tree) -> Page {
        let page = root.child("page");
        let g = |t: Option<&Tree>, k: &str| t.and_then(|t| t.get(k)).map(str::to_string);
        let f = |t: Option<&Tree>, k: &str, d: f32| {
            g(t, k)
                .and_then(|s| s.trim().parse::<f32>().ok())
                .unwrap_or(d)
        };
        let m = root.child("margins");
        let band = |name: &str| -> [Part; 3] {
            let b = root.child(name);
            ["left", "middle", "right"].map(|p| part(b.and_then(|b| b.child(p))))
        };
        let orientation = if g(page, "orientation").as_deref() == Some("2") {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        let mut s = Page {
            orientation,
            paper: g(page, "paper_size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            width: f(page, "width", 8.5),
            length: f(page, "length", 11.),
            scale: f(page, "scale", 100.).clamp(10., 400.),
            fit_to_page: g(page, "use_fit_to").as_deref() == Some("Y"),
            margins: Margins {
                left: f(m, "left_margin", 0.5),
                right: f(m, "right_margin", 0.5),
                top: f(m, "top_margin", 0.5),
                bottom: f(m, "bottom_margin", 0.5),
                header: f(m, "header_margin", 0.5),
                footer: f(m, "footer_margin", 0.5),
            },
            header: band("header"),
            footer: band("footer"),
        };
        s.sanitize();
        s
    }

    /// Keeps sizes in a printable range.
    pub fn sanitize(&mut self) {
        self.width = self.width.clamp(1., 60.);
        self.length = self.length.clamp(1., 60.);
        let m = &mut self.margins;
        for v in [
            &mut m.left,
            &mut m.right,
            &mut m.top,
            &mut m.bottom,
            &mut m.header,
            &mut m.footer,
        ] {
            *v = v.clamp(0., 5.);
        }
        // Margins cannot swallow the page.
        let (w, l) = (self.width, self.length);
        if m.left + m.right > w - 0.5 {
            m.left = 0.25;
            m.right = 0.25;
        }
        if m.top + m.bottom > l - 0.5 {
            m.top = 0.25;
            m.bottom = 0.25;
        }
    }

    /// Printable area in inches.
    pub fn content(&self) -> (f32, f32) {
        (
            self.width - self.margins.left - self.margins.right,
            self.length - self.margins.top - self.margins.bottom,
        )
    }

    /// Chooses a paper and orientation together with the size they print at.
    pub fn set_paper(&mut self, code: i32, orientation: Orientation) {
        let (w, l) = PAPERS.iter().find(|p| p.0 == code).map_or(
            (self.width.min(self.length), self.width.max(self.length)),
            |p| (p.2, p.3),
        );
        self.paper = code;
        self.orientation = orientation;
        match orientation {
            Orientation::Portrait => {
                self.width = w;
                self.length = l;
            }
            Orientation::Landscape => {
                self.width = l;
                self.length = w;
            }
        }
        self.sanitize();
    }

    /// Writes the settings that differ from what `root` holds, keeping
    /// everything else in it (including the exact text of unchanged words).
    pub fn apply(&self, root: &mut Tree) {
        let old = Page::from_tree(root);
        let num = |v: f32| {
            let s = format!("{v:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        fn node<'a>(root: &'a mut Tree, name: &str) -> &'a mut Tree {
            if root.child(name).is_none() {
                root.children.push(Tree::new(name));
            }
            root.child_mut(name).expect("node was just added")
        }
        let p = node(root, "page");
        if old.orientation != self.orientation || p.get("orientation").is_none() {
            p.set(
                "orientation",
                if self.orientation == Orientation::Landscape {
                    "2"
                } else {
                    "1"
                },
            );
        }
        if old.paper != self.paper || p.get("paper_size").is_none() {
            p.set("paper_size", &self.paper.to_string());
        }
        if old.length != self.length || p.get("length").is_none() {
            p.set("length", &num(self.length));
        }
        if old.width != self.width || p.get("width").is_none() {
            p.set("width", &num(self.width));
        }
        if old.scale != self.scale || p.get("scale").is_none() {
            p.set("scale", &num(self.scale));
        }
        if old.fit_to_page != self.fit_to_page || p.get("use_fit_to").is_none() {
            p.set("use_fit_to", if self.fit_to_page { "Y" } else { "N" });
        }
        let m = node(root, "margins");
        for (key, was, now) in [
            ("left_margin", old.margins.left, self.margins.left),
            ("right_margin", old.margins.right, self.margins.right),
            ("top_margin", old.margins.top, self.margins.top),
            ("bottom_margin", old.margins.bottom, self.margins.bottom),
            ("header_margin", old.margins.header, self.margins.header),
            ("footer_margin", old.margins.footer, self.margins.footer),
        ] {
            if was != now || m.get(key).is_none() {
                m.set(key, &num(now));
            }
        }
        for (name, was, parts) in [
            ("header", &old.header, &self.header),
            ("footer", &old.footer, &self.footer),
        ] {
            let band = node(root, name);
            for (k, slot) in ["left", "middle", "right"].iter().enumerate() {
                let part = &parts[k];
                let n = node(band, slot);
                if n.get("text").is_none() || was[k].text != part.text {
                    n.set("text", &tree::escape(&part.text));
                }
                if n.get("image_file_name").is_none() {
                    n.set("image_file_name", "");
                }
                if n.get("mode").is_none() {
                    n.set("mode", "T");
                }
                let font = node(n, "font");
                let (a, b) = (&was[k], part);
                if a.font_name != b.font_name || font.get("font_name").is_none() {
                    font.set("font_name", &b.font_name);
                }
                if a.color != b.color || font.get("color").is_none() {
                    font.set("color", &b.color.to_string());
                }
                if a.size != b.size || font.get("size").is_none() {
                    font.set("size", &num(b.size));
                }
                if (a.bold, a.italic) != (b.bold, b.italic) || font.get("style").is_none() {
                    font.set(
                        "style",
                        match (b.bold, b.italic) {
                            (false, false) => "Regular",
                            (true, false) => "Bold",
                            (false, true) => "Italic",
                            (true, true) => "Bold Italic",
                        },
                    );
                }
            }
        }
    }
}

/// The payload text of a list of lines.
pub fn payload_text(lines: &[String]) -> String {
    lines.join("\r\n")
}

/// The payload tree of a property's lines; `None` when there is none or it
/// does not read.
pub fn read_payload(lines: &[String]) -> Option<Tree> {
    if lines.is_empty() {
        return None;
    }
    Tree::parse(&payload_text(lines)).ok()
}

/// The lines of a payload tree.
pub fn payload_lines(t: &Tree) -> Vec<String> {
    t.write("\r\n").split("\r\n").map(String::from).collect()
}

/// A payload for a new report: Letter landscape, half-inch margins, the
/// date at the top right and the page number at the bottom middle.
pub fn new_payload() -> Tree {
    let mut root = Tree::new("");
    let mut page = Page::from_tree(&root);
    page.set_paper(1, Orientation::Landscape);
    page.margins = Margins {
        left: 0.5,
        right: 0.5,
        top: 0.5,
        bottom: 0.5,
        header: 0.3,
        footer: 0.3,
    };
    page.header[2].text = "[date] [time]".into();
    page.footer[1].text = "Page [page_number] of [total_pages]".into();
    page.apply(&mut root);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_settings_round_trip_through_the_payload() {
        let mut root = new_payload();
        let mut page = Page::from_tree(&root);
        assert_eq!(page.orientation, Orientation::Landscape);
        assert_eq!((page.width, page.length), (11., 8.5));
        assert_eq!(page.header[2].text, "[date] [time]");
        page.set_paper(9, Orientation::Portrait);
        page.margins.left = 0.75;
        page.header[0].text = "Plan (rev 2) | draft".into();
        page.footer[1].bold = true;
        page.apply(&mut root);
        let again = Page::from_tree(&read_payload(&payload_lines(&root)).unwrap());
        assert_eq!(again, page);
        assert_eq!(again.header[0].text, "Plan (rev 2) | draft");
    }

    #[test]
    fn unknown_nodes_are_kept() {
        let mut root = Tree::parse("(0||()(\r\n  (0||legend()())\r\n  (0||page(orientation|1|paper_size|1|extra|keep)()))))").unwrap();
        let mut page = Page::from_tree(&root);
        page.scale = 80.;
        page.apply(&mut root);
        assert!(root.child("legend").is_some());
        assert_eq!(root.child("page").unwrap().get("extra"), Some("keep"));
        assert_eq!(root.child("page").unwrap().get("scale"), Some("80"));
    }

    #[test]
    fn margins_cannot_swallow_the_page() {
        let mut p = Page::default();
        p.margins.left = 5.;
        p.margins.right = 5.;
        p.sanitize();
        assert!(p.content().0 > 1.);
    }
}
