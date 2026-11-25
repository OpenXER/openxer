//! Quote marks inside XER values: the file writes each one as two, the model
//! and every consumer see one, and export writes the doubled form back.
use openxer_xer::{escape_quotes, unescape_quotes, Document, ProjectData};

const CAL: &str = "(0||CalendarData()((0||DaysOfWeek()((0||1()())(0||2()((0||0(s|08:00|f|16:00)())))(0||3()((0||0(s|08:00|f|16:00)())))(0||4()((0||0(s|08:00|f|16:00)())))(0||5()((0||0(s|08:00|f|16:00)())))(0||6()((0||0(s|08:00|f|16:00)())))(0||7()())))(0||Exceptions()())))";

/// A one-project file whose activity and WBS names are the given raw (file
/// form) texts.
fn file(task_a: &str, task_b: &str, wbs: &str) -> String {
    let mut s = String::new();
    s.push_str("ERMHDR\t25.12\t2026-11-04\tProject\tadmin\tAdmin\tdbxDatabaseNoName\tProject Management\tUSD\r\n");
    s.push_str("%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tlast_recalc_date\tplan_start_date\r\n");
    s.push_str("%R\t1\tALPHA\t7\t2026-11-02 08:00\t2026-11-02 08:00\r\n");
    s.push_str("%T\tCALENDAR\r\n%F\tclndr_id\tdefault_flag\tclndr_name\tclndr_data\r\n");
    s.push_str(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"));
    s.push_str("%T\tPROJWBS\r\n%F\twbs_id\tproj_id\tseq_num\tproj_node_flag\twbs_short_name\twbs_name\tparent_wbs_id\r\n");
    s.push_str(&format!("%R\t10\t1\t0\tY\tALPHA\t{wbs}\t\r\n"));
    s.push_str("%T\tTASK\r\n%F\ttask_id\tproj_id\twbs_id\tclndr_id\ttask_type\tstatus_code\ttask_code\ttask_name\tremain_drtn_hr_cnt\ttarget_drtn_hr_cnt\tdriving_path_flag\r\n");
    s.push_str(&format!(
        "%R\t100\t1\t10\t7\tTT_Task\tTK_NotStart\tA100\t{task_a}\t16\t16\tN\r\n"
    ));
    s.push_str(&format!(
        "%R\t110\t1\t10\t7\tTT_Task\tTK_NotStart\tA110\t{task_b}\t8\t8\tN\r\n"
    ));
    s.push_str("%E\r\n");
    s
}

fn import(text: &str) -> ProjectData {
    ProjectData::from_document(Document::parse(text).unwrap(), 1, "q".into()).unwrap()
}

#[test]
fn decode_and_encode_cases() {
    let cases = [
        ("plain", "plain"),
        ("", ""),
        ("Area \"\"B\"\"", "Area \"B\""),
        ("Weld 8\"\"", "Weld 8\""),
        ("\"\"Rough in\"\" only", "\"Rough in\" only"),
        ("\"\"", "\""),
        ("\"\"\"\"", "\"\""),
        ("a\"\"\"\"b", "a\"\"b"),
        (
            "<font face=\"\"Arial\"\" size=\"\"2\"\">",
            "<font face=\"Arial\" size=\"2\">",
        ),
    ];
    for (raw, text) in cases {
        assert_eq!(unescape_quotes(raw), text, "decode {raw:?}");
        assert_eq!(escape_quotes(text), raw, "encode {text:?}");
        assert_eq!(unescape_quotes(&escape_quotes(text)), text);
    }
    // A lone quote mark is kept as it is when reading.
    assert_eq!(unescape_quotes("18\"-24\""), "18\"-24\"");
    assert_eq!(unescape_quotes("\"\"\""), "\"\"");
}

#[test]
fn every_column_and_position_is_decoded_then_written_back() {
    let text = file(
        "Area \"\"B\"\" tie-in",
        "\"\"Rough in\"\" and Weld 8\"\"",
        "Unit \"\"3\"\"",
    );
    let doc = Document::parse(&text).unwrap();
    let t = doc.table("TASK").unwrap();
    assert_eq!(t.value(&t.rows[0], "task_name"), "Area \"B\" tie-in");
    assert_eq!(
        t.value(&t.rows[1], "task_name"),
        "\"Rough in\" and Weld 8\""
    );
    let w = doc.table("PROJWBS").unwrap();
    assert_eq!(w.value(&w.rows[0], "wbs_name"), "Unit \"3\"");
    assert_eq!(doc.text(), text);
    assert!(doc.lone_quotes.is_empty());

    let p = import(&text);
    assert_eq!(p.details[&100].name, "Area \"B\" tie-in");
    assert_eq!(p.details[&110].name, "\"Rough in\" and Weld 8\"");
    assert_eq!(p.wbs[0].name, "Unit \"3\"");
    assert_eq!(p.updated_document().text(), text);
}

#[test]
fn an_edit_that_adds_a_quote_is_written_doubled() {
    let text = file("First", "Second", "Alpha");
    let mut p = import(&text);
    p.details.get_mut(&100).unwrap().name = "Set 6\" valve \"V-1\"".into();
    let mut doc = p.updated_document();
    doc.table_mut("PROJWBS")
        .set(0, "wbs_name", "Unit \"A\"".into());
    let out = doc.text();
    assert!(out.contains("\tSet 6\"\" valve \"\"V-1\"\"\t16\t"), "{out}");
    assert!(out.contains("\tUnit \"\"A\"\"\t"), "{out}");
    // And it reads back as the single-quote text.
    let back = import(&out);
    assert_eq!(back.details[&100].name, "Set 6\" valve \"V-1\"");
    assert_eq!(back.wbs[0].name, "Unit \"A\"");
    assert_eq!(back.updated_document().text(), out);
}

#[test]
fn a_value_cannot_break_the_row_with_quotes_around_tabs_and_line_breaks() {
    let text = file("First", "Second", "Alpha");
    let mut p = import(&text);
    p.details.get_mut(&100).unwrap().name = "a\"\tb\r\n\"c".into();
    let out = p.updated_document().text();
    assert!(out.contains("\ta\"\" b \"\"c\t16\t"), "{out}");
    let back = import(&out);
    assert_eq!(back.details[&100].name, "a\" b \"c");
}

#[test]
fn a_lone_quote_in_a_hand_made_file_round_trips_exactly() {
    let text = file(
        "Hydro-Excavate 18\"-24\" holes",
        "Second \"\"x\"\"",
        "Alpha",
    );
    let doc = Document::parse(&text).unwrap();
    let t = doc.table("TASK").unwrap();
    assert_eq!(
        t.value(&t.rows[0], "task_name"),
        "Hydro-Excavate 18\"-24\" holes"
    );
    assert_eq!(t.value(&t.rows[1], "task_name"), "Second \"x\"");
    assert_eq!(doc.text(), text);
    let mut p = import(&text);
    assert_eq!(p.updated_document().text(), text);
    // Editing the value moves it to the reference's form.
    p.details.get_mut(&100).unwrap().name = "Hydro-Excavate 18\"-24\" pole holes".into();
    let out = p.updated_document().text();
    assert!(out.contains("18\"\"-24\"\" pole holes"), "{out}");
    assert!(out.contains("Second \"\"x\"\""), "{out}");
}

#[test]
fn the_same_text_written_both_ways_is_written_doubled() {
    let text = file("a\"b", "a\"\"b", "Alpha");
    let doc = Document::parse(&text).unwrap();
    let out = doc.text();
    assert!(
        out.contains("\ta\"\"b\t16\t") && out.contains("\ta\"\"b\t8\t"),
        "{out}"
    );
}

#[test]
fn rows_copied_between_documents_keep_their_file_form() {
    let text = file("Hydro 18\"-24\"", "Second", "Alpha");
    let doc = Document::parse(&text).unwrap();
    let mut other = Document::parse(&file("One", "Two", "Beta")).unwrap();
    other.keep_lone_quotes_of(&doc);
    other.table_mut("TASK").rows[0][7] = "Hydro 18\"-24\"".into();
    assert!(other.text().contains("Hydro 18\"-24\"\t"));
}

/// Opt-in check on a real file with quoted names: set `OPENXER_QUOTES_XER` to
/// its path. It must read back to the same bytes and show single quote marks.
#[test]
fn a_real_file_with_quoted_names_reads_single_and_writes_identically() {
    let Some(path) = std::env::var_os("OPENXER_QUOTES_XER") else {
        println!("SKIPPED: set OPENXER_QUOTES_XER to an XER with quoted names");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let doc = Document::from_bytes(&bytes).unwrap();
    assert_eq!(doc.to_bytes().unwrap(), bytes);
    let mut counts = std::collections::BTreeMap::new();
    for t in &doc.tables {
        for row in &t.rows {
            for (i, v) in row.iter().enumerate() {
                if v.contains('"') {
                    *counts
                        .entry((t.name.clone(), t.fields[i].clone()))
                        .or_insert(0) += 1;
                    assert!(!v.contains("\"\""), "{} {v:?}", t.name);
                }
            }
        }
    }
    println!("{counts:?}");
    assert!(!counts.is_empty());
}
