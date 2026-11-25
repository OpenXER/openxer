//! The project record helpers live in `openxer_xer::edit::records`, shared with
//! the command line and MCP edit commands.
pub use openxer_xer::edit::records::*;

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_xer::Document;
    #[test]
    fn steps_round_trip_through_export() {
        let mut p = openxer_xer::starter();
        let pid = p.model.project.proj_id;
        let id = next_id(&p.document, "TASKPROC", "proc_id");
        add_row(
            &mut p.document,
            "TASKPROC",
            &[
                ("proc_id", id.to_string()),
                ("task_id", "1000".into()),
                ("proj_id", pid.to_string()),
                ("seq_num", "1".into()),
                ("proc_name", clean("Set\tforms")),
                ("complete_pct", "50".into()),
            ],
        );
        let text = p.updated_document().text();
        let reread = Document::parse(&text).unwrap();
        let rows = rows_for(&reread, "TASKPROC", 1000, pid);
        assert_eq!(rows.len(), 1);
        assert_eq!(get(&reread, "TASKPROC", rows[0], "proc_name"), "Set forms");
        assert_eq!(
            reread.table("TASKPROC").unwrap().fields[..3],
            ["proc_id", "task_id", "proj_id"]
        );
    }
    #[test]
    fn raw_task_columns_survive_export_and_new_activities() {
        let mut p = openxer_xer::starter();
        let pid = p.model.project.proj_id;
        let i = p.add_activity(p.wbs[0].id);
        let task_id = p.model.activities[i].task_id;
        set_task_value(
            &mut p.document,
            task_id,
            pid,
            "suspend_date",
            "2026-11-05 08:00".into(),
        );
        let reread = Document::parse(&p.updated_document().text()).unwrap();
        assert_eq!(
            task_value(&reread, task_id, pid, "suspend_date"),
            "2026-11-05 08:00"
        );
        assert_eq!(
            task_value(&reread, task_id, pid, "task_code"),
            format!("A{task_id}")
        );
        let tasks = reread.table("TASK").unwrap();
        let count = tasks
            .rows
            .iter()
            .filter(|r| num(tasks.value(r, "task_id")) == task_id)
            .count();
        assert_eq!(count, 1);
    }
    #[test]
    fn numbers_from_a_file_are_clamped() {
        assert_eq!(num(" 42 "), 42);
        assert_eq!(num("7.9"), 7);
        assert_eq!(num("99999999999999999999"), NUM_LIMIT);
        assert_eq!(num("-1e308"), -NUM_LIMIT);
        assert_eq!(num("9223372036854775807"), NUM_LIMIT);
        assert_eq!(num("NaN"), 0);
        assert_eq!(num("inf"), NUM_LIMIT);
        assert_eq!(num("x"), 0);
    }
    #[test]
    fn memo_text_conversion() {
        let html = "<html><body><font face=\"Arial\"><p><b>SCF</b><br>Brand&#x27;s &amp; crew</p><p>Next</p></font></body></html>";
        assert_eq!(memo_to_text(html), "SCF\nBrand's & crew\nNext");
        let memo = text_to_memo("a < b\nc & d");
        assert_eq!(memo, "<html><body>a &lt; b<br>c &amp; d</body></html>");
        assert_eq!(memo_to_text(&memo), "a < b\nc & d");
        // Quote marks are single characters here; the file form is the
        // reader's and writer's business.
        let quoted = text_to_memo("say \"hi\" to 8\" pipe");
        assert_eq!(memo_to_text(&quoted), "say \"hi\" to 8\" pipe");
        assert_eq!(memo_to_text("a \"\" b"), "a \"\" b");
    }
    #[test]
    fn snapshot_restores_tables_and_task_columns() {
        let mut p = openxer_xer::starter();
        let pid = p.model.project.proj_id;
        let before = RecordSnapshot::capture(&p.document);
        add_row(&mut p.document, "TASKNOTE", &[("task_id", "1000".into())]);
        set_task_value(&mut p.document, 1000, pid, "rev_fdbk_flag", "Y".into());
        assert_ne!(RecordSnapshot::capture(&p.document), before);
        before.restore(&mut p.document);
        assert!(p.document.table("TASKNOTE").is_none());
        assert_eq!(task_value(&p.document, 1000, pid, "rev_fdbk_flag"), "");
        assert_eq!(RecordSnapshot::capture(&p.document), before);
    }
    // ------------------------------------------------ XER fidelity checks

    use openxer_xer::ProjectData;

    /// Every detail table has a column list for when it is created, and a
    /// place in the export table order (RISK is only read from files that
    /// carry it; PROJECT is only created by the starter project).
    #[test]
    fn every_detail_table_has_columns_and_an_export_place() {
        for name in DETAIL_TABLES {
            if name == "RISK" {
                continue;
            }
            assert!(
                openxer_xer::TABLE_ORDER.contains(&name),
                "{name} has no place in TABLE_ORDER"
            );
            if name == "PROJECT" {
                continue;
            }
            let fields =
                openxer_xer::standard_fields(name).unwrap_or_else(|| standard_fields(name));
            assert!(!fields.is_empty(), "{name} has no column list");
            let mut doc = openxer_xer::starter().document;
            doc.tables.clear();
            assert_eq!(ensure_table(&mut doc, name).fields, fields, "{name}");
        }
    }

    /// A hostile value for a column: parent chains with a self loop and a
    /// two-row loop, missing foreign keys, overflowing and non-finite numbers,
    /// impossible dates, control characters and a long text.
    fn hostile_value(
        column: &str,
        index: usize,
        i: usize,
        n: usize,
        task: i64,
        pid: i64,
    ) -> String {
        let id = (i + 1).to_string();
        match column {
            "proj_id" if i % 5 == 4 => return "99999999999999999999".into(),
            "proj_id" => return pid.to_string(),
            "task_id" if i % 4 == 3 => return "-5".into(),
            "task_id" => return task.to_string(),
            _ if index == 0 => return id,
            _ => {}
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
        if column.contains("date") || column.starts_with("window_") {
            return [
                "2026-13-45 99:99",
                "9999-12-31 23:59",
                "",
                "2026-11-05 08:00",
                "x",
            ][i % 5]
                .into();
        }
        let numeric = [
            "qty", "cost", "pct", "cnt", "num", "wt", "len", "hr", "factor", "bcw", "usage",
            "rate", "level", "digit", "value",
        ];
        if numeric.iter().any(|k| column.contains(k)) {
            return [
                "1e308",
                "-1",
                "99999999999999999999",
                "NaN",
                "inf",
                "",
                "0.5",
                "-0",
            ][(i + index) % 8]
                .into();
        }
        if i == 3 && n >= 20 {
            // One very long value per table (large tables only).
            return "x".repeat(50_000);
        }
        match (i + index) % 5 {
            0 => "plain".into(),
            1 => "tab\there\nline\rcr".into(),
            2 => "\u{0}\u{1}\u{7f}%R\t%T\tTASK".into(),
            3 => "<html><body>\"quoted\" &amp;</body></html>".into(),
            _ => "\u{e9}\u{2603}".into(),
        }
    }

    /// Tables the scheduling model reads (and may reject) are left alone.
    const MODEL_TABLES: [&str; 6] = [
        "PROJECT", "PROJWBS", "CALENDAR", "TASKPRED", "TASKRSRC", "TASK",
    ];

    /// The starter project with every detail table filled with hostile rows,
    /// `deep` rows in tables with a parent column, as UTF-8 XER bytes.
    fn hostile(rows: usize, deep: usize) -> Vec<u8> {
        let mut p = openxer_xer::starter();
        let pid = p.model.project.proj_id;
        let task = p.model.activities[0].task_id;
        let doc = &mut p.document;
        doc.windows_encoding = false;
        for name in DETAIL_TABLES {
            if MODEL_TABLES.contains(&name) || name == "RISK" {
                continue;
            }
            let table = ensure_table(doc, name);
            let fields = table.fields.clone();
            let tree = fields.iter().any(|f| f.starts_with("parent_"));
            let n = if tree { deep } else { rows };
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
        doc.to_bytes().unwrap()
    }

    /// Run the readers of the detail tables: enterprise trees and lists,
    /// prices, curves, shifts, currencies, financial periods and stored
    /// performance, code, UDF, cost account and category dictionaries, the
    /// record views, and an undo snapshot.
    fn read_everything(p: &ProjectData) {
        use crate::dictionaries::ops as d;
        use crate::enterprise::{ops as e, tree};
        use crate::views::{
            documents, expenses, issues, resource_edit, resources, risks, thresholds,
        };
        let pid = p.model.project.proj_id;
        let doc = &p.document;
        for spec in [e::OBS, e::ROLES, e::FUNDING] {
            for n in tree::nodes(doc, &spec, None).iter().take(3) {
                tree::subtree(doc, &spec, None, n.id);
                tree::value(doc, &spec, n.id, spec.name);
            }
        }
        for t in e::role_code_types(doc) {
            tree::nodes(doc, &e::role_value_spec(doc, t), Some(t));
            e::role_code_length(doc, t);
            e::role_code(doc, 1, t);
        }
        for spec in [
            e::CURVES,
            e::SHIFTS,
            e::CURRENCIES,
            e::FIN_CALENDARS,
            e::ROLE_CODE_TYPES,
        ] {
            for id in e::list_ids(doc, &spec).into_iter().take(4) {
                e::list_value(doc, &spec, id, spec.name);
            }
        }
        for id in [1, 2, 3, -7] {
            e::role_prices(doc, id);
            e::curve_points(doc, id);
            e::is_default_curve(doc, id);
            e::period_hours(&e::shift_periods(doc, id));
            e::currency_example(doc, id);
            e::periods(doc, id);
            e::role_uses(doc, id);
            e::funding_uses(doc, id);
            e::obs_uses(doc, id);
            e::obs_heir(doc, id);
            e::fin_calendar_uses(doc, id);
        }
        e::curve_names(doc);
        e::base_currency(doc);
        e::default_fin_calendar(doc);
        if let Some(cal) = e::project_fin_calendar(doc, pid).or(e::default_fin_calendar(doc)) {
            if let Some(period) = e::periods(doc, cal).first() {
                let mut copy = doc.clone();
                let _ = e::store_period(&mut copy, doc, pid, cal, period.id);
            }
        }
        for kind in [
            d::CodeKind::Activity,
            d::CodeKind::Project,
            d::CodeKind::Resource,
            d::CodeKind::Assignment,
        ] {
            for scope in [d::Scope::Global, d::Scope::Eps, d::Scope::Project] {
                for t in d::code_types(doc, kind, scope, pid) {
                    d::code_values(doc, kind, t.id);
                    d::code_type_usage(doc, kind, t.id);
                }
            }
        }
        for table in ["TASK", "PROJWBS", "PROJECT", "RSRC", "TASKRSRC"] {
            d::udfs(doc, table);
        }
        for a in d::accounts(doc).iter().take(3) {
            d::account_description(doc, a.id);
        }
        for c in d::Category::ALL {
            d::categories(doc, c);
        }
        d::eps_id(doc, pid);
        for t in crate::dictionaries::steps::templates(doc).iter().take(3) {
            crate::dictionaries::steps::template_steps(doc, t.id);
            let mut copy = doc.clone();
            crate::dictionaries::steps::apply_template(&mut copy, t.id, 1, pid);
        }
        resources::resources(p);
        resources::rates(p, 1);
        resources::role_names(p);
        resource_edit::rate_rows(doc, 1);
        resource_edit::code_values(doc);
        openxer_xer::edit::resources::assignment_count(doc, 1);
        crate::views::resource_merge::candidates(p, 1);
        documents::outline(&documents::documents(p));
        documents::assignments(doc, pid, 1);
        risks::risks(p);
        issues::issues(p);
        thresholds::parameters(doc);
        thresholds::thresholds(doc, pid);
        expenses::expenses(p);
        let snapshot = RecordSnapshot::capture(doc);
        let mut copy = doc.clone();
        snapshot.restore(&mut copy);
        assert_eq!(RecordSnapshot::capture(&copy), snapshot);
    }

    fn import_bytes(data: &[u8]) -> Option<ProjectData> {
        let doc = Document::from_bytes(data).ok()?;
        let pid = doc.projects().first()?.0;
        ProjectData::from_document(doc, pid, "hostile".into()).ok()
    }

    #[test]
    fn detail_table_readers_survive_hostile_rows_and_deep_chains() {
        // Processor time, so a busy machine cannot fail the bound.
        let started = crate::util::CpuTimer::start();
        let data = hostile(30, 3_000);
        let p = import_bytes(&data).expect("hostile detail tables do not stop an import");
        read_everything(&p);
        // Unedited, every table is written back as read (short rows come
        // back padded with blanks, as the reader fills them).
        let parsed = Document::from_bytes(&data).unwrap();
        assert!(!parsed.warnings.is_empty(), "short rows are reported");
        assert_eq!(
            p.updated_document().to_bytes().unwrap(),
            parsed.to_bytes().unwrap()
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "readers took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn mutation_fuzz_detail_table_readers_never_panic() {
        let base = hostile(5, 5);
        let mut seed: u64 = 0x6b31_f1de_1171;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        const TOKENS: &[&[u8]] = &[
            b"\t",
            b"\r\n",
            b"\r",
            b"%R\t",
            b"%F\t",
            b"0",
            b"-1",
            b"99999999999999999999",
            b"1e308",
            b"NaN",
            b"9999-12-31 23:59",
            b"\xff",
            b"\x00",
            b"Y",
        ];
        let mut panics = vec![];
        for i in 0..400 {
            let mut data = base.clone();
            for _ in 0..1 + next() % 4 {
                let at = next() % (data.len() + 1);
                match next() % 5 {
                    0 => {
                        let end = (at + next() % 64).min(data.len());
                        data.drain(at..end);
                    }
                    1 => {
                        let token = TOKENS[next() % TOKENS.len()];
                        data.splice(at..at, token.iter().copied());
                    }
                    2 if at < data.len() => data[at] = next() as u8,
                    3 if at < data.len() && data[at].is_ascii_digit() => {
                        data[at] = b'0' + (next() % 10) as u8;
                    }
                    _ => {
                        // Duplicate a line.
                        let start = data[..at]
                            .iter()
                            .rposition(|&b| b == b'\n')
                            .map_or(0, |k| k + 1);
                        let end = data[at..]
                            .iter()
                            .position(|&b| b == b'\n')
                            .map_or(data.len(), |k| at + k + 1);
                        let line = data[start..end].to_vec();
                        data.splice(end..end, line);
                    }
                }
            }
            let run = std::panic::catch_unwind(|| {
                if let Some(p) = import_bytes(&data) {
                    read_everything(&p);
                }
            });
            if run.is_err() {
                let path = std::env::temp_dir().join(format!("openxer-fuzz-records-{i}.xer"));
                let _ = std::fs::write(&path, &data);
                panics.push(format!("iteration {i}: saved to {}", path.display()));
            }
        }
        assert!(panics.is_empty(), "{}", panics.join("\n"));
    }

    /// Every corpus project saved to a real workspace database, reloaded,
    /// exported and reimported keeps every table, column and row, and its undo
    /// snapshot. The corpus is client data outside the repository; set
    /// `OPENXER_CORPUS` to its folder.
    #[test]
    fn corpus_round_trips_through_the_workspace_database() {
        let dir = crate::test_corpus::dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            println!(
                "SKIPPED: XER corpus not found at {} (set OPENXER_CORPUS)",
                dir.display()
            );
            return;
        };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
            .collect();
        files.sort();
        let path =
            std::env::temp_dir().join(format!("openxer-k1-corpus-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        // Whole project text per project (schema 1); a workspace with a
        // shared dictionary is checked by workspace_scope_preserves_schedules.
        let mut store = crate::store::Store::open_version_1(path.clone());
        let mut failures = vec![];
        let mut checked = 0;
        for file in &files {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let document = Document::read(file).unwrap();
            for (pid, _) in document.projects() {
                let Ok(p) = ProjectData::from_document(document.clone(), pid, name.clone()) else {
                    // A generated file the importer rejects (see xer/tests/corpus_roundtrip.rs).
                    println!("SKIPPED {name} {pid}: not importable");
                    continue;
                };
                checked += 1;
                let id = store.save(&p).unwrap();
                let back = store.load(id).unwrap();
                let bytes = back.updated_document().to_bytes().unwrap();
                let reread = Document::from_bytes(&bytes).unwrap();
                let mut problems = vec![];
                for t in &document.tables {
                    match reread.table(&t.name) {
                        Some(r) if r.fields == t.fields && r.rows == t.rows => {}
                        Some(_) => problems.push(format!("{} differs", t.name)),
                        None => problems.push(format!("{} lost", t.name)),
                    }
                }
                if reread.tables.len() != document.tables.len() {
                    problems.push("table count differs".into());
                }
                if RecordSnapshot::capture(&reread) != RecordSnapshot::capture(&document) {
                    problems.push("undo snapshot differs".into());
                }
                if back.model != p.model || back.details != p.details {
                    problems.push("project differs after reload".into());
                }
                if !problems.is_empty() {
                    failures.push(format!("{name} {pid}: {}", problems.join(", ")));
                }
            }
        }
        drop(store);
        let _ = std::fs::remove_file(&path);
        println!("Workspace database round trip of {checked} corpus projects");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
