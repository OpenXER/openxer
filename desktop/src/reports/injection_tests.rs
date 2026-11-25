//! Activity names and codes come from XER files, which are untrusted: in
//! every report format they must stay inert text (no spreadsheet formula,
//! no markup).
use super::doc::{self, Output};
use super::html;
use super::test_support::{scheduled_starter, temp};
use crate::compare::diff::{compare, Options};
use crate::compare::report;
use openxer_interchange::xlsx_import::{read_workbook, Cell};

const HOSTILE: [&str; 3] = [
    "=HYPERLINK(\"http://x\",\"y\")",
    "+1+1",
    "<script>alert(1)</script>",
];

#[test]
fn hostile_names_export_as_inert_text() {
    let old = scheduled_starter();
    let mut new = old.clone();
    for (n, name) in HOSTILE.iter().enumerate() {
        let task_id = new.model.activities[n].task_id;
        new.details.get_mut(&task_id).unwrap().name = name.to_string();
    }
    new.model.activities[3].task_code = "@SUM(A1)".into();
    let r = report::report(&compare(&old, "a", &new, "b", &Options::default()));
    let dir = temp("injection");
    // CSV: a leading quote, then normal quoting.
    let path = dir.join("r.csv");
    doc::write(&r, Output::Csv, &path).unwrap();
    let csv = std::fs::read_to_string(&path).unwrap();
    assert!(
        csv.contains("\"'=HYPERLINK(\"\"http://x\"\",\"\"y\"\")\""),
        "{csv}"
    );
    assert!(csv.contains(",'+1+1,"), "{csv}");
    assert!(csv.contains("'@SUM(A1)"), "{csv}");
    for line in csv.lines() {
        for field in line.split(',') {
            // A plain signed number is a number, not a formula.
            assert!(
                !field.starts_with(['=', '+', '@', '-']) || field.parse::<f64>().is_ok(),
                "unguarded field {field:?} in {line:?}"
            );
        }
    }
    // Excel: string cells holding the exact text.
    let path = dir.join("r.xlsx");
    doc::write(&r, Output::Excel, &path).unwrap();
    let wb = read_workbook(&path).unwrap();
    let texts: Vec<String> = wb
        .sheets
        .iter()
        .flat_map(|s| s.rows.iter().flatten())
        .filter_map(|c| match c {
            Cell::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    for name in HOSTILE.iter().chain(["@SUM(A1)"].iter()) {
        assert!(texts.iter().any(|t| t == name), "{name} as text");
    }
    // HTML: escaped, still well formed.
    let page = html::write(&r);
    html::well_formed(&page).unwrap();
    assert!(!page.contains("<script>"));
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(page.contains("=HYPERLINK(&quot;http://x&quot;,&quot;y&quot;)"));
}

/// Delimited text with any delimiter and qualifier, including "None": a name
/// that holds the delimiter stays one quoted field, and a name that starts a
/// formula is guarded before it is quoted.
#[test]
fn delimited_text_keeps_hostile_names_in_one_inert_field() {
    let mut p = scheduled_starter();
    for (n, name) in ["ok,x\"y", "=SUM(A1),@B", "+1;-2"].iter().enumerate() {
        let task_id = p.model.activities[n].task_id;
        p.details.get_mut(&task_id).unwrap().name = name.to_string();
    }
    let def = super::wizard::Definition::new(super::wizard::Subject::Activities);
    let ix = crate::grid::columns::Index::new((false, 0.));
    let r = super::wizard::build(
        &def,
        &p,
        &super::wizard::Context {
            ix: &ix,
            users: &[],
        },
    );
    for (delimiter, qualifier) in [(",", ""), (";", ""), ("\t", ""), (",", "\""), (";", "'")] {
        let text = super::csv::write_with(&r, delimiter, qualifier);
        let field = |v: &str| super::csv::field_with(v, delimiter, qualifier);
        let q = if qualifier.is_empty() {
            "\""
        } else {
            qualifier
        };
        // Holding the delimiter or a quote: one quoted field.
        if delimiter == "," {
            assert!(
                text.contains(&format!("{q}ok,x{q}{q}y{q}")),
                "{delimiter:?} {qualifier:?}: {text}"
            );
            assert!(
                text.contains(&format!("{q}'=SUM(A1),@B{q}")),
                "{delimiter:?} {qualifier:?}: {text}"
            );
        }
        // The same value, built by the field rule, is in the text, guarded.
        assert!(text.contains(&field("+1;-2")), "{text}");
        assert!(field("+1;-2").contains("'+1"), "{text}");
        // A value that starts a formula never comes through bare.
        assert!(!text.contains(&format!("{delimiter}=SUM")), "{text}");
    }
}
