use einstein_core::generator::{GENERATOR_VERSION, GeneratedPuzzle, generate};
use einstein_core::session::{Command, Effect, Error, HintId, HintView, Session, Status};
use einstein_core::{Card, Category, Column, Grid, Rule, SIZE, Value};

use Category::{
    ArabicDigits as A, Dice as D, Letters as B, MathSymbols as F, RomanNumerals as C, Shapes as E,
};

fn card(category: Category, n: u8) -> Card {
    Card::new(category, Value::new(n).unwrap())
}

fn col(n: u8) -> Column {
    Column::new(n).unwrap()
}

fn fixture() -> GeneratedPuzzle {
    GeneratedPuzzle {
        seed: 9876,
        version: GENERATOR_VERSION,
        solution: Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap(),
        rules: vec![
            Rule::Near(card(C, 2), card(D, 3)),
            Rule::Reveal {
                card: card(A, 1),
                column: col(1),
            },
            Rule::SameColumn(card(A, 1), card(B, 1)),
            Rule::Between {
                middle: card(C, 4),
                ends: [card(E, 5), card(F, 3)],
            },
        ],
        layouts_attempted: 1,
    }
}

#[test]
fn empty_game_lifecycle_and_generating_from_a_seed() {
    let mut session = Session::new();
    assert!(session.snapshot().is_none());
    assert_eq!(session.apply(Command::Restart), Err(Error::NoGame));
    assert_eq!(session.apply(Command::End), Ok(Effect::Unchanged));
    let seed = 42;
    assert_eq!(session.apply(Command::New { seed }), Ok(Effect::Changed));
    let expected = generate(seed).unwrap();
    let state = session.snapshot().unwrap();
    assert_eq!(state.status, Status::Playing);
    assert!(state.score_eligible);
    assert!(state.candidates.includes(&expected.solution));
    assert_eq!(session.apply(Command::End), Ok(Effect::Changed));
    assert!(session.snapshot().is_none());
}

#[test]
fn openings_set_candidates_without_using_any_visible_rule() {
    let session = Session::from_puzzle(fixture()).unwrap();
    let state = session.snapshot().unwrap();
    assert_eq!(state.candidates.mask(A, col(1)), 0b000001);
    assert_eq!(state.candidates.mask(B, col(1)), 0b111111);
    assert_eq!(
        state.hints.iter().map(|h| h.id.0).collect::<Vec<_>>(),
        [0, 2, 3]
    );
    assert!(!state.score_eligible);
}

#[test]
fn valid_clicks_update_one_category_and_ignore_inactive_candidates() {
    let mut session = Session::from_puzzle(fixture()).unwrap();
    let before = session.snapshot().unwrap();
    assert_eq!(
        session.apply(Command::Select {
            card: card(A, 6),
            column: col(1)
        }),
        Ok(Effect::Unchanged)
    );
    assert_eq!(
        session.apply(Command::Exclude {
            card: card(A, 1),
            column: col(1)
        }),
        Ok(Effect::Unchanged)
    );
    assert_eq!(session.snapshot().as_ref(), Some(&before));
    assert_eq!(
        session.apply(Command::Exclude {
            card: card(D, 6),
            column: col(3)
        }),
        Ok(Effect::Changed)
    );
    assert_eq!(
        session.apply(Command::Select {
            card: card(D, 6),
            column: col(3)
        }),
        Ok(Effect::Unchanged)
    );
    assert_eq!(
        session.apply(Command::Select {
            card: card(D, 3),
            column: col(3)
        }),
        Ok(Effect::Changed)
    );
    let state = session.snapshot().unwrap();
    assert_eq!(state.candidates.mask(D, col(3)), 0b000100);
    assert!(!state.candidates.allows(card(D, 3), col(4)));
    assert_eq!(
        state.candidates.mask(C, col(2)),
        before.candidates.mask(C, col(2))
    );
    assert_eq!(state.status, Status::Playing);
}

#[test]
fn a_logically_possible_wrong_move_fails_immediately_against_the_target() {
    for wrong in [
        Command::Exclude {
            card: card(B, 2),
            column: col(2),
        },
        Command::Select {
            card: card(B, 3),
            column: col(2),
        },
    ] {
        let mut session = Session::from_puzzle(fixture()).unwrap();
        assert_eq!(session.apply(wrong), Ok(Effect::Failed));
        let failed = session.snapshot().unwrap();
        assert_eq!(failed.status, Status::Failed);
        assert!(!failed.candidates.allows(card(B, 2), col(2)));
        assert_eq!(session.apply(wrong), Err(Error::NotPlaying(Status::Failed)));
        assert_eq!(session.snapshot().unwrap(), failed);
    }
}

#[test]
fn hint_slots_survive_dismiss_switch_and_restore() {
    let mut session = Session::from_puzzle(fixture()).unwrap();
    let original = session.snapshot().unwrap();
    let unused_slot = HintId(1); // start reveal present in the rule list
    assert_eq!(
        session.apply(Command::ToggleHint { id: unused_slot }),
        Err(Error::UnknownHint(unused_slot))
    );
    let command = Command::ToggleHint { id: HintId(2) };
    assert_eq!(session.apply(command), Ok(Effect::Changed));
    assert_eq!(session.apply(command), Ok(Effect::Unchanged));
    let hidden = session.snapshot().unwrap();
    assert_eq!(hidden.candidates, original.candidates);
    assert_eq!(hidden.hints[1].rule, original.hints[1].rule);
    assert!(!hidden.hints[1].visible(HintView::Active));
    assert_eq!(session.apply(Command::SwitchHints), Ok(Effect::Changed));
    let alternate = session.snapshot().unwrap();
    assert_eq!(alternate.hint_view, HintView::Dismissed);
    assert!(alternate.hints[1].visible(alternate.hint_view));
    assert_eq!(session.apply(command), Ok(Effect::Changed));
    assert!(!session.snapshot().unwrap().hints[1].visible(HintView::Dismissed));
    assert_eq!(session.apply(Command::SwitchHints), Ok(Effect::Changed));
    assert_eq!(session.snapshot().unwrap(), original);
}

#[test]
fn timer_freezes_on_pause_and_final_outcomes_and_restart_resets_state() {
    let mut session = Session::from_puzzle(fixture()).unwrap();
    let original = session.snapshot().unwrap();
    session
        .apply(Command::AdvanceClock { millis: 1200 })
        .unwrap();
    assert_eq!(session.apply(Command::Pause), Ok(Effect::Changed));
    assert_eq!(session.apply(Command::Pause), Ok(Effect::Unchanged));
    assert_eq!(
        session.apply(Command::AdvanceClock { millis: 300 }),
        Ok(Effect::Unchanged)
    );
    assert_eq!(
        session.apply(Command::SwitchHints),
        Err(Error::NotPlaying(Status::Paused))
    );
    assert_eq!(session.snapshot().unwrap().elapsed_ms, 1200);
    assert_eq!(session.apply(Command::Resume), Ok(Effect::Changed));
    session.apply(Command::AdvanceClock { millis: 1 }).unwrap();
    session
        .apply(Command::ToggleHint { id: HintId(2) })
        .unwrap();
    session.apply(Command::SwitchHints).unwrap();
    assert_eq!(
        session.apply(Command::Exclude {
            card: card(B, 1),
            column: col(1)
        }),
        Ok(Effect::Failed)
    );
    assert_eq!(
        session.apply(Command::AdvanceClock { millis: 300 }),
        Ok(Effect::Unchanged)
    );
    assert_eq!(session.snapshot().unwrap().elapsed_ms, 1201);
    assert_eq!(session.apply(Command::Restart), Ok(Effect::Changed));
    assert_eq!(session.snapshot().unwrap(), original);
}

#[test]
fn reaching_the_canonical_solution_emits_one_win() {
    let puzzle = fixture();
    let mut session = Session::from_puzzle(puzzle.clone()).unwrap();
    let mut wins = 0;
    for category in Category::ALL {
        for column in Column::ALL.into_iter().take(5) {
            if session.snapshot().unwrap().status != Status::Won {
                let effect = session
                    .apply(Command::Select {
                        column,
                        card: puzzle.solution.card_at(category, column),
                    })
                    .unwrap();
                if effect == Effect::Won {
                    wins += 1;
                }
            }
        }
    }
    assert_eq!(wins, 1);
    let state = session.snapshot().unwrap();
    assert_eq!(state.status, Status::Won);
    assert_eq!(state.candidates.resolved(), Some(puzzle.solution));
    assert_eq!(session.apply(Command::Resume), Ok(Effect::Unchanged));
    assert_eq!(
        session.apply(Command::Exclude {
            card: card(F, 1),
            column: col(3)
        }),
        Err(Error::NotPlaying(Status::Won))
    );
}

#[test]
fn invalid_fixture_is_rejected_and_debug_output_has_no_active_solution_field() {
    let mut invalid = fixture();
    invalid.rules.push(Rule::Near(card(B, 1), card(C, 4)));
    assert_eq!(
        Session::from_puzzle(invalid),
        Err(Error::RuleFalse { index: 4 })
    );
    let debug = format!("{:?}", Session::from_puzzle(fixture()).unwrap());
    assert!(!debug.contains("solution"));
    assert!(!debug.contains("9876"));
}
