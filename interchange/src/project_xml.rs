//! Project XML: the published project exchange schema whose root element is
//! `APIBusinessObjects`, written from and read into the same XER tables the
//! XER export writes and the XER import reads. Nothing here holds a business
//! rule: the writer maps the export document (`xer_export::export_document`,
//! baselines added as the XER export adds them) field by field, and the
//! reader builds a `Document` that the import wizard, the CLI and MCP then
//! treat exactly like a document read from an XER file.
//!
//! Element and field names are the schema's own interface names, as XER
//! table and column names are. The element types, the order of their
//! children and the value forms follow a file the reference exported
//! (`project_xml_specs.rs` lists each type and marks what was not
//! observed). What is carried: projects with their options, root WBS
//! values and schedule options, WBS, activities with dates, durations,
//! constraints, percent complete and status, relationships with lag,
//! calendars with their work week and exceptions, resources with their
//! codes, assignments, activity code types and values with each activity's
//! codes, UDF types and values, expenses, notebook topics with project, WBS
//! and activity notes, the OBS, currencies and the financial period
//! template, plus cost accounts, expense categories, units of measure,
//! roles and rates, resource rates and roles and baselines where the
//! project has them (those last are not observed in a reference file).
//!
//! Conventions of the format: dates are `YYYY-MM-DDTHH:MM:SS`, durations
//! and lag are in hours, percents are fractions (0.5 for 50 %), flags are
//! `1` / `0`, enumerations are their labels (`Task Dependent`, `Finish to
//! Start`), an empty field the reference writes as null is
//! `<Name xsi:nil="true" />`, and a work period's finish is written one
//! minute early (`16:59:00` for a period that ends at 17:00); a finish read
//! one minute short of a quarter hour is moved one minute on.
use crate::util::num;
use openxer_xer::calendar_data::{CalendarData, Shift};
use openxer_xer::{Document, Table};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[path = "project_xml_specs.rs"]
mod specs;
use specs::*;

/// The root element.
pub const ROOT: &str = "APIBusinessObjects";

// The exchange format requires these identifiers on its root element. They
// are functional interface identifiers of the format, of the same standing
// as the XER table and column names, and the only place the format owner's
// name is written.
/// The schema location base, with `{version}` where the release goes.
pub const SCHEMA_BASE: &str = "http://xmlns.oracle.com/Primavera/P6Professional/V{version}/API";
/// The schema release written into [`SCHEMA_BASE`].
pub const SCHEMA_VERSION: &str = "25.12";
/// The last part of the namespace, after the base.
pub const NAMESPACE_LEAF: &str = "BusinessObjects";
/// The schema file named in the schema location, after the base.
pub const SCHEMA_FILE: &str = "p6apibo.xsd";
// End of the format owner's identifiers.

/// The XML Schema instance namespace (for `xsi:nil` and the schema location).
pub const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

fn schema_base() -> String {
    SCHEMA_BASE.replace("{version}", SCHEMA_VERSION)
}
/// The root namespace as written.
pub fn namespace() -> String {
    format!("{}/{NAMESPACE_LEAF}", schema_base())
}
/// The `xsi:schemaLocation` value: the namespace, a space, the schema file.
pub fn schema_location() -> String {
    format!("{} {}/{SCHEMA_FILE}", namespace(), schema_base())
}
/// The root start tag as written.
pub fn root_tag() -> String {
    format!(
        "<{ROOT} xmlns=\"{}\" xmlns:xsi=\"{XSI}\" xsi:schemaLocation=\"{}\">",
        namespace(),
        schema_location()
    )
}
/// The first line of a written file.
pub const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";

/// The XER header a document read from XML gets, around the date.
const HEADER_START: &str = "ERMHDR\t25.12";
const HEADER_END: &str = "Project\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD";

/// The XER tables and columns the XML form carries both ways, for tests and
/// for the report of what an XML export leaves out. Calendar work times
/// (`clndr_data`), activity and resource codes (`TASKACTV`, `RSRCRCAT`),
/// UDF values (`UDFVALUE`) and the root WBS row are carried through their
/// own elements.
pub fn carried_columns() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = SPECS
        .iter()
        .map(|s| {
            let mut cols: Vec<&'static str> = s
                .fields
                .iter()
                .map(|f| f.xer)
                .filter(|c| !c.contains(['.', '=']))
                .collect();
            if s.table == "SCHEDOPTIONS" {
                cols.extend(["sched_retained_logic", "sched_progress_override"]);
            }
            (s.table, cols)
        })
        .collect();
    out.push((
        "TASKACTV",
        vec!["task_id", "actv_code_type_id", "actv_code_id", "proj_id"],
    ));
    out.push((
        "RSRCRCAT",
        vec!["rsrc_id", "rsrc_catg_type_id", "rsrc_catg_id"],
    ));
    out.push((
        "UDFVALUE",
        vec![
            "udf_type_id",
            "fk_id",
            "proj_id",
            "udf_text",
            "udf_number",
            "udf_date",
        ],
    ));
    out
}

/// The root WBS columns the project element carries.
pub fn carried_root_columns() -> Vec<&'static str> {
    let mut cols = vec!["proj_id", "wbs_short_name"];
    cols.extend(
        PROJECT
            .fields
            .iter()
            .filter_map(|f| f.xer.strip_prefix("wbs."))
            .filter(|c| *c != "parent_wbs_id"),
    );
    cols
}

// ------------------------------------------------------------ values

/// Move the decimal point of a plain decimal number `places` to the right
/// (left when negative), exactly, without floating point.
fn shift_decimal(s: &str, places: i32) -> Option<String> {
    let s = s.trim();
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if body.is_empty() || body.contains(['e', 'E']) {
        let v = s.parse::<f64>().ok()?;
        return Some(trim_number(&format!("{:.10}", v * 10f64.powi(places))));
    }
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits: String = format!("{int}{frac}");
    // Position of the decimal point within `digits` after the shift.
    let point = int.len() as i32 + places;
    let (mut whole, mut part) = if point <= 0 {
        (
            String::new(),
            format!("{}{digits}", "0".repeat((-point) as usize)),
        )
    } else if point as usize >= digits.len() {
        (
            format!("{digits}{}", "0".repeat(point as usize - digits.len())),
            String::new(),
        )
    } else {
        (
            digits[..point as usize].to_string(),
            digits[point as usize..].to_string(),
        )
    };
    while part.ends_with('0') {
        part.pop();
    }
    let trimmed = whole.trim_start_matches('0').to_string();
    whole = if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed
    };
    let zero = whole == "0" && part.is_empty();
    let mut out = String::new();
    if negative && !zero {
        out.push('-');
    }
    out.push_str(&whole);
    if !part.is_empty() {
        out.push('.');
        out.push_str(&part);
    }
    Some(out)
}

fn trim_number(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" {
        "0".into()
    } else {
        t.to_string()
    }
}

/// A plain decimal number with any trailing fraction zeros dropped (`8.0`
/// as `8`); anything else as it is.
fn plain_number(v: &str) -> String {
    let body = v.strip_prefix('-').unwrap_or(v);
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let decimal = !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.bytes().all(|b| b.is_ascii_digit());
    if decimal && v.contains('.') {
        trim_number(v)
    } else {
        v.to_string()
    }
}

/// An XER value as the XML writes it; `None` has no value.
fn to_xml(kind: Kind, value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    match kind {
        Kind::Text => Some(value.to_string()),
        Kind::Num => Some(plain_number(v)),
        Kind::Date => xml_date(v),
        Kind::Flag => match v {
            "Y" | "y" => Some("1".into()),
            "N" | "n" => Some("0".into()),
            _ => None,
        },
        Kind::Pct => shift_decimal(v, -2),
        Kind::Color => {
            let hex = v.trim_start_matches('#');
            (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| format!("#{}", hex.to_ascii_uppercase()))
        }
        Kind::Choice(list) => list
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(v))
            .map(|(_, label)| label.to_string()),
    }
}

/// An XML value as the XER tables hold it; `None` leaves the column blank.
fn from_xml(kind: Kind, value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    match kind {
        Kind::Text => Some(value.to_string()),
        Kind::Num => Some(v.to_string()),
        Kind::Date => xer_date(v),
        Kind::Flag => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "y" | "yes" => Some("Y".into()),
            "0" | "false" | "n" | "no" => Some("N".into()),
            _ => None,
        },
        Kind::Pct => shift_decimal(v, 2),
        Kind::Color => {
            let hex = v.trim_start_matches('#');
            (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| hex.to_ascii_uppercase())
        }
        Kind::Choice(list) => list
            .iter()
            .find(|(_, label)| label.eq_ignore_ascii_case(v))
            .map(|(code, _)| code.to_string()),
    }
}

/// `2026-01-05 08:00` (or a date alone) as `2026-01-05T08:00:00`.
fn xml_date(v: &str) -> Option<String> {
    let v = v.trim();
    let day = v.get(..10)?;
    if !valid_day(day) {
        return None;
    }
    let time = v.get(11..).map(str::trim).unwrap_or("");
    let (h, m) = time.split_once(':').unwrap_or(("00", "00"));
    let (h, m) = (h.parse::<u32>().ok()?, m.get(..2)?.parse::<u32>().ok()?);
    Some(format!("{day}T{h:02}:{m:02}:00"))
}

/// `2026-01-05T08:00:00` (seconds, fractions and a zone are ignored) or a
/// date alone as `2026-01-05 08:00`.
fn xer_date(v: &str) -> Option<String> {
    let day = v.get(..10)?;
    if !valid_day(day) {
        return None;
    }
    let rest = v.get(11..).unwrap_or("");
    let (h, m) = match rest.split(':').collect::<Vec<_>>().as_slice() {
        [h, m, ..] => (h.parse::<u32>().ok()?, m.get(..2)?.parse::<u32>().ok()?),
        _ => (0, 0),
    };
    Some(format!("{day} {h:02}:{m:02}"))
}

fn valid_day(day: &str) -> bool {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok()
}

/// Day 0 of the serial day numbers calendar exceptions are keyed by.
fn serial_origin() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(1899, 12, 30).expect("a valid date")
}

fn serial_to_day(serial: i64) -> String {
    (serial_origin() + chrono::Duration::days(serial))
        .format("%Y-%m-%d")
        .to_string()
}

fn day_to_serial(day: &str) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(day.get(..10)?, "%Y-%m-%d").ok()?;
    Some((d - serial_origin()).num_days())
}

// ------------------------------------------------------------ the tree

/// One element: its local name (any namespace prefix dropped), its
/// attributes, its text and its child elements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    /// Attributes by local name, in file order (`xmlns` declarations left out).
    pub attrs: Vec<(String, String)>,
    pub text: String,
    /// `xsi:nil="true"`.
    pub nil: bool,
    /// Written as a CDATA section.
    pub cdata: bool,
    pub children: Vec<Element>,
}

impl Element {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
    fn leaf(name: &str, text: String) -> Self {
        Self {
            name: name.into(),
            text,
            ..Self::default()
        }
    }
    fn nil(name: &str) -> Self {
        Self {
            name: name.into(),
            nil: true,
            ..Self::default()
        }
    }
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// The text of a child element.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.as_str())
    }
    /// The names of the children, in order.
    pub fn child_names(&self) -> Vec<&str> {
        self.children.iter().map(|c| c.name.as_str()).collect()
    }
}

/// Nesting deeper than this is not a project file.
const MAX_DEPTH: usize = 64;

fn decode_entities(raw: &str, out: &mut String) {
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|&e| e <= 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let c = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let n = if let Some(h) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(h, 16).ok()
                } else if let Some(d) = entity.strip_prefix('#') {
                    d.parse::<u32>().ok()
                } else {
                    None
                };
                n.and_then(char::from_u32)
            }
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The attributes of a start tag's inside after its name.
fn parse_attrs(mut rest: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    loop {
        rest = rest.trim_start();
        let Some(eq) = rest.find('=') else {
            break;
        };
        let name = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(end) = after[1..].find(q) else {
            break;
        };
        let mut value = String::new();
        decode_entities(&after[1..1 + end], &mut value);
        if name != "xmlns" && !name.starts_with("xmlns:") {
            out.push((local(&name).to_string(), value));
        }
        rest = &after[end + 2..];
    }
    out
}

/// Parse XML text into its root element.
pub fn parse_tree(text: &str) -> Result<Element, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut stack: Vec<Element> = vec![];
    let mut root: Option<Element> = None;
    let mut pos = 0;
    let err = |what: &str| Err(format!("Not a valid XML file: {what}"));
    while pos < text.len() {
        let rest = &text[pos..];
        let Some(lt) = rest.find('<') else {
            if !rest.trim().is_empty() && !stack.is_empty() {
                return err("the file ends inside an element");
            }
            break;
        };
        if lt > 0 {
            if let Some(top) = stack.last_mut() {
                decode_entities(&rest[..lt], &mut top.text);
            } else if !rest[..lt].trim().is_empty() {
                return err("text outside the root element");
            }
        }
        let tag = &rest[lt..];
        let skip_to =
            |marker: &str, from: usize| tag[from..].find(marker).map(|e| from + e + marker.len());
        if tag.starts_with("<?") {
            let Some(e) = skip_to("?>", 2) else {
                return err("unterminated declaration");
            };
            pos += lt + e;
        } else if tag.starts_with("<!--") {
            let Some(e) = skip_to("-->", 4) else {
                return err("unterminated comment");
            };
            pos += lt + e;
        } else if let Some(body) = tag.strip_prefix("<![CDATA[") {
            let Some(e) = body.find("]]>") else {
                return err("unterminated CDATA section");
            };
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&body[..e]);
                top.cdata = true;
            }
            pos += lt + 9 + e + 3;
        } else if tag.starts_with("<!") {
            // A document type declaration, with any internal subset.
            let mut depth = 0i32;
            let mut end = None;
            for (i, c) in tag.char_indices().skip(2) {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    '>' if depth <= 0 => {
                        end = Some(i + 1);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(e) = end else {
                return err("unterminated declaration");
            };
            pos += lt + e;
        } else if let Some(close) = tag.strip_prefix("</") {
            let Some(e) = close.find('>') else {
                return err("unterminated end tag");
            };
            let name = local(close[..e].trim());
            let Some(mut done) = stack.pop() else {
                return err("an end tag without its start tag");
            };
            if done.name != name {
                return Err(format!(
                    "Not a valid XML file: <{}> is closed by </{name}>",
                    done.name
                ));
            }
            // Whitespace between child elements is layout, not text.
            if !done.children.is_empty() && done.text.trim().is_empty() {
                done.text.clear();
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => root = Some(done),
            }
            pos += lt + 2 + e + 1;
        } else {
            // A start tag: the name, then attributes up to `>` outside quotes.
            let mut quote = None;
            let mut end = None;
            for (i, c) in tag.char_indices().skip(1) {
                match (quote, c) {
                    (Some(q), c) if c == q => quote = None,
                    (Some(_), _) => {}
                    (None, '"') | (None, '\'') => quote = Some(c),
                    (None, '>') => {
                        end = Some(i);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(e) = end else {
                return err("unterminated start tag");
            };
            let inner = &tag[1..e];
            let empty = inner.ends_with('/');
            let inner = inner.trim_end_matches('/');
            let name = inner
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("");
            if name.is_empty() {
                return err("an element without a name");
            }
            if root.is_some() {
                return err("more than one root element");
            }
            let mut element = Element::new(local(name));
            element.attrs = parse_attrs(&inner[name.len()..]);
            element.nil = element
                .attrs
                .iter()
                .any(|(k, v)| k == "nil" && v.trim() == "true");
            element.attrs.retain(|(k, _)| k != "nil");
            if empty {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            } else {
                if stack.len() >= MAX_DEPTH {
                    return err("elements nested too deeply");
                }
                stack.push(element);
            }
            pos += lt + e + 1;
        }
    }
    if !stack.is_empty() {
        return err("the file ends inside an element");
    }
    root.ok_or_else(|| "Not a valid XML file: no root element".to_string())
}

/// Text content as the reference writes it: `&`, `<` and `>` escaped,
/// quotes as they are.
fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\t' | '\n' | '\r' => out.push(c),
            // Characters XML 1.0 cannot hold are left out.
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
}

fn escape_attr(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            c => escape(c.encode_utf8(&mut [0; 4]), out),
        }
    }
}

/// Lines end with CR LF, as in the reference's files.
const EOL: &str = "\r\n";

fn write_element(e: &Element, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    out.push_str(&pad);
    out.push('<');
    out.push_str(&e.name);
    for (k, v) in &e.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_attr(v, out);
        out.push('"');
    }
    if e.nil {
        out.push_str(" xsi:nil=\"true\"");
    }
    if e.children.is_empty() {
        if e.text.is_empty() {
            out.push_str(" />");
        } else {
            out.push('>');
            if e.cdata && !e.text.contains("]]>") {
                out.push_str("<![CDATA[");
                out.push_str(&e.text);
                out.push_str("]]>");
            } else {
                escape(&e.text, out);
            }
            out.push_str("</");
            out.push_str(&e.name);
            out.push('>');
        }
        out.push_str(EOL);
        return;
    }
    out.push('>');
    out.push_str(EOL);
    for c in &e.children {
        write_element(c, depth + 1, out);
    }
    out.push_str(&pad);
    out.push_str("</");
    out.push_str(&e.name);
    out.push('>');
    out.push_str(EOL);
}

/// The whole file text of a root element.
fn write_document(root: &Element) -> String {
    let mut out = String::from(DECLARATION);
    out.push_str(EOL);
    out.push_str(&root_tag());
    out.push_str(EOL);
    for c in &root.children {
        write_element(c, 1, &mut out);
    }
    out.push_str(&format!("</{ROOT}>"));
    out.push_str(EOL);
    out
}

// ------------------------------------------------------------ writing

fn row_id(t: &Table, r: &[String], col: &str) -> Option<i64> {
    let v = t.value(r, col).trim();
    (!v.is_empty()).then(|| num(v))
}

/// The element for one row: its fields in the spec's order, each value
/// from `get` (a column name, or a `.`/`=` name the caller resolves), then
/// `nested` children, which the caller gives in the spec's nested order.
fn spec_element(spec: &Spec, get: &dyn Fn(&str) -> String, nested: Vec<Element>) -> Element {
    let mut e = Element::new(spec.element);
    for field in spec.fields {
        match to_xml(field.kind, &get(field.xer)) {
            Some(v) => e.children.push(Element::leaf(field.xml, v)),
            None => match field.empty {
                Empty::Omit => {}
                Empty::Nil => e.children.push(Element::nil(field.xml)),
                Empty::Blank => e.children.push(Element::new(field.xml)),
            },
        }
    }
    e.children.extend(nested);
    e
}

fn clock(minutes: i64) -> String {
    let m = minutes.rem_euclid(1440);
    format!("{:02}:{:02}:00", m / 60, m % 60)
}

/// `WorkTime` per period (`Start` then `Finish`, the finish its last
/// minute), or one nil `WorkTime` for a day without work.
fn work_times(shifts: &[Shift]) -> Vec<Element> {
    if shifts.is_empty() {
        return vec![Element::nil("WorkTime")];
    }
    shifts
        .iter()
        .map(|&(s, f)| {
            let mut w = Element::new("WorkTime");
            w.children.push(Element::leaf("Start", clock(s)));
            w.children.push(Element::leaf("Finish", clock(f - 1)));
            w
        })
        .collect()
}

/// The work week and exceptions of a `clndr_data` text.
fn calendar_children(data: &str) -> Vec<Element> {
    let Ok(cal) = CalendarData::parse(data) else {
        return vec![];
    };
    let mut week = Element::new("StandardWorkWeek");
    for (d, shifts) in cal.week.iter().enumerate() {
        let mut day = Element::new("StandardWorkHours");
        day.children
            .push(Element::leaf("DayOfWeek", DAY_NAMES[d].into()));
        day.children.extend(work_times(shifts));
        week.children.push(day);
    }
    let mut exceptions = Element::new("HolidayOrExceptions");
    for (serial, shifts) in &cal.exceptions {
        let mut day = Element::new("HolidayOrException");
        day.children.push(Element::leaf(
            "Date",
            format!("{}T00:00:00", serial_to_day(*serial)),
        ));
        day.children.extend(work_times(shifts));
        exceptions.children.push(day);
    }
    vec![week, exceptions]
}

/// UDF value element names by data type. Seen: `TextValue`, `CostValue`;
/// the others are not observed.
fn udf_value_element(data_type: &str) -> Option<&'static str> {
    Some(match data_type {
        "FT_TEXT" => "TextValue",
        "FT_MONEY" => "CostValue",
        "FT_START_DATE" => "StartDateValue",
        "FT_END_DATE" => "FinishDateValue",
        "FT_FLOAT_2_DECIMALS" | "FT_FLOAT" => "DoubleValue",
        "FT_INT" => "IntegerValue",
        "FT_STATICTYPE" => "IndicatorValue",
        _ => return None,
    })
}

/// The XER column a UDF value of a data type is held in; `None` for kinds
/// the XML form does not carry (code lists).
fn udf_column(data_type: &str) -> Option<&'static str> {
    match data_type {
        "FT_TEXT" | "FT_STATICTYPE" => Some("udf_text"),
        "FT_START_DATE" | "FT_END_DATE" | "FT_DATE" => Some("udf_date"),
        "FT_MONEY" | "FT_FLOAT_2_DECIMALS" | "FT_FLOAT" | "FT_INT" => Some("udf_number"),
        _ => None,
    }
}

/// Indexes over the document the writer looks rows up in.
struct Lookup<'a> {
    doc: &'a Document,
    /// UDF values by (owner table, owner id).
    udf_values: HashMap<(String, i64), Vec<usize>>,
    /// UDF type id to (owner table, data type).
    udf_types: HashMap<i64, (String, String)>,
    /// Activity code assignments by activity id.
    codes: HashMap<i64, Vec<usize>>,
    /// Resource code assignments by resource id.
    resource_codes: HashMap<i64, Vec<usize>>,
    /// Activity code type id to the project it belongs to.
    code_type_project: HashMap<i64, Option<i64>>,
    /// Activity id to its WBS id.
    task_wbs: HashMap<i64, String>,
    /// Project id to its root WBS row index.
    roots: HashMap<i64, usize>,
}

impl<'a> Lookup<'a> {
    fn new(doc: &'a Document) -> Self {
        let mut udf_types = HashMap::new();
        if let Some(t) = doc.table("UDFTYPE") {
            for r in &t.rows {
                udf_types.insert(
                    num(t.value(r, "udf_type_id")),
                    (
                        t.value(r, "table_name").trim().to_ascii_uppercase(),
                        t.value(r, "logical_data_type").trim().to_string(),
                    ),
                );
            }
        }
        let mut udf_values: HashMap<(String, i64), Vec<usize>> = HashMap::new();
        if let Some(t) = doc.table("UDFVALUE") {
            for (i, r) in t.rows.iter().enumerate() {
                if let Some((table, _)) = udf_types.get(&num(t.value(r, "udf_type_id"))) {
                    udf_values
                        .entry((table.clone(), num(t.value(r, "fk_id"))))
                        .or_default()
                        .push(i);
                }
            }
        }
        let index = |table: &str, col: &str| {
            let mut out: HashMap<i64, Vec<usize>> = HashMap::new();
            if let Some(t) = doc.table(table) {
                for (i, r) in t.rows.iter().enumerate() {
                    out.entry(num(t.value(r, col))).or_default().push(i);
                }
            }
            out
        };
        let mut code_type_project = HashMap::new();
        if let Some(t) = doc.table("ACTVTYPE") {
            for r in &t.rows {
                code_type_project.insert(
                    num(t.value(r, "actv_code_type_id")),
                    row_id(t, r, "proj_id"),
                );
            }
        }
        let mut task_wbs = HashMap::new();
        if let Some(t) = doc.table("TASK") {
            for r in &t.rows {
                task_wbs.insert(
                    num(t.value(r, "task_id")),
                    t.value(r, "wbs_id").trim().to_string(),
                );
            }
        }
        let mut roots = HashMap::new();
        if let Some(t) = doc.table("PROJWBS") {
            for (i, r) in t.rows.iter().enumerate() {
                if t.value(r, "proj_node_flag") == "Y" {
                    roots.entry(num(t.value(r, "proj_id"))).or_insert(i);
                }
            }
        }
        Self {
            doc,
            udf_values,
            udf_types,
            codes: index("TASKACTV", "task_id"),
            resource_codes: index("RSRCRCAT", "rsrc_id"),
            code_type_project,
            task_wbs,
            roots,
        }
    }

    fn root_wbs(&self, pid: i64) -> Option<(&Table, &[String])> {
        let t = self.doc.table("PROJWBS")?;
        let i = *self.roots.get(&pid)?;
        Some((t, &t.rows[i]))
    }

    /// `UDF` children of the row `id` of `table`: `TypeObjectId`, then the
    /// typed value.
    fn udfs(&self, table: &str, id: i64) -> Vec<Element> {
        let Some(rows) = self.udf_values.get(&(table.to_string(), id)) else {
            return vec![];
        };
        let t = self.doc.table("UDFVALUE").expect("indexed");
        rows.iter()
            .filter_map(|&i| {
                let r = &t.rows[i];
                let type_id = num(t.value(r, "udf_type_id"));
                let data_type = self.udf_types.get(&type_id).map_or("", |x| x.1.as_str());
                let element = udf_value_element(data_type)?;
                let value = match udf_column(data_type)? {
                    "udf_date" => to_xml(Kind::Date, t.value(r, "udf_date")),
                    "udf_text" => to_xml(Kind::Text, t.value(r, "udf_text")),
                    col => to_xml(Kind::Num, t.value(r, col)),
                }?;
                let mut e = Element::new("UDF");
                e.children
                    .push(Element::leaf("TypeObjectId", type_id.to_string()));
                e.children.push(Element::leaf(element, value));
                Some(e)
            })
            .collect()
    }

    /// `Code` children (`TypeObjectId`, `ValueObjectId`) from a code
    /// assignment table.
    fn code_children(
        &self,
        index: &HashMap<i64, Vec<usize>>,
        table: &str,
        cols: [&str; 2],
        id: i64,
    ) -> Vec<Element> {
        let (Some(rows), Some(t)) = (index.get(&id), self.doc.table(table)) else {
            return vec![];
        };
        rows.iter()
            .map(|&i| {
                let r = &t.rows[i];
                let mut e = Element::new("Code");
                e.children.push(Element::leaf(
                    "TypeObjectId",
                    t.value(r, cols[0]).trim().into(),
                ));
                e.children.push(Element::leaf(
                    "ValueObjectId",
                    t.value(r, cols[1]).trim().into(),
                ));
                e
            })
            .collect()
    }

    /// The nested children of one row of `spec`'s table, in the spec's
    /// nested order.
    fn nested(&self, spec: &Spec, t: &Table, r: &[String]) -> Vec<Element> {
        let id = spec.key.and_then(|k| row_id(t, r, k));
        let mut out = vec![];
        for name in spec.nested {
            let part = match (*name, id) {
                ("Code", Some(id)) if spec.table == "TASK" => {
                    let cols = ["actv_code_type_id", "actv_code_id"];
                    self.code_children(&self.codes, "TASKACTV", cols, id)
                }
                ("Code", Some(id)) if spec.table == "RSRC" => {
                    let cols = ["rsrc_catg_type_id", "rsrc_catg_id"];
                    self.code_children(&self.resource_codes, "RSRCRCAT", cols, id)
                }
                ("UDF", Some(id)) => self.udfs(spec.table, id),
                // The work week and the exceptions, together.
                ("StandardWorkWeek", _) => calendar_children(t.value(r, "clndr_data")),
                _ => vec![],
            };
            out.extend(part);
        }
        out
    }

    /// The project an activity code value's type belongs to.
    fn code_project(&self, t: &Table, r: &[String]) -> Option<i64> {
        self.code_type_project
            .get(&num(t.value(r, "actv_code_type_id")))
            .copied()
            .flatten()
    }

    /// A value of a row for the writer: its own column, or a looked-up one.
    fn get(&self, t: &Table, r: &[String], col: &str) -> String {
        if let Some(c) = col.strip_prefix("task.") {
            return self
                .task_wbs
                .get(&num(t.value(r, "task_id")))
                .filter(|_| c == "wbs_id")
                .cloned()
                .unwrap_or_default();
        }
        match col {
            "=proj_id" => t.value(r, "proj_id").to_string(),
            "=code_project" => id_text(self.code_project(t, r)),
            "=start" => first_of(t, r, &START_COLUMNS),
            "=finish" => first_of(t, r, &FINISH_COLUMNS),
            // The percent complete shown: the physical one on a physical
            // activity (the observed kind); the others are not written.
            "=percent" if t.value(r, "complete_pct_type") == "CP_Phys" => {
                t.value(r, "phys_complete_pct").to_string()
            }
            c if c.starts_with('=') || c.contains('.') => String::new(),
            c => t.value(r, c).to_string(),
        }
    }

    /// Elements for the rows of `spec`'s table that `keep` selects.
    fn elements(&self, spec: &Spec, keep: &dyn Fn(&Table, &[String]) -> bool) -> Vec<Element> {
        let Some(t) = self.doc.table(spec.table) else {
            return vec![];
        };
        t.rows
            .iter()
            .filter(|r| keep(t, r))
            .map(|r| spec_element(spec, &|c| self.get(t, r, c), self.nested(spec, t, r)))
            .collect()
    }
}

/// An activity's current start: its actual start, else its early start.
const START_COLUMNS: [&str; 3] = ["act_start_date", "early_start_date", "restart_date"];
/// An activity's current finish: its actual finish, else its early finish.
const FINISH_COLUMNS: [&str; 3] = ["act_end_date", "early_end_date", "reend_date"];

fn id_text(id: Option<i64>) -> String {
    id.map_or(String::new(), |v| v.to_string())
}

fn first_of(t: &Table, r: &[String], cols: &[&str]) -> String {
    cols.iter()
        .map(|c| t.value(r, c).trim())
        .find(|v| !v.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Replace each child `name` whose value is `id` by a nil one.
fn nil_where(elements: &mut [Element], name: &str, id: Option<i64>) {
    for e in elements {
        for c in &mut e.children {
            if c.name == name && c.text.trim().parse::<i64>().ok() == id {
                *c = Element::nil(name);
            }
        }
    }
}

/// The schedule options element of project `pid`, if it has options.
fn schedule_options_element(l: &Lookup, project: &[String], pid: i64) -> Vec<Element> {
    let (Some(p), Some(s)) = (l.doc.table("PROJECT"), l.doc.table("SCHEDOPTIONS")) else {
        return vec![];
    };
    let Some(r) = s.rows.iter().find(|r| num(s.value(r, "proj_id")) == pid) else {
        return vec![];
    };
    let get = |c: &str| -> String {
        if c == "=oos" {
            return out_of_sequence_code(s, r);
        }
        match c.strip_prefix("project.") {
            Some(c) => p.value(project, c).to_string(),
            None => s.value(r, c).to_string(),
        }
    };
    vec![spec_element(&SCHEDULE_OPTIONS, &get, vec![])]
}

/// The project element (or baseline element) of project `pid`.
fn project_element(l: &Lookup, pid: i64, baseline_of: Option<i64>) -> Option<Element> {
    let t = l.doc.table("PROJECT")?;
    let row = t.rows.iter().find(|r| num(t.value(r, "proj_id")) == pid)?;
    let root = l.root_wbs(pid);
    let root_id = root.and_then(|(w, r)| row_id(w, r, "wbs_id"));
    let own = |t: &Table, r: &[String]| row_id(t, r, "proj_id") == Some(pid);
    let own_wbs = |t: &Table, r: &[String]| own(t, r) && t.value(r, "proj_node_flag") != "Y";
    let own_code = |t: &Table, r: &[String]| l.code_project(t, r) == Some(pid);
    let mut nested = vec![];
    for name in PROJECT.nested {
        let part = match *name {
            "Calendar" => l.elements(&CALENDAR, &own),
            "WBS" => {
                // A top WBS element has no parent element: nil.
                let mut wbs = l.elements(&WBS, &own_wbs);
                nil_where(&mut wbs, "ParentObjectId", root_id);
                wbs
            }
            "ProjectNote" => {
                // The project's own notes name no WBS element.
                let mut notes = l.elements(&PROJECT_NOTE, &own);
                nil_where(&mut notes, "WBSObjectId", root_id);
                notes
            }
            "ActivityCodeType" => l.elements(&CODE_TYPE, &own),
            "ActivityCode" => l.elements(&CODE, &own_code),
            "Activity" => l.elements(&ACTIVITY, &own),
            "ResourceAssignment" => l.elements(&ASSIGNMENT, &own),
            "ActivityExpense" => l.elements(&EXPENSE, &own),
            "ActivityNote" => l.elements(&ACTIVITY_NOTE, &own),
            "Relationship" => l.elements(&RELATIONSHIP, &own),
            "ScheduleOptions" => schedule_options_element(l, row, pid),
            "UDF" => l.udfs("PROJECT", pid),
            _ => vec![],
        };
        nested.extend(part);
    }
    let get = |c: &str| -> String {
        if let Some(c) = c.strip_prefix("wbs.") {
            return root.map_or(String::new(), |(w, r)| w.value(r, c).to_string());
        }
        match c {
            "=baseline_of" => id_text(baseline_of),
            c => t.value(row, c).to_string(),
        }
    };
    let mut e = spec_element(&PROJECT, &get, nested);
    for c in &mut e.children {
        // The reference writes the project's ID and name as CDATA.
        if c.name == "Id" || c.name == "Name" {
            c.cdata = true;
        }
    }
    if baseline_of.is_some() {
        e.name = "BaselineProject".into();
    }
    Some(e)
}

/// The out of sequence choice of an XER options row.
fn out_of_sequence_code(t: &Table, r: &[String]) -> String {
    let flag = |c: &str| t.value(r, c).trim().to_string();
    if flag("sched_retained_logic") == "Y" {
        "retained"
    } else if flag("sched_progress_override") == "Y" {
        "override"
    } else if flag("sched_retained_logic") == "N" {
        "actual"
    } else {
        ""
    }
    .into()
}

/// The `ProjectList` element: each project (not a baseline) by its
/// `ObjectId`, with its ID and name.
fn project_list(l: &Lookup, projects: &[i64]) -> Element {
    let mut list = Element::new("ProjectList");
    let Some(t) = l.doc.table("PROJECT") else {
        return list;
    };
    for pid in projects {
        let Some(row) = t.rows.iter().find(|r| num(t.value(r, "proj_id")) == *pid) else {
            continue;
        };
        let name = l
            .root_wbs(*pid)
            .map(|(w, r)| w.value(r, "wbs_name"))
            .unwrap_or("");
        let mut e = Element::new("Project");
        e.attrs.push(("ObjectId".into(), pid.to_string()));
        for (field, value) in [("Id", t.value(row, "proj_short_name")), ("Name", name)] {
            if !value.trim().is_empty() {
                e.children.push(Element::leaf(field, value.to_string()));
            }
        }
        list.children.push(e);
    }
    list
}

/// The `DisplayCurrency` element: the base currency (exchange rate 1, else
/// the first).
fn display_currency(l: &Lookup) -> Vec<Element> {
    let mut currencies = l.elements(&CURRENCY, &|_, _| true);
    let base = currencies
        .iter()
        .position(|c| c.value("ExchangeRate") == Some("1"))
        .unwrap_or(0);
    if base >= currencies.len() {
        return vec![];
    }
    let mut e = Element::new("DisplayCurrency");
    e.children.push(currencies.swap_remove(base));
    vec![e]
}

/// The XML text of an export document: every project in it (a project that
/// is a baseline of another project in it as `BaselineProject`) and the
/// dictionaries they use.
pub fn write(doc: &Document) -> Result<String, String> {
    let projects = doc.projects();
    if projects.is_empty() {
        return Err("There is no project to export".into());
    }
    let baselines: HashMap<i64, i64> = crate::baseline::baselines_in(doc).into_iter().collect();
    let l = Lookup::new(doc);
    let global = |t: &Table, r: &[String]| row_id(t, r, "proj_id").is_none();
    let global_code = |t: &Table, r: &[String]| l.code_project(t, r).is_none();
    let all = |_: &Table, _: &[String]| true;
    let plain: Vec<i64> = projects
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| !baselines.contains_key(p))
        .collect();
    let mut root = Element::new(ROOT);
    for name in ROOT_ORDER {
        let part = match *name {
            "ProjectList" => vec![project_list(&l, &plain)],
            "DisplayCurrency" => display_currency(&l),
            "Currency" => l.elements(&CURRENCY, &all),
            "UnitOfMeasure" => l.elements(&UNIT, &all),
            "CostAccount" => l.elements(&ACCOUNT, &all),
            "ExpenseCategory" => l.elements(&COST_TYPE, &all),
            "UDFType" => l.elements(&UDF_TYPE, &all),
            "NotebookTopic" => l.elements(&NOTEBOOK_TOPIC, &all),
            "OBS" => l.elements(&OBS, &all),
            "ActivityCodeType" => l.elements(&CODE_TYPE, &global),
            "ActivityCode" => l.elements(&CODE, &global_code),
            "Calendar" => l.elements(&CALENDAR, &global),
            "Role" => l.elements(&ROLE, &all),
            "RoleRate" => l.elements(&ROLE_RATE, &all),
            "ResourceCodeType" => l.elements(&RESOURCE_CODE_TYPE, &all),
            "ResourceCode" => l.elements(&RESOURCE_CODE, &all),
            "Resource" => l.elements(&RESOURCE, &all),
            "ResourceRate" => l.elements(&RESOURCE_RATE, &all),
            "ResourceRole" => l.elements(&RESOURCE_ROLE, &all),
            "FinancialPeriodTemplate" => l.elements(&FIN_TEMPLATE, &all),
            "Project" => plain
                .iter()
                .filter_map(|p| project_element(&l, *p, None))
                .collect(),
            "BaselineProject" => projects
                .iter()
                .filter_map(|(p, _)| project_element(&l, *p, Some(*baselines.get(p)?)))
                .collect(),
            _ => vec![],
        };
        root.children.extend(part);
    }
    Ok(write_document(&root))
}

/// Write an export document to `path` as XML.
pub fn write_file(doc: &Document, path: &Path) -> Result<(), String> {
    let text = write(doc)?;
    openxer_xer::write_file_atomic(path, text.as_bytes())
}

// ------------------------------------------------------------ reading

/// Whether file contents are XML (their first character is `<`) rather
/// than XER text.
pub fn is_xml(data: &[u8]) -> bool {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    data.iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b'<')
}

/// Read a schedule file: XML or XER, told apart by its contents.
pub fn read_file(path: &Path) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    read_bytes(&data)
}

const NOT_UTF8: &str = "The XML file is not UTF-8 text; save it as UTF-8 and try again";

/// Decode schedule file contents: XML or XER, told apart by the contents.
pub fn read_bytes(data: &[u8]) -> Result<Document, String> {
    if !is_xml(data) {
        return Document::from_bytes(data);
    }
    read(xml_text(data)?)
}

fn xml_text(data: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(data).map_err(|_| NOT_UTF8.to_string())
}

/// What the reader did with an XML file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReadReport {
    /// Element types read, with their counts (`Activity`, `Activity/Code`).
    pub elements: BTreeMap<String, usize>,
    /// Element types OpenXER has no place for, skipped, with their counts.
    pub unhandled: BTreeMap<String, usize>,
    /// Fields of read element types that were not read (values OpenXER
    /// works out itself, or has no column for), with their counts.
    pub fields_not_read: BTreeMap<String, usize>,
}

/// Read XML file contents with the reader's report.
pub fn read_bytes_with_report(data: &[u8]) -> Result<(Document, ReadReport), String> {
    read_with_report(xml_text(data)?)
}

struct Reader {
    doc: Document,
    /// UDF type id to its data type (XER code), read before the values.
    udf_types: HashMap<i64, String>,
    warnings: BTreeMap<String, usize>,
    report: ReadReport,
}

impl Reader {
    fn warn(&mut self, what: String) {
        *self.warnings.entry(what).or_default() += 1;
    }

    fn count(&mut self, what: &str) {
        *self.report.elements.entry(what.to_string()).or_default() += 1;
    }

    fn unhandled(&mut self, what: String) {
        *self.report.unhandled.entry(what).or_default() += 1;
    }

    /// Note what an element of `spec` holds that is not read: fields that
    /// are not in the spec, and nested elements that are not.
    fn note_unread(&mut self, spec: &Spec, e: &Element) {
        for c in &e.children {
            let name = c.name.as_str();
            if spec.nested.contains(&name) || spec.fields.iter().any(|f| f.xml == name) {
                continue;
            }
            if c.children.is_empty() {
                let key = format!("{}.{name}", spec.element);
                *self.report.fields_not_read.entry(key).or_default() += 1;
            } else {
                self.unhandled(format!("{}/{name}", spec.element));
            }
        }
    }

    /// Add the row an element holds to `spec`'s table; `defaults` fill in
    /// columns the element leaves out. Returns the row's id (for a type
    /// without an id, its row number from 1).
    fn add(&mut self, spec: &Spec, e: &Element, defaults: &[(&str, String)]) -> Option<i64> {
        self.count(spec.element);
        self.note_unread(spec, e);
        let mut values: Vec<(&str, String)> = vec![];
        for field in spec.fields {
            if field.xer.contains(['.', '=']) {
                continue;
            }
            if let Some(v) = e.value(field.xml).and_then(|v| from_xml(field.kind, v)) {
                values.push((field.xer, v));
            }
        }
        let id = match spec.key {
            Some(key) => match values.iter().find(|(c, _)| *c == key) {
                Some((_, v)) => num(v),
                None => {
                    self.warn(format!(
                        "{} elements without an ObjectId were skipped",
                        spec.element
                    ));
                    return None;
                }
            },
            None => self.doc.table(spec.table).map_or(0, |t| t.rows.len()) as i64 + 1,
        };
        for (k, v) in defaults {
            if !values.iter().any(|(c, _)| c == k) {
                values.push((*k, v.clone()));
            }
        }
        self.doc.table_mut(spec.table).add(&values);
        if spec.nested.contains(&"UDF") {
            // A UDF value carries the project of the row it belongs to.
            let pid = if spec.table == "PROJECT" {
                id.to_string()
            } else {
                values
                    .iter()
                    .find(|(k, _)| *k == "proj_id")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            self.add_udfs(spec, id, e, &pid);
        }
        Some(id)
    }

    fn add_udfs(&mut self, spec: &Spec, id: i64, e: &Element, pid: &str) {
        for u in e.all("UDF") {
            self.count(&format!("{}/UDF", spec.element));
            let Some(type_id) = u.value("TypeObjectId").and_then(|v| from_xml(Kind::Num, v)) else {
                continue;
            };
            let data_type = self
                .udf_types
                .get(&num(&type_id))
                .cloned()
                .unwrap_or_default();
            // The typed value element: `TextValue`, `CostValue` and the like.
            let Some(raw) = u
                .children
                .iter()
                .find(|c| c.name != "TypeObjectId" && c.name.ends_with("Value"))
                .map(|c| c.text.as_str())
            else {
                continue;
            };
            let (column, value) = match udf_column(&data_type) {
                Some("udf_date") => ("udf_date", from_xml(Kind::Date, raw)),
                Some("udf_number") => ("udf_number", from_xml(Kind::Num, raw)),
                _ => ("udf_text", from_xml(Kind::Text, raw)),
            };
            let Some(value) = value else {
                continue;
            };
            let mut row = vec![
                ("udf_type_id", type_id),
                ("fk_id", id.to_string()),
                (column, value),
            ];
            if !pid.is_empty() && spec.table != "RSRC" {
                row.push(("proj_id", pid.to_string()));
            }
            self.doc.table_mut("UDFVALUE").add(&row);
        }
    }

    fn calendar(&mut self, e: &Element, defaults: &[(&str, String)]) {
        let Some(id) = self.add(&CALENDAR, e, defaults) else {
            return;
        };
        let mut cal = CalendarData::default();
        let times = |day: &Element| -> Vec<Shift> {
            day.all("WorkTime")
                .filter_map(|w| {
                    let s = read_clock(w.value("Start")?, false)?;
                    let f = read_clock(w.value("Finish")?, true)?;
                    let f = if f <= s { f + 1440 } else { f };
                    Some((s, f))
                })
                .collect()
        };
        if let Some(week) = e.child("StandardWorkWeek") {
            for day in week.all("StandardWorkHours") {
                self.count("StandardWorkHours");
                let name = day.value("DayOfWeek").unwrap_or("").trim();
                if let Some(d) = DAY_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)) {
                    cal.week[d] = times(day);
                }
            }
        }
        if let Some(list) = e.child("HolidayOrExceptions") {
            for day in list.all("HolidayOrException") {
                self.count("HolidayOrException");
                if let Some(serial) = day.value("Date").and_then(|d| day_to_serial(d.trim())) {
                    cal.exceptions.insert(serial, times(day));
                }
            }
        }
        let t = self.doc.table_mut("CALENDAR");
        let row = t.rows.len() - 1;
        debug_assert_eq!(num(t.value(&t.rows[row], "clndr_id")), id);
        t.set(row, "clndr_data", cal.serialize());
    }

    fn resource(&mut self, e: &Element) {
        let Some(id) = self.add(&RESOURCE, e, &[]) else {
            return;
        };
        for code in e.all("Code") {
            self.count("Resource/Code");
            let get = |k: &str| code.value(k).and_then(|v| from_xml(Kind::Num, v));
            if let (Some(ty), Some(value)) = (get("TypeObjectId"), get("ValueObjectId")) {
                self.doc.table_mut("RSRCRCAT").add(&[
                    ("rsrc_id", id.to_string()),
                    ("rsrc_catg_type_id", ty),
                    ("rsrc_catg_id", value),
                ]);
            }
        }
    }

    fn project(&mut self, e: &Element, baseline: bool) {
        let mut defaults = vec![];
        if baseline {
            if let Some(v) = e
                .value("OriginalProjectObjectId")
                .and_then(|v| from_xml(Kind::Num, v))
            {
                defaults.push(("orig_proj_id", v));
            }
        }
        let Some(pid) = self.add(&PROJECT, e, &defaults) else {
            return;
        };
        let p = pid.to_string();
        let own = [("proj_id", p.clone())];
        for c in &e.children {
            match c.name.as_str() {
                "Calendar" => self.calendar(c, &own),
                "ActivityCodeType" => {
                    self.add(&CODE_TYPE, c, &own);
                }
                "ActivityCode" => {
                    self.add(&CODE, c, &[]);
                }
                _ => {}
            }
        }
        let first_wbs = self.doc.table("PROJWBS").map_or(0, |t| t.rows.len());
        let wbs_defaults = [("proj_id", p.clone()), ("proj_node_flag", "N".into())];
        for c in e.all("WBS") {
            self.add(&WBS, c, &wbs_defaults);
        }
        let wbs_ids: HashSet<i64> = self.doc.table("PROJWBS").map_or(HashSet::new(), |t| {
            t.rows[first_wbs..]
                .iter()
                .map(|r| num(t.value(r, "wbs_id")))
                .collect()
        });
        // The project's root WBS row: its id from the project element (it
        // names no WBS element), or a new one above every WBS id the file
        // holds. It is the project's own node, not a WBS element.
        let root_id = e
            .value("WBSObjectId")
            .and_then(|v| from_xml(Kind::Num, v))
            .map(|v| num(&v))
            .filter(|id| !wbs_ids.contains(id))
            .unwrap_or_else(|| {
                self.doc.table("PROJWBS").map_or(0, |t| {
                    t.rows
                        .iter()
                        .map(|r| num(t.value(r, "wbs_id")))
                        .max()
                        .unwrap_or(0)
                }) + 1
            });
        {
            let short = e.value("Id").unwrap_or("").to_string();
            let mut root = vec![
                ("wbs_id", root_id.to_string()),
                ("proj_id", p.clone()),
                ("seq_num", "0".into()),
                ("proj_node_flag", "Y".into()),
                ("wbs_short_name", short.clone()),
            ];
            for field in PROJECT.fields {
                let Some(col) = field.xer.strip_prefix("wbs.") else {
                    continue;
                };
                if matches!(col, "wbs_id" | "parent_wbs_id") {
                    continue;
                }
                if let Some(v) = e.value(field.xml).and_then(|v| from_xml(field.kind, v)) {
                    root.push((col, v));
                }
            }
            if !root.iter().any(|(c, _)| *c == "wbs_name") {
                root.push(("wbs_name", short));
            }
            if !root.iter().any(|(c, _)| *c == "status_code") {
                root.push(("status_code", "WS_Open".into()));
            }
            let t = self.doc.table_mut("PROJWBS");
            for row in first_wbs..t.rows.len() {
                let parent = t.value(&t.rows[row], "parent_wbs_id").trim().to_string();
                if parent.is_empty() || !wbs_ids.contains(&num(&parent)) {
                    t.set(row, "parent_wbs_id", root_id.to_string());
                }
            }
            t.add(&root);
        }
        let in_project = |id: &str| wbs_ids.contains(&num(id)) || num(id) == root_id;
        for c in e.all("ProjectNote") {
            if self.add(&PROJECT_NOTE, c, &own).is_some() {
                let t = self.doc.table_mut("WBSMEMO");
                let row = t.rows.len() - 1;
                let wbs = t.value(&t.rows[row], "wbs_id").trim().to_string();
                if wbs.is_empty() || !in_project(&wbs) {
                    t.set(row, "wbs_id", root_id.to_string());
                }
            }
        }
        let first_task = self.doc.table("TASK").map_or(0, |t| t.rows.len());
        for c in e.all("Activity") {
            let Some(task_id) = self.add(&ACTIVITY, c, &own) else {
                continue;
            };
            // The current dates: early dates until the project is scheduled.
            for (field, col) in [
                ("StartDate", "early_start_date"),
                ("FinishDate", "early_end_date"),
            ] {
                if let Some(v) = c.value(field).and_then(|v| from_xml(Kind::Date, v)) {
                    let t = self.doc.table_mut("TASK");
                    let row = t.rows.len() - 1;
                    t.set(row, col, v);
                }
            }
            for code in c.all("Code") {
                self.count("Activity/Code");
                let get = |k: &str| code.value(k).and_then(|v| from_xml(Kind::Num, v));
                if let (Some(ty), Some(value)) = (get("TypeObjectId"), get("ValueObjectId")) {
                    self.doc.table_mut("TASKACTV").add(&[
                        ("task_id", task_id.to_string()),
                        ("actv_code_type_id", ty),
                        ("actv_code_id", value),
                        ("proj_id", p.clone()),
                    ]);
                }
            }
        }
        {
            let t = self.doc.table_mut("TASK");
            for row in first_task..t.rows.len() {
                let wbs = t.value(&t.rows[row], "wbs_id").trim().to_string();
                if wbs.is_empty() || !in_project(&wbs) {
                    t.set(row, "wbs_id", root_id.to_string());
                }
            }
        }
        for c in &e.children {
            match c.name.as_str() {
                "ResourceAssignment" => {
                    self.add(&ASSIGNMENT, c, &own);
                }
                "ActivityExpense" => {
                    self.add(&EXPENSE, c, &own);
                }
                "ActivityNote" => {
                    self.add(&ACTIVITY_NOTE, c, &own);
                }
                "Relationship" => {
                    let link_defaults = [("proj_id", p.clone()), ("pred_proj_id", p.clone())];
                    self.add(&RELATIONSHIP, c, &link_defaults);
                }
                "ScheduleOptions" => self.schedule_options(c, &p),
                _ => {}
            }
        }
    }

    fn schedule_options(&mut self, e: &Element, pid: &str) {
        let Some(id) = self.add(&SCHEDULE_OPTIONS, e, &[("proj_id", pid.to_string())]) else {
            return;
        };
        let oos = e
            .value("OutOfSequenceScheduleType")
            .and_then(|v| from_xml(Kind::Choice(OUT_OF_SEQUENCE), v));
        let t = self.doc.table_mut("SCHEDOPTIONS");
        let row = t.rows.len() - 1;
        t.set(row, "schedoptions_id", id.to_string());
        let (retained, progress) = match oos.as_deref() {
            Some("retained") => ("Y", "N"),
            Some("override") => ("N", "Y"),
            Some("actual") => ("N", "N"),
            _ => return,
        };
        t.set(row, "sched_retained_logic", retained.into());
        t.set(row, "sched_progress_override", progress.into());
    }
}

fn read_clock(text: &str, finish: bool) -> Option<i64> {
    let text = text.trim();
    let text = text.split_once('T').map_or(text, |(_, t)| t);
    let mut parts = text.split(':');
    let h = parts.next()?.trim().parse::<i64>().ok()?;
    let m = parts.next().unwrap_or("0").trim().parse::<i64>().ok()?;
    if !(0..=24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    let mut minutes = h * 60 + m;
    // A period's finish is written one minute early: a finish a minute
    // short of a quarter hour (16:59, 12:29) ends on that quarter hour.
    if finish && (m + 1) % 15 == 0 {
        minutes += 1;
    }
    Some(minutes)
}

/// Read XML text into a document the import treats like one read from an
/// XER file.
pub fn read(text: &str) -> Result<Document, String> {
    read_with_report(text).map(|(doc, _)| doc)
}

/// Read XML text, with the report of what was read and what was skipped.
pub fn read_with_report(text: &str) -> Result<(Document, ReadReport), String> {
    let root = parse_tree(text)?;
    if root.name != ROOT {
        return Err(format!(
            "This XML file is not a project file: its root element is <{}>, not <{ROOT}>",
            root.name
        ));
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut r = Reader {
        doc: Document {
            header: format!("{HEADER_START}\t{today}\t{HEADER_END}"),
            tables: vec![],
            windows_encoding: text.is_ascii(),
            warnings: vec![],
            lone_quotes: BTreeMap::new(),
            id_floor: BTreeMap::new(),
        },
        udf_types: HashMap::new(),
        warnings: BTreeMap::new(),
        report: ReadReport::default(),
    };
    for u in root.all("UDFType") {
        if let (Some(id), Some(ty)) = (
            u.value("ObjectId").and_then(|v| from_xml(Kind::Num, v)),
            u.value("DataType")
                .and_then(|v| from_xml(Kind::Choice(UDF_DATA_TYPE), v)),
        ) {
            r.udf_types.insert(num(&id), ty);
        }
    }
    for c in &root.children {
        match c.name.as_str() {
            // An index of the projects in the file, and the currency the
            // file's amounts are shown in (also written as a Currency).
            "ProjectList" | "DisplayCurrency" => r.count(&c.name),
            "Currency" => {
                r.add(&CURRENCY, c, &[]);
            }
            "UDFType" => {
                let id = r.add(&UDF_TYPE, c, &[]);
                // The XER form also names each field; the name is made from its id.
                if let Some(id) = id {
                    let t = r.doc.table_mut("UDFTYPE");
                    let row = t.rows.len() - 1;
                    t.set(row, "udf_type_name", format!("user_field_{id}"));
                }
            }
            "NotebookTopic" => {
                r.add(&NOTEBOOK_TOPIC, c, &[]);
            }
            "OBS" => {
                r.add(&OBS, c, &[]);
            }
            "ActivityCodeType" => {
                r.add(&CODE_TYPE, c, &[]);
            }
            "ActivityCode" => {
                r.add(&CODE, c, &[]);
            }
            "Calendar" => r.calendar(c, &[]),
            "CostAccount" => {
                r.add(&ACCOUNT, c, &[]);
            }
            "ExpenseCategory" => {
                r.add(&COST_TYPE, c, &[]);
            }
            "UnitOfMeasure" => {
                r.add(&UNIT, c, &[]);
            }
            "Role" => {
                r.add(&ROLE, c, &[]);
            }
            "RoleRate" => {
                r.add(&ROLE_RATE, c, &[]);
            }
            "ResourceCodeType" => {
                r.add(&RESOURCE_CODE_TYPE, c, &[]);
            }
            "ResourceCode" => {
                r.add(&RESOURCE_CODE, c, &[]);
            }
            "Resource" => r.resource(c),
            "ResourceRate" => {
                r.add(&RESOURCE_RATE, c, &[]);
            }
            "ResourceRole" => {
                r.add(&RESOURCE_ROLE, c, &[]);
            }
            "FinancialPeriodTemplate" => {
                r.add(&FIN_TEMPLATE, c, &[]);
            }
            "Project" => r.project(c, false),
            "BaselineProject" => r.project(c, true),
            other => r.unhandled(other.to_string()),
        }
    }
    if r.doc.projects().is_empty() {
        return Err("The XML file holds no project".into());
    }
    let mut doc = r.doc;
    doc.warnings = r
        .warnings
        .into_iter()
        .map(|(w, n)| format!("{w} ({n})"))
        .collect();
    Ok((doc, r.report))
}

#[cfg(test)]
#[path = "project_xml_tests.rs"]
mod tests;
