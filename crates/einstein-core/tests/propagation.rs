use einstein_core::propagation::{Candidates, Contradiction, PropagationErrorKind, historical};
use einstein_core::{Card, Category, Column, Grid, Rule, SIZE, Value};

use Category::{ArabicDigits as A, Dice as D, Letters as B, RomanNumerals as C};

fn col(n: u8) -> Column {
    Column::new(n).unwrap()
}

fn card(category: Category, n: u8) -> Card {
    Card::new(category, Value::new(n).unwrap())
}

fn positions(board: &Candidates, card: Card) -> Vec<u8> {
    Column::ALL
        .into_iter()
        .filter(|&c| board.allows(card, c))
        .map(Column::number)
        .collect()
}

fn reveal(card: Card, n: u8) -> Rule {
    Rule::Reveal {
        card,
        column: col(n),
    }
}

#[test]
fn exclusion_propagates_a_cell_singleton_only_in_its_category() {
    let mut board = Candidates::full();
    for n in 2..=6 {
        assert!(board.exclude(card(A, n), col(1)).unwrap());
    }
    assert_eq!(positions(&board, card(A, 1)), [1]);
    assert_eq!(positions(&board, card(B, 1)), [1, 2, 3, 4, 5, 6]);
    assert!(!board.exclude(card(A, 2), col(1)).unwrap());
}

#[test]
fn exclusion_propagates_a_value_with_only_one_possible_column() {
    let mut board = Candidates::full();
    for n in 1..=5 {
        board.exclude(card(A, 6), col(n)).unwrap();
    }
    assert_eq!(
        board
            .values(A, col(6))
            .map(Value::number)
            .collect::<Vec<_>>(),
        [6]
    );
    assert_eq!(positions(&board, card(A, 6)), [6]);
}

#[test]
fn no_pair_or_matching_deductions_are_made_by_row_singles() {
    let mut board = Candidates::full();
    for column in [col(1), col(2)] {
        for n in 3..=6 {
            board.exclude(card(A, n), column).unwrap();
        }
    }
    // A stronger matching propagator would exclude 1 and 2 outside these cells.
    assert!(board.allows(card(A, 1), col(3)));
    assert!(board.allows(card(A, 2), col(3)));
}

#[test]
fn moves_that_would_break_board_invariants_are_atomic() {
    let mut board = Candidates::full();
    let selected = card(A, 1);
    board.select(selected, col(1)).unwrap();
    let before = board.clone();
    assert_eq!(
        board.exclude(selected, col(1)),
        Err(Contradiction::EmptyCell {
            category: A,
            column: col(1)
        })
    );
    assert_eq!(
        board.select(selected, col(2)),
        Err(Contradiction::ExcludedSelection {
            card: selected,
            column: col(2)
        })
    );
    assert_eq!(board, before);
    assert!(!board.select(selected, col(1)).unwrap());
}

#[test]
fn rules_respect_order_and_repeat_to_a_fixed_point() {
    let a = card(A, 1);
    let b = card(B, 2);
    let c = card(C, 3);
    let reverse_chain = [Rule::SameColumn(b, c), Rule::SameColumn(a, b), reveal(a, 1)];
    let forward_chain = [reveal(a, 1), Rule::SameColumn(a, b), Rule::SameColumn(b, c)];
    let reverse = historical(&reverse_chain, None).unwrap();
    let forward = historical(&forward_chain, None).unwrap();
    assert_eq!(positions(&reverse.candidates, c), [1]);
    assert_eq!(reverse.candidates, forward.candidates);
    assert_eq!(reverse.passes, 4);
    assert_eq!(forward.passes, 2);
}

#[test]
fn initial_openings_do_not_apply_any_visible_rules() {
    let a = card(A, 1);
    let b = card(B, 2);
    let rules = [reveal(a, 1), Rule::Near(a, b)];
    assert_eq!(
        positions(&Candidates::for_play(&rules).unwrap(), b),
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        positions(&historical(&rules, None).unwrap().candidates, b),
        [2]
    );
}

#[test]
fn left_of_keeps_strict_bounds_without_requiring_adjacency() {
    let left = card(A, 1);
    let right = card(B, 2);
    let rule = Rule::LeftOf { left, right };
    let board = historical(&[rule], None).unwrap().candidates;
    assert_eq!(positions(&board, left), [1, 2, 3, 4, 5]);
    assert_eq!(positions(&board, right), [2, 3, 4, 5, 6]);
    let board = historical(&[rule, reveal(left, 2), reveal(right, 6)], None)
        .unwrap()
        .candidates;
    assert_eq!(positions(&board, right), [6]);
}

#[test]
fn between_prunes_supports_without_choosing_an_orientation() {
    let a = card(A, 1);
    let middle = card(B, 2);
    let b = card(C, 3);
    let rule = Rule::Between {
        middle,
        ends: [a, b],
    };
    let lone = historical(&[rule], None).unwrap().candidates;
    assert_eq!(positions(&lone, middle), [2, 3, 4, 5]);
    let with_middle = historical(&[rule, reveal(middle, 4)], None)
        .unwrap()
        .candidates;
    assert_eq!(positions(&with_middle, a), [3, 5]);
    assert_eq!(positions(&with_middle, b), [3, 5]);
    let fixed = historical(&[rule, reveal(middle, 4), reveal(a, 5)], None)
        .unwrap()
        .candidates;
    assert_eq!(positions(&fixed, b), [3]);
}

#[test]
fn target_is_checked_but_never_used_to_infer_a_move() {
    let target = Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap();
    let empty = historical(&[], Some(&target)).unwrap();
    assert_eq!(empty.candidates, Candidates::full());
    assert_eq!(empty.passes, 1);
    assert_eq!(empty.candidates.resolved(), None);
    let err = historical(&[reveal(card(A, 1), 2)], Some(&target)).unwrap_err();
    assert_eq!(err.rule_index, 0);
    assert_eq!(err.kind, PropagationErrorKind::TargetEliminated);
}

#[test]
fn contradictory_reveals_and_degenerate_rules_are_reported() {
    let a = card(A, 1);
    let err = historical(&[reveal(a, 1), reveal(a, 2)], None).unwrap_err();
    assert_eq!(err.rule_index, 1);
    assert_eq!(
        err.kind,
        PropagationErrorKind::Contradiction(Contradiction::ExcludedSelection {
            card: a,
            column: col(2)
        })
    );
    let near_itself = Rule::Near(a, a);
    let err = historical(&[near_itself], None).unwrap_err();
    assert_eq!(
        err.kind,
        PropagationErrorKind::Contradiction(Contradiction::IncompatibleRule(near_itself))
    );
}

#[test]
fn each_true_rule_preserves_the_target_even_with_repeated_categories() {
    let target = Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap();
    for first in Category::ALL {
        for second in Category::ALL {
            for third in Category::ALL {
                let rules = [
                    Rule::Near(card(first, 4), card(second, 5)),
                    Rule::SameColumn(card(first, 3), card(second, 3)),
                    Rule::LeftOf {
                        left: card(first, 2),
                        right: card(second, 5),
                    },
                    Rule::Between {
                        middle: card(second, 4),
                        ends: [card(first, 5), card(third, 3)],
                    },
                    reveal(card(first, 5), 5),
                ];
                assert!(rules.iter().all(|rule| rule.holds(&target)));
                assert!(
                    historical(&rules, Some(&target))
                        .unwrap()
                        .candidates
                        .includes(&target)
                );
            }
        }
    }
}

#[test]
fn fixed_point_can_produce_a_known_grid_without_search() {
    let target = Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap();
    let mut rules = vec![
        Rule::Near(card(D, 1), card(B, 2)),
        Rule::Between {
            middle: card(C, 5),
            ends: [card(A, 4), card(B, 6)],
        },
    ];
    for category in Category::ALL {
        for col in Column::ALL[..5].iter().copied() {
            rules.push(Rule::Reveal {
                card: target.card_at(category, col),
                column: col,
            });
        }
    }
    let result = historical(&rules, Some(&target)).unwrap();
    assert_eq!(result.candidates.resolved().as_ref(), Some(&target));
    assert!(rules.iter().all(|r| r.holds(&target)));
}
