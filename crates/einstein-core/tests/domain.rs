use einstein_core::{Card, Category, Column, Grid, GridError, Rule, RulePlacement, SIZE, Value};

fn card(category: Category, number: u8) -> Card {
    Card::new(category, Value::new(number).unwrap())
}

fn ordered() -> Grid {
    Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap()
}

#[test]
fn category_order_and_column_indices_are_stable() {
    assert_eq!(
        Category::ALL,
        [
            Category::ArabicDigits,
            Category::Letters,
            Category::RomanNumerals,
            Category::Dice,
            Category::Shapes,
            Category::MathSymbols,
        ]
    );
    for (i, category) in Category::ALL.into_iter().enumerate() {
        assert_eq!(category.index(), i);
        assert_eq!(Column::ALL[i].index(), i);
        assert_eq!(Column::ALL[i].number(), i as u8 + 1);
    }
}

#[test]
fn coordinates_and_values_reject_every_out_of_bounds_byte() {
    for number in u8::MIN..=u8::MAX {
        let valid = (1..=6).contains(&number);
        assert_eq!(Column::new(number).is_some(), valid);
        assert_eq!(Value::new(number).is_some(), valid);
    }
}

#[test]
fn grid_reports_a_bad_value_with_its_location() {
    let mut rows = [[1, 2, 3, 4, 5, 6]; SIZE];
    rows[Category::Shapes.index()][4] = 0;
    assert_eq!(
        Grid::try_from(rows),
        Err(GridError::InvalidValue {
            category: Category::Shapes,
            column: Column::new(5).unwrap(),
            number: 0,
        })
    );
}

#[test]
fn permutations_are_checked_per_category_not_per_column() {
    let mut rows = [[1, 2, 3, 4, 5, 6]; SIZE];
    assert!(Grid::try_from(rows).is_ok());
    rows[Category::Dice.index()][5] = 1;
    assert_eq!(
        Grid::try_from(rows),
        Err(GridError::DuplicateValue {
            category: Category::Dice,
            value: Value::new(1).unwrap(),
        })
    );
}

#[test]
fn positions_and_values_round_trip_on_a_nontrivial_grid() {
    let mut rows = [Value::ALL; SIZE];
    for category in Category::ALL {
        rows[category.index()].rotate_left(category.index());
    }
    let grid = Grid::from_rows(rows).unwrap();
    for (category, expected) in Category::ALL.into_iter().zip(rows) {
        assert_eq!(grid.row(category), &expected);
        for (column, value) in Column::ALL.into_iter().zip(expected) {
            let card = Card::new(category, value);
            assert_eq!(grid.card_at(category, column), card);
            assert_eq!(grid.column_of(card), column);
        }
    }
}

#[test]
fn binary_rules_match_all_column_pairs_and_keep_endpoint_identity() {
    let grid = ordered();
    for a in 1u8..=6 {
        for b in 1u8..=6 {
            let first = card(Category::ArabicDigits, a);
            let second = card(Category::Letters, b);
            let same = Rule::SameColumn(first, second);
            let near = Rule::Near(first, second);
            let left = Rule::LeftOf {
                left: first,
                right: second,
            };
            assert_eq!(same.holds(&grid), a == b);
            assert_eq!(
                same.holds(&grid),
                Rule::SameColumn(second, first).holds(&grid)
            );
            assert_eq!(
                near.holds(&grid),
                [-1, 1].contains(&(i16::from(a) - i16::from(b)))
            );
            assert_eq!(near.holds(&grid), Rule::Near(second, first).holds(&grid));
            assert_eq!(left.holds(&grid), a < b);
            assert_ne!(near, Rule::Near(second, first));
            assert_eq!(same.placement(), RulePlacement::Vertical);
            assert_eq!(near.placement(), RulePlacement::Horizontal);
            assert_eq!(left.placement(), RulePlacement::Horizontal);
        }
    }
}

#[test]
fn between_matches_all_triples_in_both_orientations() {
    let grid = ordered();
    for a in 1..=6 {
        for b in 1..=6 {
            for center in 1..=6 {
                let middle = card(Category::RomanNumerals, center);
                let first = card(Category::ArabicDigits, a);
                let second = card(Category::Letters, b);
                let forward = Rule::Between {
                    middle,
                    ends: [first, second],
                };
                let reverse = Rule::Between {
                    middle,
                    ends: [second, first],
                };
                let ordered = [a.min(b), center, a.max(b)];
                let expected = [[1, 2, 3], [2, 3, 4], [3, 4, 5], [4, 5, 6]].contains(&ordered);
                assert_eq!(forward.holds(&grid), expected);
                assert_eq!(reverse.holds(&grid), expected);
                assert_ne!(forward, reverse);
                assert_eq!(forward.placement(), RulePlacement::Horizontal);
            }
        }
    }
}

#[test]
fn relations_may_reference_the_same_category() {
    let grid = ordered();
    let first = card(Category::Dice, 1);
    let second = card(Category::Dice, 2);
    let third = card(Category::Dice, 3);
    assert!(Rule::Near(first, second).holds(&grid));
    assert!(!Rule::Near(first, first).holds(&grid));
    assert!(
        Rule::Between {
            middle: second,
            ends: [first, third]
        }
        .holds(&grid)
    );
}

#[test]
fn reveals_pin_an_exact_column_without_becoming_visible_hints() {
    let grid = ordered();
    for category in Category::ALL {
        for value in Value::ALL {
            for column in Column::ALL {
                let rule = Rule::Reveal {
                    card: Card::new(category, value),
                    column,
                };
                assert_eq!(rule.placement(), RulePlacement::InitialReveal);
                assert_eq!(rule.holds(&grid), value.number() == column.number());
            }
        }
    }
}
