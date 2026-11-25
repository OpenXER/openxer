//! Encoding, malformed input, export validity and a seeded mutation fuzz test.
use openxer_xer::{starter, Document, ProjectData};

const CAL: &str = "(0||CalendarData()((0||DaysOfWeek()((0||1()())(0||2()((0||0(s|08:00|f|16:00)())))(0||3()((0||0(s|08:00|f|16:00)())))(0||4()((0||0(s|08:00|f|16:00)())))(0||5()((0||0(s|08:00|f|16:00)())))(0||6()((0||0(s|08:00|f|16:00)())))(0||7()())))(0||Exceptions()())))";

/// A small two-project file in the usual XER layout. `extra_tasks` and `extra_preds` are
/// appended as raw `%R` lines.
fn mini(extra_tasks: &str, extra_preds: &str) -> String {
    let mut s = String::new();
    s.push_str("ERMHDR\t25.12\t2026-11-04\tProject\tadmin\tAdmin\tdbxDatabaseNoName\tProject Management\tUSD\r\n");
    s.push_str("%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tlast_recalc_date\tplan_start_date\r\n");
    s.push_str("%R\t1\tALPHA\t7\t2026-11-02 08:00\t2026-11-02 08:00\r\n");
    s.push_str("%R\t2\tBETA\t7\t2026-11-02 08:00\t2026-11-02 08:00\r\n");
    s.push_str("%T\tCALENDAR\r\n%F\tclndr_id\tdefault_flag\tclndr_name\tclndr_data\r\n");
    s.push_str(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"));
    s.push_str("%T\tPROJWBS\r\n%F\twbs_id\tproj_id\tseq_num\tproj_node_flag\twbs_short_name\twbs_name\tparent_wbs_id\r\n");
    s.push_str("%R\t10\t1\t0\tY\tALPHA\tAlpha\t\r\n%R\t20\t2\t0\tY\tBETA\tBeta\t\r\n");
    s.push_str("%T\tTASK\r\n%F\ttask_id\tproj_id\twbs_id\tclndr_id\ttask_type\tstatus_code\ttask_code\ttask_name\tremain_drtn_hr_cnt\ttarget_drtn_hr_cnt\tdriving_path_flag\r\n");
    s.push_str("%R\t100\t1\t10\t7\tTT_Task\tTK_NotStart\tA100\tFirst\t16\t16\tN\r\n");
    s.push_str("%R\t110\t1\t10\t7\tTT_Task\tTK_NotStart\tA110\tSecond\t8\t8\tN\r\n");
    s.push_str("%R\t200\t2\t20\t7\tTT_Task\tTK_NotStart\tB200\tOther project\t8\t8\tN\r\n");
    s.push_str(extra_tasks);
    s.push_str("%T\tTASKPRED\r\n%F\ttask_pred_id\ttask_id\tpred_task_id\tproj_id\tpred_proj_id\tpred_type\tlag_hr_cnt\tcomments\r\n");
    s.push_str("%R\t1\t110\t100\t1\t1\tPR_FS\t0\tkeep \"quoted\" <b>note</b>\r\n");
    s.push_str("%R\t2\t200\t110\t2\t1\tPR_FS\t0\t\r\n");
    s.push_str(extra_preds);
    s.push_str("%E\r\n");
    s
}

fn import(text: &str, pid: i64) -> Result<ProjectData, String> {
    ProjectData::from_document(Document::parse(text)?, pid, "test".into())
}

// ---------------------------------------------------------------- encoding

#[test]
fn ascii_files_are_read_as_windows_1252() {
    let doc = Document::from_bytes(mini("", "").as_bytes()).unwrap();
    assert!(
        doc.windows_encoding,
        "the reference writes Windows-1252; ASCII is a subset"
    );
    assert!(starter().document.windows_encoding);
}

#[test]
fn windows_1252_bytes_round_trip_exactly() {
    // Every byte 0x80..=0xFF, including the five that 1252 leaves undefined.
    let mut bytes = mini("", "").into_bytes();
    let at = bytes.windows(5).position(|w| w == b"First").unwrap();
    let high: Vec<u8> = (0x80..=0xFFu8).filter(|b| *b != b'\t').collect();
    bytes.splice(at..at + 5, high.iter().copied());
    let doc = Document::from_bytes(&bytes).unwrap();
    assert!(doc.windows_encoding);
    let t = doc.table("TASK").unwrap();
    assert!(t.value(&t.rows[0], "task_name").contains('\u{e9}'));
    assert_eq!(doc.to_bytes().unwrap(), bytes);
    let p = ProjectData::from_document(doc, 1, "1252".into()).unwrap();
    assert_eq!(p.updated_document().to_bytes().unwrap(), bytes);
}

#[test]
fn utf8_files_stay_utf8() {
    let text = mini("", "").replace("First", "Caf\u{e9} \u{2603} \u{4e2d}");
    let doc = Document::from_bytes(text.as_bytes()).unwrap();
    assert!(!doc.windows_encoding);
    let p = ProjectData::from_document(doc, 1, "utf8".into()).unwrap();
    assert_eq!(p.details[&100].name, "Caf\u{e9} \u{2603} \u{4e2d}");
    assert_eq!(p.updated_document().to_bytes().unwrap(), text.as_bytes());
}

#[test]
fn unrepresentable_characters_are_a_clear_error() {
    let doc = Document::from_bytes(mini("", "").as_bytes()).unwrap();
    let mut p = ProjectData::from_document(doc, 1, "1252".into()).unwrap();
    assert!(p.document.windows_encoding);
    p.details.get_mut(&110).unwrap().name = "Pump \u{2603} skid".into();
    let error = p.updated_document().to_bytes().unwrap_err();
    assert!(error.contains("U+2603"), "{error}");
    assert!(error.contains("TASK.task_name"), "{error}");
    // Latin-1 range characters are representable and written as single bytes.
    p.details.get_mut(&110).unwrap().name = "Caf\u{e9}".into();
    let bytes = p.updated_document().to_bytes().unwrap();
    assert!(bytes.windows(4).any(|w| w == b"Caf\xe9"));
}

#[test]
fn lf_line_endings_bom_and_trailing_tabs() {
    let crlf = mini("", "");
    let lf = crlf.replace("\r\n", "\n");
    let a = Document::parse(&crlf).unwrap();
    let b = Document::parse(&format!("\u{feff}{lf}")).unwrap();
    assert_eq!(a.text(), b.text(), "LF input exports as CRLF, BOM dropped");
    assert!(a.warnings.is_empty() && b.warnings.is_empty());
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(crlf.as_bytes());
    let c = Document::from_bytes(&bom).unwrap();
    assert_eq!(c.text(), crlf);
    // Trailing empty values survive.
    let t = a.table("TASKPRED").unwrap();
    assert_eq!(t.value(&t.rows[1], "comments"), "");
    assert!(a.text().contains("PR_FS\t0\t\r\n"));
}

#[test]
fn memo_text_with_quotes_html_del_cr_and_line_breaks_survives() {
    let memo = "<html><body><p class=\"x\">a \"quote\" &amp; tab-free\u{7f}\u{7f}line\rbare CR</p>\nsecond line</body></html>";
    let extra = format!("%T\tTASKMEMO\r\n%F\tmemo_id\ttask_id\tmemo_type_id\tproj_id\ttask_memo\r\n%R\t1\t100\t1\t1\t{memo}\r\n");
    let text = mini("", "").replace("%E\r\n", &format!("{extra}%E\r\n"));
    let p = import(&text, 1).unwrap();
    let t = p.document.table("TASKMEMO").unwrap();
    assert_eq!(t.value(&t.rows[0], "task_memo"), memo);
    // The lone CR before an ordinary character is kept, as the reference writes it; the
    // embedded line feed cannot be written inside a row and becomes a space.
    assert_eq!(
        p.updated_document().text(),
        text.replace("</p>\nsecond", "</p> second")
    );
}

#[test]
fn memo_blank_lines_and_percent_lines_continue_the_row() {
    let memo_file = |memo: &str| {
        let extra = format!("%T\tTASKMEMO\r\n%F\tmemo_id\ttask_id\tmemo_type_id\tproj_id\ttask_memo\r\n%R\t1\t100\t1\t1\t{memo}\r\n");
        mini("", "").replace("%E\r\n", &format!("{extra}%E\r\n"))
    };
    for memo in [
        "line1\r\n\r\nline3",
        "line1\r\n\r\n\r\nline4",
        "done\r\n% complete is 50",
        "first\r\n%Rough estimate",
        "last line\r\n%",
    ] {
        let text = memo_file(memo);
        let p = import(&text, 1).unwrap_or_else(|e| panic!("{memo:?}: {e}"));
        let t = p.document.table("TASKMEMO").unwrap();
        assert_eq!(t.rows.len(), 1, "{memo:?}");
        assert_eq!(
            t.value(&t.rows[0], "task_memo").replace('\r', ""),
            memo.replace('\r', ""),
            "{memo:?}"
        );
        // Every table and row survives a save and reopen; the line breaks
        // are written as spaces, as for a single break.
        let again = Document::parse(&p.updated_document().text()).unwrap();
        for table in &p.document.tables {
            assert_eq!(
                again.table(&table.name).unwrap().rows.len(),
                table.rows.len(),
                "{memo:?} {}",
                table.name
            );
        }
    }
    // A blank line between records is still only a blank line.
    let text = mini("", "").replace("%T\tTASKPRED", "\r\n%T\tTASKPRED");
    let p = import(&text, 1).unwrap();
    assert_eq!(p.details[&110].name, "Second");
    // Empty values past the last column are dropped with a warning; a real
    // value there is still an error.
    let text = mini("", "").replace("\tSecond\t8\t8\tN\r\n", "\tSecond\t8\t8\tN\t\t\r\n");
    let doc = Document::parse(&text).unwrap();
    assert!(
        doc.warnings.iter().any(|w| w.contains("TASK")),
        "{:?}",
        doc.warnings
    );
    let t = doc.table("TASK").unwrap();
    assert!(t.rows.iter().all(|r| r.len() == t.fields.len()));
    let text = mini("", "").replace("\tSecond\t8\t8\tN\r\n", "\tSecond\t8\t8\tN\textra\r\n");
    assert!(Document::parse(&text).unwrap_err().contains("only"));
}

// --------------------------------------------------------- row injection

/// Every line of the text starts a record, and the reparsed document has the
/// same tables and row counts.
fn assert_structure_intact(doc: &Document) {
    let text = doc.text();
    let body: Vec<&str> = text.split("\r\n").collect();
    for line in &body[1..] {
        assert!(
            line.is_empty()
                || ["%T\t", "%F\t", "%R\t", "%E"]
                    .iter()
                    .any(|t| line.starts_with(t)),
            "stray line {line:?}"
        );
        assert!(!line.contains('\n'), "line feed inside {line:?}");
        // A reader that also splits on a lone CR must not find a record either.
        for part in line.split('\r').skip(1) {
            assert!(
                !part.trim_start().starts_with('%') && !part.is_empty(),
                "CR exposes {part:?}"
            );
        }
    }
    let back = Document::parse(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let shape = |d: &Document| {
        d.tables
            .iter()
            .map(|t| (t.fields.len(), t.rows.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&back), shape(doc));
    for (a, b) in doc.tables.iter().zip(&back.tables) {
        for (ra, rb) in a.rows.iter().zip(&b.rows) {
            assert_eq!(ra.len(), rb.len());
        }
    }
}

const ATTACKS: &[&str] = &[
    "a\tb",
    "x\r\n%R\t999\t1\tinjected",
    "x\n%T\tPROJECT\n%F\tproj_id\n%R\t666",
    "x\r%E",
    "x\r\t%R",
    "x\r \t%R",
    "trailing CR\r",
    "\r%F\tevil",
    "%R\tleading percent is harmless mid-row",
    "\n\n\r\r\t\t",
];

#[test]
fn values_cannot_inject_rows_or_tables() {
    for attack in ATTACKS {
        let mut p = starter();
        p.details.get_mut(&1000).unwrap().name = attack.to_string();
        p.model.activities[1].task_code = attack.to_string();
        p.model.project.name = attack.to_string();
        let mut doc = p.updated_document();
        doc.table_mut("TASKMEMO").add(&[
            ("memo_id", "1".into()),
            ("task_id", "1000".into()),
            ("task_memo", attack.to_string()),
        ]);
        assert_structure_intact(&doc);
        let back = ProjectData::from_document(Document::parse(&doc.text()).unwrap(), 1, "x".into())
            .unwrap();
        assert_eq!(back.model.activities.len(), p.model.activities.len());
        assert_eq!(back.document.projects().len(), 1);
        let name = &back.details[&1000].name;
        assert!(!name.contains(['\t', '\n']), "{name:?}");
    }
}

#[test]
fn header_table_and_column_names_are_sanitized() {
    let mut doc = Document::parse(&mini("", "")).unwrap();
    doc.header.push_str("\r\n%R\tx");
    doc.table_mut("TASK").fields[1] = "proj\tid\r\n%E".into();
    doc.tables[0].name = "PROJECT\r\n%T\tEVIL".into();
    assert_structure_intact(&doc);
    assert!(
        doc.text().starts_with("ERMHDR\t25.12\t"),
        "header tabs are kept"
    );
}

#[test]
fn random_control_characters_never_break_structure() {
    let mut rng = Rng(0x1234_5678);
    let alphabet = [
        '\t', '\r', '\n', '%', 'R', 'T', 'E', 'F', ' ', 'a', '\u{7f}',
    ];
    for _ in 0..2000 {
        let value: String = (0..rng.below(12))
            .map(|_| alphabet[rng.below(alphabet.len())])
            .collect();
        let mut doc = Document::parse(&mini("", "")).unwrap();
        let t = doc.table_mut("TASK");
        let last = t.fields.len() - 1;
        t.rows[0][last] = value.clone();
        t.rows[1][0] = value;
        assert_structure_intact(&doc);
    }
}

#[test]
fn reference_lone_cr_in_memo_round_trips_byte_identically() {
    // As in a corpus memo: a CR followed by an ordinary character.
    let memo = "pattern (?=\r?\u{7f}\u{7f}) here";
    let text = mini("", "").replace("keep \"quoted\" <b>note</b>", memo);
    let p = import(&text, 1).unwrap();
    assert_eq!(p.updated_document().text(), text);
}

#[test]
fn very_long_lines_survive() {
    let long = "x".repeat(2_000_000);
    let text = mini("", "").replace("First", &long);
    let p = import(&text, 1).unwrap();
    assert_eq!(p.details[&100].name.len(), long.len());
    assert_eq!(p.updated_document().text(), text);
}

#[test]
fn assignments_load_in_file_order() {
    let rsrc = "%T\tTASKRSRC\r\n%F\ttaskrsrc_id\ttask_id\tproj_id\trsrc_id\tremain_qty\tremain_qty_per_hr\trelag_drtn_hr_cnt\trollup_dates_flag\r\n%R\t1\t110\t1\t5\t16\t2\t1\tY\r\n%R\t2\t110\t1\t6\t4\t1\t0\tN\r\n%R\t3\t200\t2\t5\t8\t1\t0\tY\r\n";
    let text = mini("", "").replace("%E\r\n", &format!("{rsrc}%E\r\n"));
    let p = import(&text, 1).unwrap();
    let a = &p.model.activities[1].assignments;
    assert_eq!(a.len(), 2);
    assert_eq!(
        (a[0].rsrc_id, a[0].rem, a[0].relag, a[0].rollup),
        (5, 480, 60, true)
    );
    assert_eq!((a[1].rsrc_id, a[1].rem, a[1].rollup), (6, 240, false));
    assert!(p.model.activities[0].assignments.is_empty());
}

// ------------------------------------------------------- malformed input

#[test]
fn structural_errors_are_reported_with_line_numbers() {
    let cases = [
        ("", "Empty XER file"),
        ("hello\r\n", "ERMHDR header is missing"),
        ("ERMHDR\t25.12\r\n%F\ta\r\n", "%F comes before any %T"),
        ("ERMHDR\t25.12\r\n%R\t1\r\n", "%R comes before any %T"),
        ("ERMHDR\t25.12\r\n%T\tTASK\r\n%R\t1\r\n", "before its %F"),
        ("ERMHDR\t25.12\r\n%T\r\n", "has no table name"),
        (
            "ERMHDR\t25.12\r\n%T\tA\r\n%F\ta\r\n%R\t1\t2\r\n",
            "2 values but only 1 columns",
        ),
        ("ERMHDR\t25.12\r\n%T\tA\r\n%F\ta\r\n%F\tb\r\n", "second %F"),
        ("ERMHDR\t25.12\r\n%X\tA\r\n", "unknown XER record"),
        ("ERMHDR\t25.12\r\nstray\r\n", "unexpected text"),
        (
            "ERMHDR\t25.12\r\n%T\tA\r\n%F\ta\r\n%T\tA\r\n%F\tb\r\n",
            "different columns",
        ),
    ];
    for (text, expected) in cases {
        let error = Document::parse(text).unwrap_err();
        assert!(error.contains(expected), "{text:?}: {error}");
    }
    let error = Document::parse("ERMHDR\t25.12\r\n%T\tA\r\n%F\ta\r\n%R\t1\t2\r\n").unwrap_err();
    assert!(error.starts_with("Line 4:"), "{error}");
}

#[test]
fn tolerated_problems_become_warnings() {
    let truncated = mini("", "").replace("%E\r\n", "");
    let p = import(&truncated, 1).unwrap();
    assert!(
        p.warnings.iter().any(|w| w.contains("no %E")),
        "{:?}",
        p.warnings
    );

    let short = mini("%R\t120\t1\t10\t7\tTT_Task\r\n", "");
    let p = import(&short, 1).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("fewer values")));
    assert_eq!(p.model.activities.len(), 3);

    let after = format!("{}junk after end\r\n", mini("", ""));
    let p = import(&after, 1).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("after the %E")));

    let twice = mini("", "").replace(
        "%E\r\n",
        "%T\tTASK\r\n%F\ttask_id\tproj_id\twbs_id\tclndr_id\ttask_type\tstatus_code\ttask_code\ttask_name\tremain_drtn_hr_cnt\ttarget_drtn_hr_cnt\tdriving_path_flag\r\n%R\t130\t1\t10\t7\tTT_Task\tTK_NotStart\tA130\tLate table\t8\t8\tN\r\n%E\r\n",
    );
    let p = import(&twice, 1).unwrap();
    assert!(p
        .warnings
        .iter()
        .any(|w| w.contains("appears more than once")));
    assert!(p.details.contains_key(&130));
}

/// A negative remaining duration is refused by the engine's validation, so
/// the importer reads it as zero with a warning and the file still opens and
/// schedules.
#[test]
fn a_negative_remaining_duration_is_read_as_zero_with_a_warning() {
    let text = mini(
        "%R\t120\t1\t10\t7\tTT_Task\tTK_NotStart\tA120\tNegative\t-8\t8\tN\r\n",
        "",
    );
    let mut p = import(&text, 1).unwrap();
    let a = p
        .model
        .activities
        .iter()
        .find(|a| a.task_code == "A120")
        .unwrap();
    assert_eq!(a.rem, 0);
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("negative remaining duration") && w.contains("A120")),
        "{:?}",
        p.warnings
    );
    p.model.validate().unwrap();
    openxer_engine::scheduler::schedule(&mut p.model).unwrap();
}

/// `mini` with a SCHEDOPTIONS row for project 1 that turns multiple float
/// paths on, ending with activity `end`.
fn mini_with_float_path_end(end: &str) -> String {
    let options = format!(
        "%T\tSCHEDOPTIONS\r\n%F\tschedoptions_id\tproj_id\tenable_multiple_longest_path_calc\tmax_multiple_longest_path\tkey_activity_for_multiple_longest_paths\r\n%R\t1\t1\tY\t10\t{end}\r\n%E\r\n"
    );
    mini("", "").replace("%E\r\n", &options)
}

/// A file can name a float-path ending activity it does not hold (the ID of
/// an activity in the project it was copied from, say). The engine's
/// validation refuses that, so the importer reads the ending activity as
/// blank with a warning and the file still opens and schedules. The stale
/// cell is left as it was on export.
#[test]
fn a_float_path_end_activity_not_in_the_file_is_read_as_blank_with_a_warning() {
    let text = mini_with_float_path_end("999");
    let mut p = import(&text, 1).unwrap();
    assert!(p.model.options.multiple_float_paths);
    assert_eq!(p.model.options.float_paths_end_activity, None);
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("float paths") && w.contains("not in the file")),
        "{:?}",
        p.warnings
    );
    assert_eq!(p.updated_document().to_bytes().unwrap(), text.as_bytes());
    p.model.validate().unwrap();
    openxer_engine::scheduler::schedule(&mut p.model).unwrap();
    // An activity of another project in the same file is not in this project.
    let other = import(&mini_with_float_path_end("200"), 1).unwrap();
    assert_eq!(other.model.options.float_paths_end_activity, None);
}

/// An ending activity the file does hold is kept, with no warning.
#[test]
fn a_float_path_end_activity_in_the_file_is_kept() {
    let text = mini_with_float_path_end("110");
    let p = import(&text, 1).unwrap();
    assert_eq!(p.model.options.float_paths_end_activity, Some(110));
    assert!(
        !p.warnings.iter().any(|w| w.contains("float paths")),
        "{:?}",
        p.warnings
    );
    assert_eq!(p.updated_document().to_bytes().unwrap(), text.as_bytes());
}

/// `mini` with multiple float paths on and `count` as the number of paths.
fn mini_with_float_path_count(count: &str) -> String {
    let options = format!(
        "%T\tSCHEDOPTIONS\r\n%F\tschedoptions_id\tproj_id\tenable_multiple_longest_path_calc\tmax_multiple_longest_path\tkey_activity_for_multiple_longest_paths\r\n%R\t1\t1\tY\t{count}\t\r\n%E\r\n"
    );
    mini("", "").replace("%E\r\n", &options)
}

/// A path count of zero, or one that is not a number, is refused by the
/// engine; the importer reads the default count with a warning and leaves
/// the stored cell as it was. A blank count is the default without one.
#[test]
fn a_zero_or_unreadable_float_path_count_is_read_as_the_default_with_a_warning() {
    for count in ["0", "many", "-3"] {
        let text = mini_with_float_path_count(count);
        let mut p = import(&text, 1).unwrap();
        assert_eq!(p.model.options.max_float_paths, 10, "{count}");
        assert!(
            p.warnings
                .iter()
                .any(|w| w.contains("number of float paths")),
            "{count}: {:?}",
            p.warnings
        );
        assert_eq!(p.updated_document().to_bytes().unwrap(), text.as_bytes());
        p.model.validate().unwrap();
        openxer_engine::scheduler::schedule(&mut p.model).unwrap();
    }
    for (count, paths) in [("", 10), ("3", 3)] {
        let p = import(&mini_with_float_path_count(count), 1).unwrap();
        assert_eq!(p.model.options.max_float_paths, paths);
        assert!(
            !p.warnings.iter().any(|w| w.contains("float paths")),
            "{count}: {:?}",
            p.warnings
        );
    }
}

#[test]
fn every_truncation_of_a_file_is_handled() {
    let text = mini("", "");
    for end in 0..=text.len() {
        if !text.is_char_boundary(end) {
            continue;
        }
        if let Ok(doc) = Document::parse(&text[..end]) {
            for (pid, _) in doc.projects() {
                if let Ok(p) = ProjectData::from_document(doc.clone(), pid, "cut".into()) {
                    let _ = p.updated_document().to_bytes();
                }
            }
        }
    }
}

#[test]
fn duplicate_activity_ids_are_rejected() {
    let text = mini(
        "%R\t100\t1\t10\t7\tTT_Task\tTK_NotStart\tA999\tCopy\t8\t8\tN\r\n",
        "",
    );
    let error = import(&text, 1).unwrap_err();
    assert!(error.contains("Activity ID 100"), "{error}");
    assert!(error.contains("A100") && error.contains("A999"), "{error}");
    // The same ID in another project is someone else's problem.
    assert!(import(&text, 2).is_ok());
}

#[test]
fn dangling_references_warn_and_survive_export() {
    let text = mini(
        "%R\t120\t1\t99\t42\tTT_Task\tTK_NotStart\tA120\tOrphan\t8\t8\tN\r\n",
        "%R\t3\t120\t555\t1\t1\tPR_FS\t0\tmissing predecessor\r\n",
    );
    let p = import(&text, 1).unwrap();
    let joined = p.warnings.join("\n");
    assert!(
        joined.contains("calendar that is not in the file (A120)"),
        "{joined}"
    );
    assert!(
        joined.contains("WBS element that is not in the file (A120)"),
        "{joined}"
    );
    assert!(
        joined.contains("1 relationships refer to activities that are not in the file"),
        "{joined}"
    );
    let a = p
        .model
        .activities
        .iter()
        .find(|a| a.task_id == 120)
        .unwrap();
    assert_eq!(a.cal, p.model.project_calendar);
    assert_eq!(p.wbs_label(99), "Unassigned");
    assert_eq!(
        p.updated_document().text(),
        text,
        "unchanged export keeps every row"
    );
    let mut scheduled = p.clone();
    openxer_engine::scheduler::schedule(&mut scheduled.model).unwrap();
    let out = scheduled.updated_document();
    let t = out.table("TASKPRED").unwrap();
    assert!(t.rows.iter().any(|r| t.value(r, "pred_task_id") == "555"));
    let tasks = out.table("TASK").unwrap();
    let orphan = tasks
        .rows
        .iter()
        .find(|r| tasks.value(r, "task_id") == "120")
        .unwrap();
    assert_eq!(tasks.value(orphan, "clndr_id"), "42");
}

#[test]
fn missing_project_calendar_falls_back_to_default() {
    let text = mini("", "").replace("%R\t1\tALPHA\t7\t", "%R\t1\tALPHA\t8\t");
    let p = import(&text, 1).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("Project calendar 8")));
    let none = mini("", "").replace(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"), "");
    assert!(import(&none, 1).unwrap_err().contains("no calendars"));
}

/// A project with a WBS but no calendars and no activities, which opens.
fn without_calendars() -> ProjectData {
    let text = "ERMHDR\t25.12\r\n%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tplan_start_date\r\n%R\t5\tEMPTY\t\t2026-01-05\r\n%T\tPROJWBS\r\n%F\twbs_id\tproj_id\tseq_num\tproj_node_flag\twbs_short_name\twbs_name\tparent_wbs_id\r\n%R\t50\t5\t0\tY\tEMPTY\tEmpty\t\r\n%E\r\n";
    let p = import(text, 5).unwrap();
    assert!(p.model.calendars.is_empty());
    p
}

#[test]
fn an_activity_added_to_a_project_without_calendars_saves_and_reopens() {
    let mut p = without_calendars();
    let i = p.add_activity(50);
    p.rebuild_assignments();
    assert_eq!(p.model.calendars.len(), 1, "the standard calendar is added");
    assert_eq!(p.model.activities[i].cal, 0);
    let text = p.updated_document().text();
    let again = import(&text, 5).unwrap();
    assert_eq!(again.model.activities.len(), 1);
    assert_eq!(
        again.calendar_names,
        [openxer_xer::STANDARD_CALENDAR_NAME.to_string()]
    );
    openxer_engine::scheduler::schedule(&mut again.model.clone()).unwrap();
}

#[test]
fn the_first_calendar_can_be_added_to_a_project_without_calendars() {
    let mut p = without_calendars();
    p.document.table_mut("CALENDAR").add(&[
        ("clndr_id", "3".into()),
        ("clndr_name", "Mine".into()),
        ("clndr_data", openxer_xer::standard_calendar_data()),
    ]);
    p.rebuild_calendars().unwrap();
    assert_eq!(p.calendar_names, ["Mine"]);
    assert_eq!(p.model.project_calendar, 0);
}

#[test]
fn the_first_calendar_added_through_the_calendar_operations_is_the_project_calendar() {
    use openxer_xer::edit::{calendars, records};
    let project_calendar = |p: &ProjectData| {
        let t = p.document.table("PROJECT").unwrap();
        let row = t
            .rows
            .iter()
            .find(|r| t.value(r, "proj_id") == "5")
            .unwrap();
        t.value(row, "clndr_id").to_string()
    };
    for kind in [calendars::Kind::Project, calendars::Kind::Global] {
        let mut p = without_calendars();
        assert_eq!(project_calendar(&p), "");
        let id = calendars::add(&mut p, kind, None, "2026-11-09 08:00").unwrap();
        assert_eq!(p.model.project_calendar, 0);
        assert_eq!(p.model.calendars[0].id, id);
        assert_eq!(project_calendar(&p), id, "{kind:?}");
        // A second calendar leaves the project calendar alone.
        calendars::add(&mut p, calendars::Kind::Global, None, "2026-11-09 08:00").unwrap();
        assert_eq!(project_calendar(&p), id);
        // It survives save and reopen, and an activity then takes it.
        let mut again = import(&p.updated_document().text(), 5).unwrap();
        assert_eq!(again.model.calendars[again.model.project_calendar].id, id);
        let i = again.add_activity(50);
        assert_eq!(again.model.calendars[again.model.activities[i].cal].id, id);
        assert_eq!(records::num(&project_calendar(&again)), records::num(&id));
    }
}

#[test]
fn shift_times_outside_the_day_are_refused_not_overflowed() {
    for time in ["99:00", "-1:00", "08:75", "200000000000000000:00"] {
        let text = mini("", "").replace(CAL, &CAL.replacen("16:00", time, 1));
        let error = import(&text, 1).unwrap_err();
        assert!(error.contains("Calendar 7"), "{time}: {error}");
    }
    // The edges of the day still read.
    let text = mini("", "").replace(CAL, &CAL.replace("08:00|f|16:00", "00:00|f|24:00"));
    import(&text, 1).unwrap();
}

#[test]
fn calendar_edits_follow_the_import_rules_for_unreadable_and_duplicate_calendars() {
    // An unused calendar that cannot be read is skipped on import; calendar
    // edits must skip it too instead of failing every time.
    let broken = format!("%R\t9\tN\tBroken\t(0||CalendarData()(\r\n%R\t7\tY\tFive day\t{CAL}\r\n");
    let text = mini("", "").replace(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"), &broken);
    let mut p = import(&text, 1).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("Calendar 9")));
    let before = p.model.clone();
    p.rebuild_calendars().unwrap();
    assert_eq!(p.model, before);
    // A duplicated calendar ID: the first row wins, on import and after an edit.
    let seven = CAL.replace("16:00", "12:00");
    let twice = format!("%R\t7\tY\tFive day\t{CAL}\r\n%R\t7\tN\tShort day\t{seven}\r\n");
    let text = mini("", "").replace(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"), &twice);
    let mut p = import(&text, 1).unwrap();
    let before = p.model.clone();
    let names = p.calendar_names.clone();
    p.rebuild_calendars().unwrap();
    assert_eq!(p.model, before, "activities keep the first calendar's work");
    assert_eq!(p.calendar_names, names);
}

#[test]
fn empty_tables_and_empty_projects() {
    let text = "ERMHDR\t25.12\r\n%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tplan_start_date\r\n%R\t5\tEMPTY\t\t2026-01-05\r\n%T\tTASK\r\n%F\ttask_id\tproj_id\r\n%T\tTASKPRED\r\n%F\ttask_pred_id\r\n%E\r\n";
    let p = import(text, 5).unwrap();
    assert!(p.model.activities.is_empty());
    assert_eq!(p.updated_document().text(), text);
    assert!(import(text, 6)
        .unwrap_err()
        .contains("Project 6 is not in this file"));
    assert!(import("ERMHDR\r\n%E\r\n", 1)
        .unwrap_err()
        .contains("no PROJECT table"));
}

#[test]
fn multiple_projects_export_only_their_own_rows() {
    let text = mini("", "");
    let doc = Document::parse(&text).unwrap();
    assert_eq!(
        doc.projects(),
        vec![(1, "ALPHA".to_string()), (2, "BETA".to_string())]
    );
    let mut alpha = ProjectData::from_document(doc.clone(), 1, "multi".into()).unwrap();
    let beta = ProjectData::from_document(doc, 2, "multi".into()).unwrap();
    assert_eq!(alpha.model.activities.len(), 2);
    assert_eq!(beta.model.activities.len(), 1);
    assert!(alpha.warnings.iter().any(|w| w.contains("1 cross-project")));
    assert!(beta.warnings.iter().any(|w| w.contains("1 cross-project")));
    openxer_engine::scheduler::schedule(&mut alpha.model).unwrap();
    alpha.model.links.clear();
    let out = alpha.updated_document();
    let tasks = out.table("TASK").unwrap();
    let other = tasks
        .rows
        .iter()
        .find(|r| tasks.value(r, "task_id") == "200")
        .unwrap();
    // Columns added for this project's dates pad other rows with blanks only.
    let original = doc_row(&text, "TASK", "200");
    assert_eq!(other[..original.len()], original[..]);
    assert!(other[original.len()..].iter().all(String::is_empty));
    let preds = out.table("TASKPRED").unwrap();
    let ids: Vec<_> = preds
        .rows
        .iter()
        .map(|r| preds.value(r, "task_pred_id"))
        .collect();
    assert_eq!(
        ids,
        ["2"],
        "the deleted local link goes, the external one stays"
    );
    // A new activity's ID is above every ID in the file, not only this project's.
    let mut beta = beta;
    let i = beta.add_activity(20);
    assert_eq!(beta.model.activities[i].task_id, 210);
}

fn doc_row(text: &str, table: &str, id: &str) -> Vec<String> {
    let doc = Document::parse(text).unwrap();
    let t = doc.table(table).unwrap();
    t.rows.iter().find(|r| r[0] == id).unwrap().clone()
}

#[test]
fn absurd_numbers_are_rejected_not_overflowed() {
    for value in ["1e300", "inf", "NaN", "12x"] {
        let text = mini("", "").replace("First\t16\t16", &format!("First\t{value}\t16"));
        let error = import(&text, 1).unwrap_err();
        assert!(
            error.contains("A100 remaining duration"),
            "{value}: {error}"
        );
    }
    let text = mini("", "").replace("PR_FS\t0\tkeep", "PR_FS\t9e99\tkeep");
    assert!(import(&text, 1).unwrap_err().contains("lag"));
    // Assignment durations come from units / units per hour, and assignment
    // lags are hours: the same bound applies to both.
    let assignment = |qty: &str, rate: &str, lag: &str| {
        let row = format!(
            "%T\tTASKRSRC\r\n%F\ttaskrsrc_id\ttask_id\tproj_id\trsrc_type\trollup_dates_flag\tremain_qty\tremain_qty_per_hr\ttarget_qty\ttarget_qty_per_hr\trelag_drtn_hr_cnt\r\n\
             %R\t5\t100\t1\tRT_Labor\tY\t{qty}\t{rate}\t16\t1\t{lag}\r\n"
        );
        mini("", "").replace("%E\r\n", &format!("{row}%E\r\n"))
    };
    let error = import(&assignment("1000000", "0.000001", "0"), 1).unwrap_err();
    assert!(
        error.contains("Assignment 5 of activity A100 remaining"),
        "{error}"
    );
    let error = import(&assignment("16", "1", "9e99"), 1).unwrap_err();
    assert!(
        error.contains("Assignment 5 of activity A100 lag"),
        "{error}"
    );
    import(&assignment("16", "2", "4"), 1).expect("ordinary values import");
    import(&assignment("16", "", "x"), 1).expect("text that is not a number keeps its reading");
}

#[test]
fn wbs_cycles_and_deep_calendars_do_not_hang() {
    let text = mini("", "").replace(
        "%R\t10\t1\t0\tY\tALPHA\tAlpha\t\r\n",
        "%R\t10\t1\t0\tY\tALPHA\tAlpha\t11\r\n%R\t11\t1\t1\tN\tLOOP\tLoop\t10\r\n",
    );
    let p = import(&text, 1).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("circular")));
    let mut deep = "(0||CalendarData()(".to_string();
    for _ in 0..100_000 {
        deep.push_str("(0||x()(");
    }
    let text = mini("", "").replace(CAL, &deep);
    assert!(import(&text, 1).unwrap_err().contains("nested too deeply"));
}

// -------------------------------------------------------- export validity

#[test]
fn starter_export_uses_reference_header_and_table_order() {
    let doc = starter().updated_document();
    assert_eq!(doc.header.split('\t').count(), 9, "{}", doc.header);
    let order: Vec<_> = doc.tables.iter().map(|t| t.name.as_str()).collect();
    let rank = |n: &str| {
        openxer_xer::TABLE_ORDER
            .iter()
            .position(|t| *t == n)
            .unwrap()
    };
    assert!(
        order.windows(2).all(|w| rank(w[0]) < rank(w[1])),
        "{order:?}"
    );
    let task = doc.table("TASK").unwrap();
    assert_eq!(
        task.fields,
        openxer_xer::standard_fields("TASK").unwrap(),
        "a created TASK table has the reference column list"
    );
    let unchanged = starter();
    assert_eq!(
        unchanged.updated_document().text(),
        unchanged.document.text(),
        "an unchanged project exports its document unchanged"
    );
}

#[test]
fn new_tables_are_inserted_in_reference_order() {
    let mut doc = Document::parse(&mini("", "")).unwrap();
    doc.table_mut("TASKMEMO");
    doc.table_mut("CURRTYPE");
    doc.table_mut("ZZCUSTOM");
    let order: Vec<_> = doc.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        order,
        [
            "CURRTYPE", "PROJECT", "CALENDAR", "PROJWBS", "TASK", "TASKMEMO", "TASKPRED",
            "ZZCUSTOM"
        ]
    );
}

#[test]
fn new_activities_and_relationships_carry_reference_columns() {
    let mut p = starter();
    let i = p.add_activity(10);
    let pred = p.model.activities.len() - 2;
    p.model.links.push(openxer_engine::model::Link {
        pred,
        succ: i,
        link_type: openxer_engine::model::LinkType::SS,
        lag: 90,
        driving_path: false,
    });
    p.model.rebuild_links().unwrap();
    let doc = p.updated_document();
    let t = doc.table("TASK").unwrap();
    let id = p.model.activities[i].task_id.to_string();
    let row = t.rows.iter().find(|r| t.value(r, "task_id") == id).unwrap();
    for (k, v) in [
        ("proj_id", "1"),
        ("wbs_id", "10"),
        ("clndr_id", "1"),
        ("task_type", "TT_Task"),
        ("status_code", "TK_NotStart"),
        ("duration_type", "DT_FixedDUR2"),
        ("complete_pct_type", "CP_Drtn"),
        ("priority_type", "PT_Normal"),
        ("rev_fdbk_flag", "N"),
        ("lock_plan_flag", "N"),
        ("auto_compute_act_flag", "N"),
        ("est_wt", "1"),
        ("phys_complete_pct", "0"),
        ("remain_drtn_hr_cnt", "8"),
        ("target_drtn_hr_cnt", "8"),
        ("driving_path_flag", "N"),
        // Dated as an activity without logic: the data date, then one day.
        ("target_start_date", "2026-11-02 08:00"),
        ("target_end_date", "2026-11-02 17:00"),
    ] {
        assert_eq!(t.value(row, k), v, "{k}");
    }
    let guid = t.value(row, "guid");
    assert_eq!(guid.len(), 22);
    assert!(guid
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/'));
    assert_eq!(t.value(row, "create_date").len(), 16);
    let r = doc.table("TASKPRED").unwrap();
    let link = r.rows.last().unwrap();
    assert_eq!(r.value(link, "task_pred_id"), "17");
    assert_eq!(r.value(link, "pred_type"), "PR_SS");
    assert_eq!(r.value(link, "lag_hr_cnt"), "1.5");
    assert_eq!(r.value(link, "pred_proj_id"), "1");
    let back =
        ProjectData::from_document(Document::parse(&doc.text()).unwrap(), 1, "x".into()).unwrap();
    assert_eq!(back.model.links.len(), p.model.links.len());
    assert_eq!(
        back.details[&p.model.activities[i].task_id],
        p.details[&p.model.activities[i].task_id]
    );
}

// ------------------------------------------------------------------- fuzz

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const TOKENS: &[&[u8]] = &[
    b"\t",
    b"\r\n",
    b"\n",
    b"\r",
    b"%R\t",
    b"%T\tTASK",
    b"%F\t",
    b"%E",
    b"0",
    b"-1",
    b"99999999999999999999",
    b"1e308",
    b"2026-13-45 99:99",
    b"9999-12-31 23:59",
    b"\xff",
    b"\xe9",
    b"\x7f\x7f",
    b"(0||",
    b")",
    b"TT_Mile",
    b"TK_Complete",
    b"CS_MSO",
    b"PR_SF",
    b"\xef\xbb\xbf",
];

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut data = base.to_vec();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(data.len() + 1);
        match rng.below(7) {
            0 => {
                let end = (at + rng.below(64)).min(data.len());
                data.drain(at..end);
            }
            1 => {
                let token = TOKENS[rng.below(TOKENS.len())];
                data.splice(at..at, token.iter().copied());
            }
            2 if at < data.len() => data[at] = rng.next() as u8,
            3 if at < data.len() && data[at].is_ascii_digit() => {
                data[at] = b'0' + rng.below(10) as u8;
            }
            4 => data.truncate(at),
            5 => {
                // Duplicate a line.
                let start = data[..at]
                    .iter()
                    .rposition(|&b| b == b'\n')
                    .map_or(0, |i| i + 1);
                let end = data[at..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(data.len(), |i| at + i + 1);
                let line = data[start..end].to_vec();
                data.splice(end..end, line);
            }
            _ => {
                // Swap a tab-separated value with the next one.
                if let Some(t) = data[at..].iter().position(|&b| b == b'\t') {
                    let t = at + t;
                    data.remove(t);
                    let next = (t + rng.below(8)).min(data.len());
                    data.insert(next, b'\t');
                }
            }
        }
    }
    data
}

/// Import, export, reimport and schedule each mutant. Errors are fine; panics are not.
fn exercise(data: &[u8], schedule: bool) {
    let Ok(doc) = Document::from_bytes(data) else {
        return;
    };
    for (pid, _) in doc.projects().into_iter().take(3) {
        let Ok(mut p) = ProjectData::from_document(doc.clone(), pid, "fuzz".into()) else {
            continue;
        };
        if let Ok(bytes) = p.updated_document().to_bytes() {
            let _ = Document::from_bytes(&bytes)
                .and_then(|d| ProjectData::from_document(d, pid, "fuzz".into()));
        }
        if schedule && p.model.activities.len() < 200 {
            p.refresh_derived_durations();
            let _ = openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources);
            let _ = p.updated_document().to_bytes();
        }
    }
}

fn fuzz(name: &str, base: &[u8], iterations: usize, seed: u64, schedule: bool) {
    let mut rng = Rng(seed);
    let mut panics = vec![];
    for i in 0..iterations {
        let data = mutate(&mut rng, base);
        if std::panic::catch_unwind(|| exercise(&data, schedule)).is_err() {
            let path = std::env::temp_dir().join(format!("openxer-fuzz-{name}-{i}.xer"));
            let _ = std::fs::write(&path, &data);
            panics.push(format!(
                "{name} iteration {i} (seed {seed}): saved to {}",
                path.display()
            ));
        }
    }
    assert!(panics.is_empty(), "{}", panics.join("\n"));
}

#[test]
fn mutation_fuzz_starter_never_panics() {
    let base = starter().updated_document().to_bytes().unwrap();
    fuzz("starter", &base, 3000, 0x5eed_0f0e_7e12, true);
    let base = mini("", "").into_bytes();
    fuzz("mini", &base, 3000, 0x0123_4567_89ab_cdef, true);
}

#[test]
fn mutation_fuzz_corpus_file_never_panics() {
    let dir = std::env::var_os("OPENXER_CORPUS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("OPENXER_CORPUS is not set"));
    let Ok(base) = std::fs::read(dir.join("6035.xer")) else {
        println!(
            "SKIPPED: corpus file 6035.xer not found in {} (set OPENXER_CORPUS)",
            dir.display()
        );
        return;
    };
    fuzz("corpus-6035", &base, 150, 0xc0ffee, false);
}

// ------------------------------------------- tables added after the fuzz suite

/// Tables whose readers and column lists came later (enterprise structures,
/// period performance, feedback, resource prices and roles, code, UDF and
/// admin dictionaries), with the column list used when the test creates them.
fn hostile_columns(table: &str) -> Vec<String> {
    let own: &[&str] = match table {
        "RSRC" => &[
            "rsrc_id",
            "parent_rsrc_id",
            "clndr_id",
            "role_id",
            "shift_id",
            "rsrc_short_name",
            "rsrc_name",
            "def_qty_per_hr",
            "cost_qty_type",
            "ot_factor",
            "active_flag",
            "rsrc_type",
            "unit_id",
            "curr_id",
        ],
        "RSRCRATE" => &[
            "rsrc_rate_id",
            "rsrc_id",
            "max_qty_per_hr",
            "cost_per_qty",
            "start_date",
            "shift_period_id",
        ],
        "RSRCROLE" => &["rsrc_id", "role_id", "skill_level"],
        "RSRCLEVELLIST" => &["rsrc_level_list_id", "schedoptions_id", "rsrc_id"],
        "TASKFDBK" => &["task_id", "proj_id", "task_fdbk"],
        "TASKACTV" => &["task_id", "actv_code_type_id", "actv_code_id", "proj_id"],
        "UDFVALUE" => &[
            "udf_type_id",
            "fk_id",
            "proj_id",
            "udf_date",
            "udf_number",
            "udf_text",
            "udf_code_id",
        ],
        _ => openxer_xer::standard_fields(table).unwrap_or(&[]),
    };
    own.iter().map(|s| s.to_string()).collect()
}

const HOSTILE_TABLES: &[&str] = &[
    "OBS",
    "ROLES",
    "ROLERATE",
    "ROLECATTYPE",
    "ROLECATVAL",
    "ROLERCAT",
    "RSRCCURVDATA",
    "SHIFT",
    "SHIFTPER",
    "CURRTYPE",
    "FUNDSRC",
    "FINTMPL",
    "FINDATES",
    "TASKFIN",
    "TRSRCFIN",
    "TASKFDBK",
    "RSRC",
    "RSRCRATE",
    "RSRCROLE",
    "RSRCRCAT",
    "RSRCLEVELLIST",
    "ACTVTYPE",
    "ACTVCODE",
    "TASKACTV",
    "PCATTYPE",
    "PCATVAL",
    "PROJPCAT",
    "RCATTYPE",
    "RCATVAL",
    "UDFTYPE",
    "UDFCODE",
    "UDFVALUE",
    "ACCOUNT",
    "MEMOTYPE",
    "WBSMEMO",
    "COSTTYPE",
    "UMEASURE",
    "DOCCATG",
    "DOCSTAT",
    "RISKTYPE",
    "ASGNMNTCATTYPE",
    "ASGNMNTCATVAL",
    "ASGNMNTACAT",
    "PROCGROUP",
    "PROCITEM",
];

/// Tables whose rows form a tree through a parent column.
const TREE_TABLES: &[&str] = &[
    "OBS",
    "ROLES",
    "ROLECATVAL",
    "FUNDSRC",
    "RSRC",
    "ACTVCODE",
    "PCATVAL",
    "RCATVAL",
    "ACCOUNT",
    "RISKTYPE",
    "ASGNMNTCATVAL",
];

/// A value meant to hurt a reader: ids that chain parents into a deep line
/// with a self loop and a two-row loop at the end, foreign keys that miss,
/// overflowing and non-finite numbers, impossible dates, control
/// characters and a very long text.
fn hostile_value(column: &str, index: usize, i: usize, n: usize, task: i64, pid: i64) -> String {
    const NUMBERS: &[&str] = &[
        "1e308",
        "-1",
        "99999999999999999999",
        "NaN",
        "inf",
        "",
        "0.5",
        "-0",
        "1,5",
    ];
    const DATES: &[&str] = &[
        "2026-13-45 99:99",
        "9999-12-31 23:59",
        "",
        "0000-00-00 00:00",
        "2026-11-05 08:00",
        "not a date",
    ];
    let texts = [
        "plain".to_string(),
        "tab\there".to_string(),
        "line\nfeed\r\nand\rcr".to_string(),
        "\u{0}\u{1}\u{7f}%R\t%T\tTASK".to_string(),
        "<html><body>\"quoted\" &amp;</body></html>".to_string(),
        "\u{e9}\u{2603}".to_string(),
    ];
    let id = (i + 1).to_string();
    if column == "proj_id" {
        return if i % 5 == 4 {
            "99999999999999999999".into()
        } else {
            pid.to_string()
        };
    }
    if column == "task_id" {
        return if i % 4 == 3 {
            "-5".into()
        } else {
            task.to_string()
        };
    }
    if index == 0 {
        return id;
    }
    if column.starts_with("parent_") {
        return match i {
            0 => String::new(),
            _ if i == n / 2 => id,
            _ if i + 2 == n => n.to_string(),
            _ => i.to_string(),
        };
    }
    if column.ends_with("_id") {
        return ["1", "2", "3", "-7", "1e308", ""][i % 6].into();
    }
    if column.ends_with("_flag") {
        return ["Y", "N", "", "\u{1}"][i % 4].into();
    }
    if column.contains("date") || column == "window_start" || column == "window_end" {
        return DATES[i % DATES.len()].into();
    }
    let numeric = [
        "qty", "cost", "pct", "cnt", "num", "wt", "len", "hr", "factor", "bcw", "usage", "rate",
        "level", "digit", "type_id",
    ];
    if numeric.iter().any(|k| column.contains(k)) {
        return NUMBERS[(i + index) % NUMBERS.len()].into();
    }
    // One very long value per table (large tables only, to keep the fuzz quick).
    if i == 3 && n >= 20 {
        return "x".repeat(70_000);
    }
    texts[(i + index) % texts.len()].clone()
}

/// The starter project with every table above filled with hostile rows:
/// `rows` rows each, `deep` rows for the tree tables. Every seventh row is
/// shorter than the column list (read with blanks and a warning).
fn hostile(rows: usize, deep: usize) -> Document {
    let mut p = starter();
    let pid = p.model.project.proj_id;
    let task = p.model.activities[0].task_id;
    let doc = &mut p.document;
    for name in HOSTILE_TABLES {
        let fields = hostile_columns(name);
        let n = if TREE_TABLES.contains(name) {
            deep
        } else {
            rows
        };
        let table = doc.table_mut(name);
        table.fields = fields.clone();
        for i in 0..n {
            let mut row: Vec<String> = fields
                .iter()
                .enumerate()
                .map(|(k, c)| hostile_value(c, k, i, n, task, pid))
                .collect();
            if i % 7 == 6 {
                row.truncate(row.len() / 2);
            }
            table.rows.push(row);
        }
    }
    p.document
}

#[test]
fn new_table_readers_survive_hostile_rows_and_deep_chains() {
    let started = std::time::Instant::now();
    // A row longer than its column list is a structural error, reported with
    // the table and line, for every one of these tables.
    for name in HOSTILE_TABLES {
        let mut long = hostile(2, 2);
        long.table_mut(name).rows[0].push("extra".into());
        let error = Document::parse(&long.text()).unwrap_err();
        assert!(error.contains(&format!("table {name} row")), "{error}");
    }
    let mut doc = Document::parse(&hostile(40, 20_000).text()).unwrap();
    // The hostile text holds characters outside Windows-1252.
    doc.windows_encoding = false;
    let pid = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc.clone(), pid, "hostile".into()).unwrap();
    // Nothing was edited: every table, hostile or not, is written back as read.
    assert_eq!(p.updated_document().text(), doc.text());
    let bytes = p.updated_document().to_bytes().unwrap();
    let again =
        ProjectData::from_document(Document::from_bytes(&bytes).unwrap(), pid, "x".into()).unwrap();
    assert_eq!(again.model, p.model);
    p.refresh_derived_durations();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let exported = Document::from_bytes(&p.updated_document().to_bytes().unwrap()).unwrap();
    for name in HOSTILE_TABLES {
        assert_eq!(
            exported.table(name).map(|t| t.rows.len()),
            doc.table(name).map(|t| t.rows.len()),
            "{name}"
        );
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(120),
        "deep chains took {:?}",
        started.elapsed()
    );
}

#[test]
fn mutation_fuzz_new_tables_never_panics() {
    // The hostile text holds characters outside Windows-1252: write UTF-8.
    let mut doc = hostile(6, 6);
    doc.windows_encoding = false;
    let base = doc.to_bytes().unwrap();
    fuzz("hostile", &base, 1500, 0x7ab1_e5ee_d001, true);
}

/// A new activity is dated as one without logic (data date, then its
/// duration on its calendar) and keeps one GUID and create date through
/// every export, as the CLI and MCP `add_activity` make it.
#[test]
fn a_new_activity_is_dated_and_keeps_its_guid() {
    let mut p = starter();
    let i = p.add_activity(10);
    let a = &p.model.activities[i];
    assert_eq!(a.early_start, p.model.project.data_date);
    assert!(a.early_finish > a.early_start);
    assert!(a.late_start.is_none() && a.total_float.is_none());
    let one = p.updated_document().text();
    assert_eq!(p.updated_document().text(), one);
    // Three days: the finish follows until the project is scheduled.
    let one_day = a.early_finish;
    openxer_xer::edit::activity::edit_activity(
        &mut p,
        i,
        openxer_xer::edit::activity::Edit::RemainingDuration(3 * 480),
    )
    .unwrap();
    assert!(p.model.activities[i].early_finish > one_day);
    let guid = |text: &str| {
        let doc = Document::parse(text).unwrap();
        let t = doc.table("TASK").unwrap().clone();
        let id = p.model.activities[i].task_id.to_string();
        let row = t.rows.iter().find(|r| t.value(r, "task_id") == id).unwrap();
        (
            t.value(row, "guid").to_string(),
            t.value(row, "create_date").to_string(),
        )
    };
    assert_eq!(guid(&p.updated_document().text()), guid(&one));
}
