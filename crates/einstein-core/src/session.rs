//! One state machine for the native GUI and the command adapters.
//!
//! Public projections expose only player-facing state; the canonical grid
//! never participates in deductions, only in detecting a wrong move.

use crate::generator::{GeneratedPuzzle, GenerationError, generate};
use crate::propagation::{Candidates, Contradiction};
use crate::{Card, Category, Column, Rule, RulePlacement, SIZE};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Playing,
    Paused,
    Failed,
    Won,
}

/// A stable slot in a puzzle's rule order. Revelation slots are not hints.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HintId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HintView {
    Active,
    Dismissed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    New {
        seed: u64,
    },
    Restart,
    End,
    Select {
        card: Card,
        column: Column,
    },
    Exclude {
        card: Card,
        column: Column,
    },
    ToggleHint {
        id: HintId,
    },
    SwitchHints,
    Pause,
    Resume,
    /// Elapsed monotonic time supplied by the host; not an input/click event.
    AdvanceClock {
        millis: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Unchanged,
    Changed,
    Failed,
    Won,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NoGame,
    NotPlaying(Status),
    UnknownHint(HintId),
    Generation(GenerationError),
    RuleFalse { index: usize },
    InitialContradiction(Contradiction),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGame => f.write_str("no current game"),
            Self::NotPlaying(status) => write!(f, "game is not playing: {status:?}"),
            Self::UnknownHint(id) => write!(f, "unknown hint slot {}", id.0),
            Self::Generation(error) => write!(f, "generation: {error}"),
            Self::RuleFalse { index } => {
                write!(f, "rule {index} is false on the puzzle's solution")
            }
            Self::InitialContradiction(error) => write!(f, "invalid openings: {error:?}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hint {
    pub id: HintId,
    pub rule: Rule,
    pub dismissed: bool,
}

impl Hint {
    pub const fn visible(self, view: HintView) -> bool {
        matches!(
            (self.dismissed, view),
            (false, HintView::Active) | (true, HintView::Dismissed)
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub status: Status,
    pub candidates: Candidates,
    pub hints: Vec<Hint>,
    pub hint_view: HintView,
    pub elapsed_ms: u64,
    /// Historical restarts/loaded games are excluded from high scores.
    pub score_eligible: bool,
}

/// Private save payload. Unlike `Snapshot`, this includes the hidden puzzle.
/// Do not use it as an adapter's public response.
#[derive(Clone, Eq, PartialEq)]
pub struct SaveImage {
    pub puzzle: GeneratedPuzzle,
    pub masks: [[u8; SIZE]; SIZE],
    pub dismissed: Vec<bool>,
    pub hint_view: HintView,
    pub elapsed_ms: u64,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreError {
    InvalidPuzzle(Error),
    InvalidGeneratorMetadata,
    InvalidCandidateRow(Category),
    CandidatesOutsideOpening,
    InvalidHintFlags,
    StatusMismatch,
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPuzzle(error) => write!(f, "invalid saved puzzle: {error}"),
            Self::InvalidGeneratorMetadata => f.write_str("invalid saved generator metadata"),
            Self::InvalidCandidateRow(category) => write!(f, "invalid candidates in {category:?}"),
            Self::CandidatesOutsideOpening => f.write_str("candidates contradict initial openings"),
            Self::InvalidHintFlags => f.write_str("dismissed hint flags do not match rule slots"),
            Self::StatusMismatch => f.write_str("saved status does not match the board"),
        }
    }
}
impl std::error::Error for RestoreError {}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct Session {
    game: Option<Game>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("game", &self.snapshot())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
struct Game {
    puzzle: GeneratedPuzzle,
    initial: Candidates,
    candidates: Candidates,
    status: Status,
    dismissed: Vec<bool>,
    hint_view: HintView,
    elapsed_ms: u64,
    score_eligible: bool,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply an existing puzzle. Checks semantic truth and the opening board;
    /// no generation or search is run. A score is only eligible after `New`.
    pub fn from_puzzle(puzzle: GeneratedPuzzle) -> Result<Self, Error> {
        Ok(Self {
            game: Some(Game::new(puzzle, false)?),
        })
    }

    /// Internal persistence bridge; no host or format dependency.
    pub fn save_image(&self) -> Option<SaveImage> {
        let game = self.game.as_ref()?;
        Some(SaveImage {
            puzzle: game.puzzle.clone(),
            masks: game.candidates.masks(),
            dismissed: game.dismissed.clone(),
            hint_view: game.hint_view,
            elapsed_ms: game.elapsed_ms,
            status: game.status,
        })
    }

    /// Reconstruct the board from private persisted data, not the current RNG.
    /// A loaded game is no longer eligible for a historical high score.
    pub fn restore(image: SaveImage) -> Result<Self, RestoreError> {
        if image.puzzle.version == 0 || image.puzzle.layouts_attempted == 0 {
            return Err(RestoreError::InvalidGeneratorMetadata);
        }
        let mut game = Game::new(image.puzzle, false).map_err(RestoreError::InvalidPuzzle)?;
        if game.dismissed.len() != image.dismissed.len()
            || game
                .puzzle
                .rules
                .iter()
                .zip(&image.dismissed)
                .any(|(rule, &dismissed)| {
                    dismissed && rule.placement() == RulePlacement::InitialReveal
                })
        {
            return Err(RestoreError::InvalidHintFlags);
        }
        let candidates = Candidates::from_closed_masks(image.masks)
            .map_err(RestoreError::InvalidCandidateRow)?;
        if Category::ALL.into_iter().any(|cat| {
            Column::ALL
                .into_iter()
                .any(|col| candidates.mask(cat, col) & !game.initial.mask(cat, col) != 0)
        }) {
            return Err(RestoreError::CandidatesOutsideOpening);
        }
        let target_present = candidates.includes(&game.puzzle.solution);
        let resolved = candidates.resolved().is_some();
        let status_valid = match image.status {
            Status::Playing | Status::Paused => {
                target_present && (!resolved || candidates == game.initial)
            }
            Status::Won => target_present && resolved,
            // A failed move that would empty a domain leaves the last valid
            // unresolved board in place. A failed image may retain its target.
            Status::Failed => !target_present || !resolved,
        };
        if !status_valid {
            return Err(RestoreError::StatusMismatch);
        }
        game.status = image.status;
        game.candidates = candidates;
        game.dismissed = image.dismissed;
        game.hint_view = image.hint_view;
        game.elapsed_ms = image.elapsed_ms;
        Ok(Self { game: Some(game) })
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        let game = self.game.as_ref()?;
        Some(Snapshot {
            status: game.status,
            candidates: game.candidates.clone(),
            hints: game
                .puzzle
                .rules
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, r)| r.placement() != RulePlacement::InitialReveal)
                .map(|(i, rule)| Hint {
                    id: HintId(i),
                    rule,
                    dismissed: game.dismissed[i],
                })
                .collect(),
            hint_view: game.hint_view,
            elapsed_ms: game.elapsed_ms,
            score_eligible: game.score_eligible,
        })
    }

    pub fn apply(&mut self, command: Command) -> Result<Effect, Error> {
        match command {
            Command::New { seed } => {
                let puzzle = generate(seed).map_err(Error::Generation)?;
                self.game = Some(Game::new(puzzle, true)?);
                return Ok(Effect::Changed);
            }
            Command::End => {
                return Ok(if self.game.take().is_some() {
                    Effect::Changed
                } else {
                    Effect::Unchanged
                });
            }
            _ => {}
        }
        let game = self.game.as_mut().ok_or(Error::NoGame)?;
        match command {
            Command::Restart => {
                game.candidates = game.initial.clone();
                game.status = Status::Playing;
                game.dismissed.fill(false);
                game.hint_view = HintView::Active;
                game.elapsed_ms = 0;
                game.score_eligible = false;
                Ok(Effect::Changed)
            }
            Command::Select { card, column } => game.move_card(card, column, true),
            Command::Exclude { card, column } => game.move_card(card, column, false),
            Command::ToggleHint { id } => {
                game.require_playing()?;
                if game
                    .puzzle
                    .rules
                    .get(id.0)
                    .is_none_or(|r| r.placement() == RulePlacement::InitialReveal)
                {
                    return Err(Error::UnknownHint(id));
                }
                let dismissed = &mut game.dismissed[id.0];
                if *dismissed != matches!(game.hint_view, HintView::Dismissed) {
                    return Ok(Effect::Unchanged);
                }
                *dismissed = !*dismissed;
                Ok(Effect::Changed)
            }
            Command::SwitchHints => {
                game.require_playing()?;
                game.hint_view = match game.hint_view {
                    HintView::Active => HintView::Dismissed,
                    HintView::Dismissed => HintView::Active,
                };
                Ok(Effect::Changed)
            }
            Command::Pause if game.status == Status::Playing => {
                game.status = Status::Paused;
                Ok(Effect::Changed)
            }
            Command::Resume if game.status == Status::Paused => {
                game.status = Status::Playing;
                Ok(Effect::Changed)
            }
            Command::Pause | Command::Resume => Ok(Effect::Unchanged),
            Command::AdvanceClock { millis } => {
                if game.status != Status::Playing {
                    return Ok(Effect::Unchanged);
                }
                let next = game.elapsed_ms.saturating_add(millis);
                if next == game.elapsed_ms {
                    return Ok(Effect::Unchanged);
                }
                game.elapsed_ms = next;
                Ok(Effect::Changed)
            }
            Command::End | Command::New { .. } => unreachable!("handled without a game"),
        }
    }
}

impl Game {
    fn new(puzzle: GeneratedPuzzle, score_eligible: bool) -> Result<Self, Error> {
        if let Some(index) = puzzle.rules.iter().position(|r| !r.holds(&puzzle.solution)) {
            return Err(Error::RuleFalse { index });
        }
        let initial = Candidates::for_play(&puzzle.rules).map_err(Error::InitialContradiction)?;
        Ok(Self {
            candidates: initial.clone(),
            dismissed: vec![false; puzzle.rules.len()],
            puzzle,
            initial,
            status: Status::Playing,
            hint_view: HintView::Active,
            elapsed_ms: 0,
            score_eligible,
        })
    }

    fn require_playing(&self) -> Result<(), Error> {
        if self.status == Status::Playing {
            Ok(())
        } else {
            Err(Error::NotPlaying(self.status))
        }
    }

    fn move_card(&mut self, card: Card, column: Column, select: bool) -> Result<Effect, Error> {
        self.require_playing()?;
        // Historical clicks on decided cells and removed candidates have no effect.
        // Checking for victory still mirrors the original's click on a decided cell.
        let mask = self.candidates.mask(card.category, column);
        let mut effect = Effect::Unchanged;
        if mask.count_ones() > 1 && self.candidates.allows(card, column) {
            let changed = if select {
                self.candidates.select(card, column)
            } else {
                self.candidates.exclude(card, column)
            };
            match changed {
                Err(_) => {
                    // The deduction API rolls back invalid candidate boards.
                    self.status = Status::Failed;
                    return Ok(Effect::Failed);
                }
                Ok(true) => effect = Effect::Changed,
                Ok(false) => {}
            }
        }
        if !self.candidates.includes(&self.puzzle.solution) {
            self.status = Status::Failed;
            return Ok(Effect::Failed);
        }
        // Inclusion already proves the surviving singletons match the solution.
        if self.candidates.resolved().is_some() {
            self.status = Status::Won;
            return Ok(Effect::Won);
        }
        Ok(effect)
    }
}
