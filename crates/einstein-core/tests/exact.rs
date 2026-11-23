use einstein_core::exact::{Multiplicity, count_solutions};
use einstein_core::export::{write_generated_json, write_rules_json};
use einstein_core::generator::generate;
use einstein_core::{Card, Category, Column, Grid, Rule, SIZE, Value};

fn card(category: Category, number: u8) -> Card {
    Card::new(category, Value::new(number).unwrap())
}

fn reveal(card: Card, column: u8) -> Rule {
    Rule::Reveal {
        card,
        column: Column::new(column).unwrap(),
    }
}

#[test]
fn conflicting_reveals_and_false_relations_are_unsatisfiable() {
    let a = card(Category::ArabicDigits, 1);
    let b = card(Category::Letters, 2);
    for rules in [
        vec![reveal(a, 1), reveal(a, 2)],
        vec![reveal(a, 1), reveal(b, 3), Rule::Near(a, b)],
        vec![Rule::Near(a, a)],
    ] {
        let result = count_solutions(&rules, 8);
        assert_eq!(result.multiplicity, Multiplicity::Zero);
        assert!(result.witnesses.is_empty());
    }
}

#[test]
fn impossible_row_matching_is_detected_even_with_no_empty_domains_initially() {
    let first = card(Category::ArabicDigits, 1);
    let second = card(Category::ArabicDigits, 2);
    let companion = card(Category::Letters, 3);
    let rules = [
        Rule::SameColumn(first, companion),
        Rule::SameColumn(second, companion),
        reveal(companion, 3),
    ];
    assert_eq!(count_solutions(&rules, 1).multiplicity, Multiplicity::Zero);
}

#[test]
fn a_unique_grid_returns_a_witness_even_at_the_exact_node_limit() {
    let grid = Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap();
    let rules = Category::ALL
        .into_iter()
        .flat_map(|cat| {
            let grid = &grid;
            Column::ALL
                .into_iter()
                .take(5)
                .map(move |column| reveal(grid.card_at(cat, column), column.number()))
        })
        .collect::<Vec<_>>();
    let result = count_solutions(&rules, 1);
    assert_eq!(result.multiplicity, Multiplicity::One);
    assert_eq!(result.nodes, 1);
    assert_eq!(result.witnesses, [grid]);
    assert!(rules.iter().all(|rule| rule.holds(&result.witnesses[0])));
}

#[test]
fn a_budget_can_leave_zero_or_one_witness_without_proving_uniqueness() {
    let complete = count_solutions(&[], 100);
    assert_eq!(complete.multiplicity, Multiplicity::AtLeastTwo);
    assert_eq!(complete.witnesses.len(), 2);
    assert_ne!(complete.witnesses[0], complete.witnesses[1]);
    let partial = count_solutions(&[], complete.nodes - 1);
    assert_eq!(partial.multiplicity, Multiplicity::Indeterminate);
    assert_eq!(partial.witnesses, complete.witnesses[..1]);
    let empty = count_solutions(&[], 0);
    assert_eq!(empty.multiplicity, Multiplicity::Indeterminate);
    assert!(empty.witnesses.is_empty());
    assert_eq!(empty.nodes, 0);
}

#[test]
fn generated_rules_are_counted_without_the_hidden_grid() {
    for seed in [0, 6, 42, 500, u64::MAX] {
        let puzzle = generate(seed).unwrap();
        let result = count_solutions(&puzzle.rules, 10_000);
        assert_eq!(result.multiplicity, Multiplicity::One, "seed {seed}");
        assert_eq!(result.witnesses, [puzzle.solution]);
    }
}

#[test]
fn json_exports_all_rule_kinds_in_oracle_order() {
    let a = card(Category::ArabicDigits, 5);
    let b = card(Category::RomanNumerals, 4);
    let c = card(Category::Dice, 6);
    let mut json = Vec::new();
    write_rules_json(
        &mut json,
        &[
            Rule::SameColumn(a, b),
            Rule::Near(b, c),
            Rule::LeftOf { left: a, right: c },
            Rule::Between {
                middle: c,
                ends: [a, b],
            },
            reveal(b, 3),
        ],
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(json).unwrap(),
        concat!(
            "{\"rules\":[",
            "{\"kind\":\"same_column\",\"cards\":[[1,5],[3,4]]},",
            "{\"kind\":\"near\",\"cards\":[[3,4],[4,6]]},",
            "{\"kind\":\"left_of\",\"cards\":[[1,5],[4,6]]},",
            "{\"kind\":\"between\",\"cards\":[[1,5],[4,6],[3,4]]},",
            "{\"kind\":\"reveal\",\"cards\":[[3,4]],\"column\":3}]}"
        )
    );
    let mut json = Vec::new();
    let mut puzzle = generate(2).unwrap();
    // Serialize a minimal synthetic object to check precisely what is public.
    puzzle.rules.clear();
    puzzle.layouts_attempted = 1;
    write_generated_json(&mut json, &puzzle).unwrap();
    assert_eq!(
        String::from_utf8(json).unwrap(),
        r#"{"seed":2,"generator_version":1,"layouts_attempted":1,"rules":[]}"#
    );
}
