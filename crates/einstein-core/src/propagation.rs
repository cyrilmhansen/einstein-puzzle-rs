//! Ordered, deduction-only candidate propagation for historical compatibility.
//!
//! This module neither branches nor counts solutions. Domain contradictions on
//! arbitrary input are reported instead of leaving an invalid board in play.

use crate::{Card, Category, Column, Grid, Rule, SIZE, Value};

const FULL: u8 = (1 << SIZE) - 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidates {
    rows: [[u8; SIZE]; SIZE],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Contradiction {
    EmptyCell { category: Category, column: Column },
    MissingCard(Card),
    ExcludedSelection { card: Card, column: Column },
    IncompatibleRule(Rule),
}

const fn bit(value: Value) -> u8 {
    1 << (value.number() - 1)
}

impl Default for Candidates {
    fn default() -> Self {
        Self::full()
    }
}

impl Candidates {
    pub const fn full() -> Self {
        Self {
            rows: [[FULL; SIZE]; SIZE],
        }
    }

    pub(crate) fn from_closed_masks(rows: [[u8; SIZE]; SIZE]) -> Result<Self, Category> {
        let mut board = Self { rows };
        for category in Category::ALL {
            let row = rows[category.index()];
            if row.iter().any(|&mask| mask == 0 || mask & !FULL != 0)
                || board.row_singles(category).is_err()
                || board.rows[category.index()] != row
            {
                return Err(category);
            }
        }
        Ok(board)
    }

    pub(crate) const fn masks(&self) -> [[u8; SIZE]; SIZE] {
        self.rows
    }

    /// Bits 0 through 5 correspond to values 1 through 6.
    pub const fn mask(&self, category: Category, column: Column) -> u8 {
        self.rows[category.index()][column.index()]
    }

    pub const fn allows(&self, card: Card, column: Column) -> bool {
        self.mask(card.category, column) & bit(card.value) != 0
    }

    pub fn values(&self, category: Category, column: Column) -> impl Iterator<Item = Value> + '_ {
        Value::ALL
            .into_iter()
            .filter(move |&value| self.allows(Card::new(category, value), column))
    }

    pub fn includes(&self, grid: &Grid) -> bool {
        Category::ALL.into_iter().all(|category| {
            Column::ALL
                .into_iter()
                .all(|column| self.allows(grid.card_at(category, column), column))
        })
    }

    pub fn resolved(&self) -> Option<Grid> {
        let mut rows = [Value::ALL; SIZE];
        for category in Category::ALL {
            for column in Column::ALL {
                let mask = self.mask(category, column);
                if mask.count_ones() != 1 {
                    return None;
                }
                rows[category.index()][column.index()] = Value::ALL[mask.trailing_zeros() as usize];
            }
        }
        Grid::from_rows(rows).ok()
    }

    /// Exclude a candidate and propagate only the two uniqueness rules of its
    /// row. A contradiction leaves the original board unchanged.
    pub fn exclude(&mut self, card: Card, column: Column) -> Result<bool, Contradiction> {
        let mut trial = self.clone();
        let changed = trial.remove(card, column)?;
        *self = trial;
        Ok(changed)
    }

    /// Select a candidate, without applying any printed clues. An excluded
    /// selection or subsequent contradiction leaves the original board unchanged.
    pub fn select(&mut self, card: Card, column: Column) -> Result<bool, Contradiction> {
        let mut trial = self.clone();
        let changed = trial.fix(card, column)?;
        *self = trial;
        Ok(changed)
    }

    fn remove(&mut self, card: Card, column: Column) -> Result<bool, Contradiction> {
        if !self.allows(card, column) {
            return Ok(false);
        }
        self.rows[card.category.index()][column.index()] &= !bit(card.value);
        self.row_singles(card.category)?;
        Ok(true)
    }

    fn fix(&mut self, card: Card, column: Column) -> Result<bool, Contradiction> {
        let row = &mut self.rows[card.category.index()];
        let keep = bit(card.value);
        if row[column.index()] & keep == 0 {
            return Err(Contradiction::ExcludedSelection { card, column });
        }
        let before = *row;
        for (i, allowed) in row.iter_mut().enumerate() {
            if i == column.index() {
                *allowed = keep;
            } else {
                *allowed &= !keep;
            }
        }
        if before == *row {
            return Ok(false);
        }
        self.row_singles(card.category)?;
        Ok(true)
    }

    fn row_singles(&mut self, category: Category) -> Result<(), Contradiction> {
        let row = &mut self.rows[category.index()];
        loop {
            let before = *row;
            // Both singleton scans use the same counts at the start of the pass.
            let mut positions = [0u8; SIZE];
            for (col, &choices) in before.iter().enumerate() {
                if choices == 0 {
                    return Err(Contradiction::EmptyCell {
                        category,
                        column: Column::ALL[col],
                    });
                }
                for (value, columns) in positions.iter_mut().enumerate() {
                    if choices & (1 << value) != 0 {
                        *columns |= 1 << col;
                    }
                }
            }
            for (col, choices) in before.into_iter().enumerate() {
                if choices.count_ones() == 1 {
                    for (other, allowed) in row.iter_mut().enumerate() {
                        if col != other {
                            *allowed &= !choices;
                        }
                    }
                }
            }
            for (i, columns) in positions.into_iter().enumerate() {
                match columns.count_ones() {
                    0 => {
                        return Err(Contradiction::MissingCard(Card::new(
                            category,
                            Value::ALL[i],
                        )));
                    }
                    1 => row[columns.trailing_zeros() as usize] &= 1 << i,
                    _ => {}
                }
            }
            if before == *row {
                return Ok(());
            }
        }
    }

    fn near_pass(&mut self, a: Card, b: Card) -> Result<bool, Contradiction> {
        let mut changed = false;
        for col in Column::ALL {
            for (card, other) in [(b, a), (a, b)] {
                if self.allows(card, col)
                    && !Column::ALL.into_iter().any(|candidate| {
                        candidate.number().abs_diff(col.number()) == 1
                            && self.allows(other, candidate)
                    })
                {
                    changed |= self.remove(card, col)?;
                }
            }
        }
        Ok(changed)
    }

    fn between_support(&self, center: u8, middle: Card, a: Card, b: Card) -> bool {
        (2..SIZE as u8).contains(&center)
            && self.allows(middle, Column::ALL[(center - 1) as usize])
            && self.allows(a, Column::ALL[(center - 2) as usize])
            && self.allows(b, Column::ALL[center as usize])
    }

    fn between_pass(&mut self, middle: Card, a: Card, b: Card) -> Result<bool, Contradiction> {
        let mut changed = false;
        for col in Column::ALL.into_iter().skip(1).take(SIZE - 2) {
            let center = col.number();
            if self.allows(middle, col)
                && !self.between_support(center, middle, a, b)
                && !self.between_support(center, middle, b, a)
            {
                changed |= self.remove(middle, col)?;
            }
        }
        for col in Column::ALL {
            for (end, other) in [(b, a), (a, b)] {
                let n = col.number();
                if self.allows(end, col)
                    && !self.between_support(n - 1, middle, other, end)
                    && !self.between_support(n + 1, middle, end, other)
                {
                    changed |= self.remove(end, col)?;
                }
            }
        }
        Ok(changed)
    }

    fn apply(&mut self, rule: Rule) -> Result<bool, Contradiction> {
        let mut changed = false;
        match rule {
            Rule::Reveal { card, column } => return self.fix(card, column),
            Rule::SameColumn(a, b) => {
                if a.category == b.category && a != b {
                    return Err(Contradiction::IncompatibleRule(rule));
                }
                for col in Column::ALL {
                    if !self.allows(a, col) {
                        changed |= self.remove(b, col)?;
                    }
                    if !self.allows(b, col) {
                        changed |= self.remove(a, col)?;
                    }
                }
            }
            Rule::Near(a, b) => {
                if a == b {
                    return Err(Contradiction::IncompatibleRule(rule));
                }
                while self.near_pass(a, b)? {
                    changed = true;
                }
            }
            Rule::LeftOf { left, right } => {
                if left == right {
                    return Err(Contradiction::IncompatibleRule(rule));
                }
                for col in Column::ALL {
                    changed |= self.remove(right, col)?;
                    if self.allows(left, col) {
                        break;
                    }
                }
                for col in Column::ALL.into_iter().rev() {
                    changed |= self.remove(left, col)?;
                    if self.allows(right, col) {
                        break;
                    }
                }
            }
            Rule::Between {
                middle,
                ends: [a, b],
            } => {
                if middle == a || middle == b || a == b {
                    return Err(Contradiction::IncompatibleRule(rule));
                }
                changed |= self.remove(middle, Column::ALL[0])?;
                changed |= self.remove(middle, Column::ALL[SIZE - 1])?;
                while self.between_pass(middle, a, b)? {
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    /// Initial openings only; visible clues are left for the player to use.
    pub fn for_play(rules: &[Rule]) -> Result<Self, Contradiction> {
        let mut board = Self::full();
        for &rule in rules {
            if let Rule::Reveal { card, column } = rule {
                board.fix(card, column)?;
            }
        }
        Ok(board)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Propagation {
    pub candidates: Candidates,
    /// Includes the final pass that makes no changes.
    pub passes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropagationError {
    pub rule_index: usize,
    pub kind: PropagationErrorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropagationErrorKind {
    Contradiction(Contradiction),
    TargetEliminated,
}

/// Restart from full domains and visit the rules in their supplied order until
/// a complete pass changes nothing. The target, if supplied, is checked after
/// each changed rule. It is never used to make a deduction.
pub fn historical(rules: &[Rule], target: Option<&Grid>) -> Result<Propagation, PropagationError> {
    let mut candidates = Candidates::full();
    let mut passes = 0;
    loop {
        passes += 1;
        let mut changed = false;
        for (rule_index, &rule) in rules.iter().enumerate() {
            if candidates.apply(rule).map_err(|error| PropagationError {
                rule_index,
                kind: PropagationErrorKind::Contradiction(error),
            })? {
                changed = true;
                if target.is_some_and(|grid| !candidates.includes(grid)) {
                    return Err(PropagationError {
                        rule_index,
                        kind: PropagationErrorKind::TargetEliminated,
                    });
                }
            }
        }
        if !changed {
            return Ok(Propagation { candidates, passes });
        }
    }
}
