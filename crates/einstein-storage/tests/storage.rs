use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use einstein_core::generator::GeneratedPuzzle;
use einstein_core::session::{Command, Effect, HintId, HintView, Session, Status};
use einstein_core::{Card, Category, Column, Grid, Rule, SIZE, Value};
use einstein_storage::{Error, FORMAT_VERSION, MAX_BYTES, decode, encode, load, load_into, save};

fn card(category: Category, n: u8) -> Card {
    Card::new(category, Value::new(n).unwrap())
}
fn col(n: u8) -> Column {
    Column::new(n).unwrap()
}

fn fixture() -> Session {
    use Category::{ArabicDigits as A, Letters as B, RomanNumerals as C};
    Session::from_puzzle(GeneratedPuzzle {
        seed: u64::MAX,
        version: 1,
        layouts_attempted: 2,
        solution: Grid::try_from([[1, 2, 3, 4, 5, 6]; SIZE]).unwrap(),
        rules: vec![
            Rule::Reveal {
                card: card(A, 1),
                column: col(1),
            },
            Rule::Near(card(A, 2), card(B, 3)),
            Rule::SameColumn(card(A, 3), card(B, 3)),
            Rule::LeftOf {
                left: card(B, 2),
                right: card(C, 6),
            },
            Rule::Between {
                middle: card(B, 3),
                ends: [card(A, 4), card(C, 2)],
            },
        ],
    })
    .unwrap()
}

#[test]
fn all_fields_round_trip_and_restart_keeps_the_original_puzzle() {
    let mut session = fixture();
    session
        .apply(Command::Select {
            card: card(Category::Dice, 3),
            column: col(3),
        })
        .unwrap();
    session
        .apply(Command::ToggleHint { id: HintId(1) })
        .unwrap();
    session.apply(Command::SwitchHints).unwrap();
    session
        .apply(Command::AdvanceClock { millis: 1337 })
        .unwrap();
    session.apply(Command::Pause).unwrap();
    let payload = encode(&session).unwrap();
    let mut copy = decode(&payload).unwrap();
    assert_eq!(copy.snapshot(), session.snapshot());
    assert_eq!(copy.snapshot().unwrap().hint_view, HintView::Dismissed);
    assert_eq!(encode(&copy).unwrap(), payload);
    copy.apply(Command::Restart).unwrap();
    assert_eq!(copy.snapshot(), fixture().snapshot());
}

#[test]
fn restore_does_not_regenerate_with_current_version_or_keep_score_eligible() {
    let mut session = Session::new();
    session.apply(Command::New { seed: 42 }).unwrap();
    let mut previous = session.snapshot().unwrap();
    assert!(previous.score_eligible);
    let mut payload = encode(&session).unwrap();
    // A different generator version is accepted when its whole puzzle is present.
    payload[6..8].copy_from_slice(&56u16.to_le_bytes());
    let restored = decode(&payload).unwrap();
    previous.score_eligible = false;
    assert_eq!(restored.snapshot(), Some(previous));
    assert_eq!(restored.save_image().unwrap().puzzle.version, 56);
}

#[test]
fn failed_and_won_status_can_be_saved_but_wrongly_marked_won_is_rejected() {
    let mut failed = fixture();
    assert_eq!(
        failed.apply(Command::Exclude {
            card: card(Category::Dice, 3),
            column: col(3)
        }),
        Ok(Effect::Failed)
    );
    assert_eq!(decode(&encode(&failed).unwrap()).unwrap(), failed);
    let mut image = failed.save_image().unwrap();
    image.status = Status::Won;
    assert!(Session::restore(image).is_err());
    let mut won = fixture();
    let target = won.save_image().unwrap().puzzle.solution;
    for category in Category::ALL {
        for column in Column::ALL {
            if won.snapshot().unwrap().status != Status::Won {
                won.apply(Command::Select {
                    card: target.card_at(category, column),
                    column,
                })
                .unwrap();
            }
        }
    }
    assert_eq!(won.snapshot().unwrap().status, Status::Won);
    assert_eq!(decode(&encode(&won).unwrap()).unwrap(), won);
}

#[test]
fn malformed_data_rejects_bounds_and_never_panics_on_truncated_prefixes() {
    let bytes = encode(&fixture()).unwrap();
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len]).is_err(), "prefix {len}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode(&trailing),
        Err(Error::Invalid("trailing bytes"))
    ));
    assert!(matches!(
        decode(&vec![0; MAX_BYTES + 1]),
        Err(Error::TooLarge)
    ));
    for (offset, value) in [
        (0, 33),
        (28, 9),
        (29, 2),
        (30, 7),
        (104, 7),
        (105, 0),
        (106, 7),
        (107, 0),
        (108, 2),
    ] {
        let mut tampered = bytes.clone();
        tampered[offset] = value;
        assert!(
            matches!(decode(&tampered), Err(Error::Invalid(_))),
            "offset {offset}"
        );
    }
    let mut oversized = bytes.clone();
    oversized[102..104].copy_from_slice(&257u16.to_le_bytes());
    assert!(matches!(decode(&oversized), Err(Error::TooLarge)));
    let mut unknown_format = bytes;
    unknown_format[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert!(
        matches!(decode(&unknown_format), Err(Error::UnsupportedVersion(v)) if v == FORMAT_VERSION + 1)
    );
}

#[test]
fn well_formed_but_inconsistent_state_is_rejected() {
    let bytes = encode(&fixture()).unwrap();
    let mut claimed_win = bytes.clone();
    claimed_win[28] = 3;
    assert!(matches!(decode(&claimed_win), Err(Error::Restore(_))));
    let mut dismissed_reveal = bytes.clone();
    dismissed_reveal[108] = 1;
    assert!(matches!(decode(&dismissed_reveal), Err(Error::Restore(_))));
    let mut invalid_mask = bytes.clone();
    invalid_mask[66] = 0x80;
    assert!(matches!(decode(&invalid_mask), Err(Error::Restore(_))));
    let mut reopened = bytes.clone();
    reopened[66..102].fill(0x3f);
    assert!(matches!(decode(&reopened), Err(Error::Restore(_))));
    let mut false_rule = bytes;
    false_rule[107] = 2;
    assert!(matches!(decode(&false_rule), Err(Error::Restore(_))));
}

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static RUN: AtomicU32 = AtomicU32::new(0);
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&root).unwrap();
        loop {
            let tag = RUN.fetch_add(1, Ordering::Relaxed);
            let dir = root.join(format!("storage-{}-{tag}", std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => return Self(dir),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => panic!("{error}"),
            }
        }
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn disk_replacement_is_atomic_for_readers_and_preserves_private_permissions() {
    let dir = TestDir::new();
    let file = dir.0.join("slot.ezp");
    let first = fixture();
    let mut second = fixture();
    second
        .apply(Command::AdvanceClock { millis: 3000 })
        .unwrap();
    save(&first, &file).unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..8 {
                save(&second, &file).unwrap();
                save(&first, &file).unwrap();
            }
        });
        for _ in 0..100 {
            let now = load(&file).unwrap();
            assert!(now == first || now == second);
        }
    });
    save(&second, &file).unwrap();
    assert_eq!(load(&file).unwrap(), second);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_load_and_save_do_not_destroy_current_game_or_existing_target() {
    let dir = TestDir::new();
    let slot = dir.0.join("slot.ezp");
    fs::write(&slot, b"bad").unwrap();
    let mut live = fixture();
    assert!(load_into(&mut live, &slot).is_err());
    assert_eq!(live, fixture());
    assert!(matches!(save(&Session::new(), &slot), Err(Error::NoGame)));
    assert_eq!(fs::read(&slot).unwrap(), b"bad");
    let blocked = dir.0.join("directory");
    fs::create_dir(&blocked).unwrap();
    assert!(save(&live, &blocked).is_err());
    assert!(blocked.is_dir());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2); // no staging file left
}
