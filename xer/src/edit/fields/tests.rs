use super::*;
use crate::{edit::activity::hours_text, starter, ProjectData};

/// The starter with an activity code type "Area" (North, with North-East
/// under it), a text user defined field "Crew" and a number one "Weight";
/// RB-1080 is in area North-East.
pub(crate) fn coded() -> ProjectData {
    let mut p = starter();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short, parent) in [(71, "N", ""), (72, "NE", "71"), (73, "S", "")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", format!("Area {short}")),
            ("parent_actv_code_id", parent.into()),
            ("seq_num", id.to_string()),
        ]);
    }
    doc.table_mut("TASKACTV").add(&[
        ("task_id", "1080".into()),
        ("actv_code_type_id", "7".into()),
        ("actv_code_id", "72".into()),
        ("proj_id", "1".into()),
    ]);
    for (id, label, kind) in [(5, "Crew", "FT_TEXT"), (6, "Weight", "FT_FLOAT")] {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", "TASK".into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    p
}

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap()
}

#[test]
fn the_catalog_holds_built_ins_codes_and_user_defined_fields() {
    let p = coded();
    let c = catalog(&p);
    let area = find(&c, "Area").expect("code field by title");
    assert_eq!(
        (area.id.as_str(), area.source, area.editor),
        ("code:Area", Source::Code(7), Editor::Choice)
    );
    let crew = find(&c, "udf:Crew").expect("user field by id");
    assert_eq!((crew.kind, crew.editor), (Kind::Text, Editor::Text));
    let weight = find(&c, "weight").expect("title ignores case");
    assert_eq!((weight.kind, weight.editor), (Kind::Number, Editor::Number));
    assert!(find(&c, "original_duration").is_some());
}

#[test]
fn plain_formats_show_and_read_hours_and_iso_dates() {
    let p = starter();
    let shown = display(&p, &Plain, Some(0), &Value::Duration(40 * 60));
    assert_eq!(shown, "40h");
    assert_eq!(
        parse_input(&p, &Plain, Some(0), Kind::Duration, "12.5").unwrap(),
        Value::Duration(750)
    );
    let date = parse_input(&p, &Plain, Some(0), Kind::Date, "2026-11-04 08:00").unwrap();
    assert_eq!(display(&p, &Plain, Some(0), &date), "2026-11-04 08:00");
    assert_eq!(
        input_text(&p, &Plain, 0, &Value::Percent(25.)),
        "25",
        "percents edit without the sign"
    );
}

#[test]
fn writes_go_through_the_shared_setters_and_locks() {
    let mut p = coded();
    let c = catalog(&p);
    let i = index_of(&p, "RB-1080");
    let od = find(&c, "original_duration").unwrap().clone();
    assert!(write(&mut p, &Plain, &od, i, "16", hours_text).unwrap());
    let task = p.model.activities[i].task_id;
    assert_eq!(p.details[&task].original_duration, 16 * 60);
    // Not started: Remaining Duration follows Original Duration.
    assert_eq!(p.model.activities[i].rem, 16 * 60);
    let crew = find(&c, "Crew").unwrap().clone();
    assert!(write(&mut p, &Plain, &crew, i, "Night", hours_text).unwrap());
    let ix = Index::for_project(&p);
    assert_eq!(
        value(&p, &ix, crew.source, crew.kind, i),
        Value::Text("Night".into())
    );
    // Read-only fields and locked fields refuse.
    let early = find(&c, "early_start").unwrap().clone();
    assert!(write(&mut p, &Plain, &early, i, "2026-11-04", hours_text).is_err());
    let actual = find(&c, "actual_duration").unwrap().clone();
    assert!(write(&mut p, &Plain, &actual, i, "8", hours_text).is_err());
}

#[test]
fn is_under_follows_the_wbs_and_code_hierarchies() {
    let p = coded();
    let ix = Index::for_project(&p);
    let i = index_of(&p, "RB-1080");
    let wbs = ix.wbs(&p).codes[&30].clone();
    assert!(is_under(&p, &ix, Source::Field(Field::Wbs), i, &wbs));
    assert!(is_under(&p, &ix, Source::Field(Field::Wbs), i, "rb.3"));
    assert!(!is_under(&p, &ix, Source::Field(Field::Wbs), i, "RB.1"));
    assert!(is_under(&p, &ix, Source::Code(7), i, "N"), "NE is under N");
    assert!(is_under(&p, &ix, Source::Code(7), i, "NE"));
    assert!(!is_under(&p, &ix, Source::Code(7), i, "S"));
    let other = index_of(&p, "RB-1000");
    assert!(!is_under(&p, &ix, Source::Code(7), other, "N"), "no code");
    assert!(under_applies(Source::Code(7)));
    assert!(under_applies(Source::Field(Field::Wbs)));
    assert!(!under_applies(Source::Field(Field::Calendar)));
    assert_eq!(Op::from_key("under"), Some(Op::IsUnder));
    assert!(!Op::ALL.contains(&Op::IsUnder), "only hierarchies offer it");
}
