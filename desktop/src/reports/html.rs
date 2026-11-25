//! A report as one self-contained HTML page: plain markup with inline
//! styles, every tag closed, so it opens in any browser and prints cleanly.
use super::doc::{Report, Tone};

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' && c != '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "body{font-family:Arial,Helvetica,sans-serif;font-size:10pt;color:#000;margin:24px;}\
h1{font-size:16pt;margin:0 0 4px 0;}\
h2{font-size:12pt;margin:22px 0 6px 0;padding:3px 6px;background:#D0D0D0;border-left:4px solid #1F3A5F;}\
p.header{margin:0;color:#333;}\
p.caption{margin:8px 0 4px 0;font-weight:bold;}\
table{border-collapse:collapse;margin:0 0 10px 0;}\
th{background:#F0F0F0;border:1px solid #808080;padding:2px 6px;text-align:left;font-weight:bold;}\
td{border:1px solid #C0C0C0;padding:2px 6px;vertical-align:top;}\
td.r,th.r{text-align:right;}\
tr.added td{background:#DFF5DF;}\
tr.deleted td{background:#F9DADA;}\
tr.changed td{background:#FFF7CC;}\
tr.bad td{background:#F9DADA;}\
tr.good td{background:#DFF5DF;}\
tr.band0 td{background:#1F3A5F;color:#FFFFFF;font-weight:bold;}\
tr.band1 td{background:#D6E4F2;color:#1F3A5F;font-weight:bold;}\
tr.band2 td{background:#35608F;color:#FFFFFF;font-weight:bold;}\
tr.band3 td{background:#BCD2E8;color:#1F3A5F;font-weight:bold;}\
p.footer{margin-top:24px;color:#555;font-size:8pt;}\
@media print{h2{break-after:avoid;}tr{break-inside:avoid;}}";

fn class(tone: Tone) -> String {
    match tone {
        Tone::Normal => String::new(),
        Tone::Added => " class=\"added\"".into(),
        Tone::Deleted => " class=\"deleted\"".into(),
        Tone::Changed => " class=\"changed\"".into(),
        Tone::Bad => " class=\"bad\"".into(),
        Tone::Good => " class=\"good\"".into(),
        Tone::Band(level) => format!(" class=\"band{}\"", level.min(3)),
    }
}

pub fn write(report: &Report) -> String {
    let mut s = String::new();
    s.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n");
    s.push_str(&format!("<title>{}</title>\n", escape(&report.title)));
    s.push_str(&format!("<style>{STYLE}</style>\n</head>\n<body>\n"));
    s.push_str(&format!("<h1>{}</h1>\n", escape(&report.title)));
    for line in &report.header {
        s.push_str(&format!("<p class=\"header\">{}</p>\n", escape(line)));
    }
    for section in &report.sections {
        s.push_str(&format!("<h2>{}</h2>\n", escape(&section.heading)));
        for p in &section.paragraphs {
            s.push_str(&format!("<p>{}</p>\n", escape(p)));
        }
        for table in &section.tables {
            if !table.caption.is_empty() {
                s.push_str(&format!(
                    "<p class=\"caption\">{}</p>\n",
                    escape(&table.caption)
                ));
            }
            s.push_str("<table>\n<thead>\n<tr>");
            for c in &table.columns {
                let cls = if c.right { " class=\"r\"" } else { "" };
                s.push_str(&format!("<th{cls}>{}</th>", escape(&c.title)));
            }
            s.push_str("</tr>\n</thead>\n<tbody>\n");
            for row in &table.rows {
                s.push_str(&format!("<tr{}>", class(row.tone)));
                if matches!(row.tone, Tone::Band(_))
                    && row.cells.iter().skip(1).all(|c| c.text().is_empty())
                {
                    s.push_str(&format!(
                        "<td colspan=\"{}\">{}</td>",
                        table.columns.len().max(1),
                        escape(row.cells.first().map_or("", |c| c.text()))
                    ));
                } else {
                    for (i, cell) in row.cells.iter().enumerate() {
                        let right = table.columns.get(i).is_some_and(|c| c.right);
                        let cls = if right { " class=\"r\"" } else { "" };
                        s.push_str(&format!("<td{cls}>{}</td>", escape(cell.text())));
                    }
                }
                s.push_str("</tr>\n");
            }
            s.push_str("</tbody>\n</table>\n");
        }
    }
    if !report.footer.is_empty() {
        s.push_str(&format!(
            "<p class=\"footer\">{}</p>\n",
            escape(&report.footer)
        ));
    }
    s.push_str("</body>\n</html>\n");
    s
}

/// Check that `html` is well formed: every tag closed in order, void tags
/// self-closed, attribute quotes balanced and no stray `<` or `&`. Used by
/// the tests of every report that writes HTML.
#[cfg(test)]
pub fn well_formed(html: &str) -> Result<(), String> {
    let body = html
        .strip_prefix("<!DOCTYPE html>")
        .ok_or("missing doctype")?;
    let mut stack: Vec<String> = vec![];
    let mut rest = body;
    while let Some(start) = rest.find(['<', '&']) {
        let text = &rest[..start];
        if text.contains('>') {
            return Err(format!("stray > in {text:?}"));
        }
        rest = &rest[start..];
        if rest.starts_with('&') {
            let end = rest.find(';').ok_or("unterminated entity")?;
            let name = &rest[1..end];
            if !(["amp", "lt", "gt", "quot", "#39"].contains(&name)) {
                return Err(format!("unknown entity {name}"));
            }
            rest = &rest[end + 1..];
            continue;
        }
        let end = rest.find('>').ok_or("unterminated tag")?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.matches('"').count() % 2 != 0 {
            return Err(format!("unbalanced quotes in <{tag}>"));
        }
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name => {}
                other => return Err(format!("</{name}> closes {other:?}")),
            }
        } else if tag.ends_with('/') {
            // Void element.
        } else {
            let name = tag.split_whitespace().next().unwrap_or("").to_string();
            if name.is_empty() {
                return Err("empty tag".into());
            }
            if name == "style" {
                let close = rest.find("</style>").ok_or("unclosed style")?;
                rest = &rest[close + "</style>".len()..];
                continue;
            }
            stack.push(name);
        }
    }
    if !stack.is_empty() {
        return Err(format!("unclosed tags {stack:?}"));
    }
    Ok(())
}
