//! Project dates set outside a schedule run (Project Details, Dates tab, and
//! `set_project` in the command line and MCP).
use crate::ProjectData;

/// Set the project's data date (minutes from the model origin), the date
/// progress is measured up to. Nothing is scheduled: activity dates stay as
/// they are until the next schedule run, which starts from it. Returns
/// whether the date changed.
pub fn set_data_date(p: &mut ProjectData, value: Option<i64>) -> Result<bool, String> {
    let value = value.ok_or("The project needs a data date.")?;
    if p.model.project.data_date == Some(value) {
        return Ok(false);
    }
    p.model.project.data_date = Some(value);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_date_changes_and_nothing_is_scheduled() {
        let mut p = crate::starter();
        let dates: Vec<_> = p
            .model
            .activities
            .iter()
            .map(|a| (a.early_start, a.early_finish))
            .collect();
        let new = p.model.project.data_date.unwrap() + 5 * 24 * 60;
        assert_eq!(set_data_date(&mut p, Some(new)), Ok(true));
        assert_eq!(p.model.project.data_date, Some(new));
        assert_eq!(set_data_date(&mut p, Some(new)), Ok(false), "unchanged");
        assert!(
            set_data_date(&mut p, None).is_err(),
            "a data date is needed"
        );
        let after: Vec<_> = p
            .model
            .activities
            .iter()
            .map(|a| (a.early_start, a.early_finish))
            .collect();
        assert_eq!(after, dates, "no activity moved");
        // It is what the file stores and reads back.
        let text = p.updated_document().text();
        let back = ProjectData::from_document(
            crate::Document::parse(&text).unwrap(),
            p.model.project.proj_id,
            "rt".into(),
        )
        .unwrap();
        // Minutes count from each model's own origin: compare the dates.
        let absolute = |q: &ProjectData| {
            q.model
                .project
                .data_date
                .map(|d| d + q.model.origin_serial * 24 * 60)
        };
        assert_eq!(absolute(&back), absolute(&p));
    }
}
