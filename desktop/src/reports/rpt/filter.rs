//! Filter and sort lines of a report section: a small boolean expression
//! written one item per line (`#begin_and` / `#begin_or` ... `#end`, and
//! `field op value` comparisons), and sort keys (`field ASC`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Between,
    Like,
}

impl Op {
    #[cfg(test)]
    pub fn text(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "<>",
            Op::Gt => ">",
            Op::Lt => "<",
            Op::Ge => ">=",
            Op::Le => "<=",
            Op::Between => "between",
            Op::Like => "like",
        }
    }
    fn parse(s: &str) -> Option<Op> {
        Some(match s.to_ascii_lowercase().as_str() {
            "=" | "==" => Op::Eq,
            "<>" | "!=" => Op::Ne,
            ">" => Op::Gt,
            "<" => Op::Lt,
            ">=" => Op::Ge,
            "<=" => Op::Le,
            "between" => Op::Between,
            "like" => Op::Like,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Cmp {
        field: String,
        op: Op,
        /// One value, or two for `between`; may be `[variable]` references.
        values: Vec<String>,
    },
    /// A line that could not be read; it never excludes anything.
    Unreadable(String),
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        matches!(self, Filter::And(v) | Filter::Or(v) if v.is_empty())
    }

    /// The comparisons that could not be read.
    pub fn unreadable(&self) -> Vec<&str> {
        let mut out = vec![];
        let mut stack = vec![self];
        while let Some(f) = stack.pop() {
            match f {
                Filter::And(v) | Filter::Or(v) => stack.extend(v.iter()),
                Filter::Unreadable(s) => out.push(s.as_str()),
                Filter::Cmp { .. } => {}
            }
        }
        out
    }
}

/// Deepest `#begin` nesting read; deeper lines are dropped as unreadable.
const MAX_NEST: usize = 24;

/// Reads filter lines. Several top-level items are combined with AND.
pub fn parse(lines: &[String]) -> Filter {
    let items: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut stack: Vec<(bool, Vec<Filter>)> = vec![(true, vec![])];
    let mut i = 0;
    while i < items.len() {
        let line = items[i];
        i += 1;
        let lower = line.to_ascii_lowercase();
        if lower == "#begin_and" || lower == "#begin_or" {
            if stack.len() >= MAX_NEST {
                stack
                    .last_mut()
                    .expect("root")
                    .1
                    .push(Filter::Unreadable(line.into()));
            } else {
                stack.push((lower == "#begin_and", vec![]));
            }
            continue;
        }
        if lower == "#end" {
            if stack.len() > 1 {
                let (and, v) = stack.pop().expect("nested");
                let f = if and { Filter::And(v) } else { Filter::Or(v) };
                stack.last_mut().expect("root").1.push(f);
            }
            continue;
        }
        // A comparison split over three lines (field, operator, value).
        let mut words: Vec<String> = tokens(line);
        if words.len() == 1 {
            let (a, b) = (items.get(i), items.get(i + 1));
            if let (Some(a), Some(b)) = (a, b) {
                if Op::parse(a).is_some() && !b.starts_with('#') {
                    words.push((*a).to_string());
                    words.extend(tokens(b));
                    i += 2;
                }
            }
        }
        let leaf = match (words.get(1).and_then(|w| Op::parse(w)), words.first()) {
            (Some(op), Some(field)) => {
                let values: Vec<String> = words[2..].to_vec();
                let need = if op == Op::Between { 2 } else { 1 };
                if values.len() == need || (values.is_empty() && matches!(op, Op::Eq | Op::Ne)) {
                    Filter::Cmp {
                        field: field.clone(),
                        op,
                        values,
                    }
                } else if op != Op::Between && values.len() > 1 {
                    Filter::Cmp {
                        field: field.clone(),
                        op,
                        values: vec![values.join(" ")],
                    }
                } else {
                    Filter::Unreadable(line.into())
                }
            }
            _ => Filter::Unreadable(line.into()),
        };
        stack.last_mut().expect("root").1.push(leaf);
    }
    while stack.len() > 1 {
        let (and, v) = stack.pop().expect("nested");
        let f = if and { Filter::And(v) } else { Filter::Or(v) };
        stack.last_mut().expect("root").1.push(f);
    }
    let (_, mut v) = stack.pop().expect("root");
    if v.len() == 1 {
        v.pop().expect("one item")
    } else {
        Filter::And(v)
    }
}

fn tokens(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

/// The lines of a filter: `#begin_and` / `#begin_or` ... `#end`, two spaces
/// per nesting level.
#[cfg(test)]
pub fn write(f: &Filter) -> Vec<String> {
    let root = match f {
        Filter::And(_) | Filter::Or(_) => f.clone(),
        leaf => Filter::And(vec![leaf.clone()]),
    };
    let begin = |f: &Filter| {
        if matches!(f, Filter::And(_)) {
            "#begin_and"
        } else {
            "#begin_or"
        }
    };
    let pad = |d: usize| "  ".repeat(d);
    let mut out = vec![begin(&root).to_string()];
    // Open groups and the next child to write.
    let mut stack: Vec<(&Filter, usize)> = vec![(&root, 0)];
    while let Some((node, next)) = stack.last().copied() {
        let kids = match node {
            Filter::And(v) | Filter::Or(v) => v.as_slice(),
            _ => &[],
        };
        match kids.get(next) {
            Some(kid) => {
                stack.last_mut().expect("stack").1 += 1;
                let depth = stack.len();
                match kid {
                    Filter::And(_) | Filter::Or(_) => {
                        out.push(format!("{}{}", pad(depth), begin(kid)));
                        stack.push((kid, 0));
                    }
                    leaf => out.push(format!("{}{}", pad(depth), leaf_text(leaf))),
                }
            }
            None => {
                if stack.len() > 1 {
                    out.push(format!("{}#end", pad(stack.len() - 1)));
                }
                stack.pop();
            }
        }
    }
    out.push("#end".into());
    out
}

#[cfg(test)]
fn leaf_text(f: &Filter) -> String {
    match f {
        Filter::Cmp { field, op, values } => {
            let mut s = format!("{field} {}", op.text());
            for v in values {
                s.push(' ');
                s.push_str(v);
            }
            s
        }
        Filter::Unreadable(s) => s.clone(),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub field: String,
    pub descending: bool,
}

/// Reads sort lines: `field`, `field ASC`, `field DESC`, `field*`.
pub fn parse_sort(lines: &[String]) -> Vec<SortKey> {
    lines
        .iter()
        .filter_map(|l| {
            let l = l.trim().trim_end_matches('*').trim();
            if l.is_empty() {
                return None;
            }
            let (field, dir) = match l.rsplit_once(' ') {
                Some((f, d)) if d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc") => {
                    (f.trim(), d.eq_ignore_ascii_case("desc"))
                }
                _ => (l, false),
            };
            Some(SortKey {
                field: field.to_string(),
                descending: dir,
            })
        })
        .collect()
}

#[cfg(test)]
pub fn write_sort(keys: &[SortKey]) -> Vec<String> {
    keys.iter()
        .map(|k| format!("{} {}", k.field, if k.descending { "DESC" } else { "ASC" }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn reads_nested_expressions() {
        let f = parse(&lines(&[
            "#begin_and",
            "  node_type <> EPS",
            "  #begin_or",
            "    status_code = TK_Active",
            "    target_end_date between CW CW+6D",
            "  #end",
            "#end",
        ]));
        let Filter::And(v) = &f else { panic!("{f:?}") };
        assert_eq!(v.len(), 2);
        assert!(matches!(&v[1], Filter::Or(o) if o.len() == 2));
        assert_eq!(write(&f).len(), 7);
        assert_eq!(parse(&write(&f)), f);
    }

    #[test]
    fn three_line_comparisons_and_junk() {
        let f = parse(&lines(&["node_type", "=", "EPS", "what is this"]));
        let Filter::And(v) = &f else { panic!("{f:?}") };
        assert_eq!(
            v[0],
            Filter::Cmp {
                field: "node_type".into(),
                op: Op::Eq,
                values: vec!["EPS".into()]
            }
        );
        assert!(matches!(v[1], Filter::Unreadable(_)));
        assert_eq!(f.unreadable(), ["what is this"]);
        assert!(parse(&[]).is_empty());
    }

    #[test]
    fn deep_nesting_does_not_overflow() {
        let mut l: Vec<String> = (0..100_000).map(|_| "#begin_or".to_string()).collect();
        l.push("a = 1".into());
        let f = parse(&l);
        assert!(!f.unreadable().is_empty() || !f.is_empty());
        let _ = write(&f);
    }

    #[test]
    fn sorts() {
        assert_eq!(
            parse_sort(&lines(&[
                "seq_num ASC",
                "TASK|task_code DESC",
                "x*",
                "y",
                ""
            ])),
            vec![
                SortKey {
                    field: "seq_num".into(),
                    descending: false
                },
                SortKey {
                    field: "TASK|task_code".into(),
                    descending: true
                },
                SortKey {
                    field: "x".into(),
                    descending: false
                },
                SortKey {
                    field: "y".into(),
                    descending: false
                },
            ]
        );
        assert_eq!(write_sort(&parse_sort(&lines(&["a DESC"]))), ["a DESC"]);
    }
}
