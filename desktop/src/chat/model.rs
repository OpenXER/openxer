//! The conversation as the Chat window shows it: the user's messages, the
//! agent's replies (streamed text assembled), tool activity, proposals and
//! status lines, built from the backend's events. Pure data; the window
//! draws it.
use super::backend::{AgentEvent, ChangeRow, ChatContext, ErrorKind, Selection, Usage};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ToolStatus {
    Running,
    /// Running, held until the user answers a proposal.
    Waiting,
    Done,
    Failed,
    /// The user stopped the reply while it ran.
    Stopped,
}

impl ToolStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ToolStatus::Running => "Running",
            ToolStatus::Waiting => "Waiting",
            ToolStatus::Done => "Done",
            ToolStatus::Failed => "Failed",
            ToolStatus::Stopped => "Stopped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ToolRow {
    pub call: String,
    pub tool: String,
    pub target: String,
    pub status: ToolStatus,
    pub millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ProposalState {
    /// Waiting for Apply or Discard.
    Pending,
    Applied,
    Discarded,
    /// A later proposal took its place before it was answered.
    Replaced,
    /// Apply failed; nothing was written.
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum StatusKind {
    /// "Stopped" and the like.
    Note,
    Error,
    /// The usage limit is reached.
    Limit,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AskOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AskPrompt {
    pub question: String,
    #[serde(default)]
    pub multi_select: bool,
    pub options: Vec<AskOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum QuestionState {
    Pending,
    Answered(String),
    Dismissed,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Item {
    User(String),
    /// A reply of the agent; `streaming` while more text may come.
    Reply {
        text: String,
        #[serde(default)]
        streaming: bool,
    },
    /// Tools that ran one after another, shown as one block.
    Tools(Vec<ToolRow>),
    Proposal {
        id: u64,
        rows: Vec<ChangeRow>,
        #[serde(default)]
        impact: Option<String>,
        state: ProposalState,
    },
    /// Clarifying question(s) from the agent with clickable options + Other box.
    Question {
        id: u64,
        #[serde(default)]
        title: String,
        prompts: Vec<AskPrompt>,
        state: QuestionState,
    },
    Status {
        kind: StatusKind,
        text: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Conversation {
    pub items: Vec<Item>,
    /// A reply is being written: Send shows as Stop.
    #[serde(skip)]
    pub running: bool,
    /// Tokens of the last finished reply.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Cumulative tokens used across all finished replies in this chat.
    #[serde(default)]
    pub total_usage: Usage,
    /// Number of completed replies in this conversation.
    #[serde(default)]
    pub turns: usize,
    /// The server gave no token counts for the last reply.
    #[serde(default)]
    pub usage_unknown: bool,
    /// Events count only between a send and the end of its reply; after
    /// Stop the rest of that reply is dropped.
    #[serde(skip)]
    accepting: bool,
}

impl Conversation {
    /// The user sends `text`. False when there is nothing to send or a reply
    /// is still running.
    pub(crate) fn send(&mut self, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() || self.running {
            return false;
        }
        // Any unanswered question card is superseded when the user replies manually.
        for item in &mut self.items {
            if let Item::Question { state, .. } = item {
                if *state == QuestionState::Pending {
                    *state = QuestionState::Dismissed;
                }
            }
        }
        self.items.push(Item::User(text.to_owned()));
        self.running = true;
        self.accepting = true;
        true
    }

    /// Start an automatic follow-up turn (e.g. after applying a proposal)
    /// without adding a duplicate user message bubble.
    pub(crate) fn begin_followup(&mut self) -> bool {
        if self.running {
            return false;
        }
        self.running = true;
        self.accepting = true;
        true
    }

    /// Add an interactive clarifying question card from the agent.
    pub(crate) fn ask_question(&mut self, title: String, prompts: Vec<AskPrompt>) -> u64 {
        self.close_reply();
        for item in &mut self.items {
            if let Item::Question { state, .. } = item {
                if *state == QuestionState::Pending {
                    *state = QuestionState::Dismissed;
                }
            }
        }
        let id = self
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Question { id, .. } => Some(*id),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1;
        self.items.push(Item::Question {
            id,
            title,
            prompts,
            state: QuestionState::Pending,
        });
        id
    }

    /// Resolve a pending question card (answered or skipped).
    pub(crate) fn resolve_question(&mut self, id: u64, to: QuestionState) {
        for item in &mut self.items {
            if let Item::Question {
                id: this, state, ..
            } = item
            {
                if *this == id && *state == QuestionState::Pending {
                    *state = to.clone();
                }
            }
        }
    }

    /// ID of the currently pending question card, if any.
    pub(crate) fn pending_question(&self) -> Option<u64> {
        self.items.iter().rev().find_map(|i| match i {
            Item::Question {
                id,
                state: QuestionState::Pending,
                ..
            } => Some(*id),
            _ => None,
        })
    }

    /// A new, empty conversation.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The streaming reply, if the last item is one.
    fn open_reply(&mut self) -> Option<&mut String> {
        match self.items.last_mut() {
            Some(Item::Reply {
                text,
                streaming: true,
            }) => Some(text),
            _ => None,
        }
    }

    /// Ends the streaming reply, if any: later text starts a new one.
    fn close_reply(&mut self) {
        if let Some(Item::Reply { streaming, .. }) = self.items.last_mut() {
            *streaming = false;
        }
    }

    fn tool_rows(&mut self) -> impl Iterator<Item = &mut ToolRow> {
        self.items.iter_mut().flat_map(|i| {
            match i {
                Item::Tools(rows) => rows.as_mut_slice(),
                _ => &mut [],
            }
            .iter_mut()
        })
    }

    /// One event from the backend.
    pub(crate) fn apply(&mut self, event: AgentEvent) {
        if !self.accepting {
            return;
        }
        match event {
            AgentEvent::Ready => {}
            AgentEvent::TextDelta(delta) => match self.open_reply() {
                Some(text) => text.push_str(&delta),
                None => self.items.push(Item::Reply {
                    text: delta,
                    streaming: true,
                }),
            },
            AgentEvent::Message(whole) => {
                match self.open_reply() {
                    Some(text) => *text = whole,
                    None => self.items.push(Item::Reply {
                        text: whole,
                        streaming: true,
                    }),
                }
                self.close_reply();
            }
            AgentEvent::ToolStarted { call, tool, target } => {
                self.close_reply();
                let row = ToolRow {
                    call,
                    tool,
                    target,
                    status: ToolStatus::Running,
                    millis: None,
                };
                match self.items.last_mut() {
                    Some(Item::Tools(rows)) => rows.push(row),
                    _ => self.items.push(Item::Tools(vec![row])),
                }
            }
            AgentEvent::ToolFinished { call, ok, millis } => {
                if let Some(row) = self.tool_rows().filter(|r| r.call == call).last() {
                    row.status = if ok {
                        ToolStatus::Done
                    } else {
                        ToolStatus::Failed
                    };
                    row.millis = Some(millis);
                }
            }
            AgentEvent::Proposed { proposal, rows } => {
                self.close_reply();
                for item in &mut self.items {
                    if let Item::Proposal { state, .. } = item {
                        if *state == ProposalState::Pending {
                            *state = ProposalState::Replaced;
                        }
                    }
                }
                for row in self.tool_rows() {
                    if row.status == ToolStatus::Running {
                        row.status = ToolStatus::Waiting;
                    }
                }
                self.items.push(Item::Proposal {
                    id: proposal,
                    rows,
                    impact: None,
                    state: ProposalState::Pending,
                });
            }
            AgentEvent::ProposalImpact { proposal, summary } => {
                for item in &mut self.items {
                    if let Item::Proposal { id, impact, .. } = item {
                        if *id == proposal {
                            *impact = Some(summary.clone());
                        }
                    }
                }
            }
            AgentEvent::TurnFinished { usage } => {
                self.close_reply();
                self.running = false;
                self.accepting = false;
                self.turns += 1;
                if let Some(u) = usage {
                    self.usage = Some(u);
                    self.total_usage.input = self.total_usage.input.saturating_add(u.input);
                    self.total_usage.output = self.total_usage.output.saturating_add(u.output);
                    self.usage_unknown = false;
                }
            }
            AgentEvent::UsageNotReported => {
                self.usage = None;
                self.usage_unknown = true;
            }
            AgentEvent::UiAction(_) => {}
            AgentEvent::Notice(text) => {
                self.close_reply();
                self.items.push(Item::Status {
                    kind: StatusKind::Note,
                    text,
                });
            }
            AgentEvent::Error { kind, message } => {
                self.close_reply();
                self.running = false;
                self.accepting = false;
                let (kind, text) = match kind {
                    ErrorKind::UsageLimit => (
                        StatusKind::Limit,
                        join("Usage limit reached.", &message, "Nothing was retried."),
                    ),
                    ErrorKind::NotSignedIn => (
                        StatusKind::Error,
                        join("Not signed in.", &message, "Sign in and send it again."),
                    ),
                    ErrorKind::Failed => (StatusKind::Error, join("Error:", &message, "")),
                };
                self.items.push(Item::Status { kind, text });
            }
        }
    }

    /// The user stopped the reply. True when one was running.
    pub(crate) fn stop(&mut self) -> bool {
        if !self.running {
            return false;
        }
        self.close_reply();
        for row in self.tool_rows() {
            if matches!(row.status, ToolStatus::Running | ToolStatus::Waiting) {
                row.status = ToolStatus::Stopped;
            }
        }
        self.running = false;
        self.accepting = false;
        self.items.push(Item::Status {
            kind: StatusKind::Note,
            text: "Stopped".into(),
        });
        true
    }

    /// The proposal waiting for an answer.
    pub(crate) fn pending(&self) -> Option<(u64, &[ChangeRow])> {
        self.items.iter().rev().find_map(|i| match i {
            Item::Proposal {
                id,
                rows,
                state: ProposalState::Pending,
                ..
            } => Some((*id, rows.as_slice())),
            _ => None,
        })
    }

    /// Proposal `id` reads differently now (the project changed since it
    /// was made): it shows `rows` and waits for an answer again.
    pub(crate) fn refresh(&mut self, id: u64, rows: Vec<ChangeRow>) {
        for item in &mut self.items {
            if let Item::Proposal {
                id: this,
                rows: shown,
                state,
                ..
            } = item
            {
                if *this == id && *state == ProposalState::Pending {
                    *shown = rows.clone();
                }
            }
        }
    }

    /// A note outside a reply (a refreshed proposal, a check).
    pub(crate) fn note(&mut self, text: impl Into<String>) {
        self.items.push(Item::Status {
            kind: StatusKind::Note,
            text: text.into(),
        });
    }

    /// The user answered proposal `id`.
    pub(crate) fn resolve(&mut self, id: u64, to: ProposalState) {
        for item in &mut self.items {
            if let Item::Proposal {
                id: this, state, ..
            } = item
            {
                if *this == id && *state == ProposalState::Pending {
                    *state = to.clone();
                }
            }
        }
    }

    /// What the title bar adds while the window is rolled up.
    pub(crate) fn status(&self) -> Option<String> {
        if let Some((_, rows)) = self.pending() {
            let n = rows.len();
            return Some(format!("{n} change{} ready", if n == 1 { "" } else { "s" }));
        }
        if self.pending_question().is_some() {
            return Some("question waiting".into());
        }
        if self.running {
            return Some("answering...".into());
        }
        match self.items.last() {
            Some(Item::Status {
                kind: StatusKind::Limit,
                ..
            }) => Some("usage limit reached".into()),
            Some(Item::Status {
                kind: StatusKind::Error,
                ..
            }) => Some("error".into()),
            _ => None,
        }
    }

    /// Short title for the conversation history picker (first user prompt).
    pub(crate) fn title(&self) -> String {
        let raw = self
            .items
            .iter()
            .find_map(|i| match i {
                Item::User(t) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or("Conversation");
        let single = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut out: String = single.chars().take(48).collect();
        if single.chars().count() > 48 {
            out.push_str("...");
        }
        out
    }

    /// The last user prompt sent in this conversation.
    pub(crate) fn last_user_prompt(&self) -> Option<&str> {
        self.items.iter().rev().find_map(|i| match i {
            Item::User(t) => Some(t.as_str()),
            _ => None,
        })
    }

    /// True when the last finished reply in Ask mode suggests switching to
    /// Edit mode so the UI can offer a one-click Switch to Edit & Retry action.
    pub(crate) fn suggests_edit_mode(&self) -> bool {
        if self.running {
            return false;
        }
        let Some(Item::Reply { text, .. }) = self.items.last() else {
            return false;
        };
        let lower = text.to_ascii_lowercase();
        (lower.contains("edit mode") || lower.contains("switch") && lower.contains("edit"))
            && (lower.contains("ask")
                || lower.contains("read-only")
                || lower.contains("read only")
                || lower.contains("propose"))
    }

    /// Compact recap of prior turns (capped at 2,400 chars) sent when a user
    /// continues a reopened or history-restored conversation on a fresh backend.
    pub(crate) fn recap_text(&self) -> Option<String> {
        let mut lines = vec![];
        for item in &self.items {
            match item {
                Item::User(t) => lines.push(format!("User: {}", trim_chars(t, 280))),
                Item::Reply { text, .. } => {
                    lines.push(format!("Assistant: {}", trim_chars(text, 420)))
                }
                Item::Proposal {
                    rows,
                    state: ProposalState::Applied,
                    ..
                } => lines.push(format!("(Applied proposal: {} change rows)", rows.len())),
                Item::Proposal {
                    rows,
                    state: ProposalState::Discarded,
                    ..
                } => lines.push(format!("(Discarded proposal: {} change rows)", rows.len())),
                Item::Question {
                    state: QuestionState::Answered(ans),
                    ..
                } => lines.push(format!("(Answered question: {})", trim_chars(ans, 240))),
                _ => {}
            }
        }
        if lines.is_empty() {
            return None;
        }
        // Keep the newest entries within the budget.
        let mut kept = vec![];
        let mut chars = 0usize;
        for line in lines.into_iter().rev() {
            let len = line.chars().count() + 1;
            if chars + len > 2_400 && !kept.is_empty() {
                break;
            }
            chars += len;
            kept.push(line);
        }
        kept.reverse();
        Some(format!(
            "Earlier messages in this conversation (for continuity):\n{}",
            kept.join("\n")
        ))
    }

    /// Prepare a restored conversation for display: close any incomplete
    /// streaming flags and resolve stale pending proposals as Replaced.
    pub(crate) fn sanitize_restored(&mut self) {
        self.running = false;
        self.accepting = false;
        for item in &mut self.items {
            match item {
                Item::Reply { streaming, .. } => *streaming = false,
                Item::Tools(rows) => {
                    for r in rows {
                        if matches!(r.status, ToolStatus::Running | ToolStatus::Waiting) {
                            r.status = ToolStatus::Stopped;
                        }
                    }
                }
                Item::Proposal { state, .. } if *state == ProposalState::Pending => {
                    *state = ProposalState::Replaced;
                }
                Item::Question { state, .. } if *state == QuestionState::Pending => {
                    *state = QuestionState::Dismissed;
                }
                _ => {}
            }
        }
    }
}

fn trim_chars(s: &str, max: usize) -> String {
    let single = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = single.chars().take(max).collect();
    if single.chars().count() > max {
        out.push_str("...");
    }
    out
}

fn join(head: &str, message: &str, tail: &str) -> String {
    [head, message.trim(), tail]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "N change(s) ready. Nothing is written until you apply it."
pub(crate) fn pending_line(n: usize) -> String {
    if n == 1 {
        "1 change ready. Nothing is written until you apply it.".into()
    } else {
        format!("{n} changes ready. Nothing is written until you apply them.")
    }
}

/// The read-only context line: project, selection, data date.
pub(crate) fn context_line(c: &ChatContext) -> String {
    let selection = match &c.selection {
        Selection::None => "No activity selected".to_string(),
        Selection::One(id) => format!("{id} selected"),
        Selection::Many(ids) => format!("{} activities selected", ids.len()),
    };
    format!(
        "{} \u{b7} {selection} \u{b7} Data Date {}",
        c.project_id, c.data_date
    )
}

/// A tool's run time as the tool rows show it.
pub(crate) fn duration(millis: u64) -> String {
    if millis < 60_000 {
        format!("{:.1} s", millis as f64 / 1000.)
    } else {
        format!("{}:{:02} min", millis / 60_000, millis / 1000 % 60)
    }
}

/// Tokens in a short form: 950, 1.2k, 36k.
pub(crate) fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=9_999 => format!("{:.1}k", n as f64 / 1000.),
        _ => format!("{}k", (n + 500) / 1000),
    }
}
