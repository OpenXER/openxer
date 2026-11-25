//! A report as comma separated text: the title lines, then each table with
//! its caption and header row, blank lines between tables.
use super::doc::{Report, Tone};

/// A value a spreadsheet would read as a formula (starting with `=`, `+`, `-`,
/// `@`, tab or carriage return) gets a leading `'` so it opens as inert text.
/// Names and codes come from XER files, which are untrusted input. A plain
/// signed number (`-5`, `+12.5`, `-1,234.50`, `-25%`) is not a formula and
/// stays a number.
pub fn guard(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) && !plain_number(s) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

/// A sign, digits with `,` or `.` between them, and an optional `%`.
fn plain_number(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let body = body.strip_suffix('%').unwrap_or(body);
    body.chars().any(|c| c.is_ascii_digit())
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
        && !body.starts_with(',')
        && !body.ends_with([',', '.'])
}

/// One field with a delimiter and text qualifier. The formula guard applies
/// to the whole value first. A value that holds the delimiter, the
/// qualifier or a line break is always quoted, so it stays one field: with
/// the qualifier "None" a double quote is used for that. A quote inside the
/// text is doubled.
pub fn field_with(s: &str, delimiter: &str, qualifier: &str) -> String {
    let s = guard(s);
    let q = if qualifier.is_empty() {
        "\""
    } else {
        qualifier
    };
    let wrap = (!delimiter.is_empty() && s.contains(delimiter))
        || s.contains(q)
        || s.contains(['\n', '\r']);
    if wrap {
        format!("{q}{}{q}", s.replace(q, &format!("{q}{q}")))
    } else {
        s
    }
}

fn line(cells: impl IntoIterator<Item = String>, delimiter: &str, qualifier: &str) -> String {
    let mut s = cells
        .into_iter()
        .map(|c| field_with(&c, delimiter, qualifier))
        .collect::<Vec<_>>()
        .join(delimiter);
    s.push_str("\r\n");
    s
}

/// Delimited text as file bytes: UTF-8 with a byte order mark, so a
/// spreadsheet opening the file reads accented and other non-ASCII text
/// right.
pub fn file_bytes(text: String) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    out.extend_from_slice(b"\xEF\xBB\xBF");
    out.extend_from_slice(text.as_bytes());
    out
}

pub fn write(report: &Report) -> String {
    write_with(report, ",", "\"")
}

/// The report as delimited text with the chosen delimiter and qualifier.
pub fn write_with(report: &Report, delimiter: &str, qualifier: &str) -> String {
    let line = |cells: Vec<String>| line(cells, delimiter, qualifier);
    let mut s = line(vec![report.title.clone()]);
    for h in &report.header {
        s.push_str(&line(vec![h.clone()]));
    }
    for (section, table) in report.tables() {
        s.push_str("\r\n");
        let caption = if table.caption.is_empty() {
            section.heading.clone()
        } else {
            format!("{}: {}", section.heading, table.caption)
        };
        s.push_str(&line(vec![caption]));
        let mut header: Vec<String> = table.columns.iter().map(|c| c.title.clone()).collect();
        header.push("Mark".into());
        s.push_str(&line(header));
        for row in &table.rows {
            let mut cells: Vec<String> = row.cells.iter().map(|c| c.text().to_string()).collect();
            cells.resize(table.columns.len(), String::new());
            cells.push(
                match row.tone {
                    Tone::Normal => "",
                    Tone::Added => "Added",
                    Tone::Deleted => "Deleted",
                    Tone::Changed => "Changed",
                    Tone::Bad => "Failed",
                    Tone::Good => "Passed",
                    Tone::Band(_) => "Group",
                }
                .to_string(),
            );
            s.push_str(&line(cells));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delimiter_inside_a_value_never_splits_the_field() {
        for delimiter in [",", ";", "\t", "|", " "] {
            for qualifier in ["", "\"", "'"] {
                let f = field_with("a=b", delimiter, qualifier);
                assert_eq!(f, "a=b");
                let value = format!("x{delimiter}=SUM(A1)");
                let f = field_with(&value, delimiter, qualifier);
                let q = if qualifier.is_empty() {
                    "\""
                } else {
                    qualifier
                };
                assert_eq!(f, format!("{q}{value}{q}"), "{delimiter:?} {qualifier:?}");
            }
        }
    }

    #[test]
    fn formula_prefixes_are_guarded_before_quoting() {
        for start in ["=", "+", "-", "@", "\t", "\r"] {
            let value = format!("{start}1+2,3");
            let f = field_with(&value, ",", "");
            assert!(f.starts_with("\"'"), "{f:?}");
            let f = field_with(&format!("{start}A1"), ",", "'");
            assert!(
                f.starts_with("'") && f.contains(&format!("'{start}A1")),
                "{f:?}"
            );
        }
        assert_eq!(field_with("plain", ",", ""), "plain");
    }

    #[test]
    fn files_are_utf8_with_a_byte_order_mark() {
        let bytes = file_bytes("Caf\u{e9},\u{e9}t\u{e9}\r\n".into());
        assert_eq!(&bytes[..3], b"\xEF\xBB\xBF");
        assert_eq!(
            std::str::from_utf8(&bytes[3..]).unwrap(),
            "Caf\u{e9},\u{e9}t\u{e9}\r\n"
        );
        // A report written as CSV carries the mark.
        let report = Report {
            title: "Caf\u{e9}".into(),
            ..Default::default()
        };
        let path = std::env::temp_dir().join(format!("openxer-bom-{}.csv", std::process::id()));
        crate::reports::doc::write(&report, crate::reports::doc::Output::Csv, &path).unwrap();
        let written = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            written.starts_with(b"\xEF\xBB\xBFCaf\xC3\xA9"),
            "{written:?}"
        );
    }

    #[test]
    fn plain_numbers_are_not_guarded() {
        for n in ["-5", "+5", "-12.5", "-1,234.50", "-0.25%", "-5,5", "-.5"] {
            assert_eq!(guard(n), n, "{n}");
        }
        for t in [
            "-5d", "-", "--5", "-5-3", "-1+2", "=1", "+A1", "-SUM(A1)", "@x", "-5e3", "-,", "-.",
            "-%",
        ] {
            assert_eq!(guard(t), format!("'{t}"), "{t}");
        }
    }

    #[test]
    fn a_value_after_the_delimiter_cannot_start_a_formula() {
        // A record of three values with no qualifier: the second holds the
        // delimiter and a formula after it.
        let row = line(
            vec!["a".into(), "x,=HYPERLINK(\"u\")".into(), "-5".into()],
            ",",
            "",
        );
        assert_eq!(row, "a,\"x,=HYPERLINK(\"\"u\"\")\",-5\r\n");
        // Quotes and line breaks are doubled or quoted too.
        assert_eq!(field_with("say \"hi\"", ";", ""), "\"say \"\"hi\"\"\"");
        assert_eq!(field_with("a\nb", "\t", ""), "\"a\nb\"");
    }
}
