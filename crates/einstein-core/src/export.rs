//! Diagnostic JSON exchange with the independent corpus verifier.
//! Only the given rules, and optionally replay metadata, are exported.

use std::io::{self, Write};

use crate::generator::GeneratedPuzzle;
use crate::{Card, Rule};

pub fn write_rules_json(mut out: impl Write, rules: &[Rule]) -> io::Result<()> {
    out.write_all(b"{\"rules\":")?;
    write_rules(&mut out, rules)?;
    out.write_all(b"}")
}

/// Writes one JSON object. Append a newline to stream a corpus as NDJSON.
/// The solution is deliberately not included.
pub fn write_generated_json(mut out: impl Write, puzzle: &GeneratedPuzzle) -> io::Result<()> {
    write!(
        out,
        "{{\"seed\":{},\"generator_version\":{},\"layouts_attempted\":{},\"rules\":",
        puzzle.seed, puzzle.version, puzzle.layouts_attempted,
    )?;
    write_rules(&mut out, &puzzle.rules)?;
    out.write_all(b"}")
}

fn write_rules(out: &mut impl Write, rules: &[Rule]) -> io::Result<()> {
    out.write_all(b"[")?;
    for (i, &rule) in rules.iter().enumerate() {
        if i != 0 {
            out.write_all(b",")?;
        }
        match rule {
            Rule::SameColumn(a, b) => write_relation(out, "same_column", &[a, b], None)?,
            Rule::Near(a, b) => write_relation(out, "near", &[a, b], None)?,
            Rule::LeftOf { left, right } => write_relation(out, "left_of", &[left, right], None)?,
            Rule::Between { middle, ends } => {
                write_relation(out, "between", &[ends[0], middle, ends[1]], None)?
            }
            Rule::Reveal { card, column } => {
                write_relation(out, "reveal", &[card], Some(column.number()))?
            }
        }
    }
    out.write_all(b"]")
}

fn write_relation(
    out: &mut impl Write,
    kind: &str,
    cards: &[Card],
    column: Option<u8>,
) -> io::Result<()> {
    write!(out, "{{\"kind\":\"{kind}\",\"cards\":[")?;
    for (i, card) in cards.iter().enumerate() {
        if i != 0 {
            out.write_all(b",")?;
        }
        write!(
            out,
            "[{},{}]",
            card.category.index() + 1,
            card.value.number()
        )?;
    }
    out.write_all(b"]")?;
    if let Some(column) = column {
        write!(out, ",\"column\":{column}")?;
    }
    out.write_all(b"}")
}
