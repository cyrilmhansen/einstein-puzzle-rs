use std::collections::HashSet;

use einstein_core::generator::{
    GENERATOR_VERSION, GenerationError, Limits, fits_historical_panels, generate,
    generate_with_limits,
};
use einstein_core::propagation::historical;
use einstein_core::{Category, Column, Rule};

#[test]
fn empty_generation_budgets_are_explicit_errors() {
    assert_eq!(
        generate_with_limits(
            7,
            Limits {
                layouts: 0,
                rule_draws_per_layout: 2
            }
        ),
        Err(GenerationError::LayoutLimit { limit: 0 })
    );
    assert_eq!(
        generate_with_limits(
            7,
            Limits {
                layouts: 1,
                rule_draws_per_layout: 0
            }
        ),
        Err(GenerationError::RuleDrawLimit {
            layout: 1,
            limit: 0
        })
    );
}

#[test]
fn panel_caps_are_inclusive_and_reveals_do_not_consume_space() {
    let puzzle = generate(4).unwrap();
    let a = puzzle
        .solution
        .card_at(Category::ArabicDigits, Column::ALL[0]);
    let b = puzzle.solution.card_at(Category::Dice, Column::ALL[0]);
    let c = puzzle.solution.card_at(Category::Letters, Column::ALL[1]);
    let horizontal = Rule::Near(a, c);
    let vertical = Rule::SameColumn(a, b);
    let opening = Rule::Reveal {
        card: a,
        column: Column::ALL[0],
    };
    let mut rules = [vec![horizontal; 24], vec![vertical; 15], vec![opening; 36]].concat();
    assert!(fits_historical_panels(&rules));
    rules.push(vertical);
    assert!(!fits_historical_panels(&rules));
    rules.pop();
    rules.push(horizontal);
    assert!(!fits_historical_panels(&rules));
}

#[test]
fn emitted_puzzles_are_true_solvable_and_locally_irredundant() {
    for seed in [0, 1, 2, 3, 4, 5, 23, 20261123, u64::MAX] {
        let puzzle = generate(seed).unwrap_or_else(|err| panic!("seed {seed}: {err}"));
        assert_eq!(puzzle.version, GENERATOR_VERSION);
        assert_eq!(puzzle.seed, seed);
        assert!(puzzle.layouts_attempted > 0);
        assert!(fits_historical_panels(&puzzle.rules));
        assert!(puzzle.rules.iter().all(|rule| rule.holds(&puzzle.solution)));
        let distinct: HashSet<_> = puzzle.rules.iter().collect();
        assert_eq!(distinct.len(), puzzle.rules.len());
        assert_eq!(
            historical(&puzzle.rules, Some(&puzzle.solution))
                .unwrap()
                .candidates
                .resolved()
                .as_ref(),
            Some(&puzzle.solution)
        );
        for i in 0..puzzle.rules.len() {
            let mut reduced = puzzle.rules.clone();
            reduced.remove(i);
            assert_eq!(
                historical(&reduced, Some(&puzzle.solution))
                    .unwrap()
                    .candidates
                    .resolved(),
                None,
                "retained rule {i} is removable with seed {seed}"
            );
        }
    }
}

#[test]
fn a_seed_replays_the_full_ordered_result() {
    let puzzle = generate(42).unwrap();
    assert_eq!(puzzle, generate(42).unwrap());
    assert_ne!(puzzle.solution, generate(43).unwrap().solution);
    assert_eq!(puzzle.layouts_attempted, 1);
    let rows = Category::ALL.map(|row| puzzle.solution.row(row).map(|value| value.number()));
    assert_eq!(
        rows,
        [
            [5, 4, 1, 2, 3, 6],
            [6, 3, 2, 1, 5, 4],
            [3, 2, 4, 6, 1, 5],
            [1, 6, 5, 4, 3, 2],
            [3, 6, 2, 4, 5, 1],
            [1, 4, 2, 6, 3, 5]
        ]
    );
    let key = |card: einstein_core::Card| {
        format!(
            "{}{}",
            char::from(b'A' + card.category.index() as u8),
            card.value.number()
        )
    };
    let codes: Vec<_> = puzzle
        .rules
        .iter()
        .map(|&rule| match rule {
            Rule::SameColumn(a, b) => format!("S{}{}", key(a), key(b)),
            Rule::LeftOf { left, right } => format!("L{}{}", key(left), key(right)),
            Rule::Near(a, b) => format!("N{}{}", key(a), key(b)),
            Rule::Between {
                middle,
                ends: [a, b],
            } => format!("B{}{}{}", key(a), key(middle), key(b)),
            Rule::Reveal { card, column } => format!("R{}{}", key(card), column.number()),
        })
        .collect();
    assert_eq!(
        codes,
        [
            "SE5F3", "LF6B5", "NE3F4", "BE4A3C5", "NB3F1", "NA6C1", "SB1C6", "BC6D5F4", "LB3D4",
            "LF2F6", "SD1B6", "BF6F3C5", "BA2A3C5", "SD6C2", "BE2A4D1", "BD4F2A4", "BA1A4F1",
            "LF4B4", "SF5B4", "SC3D1", "NE1D3",
        ]
    );
}
