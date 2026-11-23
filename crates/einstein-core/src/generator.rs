//! Versioned reconstruction of the historical puzzle generator.
//!
//! Version 1 replays within this reconstruction, not the original game's RNG.

use std::collections::HashSet;

use crate::propagation::{PropagationError, historical};
use crate::{Category, Column, Grid, Rule, RulePlacement, SIZE, Value};

pub const GENERATOR_VERSION: u16 = 1;
pub const HORIZONTAL_LIMIT: usize = 24;
pub const VERTICAL_LIMIT: usize = 15;
const SHUFFLE_SWAPS: usize = 30;
const MIX_SALT: u64 = 0x6d69_785f_7631_0000;
const RULE_SALT: u64 = 0x7275_6c65_5f76_3100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub layouts: u32,
    /// All proposals count, including duplicates. Applies to each layout.
    pub rule_draws_per_layout: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            layouts: 64,
            rule_draws_per_layout: 4_096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedPuzzle {
    pub seed: u64,
    pub version: u16,
    /// Kept by the engine for validating a player's moves.
    pub solution: Grid,
    /// Stable generation order, including revelations.
    pub rules: Vec<Rule>,
    pub layouts_attempted: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationError {
    LayoutLimit { limit: u32 },
    RuleDrawLimit { layout: u32, limit: u32 },
    Propagation(PropagationError),
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayoutLimit { limit } => write!(f, "no admitted puzzle after {limit} layouts"),
            Self::RuleDrawLimit { layout, limit } => {
                write!(
                    f,
                    "layout {layout} did not converge after {limit} rule draws"
                )
            }
            Self::Propagation(err) => write!(f, "generation invariant failed: {err:?}"),
        }
    }
}
impl std::error::Error for GenerationError {}
impl From<PropagationError> for GenerationError {
    fn from(value: PropagationError) -> Self {
        Self::Propagation(value)
    }
}

/// Generate with the current version and documented default limits.
pub fn generate(seed: u64) -> Result<GeneratedPuzzle, GenerationError> {
    generate_with_limits(seed, Limits::default())
}

pub fn generate_with_limits(seed: u64, limits: Limits) -> Result<GeneratedPuzzle, GenerationError> {
    let mut shuffle = SplitMix64::new(seed, MIX_SALT);
    let mut rules = SplitMix64::new(seed, RULE_SALT);
    finish_layouts(seed, limits.layouts, |layout| {
        let solution = mix_grid(&mut shuffle);
        let rules =
            accumulate_and_prune(&solution, layout, limits.rule_draws_per_layout, &mut rules)?;
        Ok((solution, rules))
    })
}

fn finish_layouts(
    seed: u64,
    max_layouts: u32,
    mut next: impl FnMut(u32) -> Result<(Grid, Vec<Rule>), GenerationError>,
) -> Result<GeneratedPuzzle, GenerationError> {
    for layout in 1..=max_layouts {
        let (solution, rules) = next(layout)?;
        if fits_historical_panels(&rules) {
            return Ok(GeneratedPuzzle {
                seed,
                version: GENERATOR_VERSION,
                solution,
                rules,
                layouts_attempted: layout,
            });
        }
    }
    Err(GenerationError::LayoutLimit { limit: max_layouts })
}

fn accumulate_and_prune(
    solution: &Grid,
    layout: u32,
    max_draws: u32,
    rng: &mut impl Draw,
) -> Result<Vec<Rule>, GenerationError> {
    let mut rules = Vec::new();
    // The ordered semantic identity matches the source's unlocalized getAsText
    // identity. Set iteration is not used; output is always insertion ordered.
    let mut seen = HashSet::new();
    for _ in 0..max_draws {
        let rule = propose(solution, rng);
        if !seen.insert(rule) {
            continue;
        }
        rules.push(rule);
        if solves(solution, &rules)? {
            return prune(rules, |trial| solves(solution, trial));
        }
    }
    Err(GenerationError::RuleDrawLimit {
        layout,
        limit: max_draws,
    })
}

fn solves(solution: &Grid, rules: &[Rule]) -> Result<bool, GenerationError> {
    Ok(historical(rules, Some(solution))?
        .candidates
        .resolved()
        .as_ref()
        == Some(solution))
}

fn prune<E>(
    mut rules: Vec<Rule>,
    mut solves: impl FnMut(&[Rule]) -> Result<bool, E>,
) -> Result<Vec<Rule>, E> {
    let mut index = 0;
    while index < rules.len() {
        let removed = rules.remove(index);
        if solves(&rules)? {
            index = 0;
        } else {
            rules.insert(index, removed);
            index += 1;
        }
    }
    Ok(rules)
}

pub fn fits_historical_panels(rules: &[Rule]) -> bool {
    let (mut horizontal, mut vertical) = (0, 0);
    for rule in rules {
        match rule.placement() {
            RulePlacement::Horizontal => horizontal += 1,
            RulePlacement::Vertical => vertical += 1,
            RulePlacement::InitialReveal => (),
        }
    }
    horizontal <= HORIZONTAL_LIMIT && vertical <= VERTICAL_LIMIT
}

trait Draw {
    /// An integer in [0, upper), with non-zero upper.
    fn below(&mut self, upper: usize) -> usize;
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64, salt: u64) -> Self {
        Self(Self::mix(seed ^ salt))
    }

    fn mix(mut v: u64) -> u64 {
        v = (v ^ (v >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        v = (v ^ (v >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        v ^ (v >> 31)
    }

    fn word(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        Self::mix(self.0)
    }
}

impl Draw for SplitMix64 {
    fn below(&mut self, upper: usize) -> usize {
        let upper = upper as u64;
        // Reject the prefix not divisible by upper; keep equal-size residues.
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let word = self.word();
            if word >= threshold {
                return (word % upper) as usize;
            }
        }
    }
}

fn mix_grid(rng: &mut impl Draw) -> Grid {
    let mut rows = [Value::ALL; SIZE];
    for row in &mut rows {
        for _ in 0..SHUFFLE_SWAPS {
            let a = rng.below(SIZE);
            let b = rng.below(SIZE);
            row.swap(a, b);
        }
    }
    Grid::from_rows(rows).expect("swaps preserve each row's permutation")
}

fn propose(grid: &Grid, rng: &mut impl Draw) -> Rule {
    let category = |rng: &mut _| Category::ALL[Draw::below(rng, SIZE)];
    let card = |row, column| grid.card_at(row, Column::ALL[column]);
    match rng.below(14) {
        0..=3 => {
            let col = rng.below(SIZE);
            let a = category(rng);
            let neighbor = match col {
                0 => 1,
                col if col == SIZE - 1 => SIZE - 2,
                col => {
                    if rng.below(2) == 1 {
                        col + 1
                    } else {
                        col - 1
                    }
                }
            };
            let b = category(rng);
            Rule::Near(card(a, col), card(b, neighbor))
        }
        4 => {
            let col = rng.below(SIZE);
            Rule::Reveal {
                card: card(category(rng), col),
                column: Column::ALL[col],
            }
        }
        5..=6 => {
            let col = rng.below(SIZE);
            let a = category(rng);
            let mut b = category(rng);
            while a == b {
                b = category(rng);
            }
            Rule::SameColumn(card(a, col), card(b, col))
        }
        7..=10 => {
            let a = category(rng);
            let b = category(rng);
            let col = rng.below(SIZE - 1);
            let right = col + 1 + rng.below(SIZE - col - 1);
            Rule::LeftOf {
                left: card(a, col),
                right: card(b, right),
            }
        }
        11..=13 => {
            let middle = category(rng);
            let a = category(rng);
            let b = category(rng);
            let col = 1 + rng.below(SIZE - 2);
            let ends = if rng.below(2) == 1 {
                [card(a, col - 1), card(b, col + 1)]
            } else {
                [card(a, col + 1), card(b, col - 1)]
            };
            Rule::Between {
                middle: card(middle, col),
                ends,
            }
        }
        _ => unreachable!("Draw::below only returns bounded values"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Script<'a>(&'a [(usize, usize)]);

    impl Draw for Script<'_> {
        fn below(&mut self, upper: usize) -> usize {
            let (&(expected, output), rest) = self.0.split_first().expect("extra draw");
            self.0 = rest;
            assert_eq!(upper, expected);
            assert!(output < upper);
            output
        }
    }

    fn grid() -> Grid {
        Grid::from_rows([Value::ALL; SIZE]).unwrap()
    }

    #[test]
    fn shuffle_consumes_two_draws_for_each_of_30_swaps_per_row() {
        let mut sequence = vec![(6, 0); 360];
        // Only the first swap of each 60-draw row is not a self-swap.
        for i in 0..6 {
            sequence[i * 60 + 1] = (6, i);
        }
        let mut script = Script(&sequence);
        let mixed = mix_grid(&mut script);
        assert!(script.0.is_empty());
        for row in Category::ALL {
            let mut expected = Value::ALL;
            expected.swap(0, row.index());
            assert_eq!(mixed.row(row), &expected);
        }
    }

    #[test]
    fn every_factory_ticket_replays_the_expected_draw_order() {
        let grid = grid();
        let v = |row, col| grid.card_at(Category::ALL[row], Column::ALL[col]);
        for ticket in 0..14 {
            let (continuation, expected): (&[(usize, usize)], Rule) = match ticket {
                0..=3 => (&[(6, 0), (6, 2), (6, 2)], Rule::Near(v(2, 0), v(2, 1))),
                4 => (
                    &[(6, 5), (6, 3)],
                    Rule::Reveal {
                        card: v(3, 5),
                        column: Column::ALL[5],
                    },
                ),
                5..=6 => (
                    &[(6, 4), (6, 1), (6, 1), (6, 2)],
                    Rule::SameColumn(v(1, 4), v(2, 4)),
                ),
                7..=10 => (
                    &[(6, 0), (6, 0), (5, 3), (2, 1)],
                    Rule::LeftOf {
                        left: v(0, 3),
                        right: v(0, 5),
                    },
                ),
                _ => (
                    &[(6, 0), (6, 1), (6, 2), (4, 3), (2, 0)],
                    Rule::Between {
                        middle: v(0, 4),
                        ends: [v(1, 5), v(2, 3)],
                    },
                ),
            };
            let inputs = [(14, ticket)]
                .into_iter()
                .chain(continuation.iter().copied())
                .collect::<Vec<_>>();
            let mut script = Script(&inputs);
            let rule = propose(&grid, &mut script);
            assert_eq!(rule, expected);
            assert!(rule.holds(&grid));
            assert!(script.0.is_empty());
        }
    }

    #[test]
    fn dedup_counts_duplicate_proposals_against_the_budget() {
        let mut script = Script(&[
            (14, 0),
            (6, 0),
            (6, 1),
            (6, 2),
            (14, 3),
            (6, 0),
            (6, 1),
            (6, 2),
        ]);
        assert_eq!(
            accumulate_and_prune(&grid(), 1, 2, &mut script),
            Err(GenerationError::RuleDrawLimit {
                layout: 1,
                limit: 2
            })
        );
        assert!(script.0.is_empty());
    }

    #[test]
    fn greedy_prune_restarts_at_the_first_slot_and_keeps_order() {
        let rules = Column::ALL[..3]
            .iter()
            .map(|&column| Rule::Reveal {
                card: grid().card_at(Category::Dice, column),
                column,
            })
            .collect::<Vec<_>>();
        let [a, b, c] = rules[..] else { unreachable!() };
        let mut trials = Vec::new();
        let result = prune(rules, |trial| {
            trials.push(trial.to_vec());
            Ok::<_, ()>(trial.contains(&a) && trial.contains(&c))
        })
        .unwrap();
        assert_eq!(result, [a, c]);
        assert_eq!(trials, [vec![b, c], vec![a, c], vec![c], vec![a]]);
    }

    #[test]
    fn outer_loop_retries_a_layout_discarded_after_prune() {
        let grid = grid();
        let a = grid.card_at(Category::Dice, Column::ALL[0]);
        let b = grid.card_at(Category::Letters, Column::ALL[1]);
        let over = vec![Rule::Near(a, b); HORIZONTAL_LIMIT + 1];
        let accepted = vec![Rule::Near(a, b)];
        let mut seen = Vec::new();
        let result = finish_layouts(42, 2, |layout| {
            seen.push(layout);
            Ok((
                grid.clone(),
                if layout == 1 {
                    over.clone()
                } else {
                    accepted.clone()
                },
            ))
        })
        .unwrap();
        assert_eq!(seen, [1, 2]);
        assert_eq!(result.layouts_attempted, 2);
        assert_eq!(result.rules, accepted);
        assert_eq!(
            finish_layouts(42, 1, |_| Ok((grid.clone(), over.clone()))),
            Err(GenerationError::LayoutLimit { limit: 1 })
        );
    }
}
