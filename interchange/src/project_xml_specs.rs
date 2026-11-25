//! The element types of the project XML format and how each child maps to
//! an XER column. Every list is in the order the reference writes the
//! children (observed in a file it exported): plain fields first, in
//! ordinal (case sensitive ASCII) order, then nested elements in a fixed
//! order the writer adds. Element types the reference did not write in the
//! observed file (cost accounts, expense categories, units of measure,
//! roles and their rates, resource rates and roles, baselines) follow the
//! same ordering rule, and their enumerated labels are not observed.
//!
//! A column with a `.` in its name is not a column of the element's own
//! row: the writer looks it up elsewhere (`wbs.` is the project's root WBS
//! row, `task.` the row's activity, `project.` the row's project, `=`
//! a value the writer works out) and the reader handles it in its own code.

/// How a value is written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    /// Text as stored.
    Text,
    /// A number or id, trimmed, whole values without a decimal point.
    Num,
    /// `YYYY-MM-DD HH:MM` in XER, `YYYY-MM-DDTHH:MM:SS` in XML.
    Date,
    /// `Y` / `N` in XER, `1` / `0` in XML.
    Flag,
    /// 0 to 100 in XER, a fraction 0 to 1 in XML.
    Pct,
    /// `RRGGBB` in XER, `#RRGGBB` upper case in XML.
    Color,
    /// An XER code and its XML label.
    Choice(&'static [(&'static str, &'static str)]),
}

/// What the writer does with a field that has no value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Empty {
    /// Leave the element out.
    Omit,
    /// `<Name xsi:nil="true" />`.
    Nil,
    /// `<Name />`.
    Blank,
}

pub(super) struct F {
    pub xer: &'static str,
    pub xml: &'static str,
    pub kind: Kind,
    pub empty: Empty,
}

/// A field left out when it has no value.
const fn f(xer: &'static str, xml: &'static str, kind: Kind) -> F {
    F {
        xer,
        xml,
        kind,
        empty: Empty::Omit,
    }
}
/// A field written as nil when it has no value.
const fn n(xer: &'static str, xml: &'static str, kind: Kind) -> F {
    F {
        xer,
        xml,
        kind,
        empty: Empty::Nil,
    }
}
/// A field written empty when it has no value.
const fn b(xer: &'static str, xml: &'static str, kind: Kind) -> F {
    F {
        xer,
        xml,
        kind,
        empty: Empty::Blank,
    }
}

pub(super) struct Spec {
    pub table: &'static str,
    pub element: &'static str,
    /// The row's own id column (written as `ObjectId`), if it has one.
    pub key: Option<&'static str>,
    /// Plain fields, in the order they are written.
    pub fields: &'static [F],
    /// Nested element names, in the order they are written.
    pub nested: &'static [&'static str],
}

use Kind::{Color, Date, Flag, Num, Pct, Text};

// ------------------------------------------------------------ labels
// In each list the labels the observed file held come first, as the
// comment above it says; the others are not observed.

/// Seen: the first four.
pub(super) const TASK_TYPE: &[(&str, &str)] = &[
    ("TT_Task", "Task Dependent"),
    ("TT_LOE", "Level of Effort"),
    ("TT_Mile", "Start Milestone"),
    ("TT_FinMile", "Finish Milestone"),
    ("TT_Rsrc", "Resource Dependent"),
    ("TT_WBS", "WBS Summary"),
];
/// Seen: all three.
pub(super) const TASK_STATUS: &[(&str, &str)] = &[
    ("TK_NotStart", "Not Started"),
    ("TK_Active", "In Progress"),
    ("TK_Complete", "Completed"),
];
/// Seen: the first two.
pub(super) const DURATION_TYPE: &[(&str, &str)] = &[
    ("DT_FixedDUR2", "Fixed Duration and Units"),
    ("DT_FixedDrtn", "Fixed Duration and Units/Time"),
    ("DT_FixedRate", "Fixed Units/Time"),
    ("DT_FixedQty", "Fixed Units"),
];
/// Seen: the first.
pub(super) const PERCENT_TYPE: &[(&str, &str)] = &[
    ("CP_Phys", "Physical"),
    ("CP_Drtn", "Duration"),
    ("CP_Units", "Units"),
];
/// Seen: the first.
pub(super) const CONSTRAINT: &[(&str, &str)] = &[
    ("CS_MEOB", "Finish On or Before"),
    ("CS_MSO", "Start On"),
    ("CS_MSOB", "Start On or Before"),
    ("CS_MSOA", "Start On or After"),
    ("CS_MEO", "Finish On"),
    ("CS_MEOA", "Finish On or After"),
    ("CS_ALAP", "As Late As Possible"),
    ("CS_MANDSTART", "Mandatory Start"),
    ("CS_MANDFIN", "Mandatory Finish"),
];
/// Seen: the first.
pub(super) const PRIORITY: &[(&str, &str)] = &[
    ("PT_Normal", "Normal"),
    ("PT_Top", "Top"),
    ("PT_High", "High"),
    ("PT_Low", "Low"),
    ("PT_Lowest", "Lowest"),
];
/// Seen: all four.
pub(super) const LINK_TYPE: &[(&str, &str)] = &[
    ("PR_FS", "Finish to Start"),
    ("PR_SS", "Start to Start"),
    ("PR_FF", "Finish to Finish"),
    ("PR_SF", "Start to Finish"),
];
/// Seen: the first two.
pub(super) const RESOURCE_TYPE: &[(&str, &str)] = &[
    ("RT_Labor", "Labor"),
    ("RT_Equip", "Nonlabor"),
    ("RT_Mat", "Material"),
];
/// Seen: the first two.
pub(super) const CALENDAR_TYPE: &[(&str, &str)] = &[
    ("CA_Base", "Global"),
    ("CA_Project", "Project"),
    ("CA_Rsrc", "Resource"),
];
/// Seen: the first.
pub(super) const WBS_STATUS: &[(&str, &str)] = &[
    ("WS_Open", "Active"),
    ("WS_Planned", "Planned"),
    ("WS_Inactive", "Inactive"),
    ("WS_Whatif", "What-If"),
    ("WS_Requested", "Requested"),
];
/// Seen: the first.
pub(super) const CODE_SCOPE: &[(&str, &str)] = &[
    ("AS_Project", "Project"),
    ("AS_Global", "Global"),
    ("AS_EPS", "EPS"),
];
/// Seen: the first.
pub(super) const CRITICAL_PATH: &[(&str, &str)] = &[
    ("CT_TotFloat", "Critical Float"),
    ("CT_DrivPath", "Longest Path"),
];
/// Seen: the first.
pub(super) const ACCRUAL: &[(&str, &str)] = &[
    ("CL_Uniform", "Uniform Over Activity"),
    ("CL_Start", "Start of Activity"),
    ("CL_End", "End of Activity"),
];
/// Seen: the first.
pub(super) const UDF_SUBJECT: &[(&str, &str)] = &[
    ("TASK", "Activity"),
    ("PROJWBS", "WBS"),
    ("PROJECT", "Project"),
    ("RSRC", "Resource"),
    ("TASKRSRC", "Resource Assignment"),
    ("PROJCOST", "Activity Expense"),
];
/// Seen: the first two.
pub(super) const UDF_DATA_TYPE: &[(&str, &str)] = &[
    ("FT_TEXT", "Text"),
    ("FT_MONEY", "Cost"),
    ("FT_START_DATE", "Start Date"),
    ("FT_END_DATE", "Finish Date"),
    ("FT_FLOAT_2_DECIMALS", "Double"),
    ("FT_INT", "Integer"),
    ("FT_STATICTYPE", "Indicator"),
];
/// Seen: the first.
pub(super) const PROFICIENCY: &[(&str, &str)] = &[
    ("3", "3 - Skilled"),
    ("1", "1 - Master"),
    ("2", "2 - Expert"),
    ("4", "4 - Proficient"),
    ("5", "5 - Inexperienced"),
];
/// Seen: the first.
pub(super) const RATE_TYPE: &[(&str, &str)] = &[
    ("COST_PER_QTY", "Price / Unit"),
    ("COST_PER_QTY2", "Price / Unit 2"),
    ("COST_PER_QTY3", "Price / Unit 3"),
    ("COST_PER_QTY4", "Price / Unit 4"),
    ("COST_PER_QTY5", "Price / Unit 5"),
];
/// Seen: the first.
pub(super) const RATE_SOURCE: &[(&str, &str)] = &[
    ("ST_Rsrc", "Resource"),
    ("ST_Role", "Role"),
    ("ST_Custom", "Override"),
];
/// Seen: the first.
pub(super) const PRICE_TIME_UNITS: &[(&str, &str)] = &[
    ("QT_Day", "Day"),
    ("QT_Hour", "Hour"),
    ("QT_Week", "Week"),
    ("QT_Month", "Month"),
    ("QT_Year", "Year"),
];
/// Seen: all.
pub(super) const SUMMARY_LEVEL: &[(&str, &str)] = &[("SL_Taskrsrc", "Assignment Level")];
/// Seen: all.
pub(super) const EV_COMPUTE: &[(&str, &str)] = &[("EC_Cmp_pct", "Activity Percent Complete")];
/// Seen: all.
pub(super) const EV_ETC: &[(&str, &str)] = &[("EE_Rem_hr", "ETC = Remaining Cost for Activity")];
/// Seen: both.
pub(super) const SYMBOL: &[(&str, &str)] = &[(".", "Period"), (",", "Comma")];
/// Seen: the first.
pub(super) const FLOAT_TYPE: &[(&str, &str)] = &[
    ("FT_Min", "Smallest of Start Float and Finish Float"),
    ("FT_SS", "Start Float"),
    ("FT_FF", "Finish Float"),
];
/// Seen: the first.
pub(super) const LAG_CALENDAR: &[(&str, &str)] = &[
    ("rcal_Predecessor", "Predecessor Activity Calendar"),
    ("rcal_Successor", "Successor Activity Calendar"),
    ("rcal_24Hour", "24 Hour Calendar"),
    ("rcal_ProjDefault", "Project Default Calendar"),
];
/// Seen: `1`. Which XER value it pairs with is not observed.
pub(super) const IGNORE_OTHER_PROJECTS: &[(&str, &str)] = &[("SD_None", "1"), ("SD_Both", "0")];
/// Out of sequence progress, from two XER flags (the writer's `=oos`).
/// Seen: the first.
pub(super) const OUT_OF_SEQUENCE: &[(&str, &str)] = &[
    ("retained", "Retained Logic"),
    ("override", "Progress Override"),
    ("actual", "Actual Dates"),
];
pub(super) const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

// ------------------------------------------------------------ root types

pub(super) const CURRENCY: Spec = Spec {
    table: "CURRTYPE",
    element: "Currency",
    key: Some("curr_id"),
    fields: &[
        f("decimal_digit_cnt", "DecimalPlaces", Num),
        f("decimal_symbol", "DecimalSymbol", Kind::Choice(SYMBOL)),
        f(
            "digit_group_symbol",
            "DigitGroupingSymbol",
            Kind::Choice(SYMBOL),
        ),
        f("base_exch_rate", "ExchangeRate", Num),
        f("curr_short_name", "Id", Text),
        f("curr_type", "Name", Text),
        f("neg_curr_fmt_type", "NegativeSymbol", Text),
        f("curr_id", "ObjectId", Num),
        f("pos_curr_fmt_type", "PositiveSymbol", Text),
        f("curr_symbol", "Symbol", Text),
    ],
    nested: &[],
};
pub(super) const UDF_TYPE: Spec = Spec {
    table: "UDFTYPE",
    element: "UDFType",
    key: Some("udf_type_id"),
    fields: &[
        f("logical_data_type", "DataType", Kind::Choice(UDF_DATA_TYPE)),
        f("udf_type_id", "ObjectId", Num),
        f("table_name", "SubjectArea", Kind::Choice(UDF_SUBJECT)),
        f("udf_type_label", "Title", Text),
    ],
    nested: &[],
};
pub(super) const NOTEBOOK_TOPIC: Spec = Spec {
    table: "MEMOTYPE",
    element: "NotebookTopic",
    key: Some("memo_type_id"),
    fields: &[
        f("task_flag", "AvailableForActivity", Flag),
        f("eps_flag", "AvailableForEPS", Flag),
        f("proj_flag", "AvailableForProject", Flag),
        f("wbs_flag", "AvailableForWBS", Flag),
        f("memo_type", "Name", Text),
        f("memo_type_id", "ObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const OBS: Spec = Spec {
    table: "OBS",
    element: "OBS",
    key: Some("obs_id"),
    fields: &[
        f("obs_descr", "Description", Text),
        f("obs_name", "Name", Text),
        f("obs_id", "ObjectId", Num),
        n("parent_obs_id", "ParentObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const CALENDAR: Spec = Spec {
    table: "CALENDAR",
    element: "Calendar",
    key: Some("clndr_id"),
    fields: &[
        n("base_clndr_id", "BaseCalendarObjectId", Num),
        f("day_hr_cnt", "HoursPerDay", Num),
        f("month_hr_cnt", "HoursPerMonth", Num),
        f("week_hr_cnt", "HoursPerWeek", Num),
        f("year_hr_cnt", "HoursPerYear", Num),
        f("default_flag", "IsDefault", Flag),
        f("rsrc_private", "IsPersonal", Flag),
        f("clndr_name", "Name", Text),
        f("clndr_id", "ObjectId", Num),
        n("proj_id", "ProjectObjectId", Num),
        f("clndr_type", "Type", Kind::Choice(CALENDAR_TYPE)),
    ],
    nested: &["StandardWorkWeek", "HolidayOrExceptions"],
};
pub(super) const RESOURCE_CODE_TYPE: Spec = Spec {
    table: "RCATTYPE",
    element: "ResourceCodeType",
    key: Some("rsrc_catg_type_id"),
    fields: &[
        f("rsrc_catg_short_len", "Length", Num),
        f("rsrc_catg_type", "Name", Text),
        f("rsrc_catg_type_id", "ObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const RESOURCE_CODE: Spec = Spec {
    table: "RCATVAL",
    element: "ResourceCode",
    key: Some("rsrc_catg_id"),
    fields: &[
        f("rsrc_catg_type_id", "CodeTypeObjectId", Num),
        f("rsrc_catg_short_name", "CodeValue", Text),
        f("rsrc_catg_name", "Description", Text),
        f("rsrc_catg_id", "ObjectId", Num),
        n("parent_rsrc_catg_id", "ParentObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const RESOURCE: Spec = Spec {
    table: "RSRC",
    element: "Resource",
    key: Some("rsrc_id"),
    fields: &[
        f("auto_compute_act_flag", "AutoComputeActuals", Flag),
        f("def_cost_qty_link_flag", "CalculateCostFromUnits", Flag),
        f("clndr_id", "CalendarObjectId", Num),
        f("curr_id", "CurrencyObjectId", Num),
        f("def_qty_per_hr", "DefaultUnitsPerTime", Num),
        n("email_addr", "EmailAddress", Text),
        n("employee_code", "EmployeeId", Text),
        f("rsrc_short_name", "Id", Text),
        f("active_flag", "IsActive", Flag),
        f("ot_flag", "IsOverTimeAllowed", Flag),
        f("rsrc_name", "Name", Text),
        f("rsrc_id", "ObjectId", Num),
        n("office_phone", "OfficePhone", Text),
        n("other_phone", "OtherPhone", Text),
        f("ot_factor", "OvertimeFactor", Num),
        n("parent_rsrc_id", "ParentObjectId", Num),
        n("role_id", "PrimaryRoleObjectId", Num),
        n("rsrc_notes", "ResourceNotes", Text),
        f("rsrc_type", "ResourceType", Kind::Choice(RESOURCE_TYPE)),
        f("rsrc_seq_num", "SequenceNumber", Num),
        n("shift_id", "ShiftObjectId", Num),
        n("rsrc_title_name", "Title", Text),
        n("unit_id", "UnitOfMeasureObjectId", Num),
    ],
    // `UDF` is not observed on a resource.
    nested: &["Code", "UDF"],
};
pub(super) const FIN_TEMPLATE: Spec = Spec {
    table: "FINTMPL",
    element: "FinancialPeriodTemplate",
    key: Some("fintmpl_id"),
    fields: &[
        f("fintmpl_name", "FinancialPeriodTemplateName", Text),
        f("fintmpl_id", "ObjectId", Num),
    ],
    nested: &[],
};

// Root types not observed.

pub(super) const ACCOUNT: Spec = Spec {
    table: "ACCOUNT",
    element: "CostAccount",
    key: Some("acct_id"),
    fields: &[
        f("acct_descr", "Description", Text),
        f("acct_short_name", "Id", Text),
        f("acct_name", "Name", Text),
        f("acct_id", "ObjectId", Num),
        n("parent_acct_id", "ParentObjectId", Num),
        f("acct_seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const COST_TYPE: Spec = Spec {
    table: "COSTTYPE",
    element: "ExpenseCategory",
    key: Some("cost_type_id"),
    fields: &[
        f("cost_type", "Name", Text),
        f("cost_type_id", "ObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const UNIT: Spec = Spec {
    table: "UMEASURE",
    element: "UnitOfMeasure",
    key: Some("unit_id"),
    fields: &[
        f("unit_abbrev", "Abbreviation", Text),
        f("unit_name", "Name", Text),
        f("unit_id", "ObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const ROLE: Spec = Spec {
    table: "ROLES",
    element: "Role",
    key: Some("role_id"),
    fields: &[
        f("def_cost_qty_link_flag", "CalculateCostFromUnits", Flag),
        f("role_short_name", "Id", Text),
        f("role_name", "Name", Text),
        f("role_id", "ObjectId", Num),
        n("parent_role_id", "ParentObjectId", Num),
        f("role_descr", "Responsibilities", Text),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const ROLE_RATE: Spec = Spec {
    table: "ROLERATE",
    element: "RoleRate",
    key: Some("role_rate_id"),
    fields: &[
        f("start_date", "EffectiveDate", Date),
        f("max_qty_per_hr", "MaxUnitsPerTime", Num),
        f("role_rate_id", "ObjectId", Num),
        f("cost_per_qty", "PricePerUnit", Num),
        f("cost_per_qty2", "PricePerUnit2", Num),
        f("cost_per_qty3", "PricePerUnit3", Num),
        f("cost_per_qty4", "PricePerUnit4", Num),
        f("cost_per_qty5", "PricePerUnit5", Num),
        f("role_id", "RoleObjectId", Num),
    ],
    nested: &[],
};
pub(super) const RESOURCE_RATE: Spec = Spec {
    table: "RSRCRATE",
    element: "ResourceRate",
    key: Some("rsrc_rate_id"),
    fields: &[
        f("start_date", "EffectiveDate", Date),
        f("max_qty_per_hr", "MaxUnitsPerTime", Num),
        f("rsrc_rate_id", "ObjectId", Num),
        f("cost_per_qty", "PricePerUnit", Num),
        f("cost_per_qty2", "PricePerUnit2", Num),
        f("cost_per_qty3", "PricePerUnit3", Num),
        f("cost_per_qty4", "PricePerUnit4", Num),
        f("cost_per_qty5", "PricePerUnit5", Num),
        f("rsrc_id", "ResourceObjectId", Num),
    ],
    nested: &[],
};
pub(super) const RESOURCE_ROLE: Spec = Spec {
    table: "RSRCROLE",
    element: "ResourceRole",
    key: None,
    fields: &[
        f("skill_level", "Proficiency", Kind::Choice(PROFICIENCY)),
        f("rsrc_id", "ResourceObjectId", Num),
        f("role_id", "RoleObjectId", Num),
    ],
    nested: &[],
};

// ------------------------------------------------------------ project types

/// The project: PROJECT columns, and `wbs.` columns of its root WBS row.
pub(super) const PROJECT: Spec = Spec {
    table: "PROJECT",
    element: "Project",
    key: Some("proj_id"),
    fields: &[
        f(
            "def_task_type",
            "ActivityDefaultActivityType",
            Kind::Choice(TASK_TYPE),
        ),
        f("clndr_id", "ActivityDefaultCalendarObjectId", Num),
        n("acct_id", "ActivityDefaultCostAccountObjectId", Num),
        f(
            "def_duration_type",
            "ActivityDefaultDurationType",
            Kind::Choice(DURATION_TYPE),
        ),
        f(
            "def_complete_pct_type",
            "ActivityDefaultPercentCompleteType",
            Kind::Choice(PERCENT_TYPE),
        ),
        f("def_cost_per_qty", "ActivityDefaultPricePerUnit", Num),
        f(
            "task_code_prefix_flag",
            "ActivityIdBasedOnSelectedActivity",
            Flag,
        ),
        f("task_code_step", "ActivityIdIncrement", Num),
        f("task_code_prefix", "ActivityIdPrefix", Text),
        f("task_code_base", "ActivityIdSuffix", Num),
        f(
            "step_complete_flag",
            "ActivityPercentCompleteBasedOnActivitySteps",
            Flag,
        ),
        f("add_act_remain_flag", "AddActualToRemaining", Flag),
        f("add_by_name", "AddedBy", Text),
        f("allow_neg_act_flag", "AllowNegativeActualUnitsFlag", Flag),
        n("wbs.anticip_end_date", "AnticipatedFinishDate", Date),
        n("wbs.anticip_start_date", "AnticipatedStartDate", Date),
        f(
            "def_rollup_dates_flag",
            "AssignmentDefaultDrivingFlag",
            Flag,
        ),
        f(
            "def_rate_type",
            "AssignmentDefaultRateType",
            Kind::Choice(RATE_TYPE),
        ),
        f("checkout_flag", "CheckOutStatus", Flag),
        f("cost_qty_recalc_flag", "CostQuantityRecalculateFlag", Flag),
        f("critical_drtn_hr_cnt", "CriticalActivityFloatLimit", Num),
        f(
            "critical_path_type",
            "CriticalActivityPathType",
            Kind::Choice(CRITICAL_PATH),
        ),
        f("last_recalc_date", "DataDate", Date),
        f("add_date", "DateAdded", Date),
        f(
            "def_qty_type",
            "DefaultPriceTimeUnits",
            Kind::Choice(PRICE_TIME_UNITS),
        ),
        f(
            "wbs.ev_compute_type",
            "EarnedValueComputeType",
            Kind::Choice(EV_COMPUTE),
        ),
        f(
            "wbs.ev_etc_compute_type",
            "EarnedValueETCComputeType",
            Kind::Choice(EV_ETC),
        ),
        f("wbs.ev_etc_user_value", "EarnedValueETCUserValue", Num),
        f("wbs.ev_user_pct", "EarnedValueUserPercent", Pct),
        f("fy_start_month_num", "FiscalYearStartMonth", Num),
        f("proj_short_name", "Id", Text),
        f("wbs.indep_remain_work_qty", "IndependentETCLaborUnits", Num),
        f(
            "wbs.indep_remain_total_cost",
            "IndependentETCTotalCost",
            Num,
        ),
        n("last_fin_dates_id", "LastFinancialPeriodObjectId", Num),
        f("priority_num", "LevelingPriority", Num),
        f(
            "act_this_per_link_flag",
            "LinkActualToActualThisPeriod",
            Flag,
        ),
        f("act_pct_link_flag", "LinkPercentCompleteWithActual", Flag),
        f(
            "rem_target_link_flag",
            "LinkPlannedAndAtCompletionFlag",
            Flag,
        ),
        f("plan_end_date", "MustFinishByDate", Date),
        f("wbs.wbs_name", "Name", Text),
        f("wbs.obs_id", "OBSObjectId", Num),
        f("proj_id", "ObjectId", Num),
        f("wbs.orig_cost", "OriginalBudget", Num),
        // Written for a baseline only (not observed).
        f("=baseline_of", "OriginalProjectObjectId", Num),
        // Written only: the reader does not place the project in a folder.
        f("wbs.parent_wbs_id", "ParentEPSObjectId", Num),
        f("plan_start_date", "PlannedStartDate", Date),
        n("fcst_start_date", "ProjectForecastStartDate", Date),
        f("reset_planned_flag", "ResetPlannedToRemainingFlag", Flag),
        f(
            "rsrc_multi_assign_flag",
            "ResourceCanBeAssignedToSameActivityMoreThanOnce",
            Flag,
        ),
        f(
            "rsrc_self_add_flag",
            "ResourcesCanAssignThemselvesToActivities",
            Flag,
        ),
        f("scd_end_date", "ScheduledFinishDate", Date),
        f("wbs.status_code", "Status", Kind::Choice(WBS_STATUS)),
        f("strgy_priority_num", "StrategicPriority", Num),
        f("wbs_max_sum_level", "SummarizeToWBSLevel", Num),
        f(
            "sum_assign_level",
            "SummaryLevel",
            Kind::Choice(SUMMARY_LEVEL),
        ),
        f(
            "use_project_baseline_flag",
            "UseProjectBaselineForEarnedValue",
            Flag,
        ),
        f("name_sep_char", "WBSCodeSeparator", Text),
        f("wbs.wbs_id", "WBSObjectId", Num),
        n("web_local_root_path", "WebSiteRootDirectory", Text),
        n("proj_url", "WebSiteURL", Text),
    ],
    // `UDF` on a project is not observed.
    nested: &[
        "Calendar",
        "WBS",
        "ProjectNote",
        "ActivityCodeType",
        "ActivityCode",
        "Activity",
        "ResourceAssignment",
        "ActivityExpense",
        "ActivityNote",
        "Relationship",
        "ScheduleOptions",
        "UDF",
    ],
};
pub(super) const WBS: Spec = Spec {
    table: "PROJWBS",
    element: "WBS",
    key: Some("wbs_id"),
    fields: &[
        n("anticip_end_date", "AnticipatedFinishDate", Date),
        n("anticip_start_date", "AnticipatedStartDate", Date),
        f("wbs_short_name", "Code", Text),
        f(
            "ev_compute_type",
            "EarnedValueComputeType",
            Kind::Choice(EV_COMPUTE),
        ),
        f(
            "ev_etc_compute_type",
            "EarnedValueETCComputeType",
            Kind::Choice(EV_ETC),
        ),
        f("ev_etc_user_value", "EarnedValueETCUserValue", Num),
        f("ev_user_pct", "EarnedValueUserPercent", Pct),
        f("indep_remain_work_qty", "IndependentETCLaborUnits", Num),
        f("indep_remain_total_cost", "IndependentETCTotalCost", Num),
        f("wbs_name", "Name", Text),
        f("obs_id", "OBSObjectId", Num),
        f("wbs_id", "ObjectId", Num),
        f("orig_cost", "OriginalBudget", Num),
        n("parent_wbs_id", "ParentObjectId", Num),
        f("proj_id", "ProjectObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
        f("status_code", "Status", Kind::Choice(WBS_STATUS)),
        n("phase_id", "WBSCategoryObjectId", Num),
    ],
    // `UDF` on a WBS element is not observed.
    nested: &["UDF"],
};
/// A project or WBS notebook entry; `WBSObjectId` is nil for the project's own.
pub(super) const PROJECT_NOTE: Spec = Spec {
    table: "WBSMEMO",
    element: "ProjectNote",
    key: Some("wbs_memo_id"),
    fields: &[
        f("wbs_memo", "Note", Text),
        f("memo_type_id", "NotebookTopicObjectId", Num),
        f("wbs_memo_id", "ObjectId", Num),
        f("proj_id", "ProjectObjectId", Num),
        n("wbs_id", "WBSObjectId", Num),
    ],
    nested: &[],
};
pub(super) const CODE_TYPE: Spec = Spec {
    table: "ACTVTYPE",
    element: "ActivityCodeType",
    key: Some("actv_code_type_id"),
    fields: &[
        n("wbs_id", "EPSObjectId", Num),
        f("actv_short_len", "Length", Num),
        f("actv_code_type", "Name", Text),
        f("actv_code_type_id", "ObjectId", Num),
        n("proj_id", "ProjectObjectId", Num),
        f("=proj_id", "RefProjectObjectIds", Num),
        f("actv_code_type_scope", "Scope", Kind::Choice(CODE_SCOPE)),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const CODE: Spec = Spec {
    table: "ACTVCODE",
    element: "ActivityCode",
    key: Some("actv_code_id"),
    fields: &[
        f("actv_code_type_id", "CodeTypeObjectId", Num),
        f("short_name", "CodeValue", Text),
        f("color", "Color", Color),
        f("actv_code_name", "Description", Text),
        f("actv_code_id", "ObjectId", Num),
        n("parent_actv_code_id", "ParentObjectId", Num),
        f("=code_project", "ProjectObjectId", Num),
        f("seq_num", "SequenceNumber", Num),
    ],
    nested: &[],
};
pub(super) const ACTIVITY: Spec = Spec {
    table: "TASK",
    element: "Activity",
    key: Some("task_id"),
    fields: &[
        n("act_end_date", "ActualFinishDate", Date),
        f("act_work_qty", "ActualLaborUnits", Num),
        f("act_equip_qty", "ActualNonLaborUnits", Num),
        n("act_start_date", "ActualStartDate", Date),
        f("act_this_per_work_qty", "ActualThisPeriodLaborUnits", Num),
        f(
            "act_this_per_equip_qty",
            "ActualThisPeriodNonLaborUnits",
            Num,
        ),
        f("auto_compute_act_flag", "AutoComputeActuals", Flag),
        f("clndr_id", "CalendarObjectId", Num),
        f("duration_type", "DurationType", Kind::Choice(DURATION_TYPE)),
        f("est_wt", "EstimatedWeight", Num),
        n("expect_end_date", "ExpectedFinishDate", Date),
        n("external_early_start_date", "ExternalEarlyStartDate", Date),
        n("external_late_end_date", "ExternalLateFinishDate", Date),
        f("=finish", "FinishDate", Date),
        f("task_code", "Id", Text),
        f("priority_type", "LevelingPriority", Kind::Choice(PRIORITY)),
        f("task_name", "Name", Text),
        f("task_id", "ObjectId", Num),
        f("=percent", "PercentComplete", Pct),
        f(
            "complete_pct_type",
            "PercentCompleteType",
            Kind::Choice(PERCENT_TYPE),
        ),
        f("phys_complete_pct", "PhysicalPercentComplete", Pct),
        f("target_drtn_hr_cnt", "PlannedDuration", Num),
        f("target_end_date", "PlannedFinishDate", Date),
        f("target_work_qty", "PlannedLaborUnits", Num),
        f("target_equip_qty", "PlannedNonLaborUnits", Num),
        f("target_start_date", "PlannedStartDate", Date),
        n("cstr_date", "PrimaryConstraintDate", Date),
        n(
            "cstr_type",
            "PrimaryConstraintType",
            Kind::Choice(CONSTRAINT),
        ),
        n("rsrc_id", "PrimaryResourceObjectId", Num),
        f("proj_id", "ProjectObjectId", Num),
        f("remain_drtn_hr_cnt", "RemainingDuration", Num),
        n("reend_date", "RemainingEarlyFinishDate", Date),
        n("restart_date", "RemainingEarlyStartDate", Date),
        f("remain_work_qty", "RemainingLaborUnits", Num),
        n("rem_late_end_date", "RemainingLateFinishDate", Date),
        n("rem_late_start_date", "RemainingLateStartDate", Date),
        f("remain_equip_qty", "RemainingNonLaborUnits", Num),
        n("resume_date", "ResumeDate", Date),
        f("rev_fdbk_flag", "ReviewRequired", Flag),
        n("cstr_date2", "SecondaryConstraintDate", Date),
        n(
            "cstr_type2",
            "SecondaryConstraintType",
            Kind::Choice(CONSTRAINT),
        ),
        f("=start", "StartDate", Date),
        f("status_code", "Status", Kind::Choice(TASK_STATUS)),
        n("suspend_date", "SuspendDate", Date),
        f("task_type", "Type", Kind::Choice(TASK_TYPE)),
        f("wbs_id", "WBSObjectId", Num),
    ],
    nested: &["Code", "UDF"],
};
pub(super) const ASSIGNMENT: Spec = Spec {
    table: "TASKRSRC",
    element: "ResourceAssignment",
    key: Some("taskrsrc_id"),
    fields: &[
        f("task_id", "ActivityObjectId", Num),
        n("act_end_date", "ActualFinishDate", Date),
        f("act_ot_cost", "ActualOvertimeCost", Num),
        f("act_ot_qty", "ActualOvertimeUnits", Num),
        f("act_reg_cost", "ActualRegularCost", Num),
        f("act_reg_qty", "ActualRegularUnits", Num),
        n("act_start_date", "ActualStartDate", Date),
        f("act_this_per_cost", "ActualThisPeriodCost", Num),
        f("act_this_per_qty", "ActualThisPeriodUnits", Num),
        n("acct_id", "CostAccountObjectId", Num),
        f("rollup_dates_flag", "DrivingActivityDatesFlag", Flag),
        f("cost_qty_link_flag", "IsCostUnitsLinked", Flag),
        f("taskrsrc_id", "ObjectId", Num),
        f("ot_factor", "OvertimeFactor", Num),
        f("target_cost", "PlannedCost", Num),
        f("target_end_date", "PlannedFinishDate", Date),
        f("target_lag_drtn_hr_cnt", "PlannedLag", Num),
        f("target_start_date", "PlannedStartDate", Date),
        f("target_qty", "PlannedUnits", Num),
        f("target_qty_per_hr", "PlannedUnitsPerTime", Num),
        f("cost_per_qty", "PricePerUnit", Num),
        f("skill_level", "Proficiency", Kind::Choice(PROFICIENCY)),
        f("proj_id", "ProjectObjectId", Num),
        f(
            "cost_per_qty_source_type",
            "RateSource",
            Kind::Choice(RATE_SOURCE),
        ),
        f("rate_type", "RateType", Kind::Choice(RATE_TYPE)),
        f("remain_cost", "RemainingCost", Num),
        n("reend_date", "RemainingFinishDate", Date),
        f("relag_drtn_hr_cnt", "RemainingLag", Num),
        n("restart_date", "RemainingStartDate", Date),
        f("remain_qty", "RemainingUnits", Num),
        f("remain_qty_per_hr", "RemainingUnitsPerTime", Num),
        n("curv_id", "ResourceCurveObjectId", Num),
        f("rsrc_id", "ResourceObjectId", Num),
        f("rsrc_type", "ResourceType", Kind::Choice(RESOURCE_TYPE)),
        n("role_id", "RoleObjectId", Num),
        f("task.wbs_id", "WBSObjectId", Num),
    ],
    // `UDF` on an assignment is not observed.
    nested: &["UDF"],
};
pub(super) const EXPENSE: Spec = Spec {
    table: "PROJCOST",
    element: "ActivityExpense",
    key: Some("cost_item_id"),
    fields: &[
        f("cost_load_type", "AccrualType", Kind::Choice(ACCRUAL)),
        f("task_id", "ActivityObjectId", Num),
        f("act_cost", "ActualCost", Num),
        f("auto_compute_act_flag", "AutoComputeActuals", Flag),
        n("acct_id", "CostAccountObjectId", Num),
        n("po_number", "DocumentNumber", Text),
        n("cost_type_id", "ExpenseCategoryObjectId", Num),
        f("cost_descr", "ExpenseDescription", Text),
        f("cost_name", "ExpenseItem", Text),
        f("cost_item_id", "ObjectId", Num),
        f("target_cost", "PlannedCost", Num),
        f("target_qty", "PlannedUnits", Num),
        f("cost_per_qty", "PricePerUnit", Num),
        f("proj_id", "ProjectObjectId", Num),
        f("remain_cost", "RemainingCost", Num),
        n("qty_name", "UnitOfMeasure", Text),
        n("vendor_name", "Vendor", Text),
        f("task.wbs_id", "WBSObjectId", Num),
    ],
    // `UDF` on an expense is not observed.
    nested: &["UDF"],
};
pub(super) const ACTIVITY_NOTE: Spec = Spec {
    table: "TASKMEMO",
    element: "ActivityNote",
    key: Some("memo_id"),
    fields: &[
        f("task_id", "ActivityObjectId", Num),
        f("task_memo", "Note", Text),
        f("memo_type_id", "NotebookTopicObjectId", Num),
        f("memo_id", "ObjectId", Num),
        f("proj_id", "ProjectObjectId", Num),
        f("task.wbs_id", "WBSObjectId", Num),
    ],
    nested: &[],
};
pub(super) const RELATIONSHIP: Spec = Spec {
    table: "TASKPRED",
    element: "Relationship",
    key: Some("task_pred_id"),
    fields: &[
        n("comments", "Comments", Text),
        f("lag_hr_cnt", "Lag", Num),
        f("task_pred_id", "ObjectId", Num),
        f("pred_task_id", "PredecessorActivityObjectId", Num),
        f("pred_proj_id", "PredecessorProjectObjectId", Num),
        f("task_id", "SuccessorActivityObjectId", Num),
        f("proj_id", "SuccessorProjectObjectId", Num),
        f("pred_type", "Type", Kind::Choice(LINK_TYPE)),
    ],
    nested: &[],
};
/// The project's scheduling and leveling options. The element has no
/// `ObjectId`; the critical settings come from the PROJECT row.
pub(super) const SCHEDULE_OPTIONS: Spec = Spec {
    table: "SCHEDOPTIONS",
    element: "ScheduleOptions",
    key: None,
    fields: &[
        f(
            "sched_use_project_end_date_for_float",
            "CalculateFloatBasedOnFinishDate",
            Flag,
        ),
        f(
            "sched_float_type",
            "ComputeTotalFloatType",
            Kind::Choice(FLOAT_TYPE),
        ),
        f(
            "project.critical_drtn_hr_cnt",
            "CriticalActivityFloatThreshold",
            Num,
        ),
        f(
            "project.critical_path_type",
            "CriticalActivityPathType",
            Kind::Choice(CRITICAL_PATH),
        ),
        f(
            "level_outer_assign_priority",
            "ExternalProjectPriorityLimit",
            Num,
        ),
        f(
            "sched_outer_depend_type",
            "IgnoreOtherProjectRelationships",
            Kind::Choice(IGNORE_OTHER_PROJECTS),
        ),
        f("level_outer_assign_flag", "IncludeExternalResAss", Flag),
        f("level_all_rsrc_flag", "LevelAllResources", Flag),
        f("level_within_float_flag", "LevelWithinFloat", Flag),
        f(
            "sched_open_critical_flag",
            "MakeOpenEndedActivitiesCritical",
            Flag,
        ),
        f(
            "max_multiple_longest_path",
            "MaximumMultipleFloatPaths",
            Num,
        ),
        f("level_float_thrs_cnt", "MinFloatToPreserve", Num),
        f(
            "enable_multiple_longest_path_calc",
            "MultipleFloatPathsEnabled",
            Flag,
        ),
        b(
            "key_activity_for_multiple_longest_paths",
            "MultipleFloatPathsEndingActivityObjectId",
            Num,
        ),
        f(
            "use_total_float_multiple_longest_paths",
            "MultipleFloatPathsUseTotalFloat",
            Flag,
        ),
        f(
            "=oos",
            "OutOfSequenceScheduleType",
            Kind::Choice(OUT_OF_SEQUENCE),
        ),
        f("level_over_alloc_pct", "OverAllocationPercentage", Num),
        f(
            "level_keep_sched_date_flag",
            "PreserveScheduledEarlyAndLateDates",
            Flag,
        ),
        f("proj_id", "ProjectObjectId", Num),
        f(
            "sched_calendar_on_relationship_lag",
            "RelationshipLagCalendar",
            Kind::Choice(LAG_CALENDAR),
        ),
        f("sched_use_expect_end_flag", "UseExpectedFinishDates", Flag),
    ],
    nested: &[],
};

/// Every type with a field list, for the reader's report and the tests.
pub(super) const SPECS: &[&Spec] = &[
    &CURRENCY,
    &UDF_TYPE,
    &NOTEBOOK_TOPIC,
    &OBS,
    &CALENDAR,
    &RESOURCE_CODE_TYPE,
    &RESOURCE_CODE,
    &RESOURCE,
    &FIN_TEMPLATE,
    &ACCOUNT,
    &COST_TYPE,
    &UNIT,
    &ROLE,
    &ROLE_RATE,
    &RESOURCE_RATE,
    &RESOURCE_ROLE,
    &PROJECT,
    &WBS,
    &PROJECT_NOTE,
    &CODE_TYPE,
    &CODE,
    &ACTIVITY,
    &ASSIGNMENT,
    &EXPENSE,
    &ACTIVITY_NOTE,
    &RELATIONSHIP,
    &SCHEDULE_OPTIONS,
];

/// Root children in the order they are written. Observed: ProjectList,
/// DisplayCurrency, Currency, UDFType, NotebookTopic, OBS, Calendar,
/// ResourceCodeType, ResourceCode, Resource, FinancialPeriodTemplate,
/// Project. The others are not observed and sit where their dictionaries
/// are used.
pub(super) const ROOT_ORDER: &[&str] = &[
    "ProjectList",
    "DisplayCurrency",
    "Currency",
    "UnitOfMeasure",
    "CostAccount",
    "ExpenseCategory",
    "UDFType",
    "NotebookTopic",
    "OBS",
    "ActivityCodeType",
    "ActivityCode",
    "Calendar",
    "Role",
    "RoleRate",
    "ResourceCodeType",
    "ResourceCode",
    "Resource",
    "ResourceRate",
    "ResourceRole",
    "FinancialPeriodTemplate",
    "Project",
    "BaselineProject",
];
