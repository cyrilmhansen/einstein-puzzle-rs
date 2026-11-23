//! Bounded, versioned save files for the Rust reconstruction.
//! This format is deliberately separate from the historical SDL save format.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use einstein_core::generator::GeneratedPuzzle;
use einstein_core::session::{HintView, RestoreError, SaveImage, Session, Status};
use einstein_core::{Card, Category, Column, Grid, Rule, SIZE, Value};

const MAGIC: &[u8; 4] = b"EPZ\0";
pub const FORMAT_VERSION: u16 = 1;
const MAX_RULES: usize = 256;
pub const MAX_BYTES: usize = 8_192;

#[derive(Debug)]
pub enum Error {
    NoGame,
    TooLarge,
    Invalid(&'static str),
    UnsupportedVersion(u16),
    Restore(RestoreError),
    Io(io::Error),
    /// The rename succeeded, but flushing the parent directory failed.
    Durability(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoGame => f.write_str("no game to save"),
            Self::TooLarge => f.write_str("save exceeds the allowed size or rule count"),
            Self::Invalid(reason) => write!(f, "invalid save: {reason}"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported save format {version}"),
            Self::Restore(error) => write!(f, "invalid saved state: {error}"),
            Self::Io(error) => error.fmt(f),
            Self::Durability(error) => {
                write!(f, "save replaced but directory sync failed: {error}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Durability(error) => Some(error),
            Self::Restore(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// The returned bytes contain the solution. Do not send them to a player view.
pub fn encode(session: &Session) -> Result<Vec<u8>, Error> {
    let SaveImage {
        puzzle,
        masks,
        dismissed,
        hint_view,
        elapsed_ms,
        status,
    } = session.save_image().ok_or(Error::NoGame)?;
    if puzzle.rules.len() > MAX_RULES {
        return Err(Error::TooLarge);
    }
    if puzzle.version == 0 || puzzle.layouts_attempted == 0 {
        return Err(Error::Restore(RestoreError::InvalidGeneratorMetadata));
    }
    let mut output = Vec::new();
    output.extend(MAGIC);
    output.extend(FORMAT_VERSION.to_le_bytes());
    output.extend(puzzle.version.to_le_bytes());
    output.extend(puzzle.seed.to_le_bytes());
    output.extend(puzzle.layouts_attempted.to_le_bytes());
    output.extend(elapsed_ms.to_le_bytes());
    output.push(match status {
        Status::Playing => 0,
        Status::Paused => 1,
        Status::Failed => 2,
        Status::Won => 3,
    });
    output.push(match hint_view {
        HintView::Active => 0,
        HintView::Dismissed => 1,
    });
    for row in Category::ALL {
        output.extend(puzzle.solution.row(row).map(Value::number));
    }
    for row in masks {
        output.extend(row);
    }
    output.extend((puzzle.rules.len() as u16).to_le_bytes());
    for (rule, flag) in puzzle.rules.into_iter().zip(dismissed) {
        encode_rule(&mut output, rule);
        output.push(u8::from(flag));
    }
    Ok(output)
}

pub fn decode(bytes: &[u8]) -> Result<Session, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let mut input = Reader(bytes);
    if &input.take::<4>()? != MAGIC {
        return Err(Error::Invalid("magic"));
    }
    let version = u16::from_le_bytes(input.take()?);
    if version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let generator_version = u16::from_le_bytes(input.take()?);
    let seed = u64::from_le_bytes(input.take()?);
    let layouts_attempted = u32::from_le_bytes(input.take()?);
    let elapsed_ms = u64::from_le_bytes(input.take()?);
    let status = match input.byte()? {
        0 => Status::Playing,
        1 => Status::Paused,
        2 => Status::Failed,
        3 => Status::Won,
        _ => return Err(Error::Invalid("status")),
    };
    let hint_view = if input.flag()? {
        HintView::Dismissed
    } else {
        HintView::Active
    };
    let mut rows = [[0; SIZE]; SIZE];
    let mut masks = [[0; SIZE]; SIZE];
    for row in &mut rows {
        *row = input.take()?;
    }
    let solution =
        Grid::try_from(rows).map_err(|_| Error::Invalid("solution row is not a permutation"))?;
    for row in &mut masks {
        *row = input.take()?;
    }
    let rule_count = usize::from(u16::from_le_bytes(input.take()?));
    if rule_count > MAX_RULES {
        return Err(Error::TooLarge);
    }
    let mut rules = Vec::with_capacity(rule_count);
    let mut dismissed = Vec::with_capacity(rule_count);
    for _ in 0..rule_count {
        rules.push(input.rule()?);
        dismissed.push(input.flag()?);
    }
    if !input.0.is_empty() {
        return Err(Error::Invalid("trailing bytes"));
    }
    Session::restore(SaveImage {
        puzzle: GeneratedPuzzle {
            seed,
            version: generator_version,
            solution,
            rules,
            layouts_attempted,
        },
        masks,
        dismissed,
        hint_view,
        elapsed_ms,
        status,
    })
    .map_err(Error::Restore)
}

fn encode_rule(out: &mut Vec<u8>, rule: Rule) {
    fn cards(out: &mut Vec<u8>, tag: u8, cards: &[Card]) {
        out.push(tag);
        for card in cards {
            out.extend([card.category.index() as u8 + 1, card.value.number()]);
        }
    }
    match rule {
        Rule::SameColumn(a, b) => cards(out, 0, &[a, b]),
        Rule::Near(a, b) => cards(out, 1, &[a, b]),
        Rule::LeftOf { left, right } => cards(out, 2, &[left, right]),
        Rule::Between { middle, ends } => cards(out, 3, &[ends[0], middle, ends[1]]),
        Rule::Reveal { card, column } => {
            cards(out, 4, &[card]);
            out.push(column.number());
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let (head, rest) = self
            .0
            .split_first_chunk()
            .ok_or(Error::Invalid("truncated"))?;
        self.0 = rest;
        Ok(*head)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take::<1>()?[0])
    }

    fn flag(&mut self) -> Result<bool, Error> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Invalid("boolean flag")),
        }
    }

    fn card(&mut self) -> Result<Card, Error> {
        let [row, value] = self.take::<2>()?;
        let category = row
            .checked_sub(1)
            .and_then(|r| Category::ALL.get(r as usize))
            .copied()
            .ok_or(Error::Invalid("card category"))?;
        let value = Value::new(value).ok_or(Error::Invalid("card value"))?;
        Ok(Card::new(category, value))
    }

    fn rule(&mut self) -> Result<Rule, Error> {
        Ok(match self.byte()? {
            0 => Rule::SameColumn(self.card()?, self.card()?),
            1 => Rule::Near(self.card()?, self.card()?),
            2 => Rule::LeftOf {
                left: self.card()?,
                right: self.card()?,
            },
            3 => {
                let a = self.card()?;
                let middle = self.card()?;
                Rule::Between {
                    middle,
                    ends: [a, self.card()?],
                }
            }
            4 => Rule::Reveal {
                card: self.card()?,
                column: Column::new(self.byte()?).ok_or(Error::Invalid("reveal column"))?,
            },
            _ => return Err(Error::Invalid("unknown rule type")),
        })
    }
}

pub fn load(path: impl AsRef<Path>) -> Result<Session, Error> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    decode(&bytes)
}

/// Load and validate first; the current game is left untouched on failure.
pub fn load_into(session: &mut Session, path: impl AsRef<Path>) -> Result<(), Error> {
    *session = load(path)?;
    Ok(())
}

/// Write and sync a private peer file, then rename it over the destination.
/// Unix also syncs the parent after rename. No parent directories are created.
pub fn save(session: &Session, path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let bytes = encode(session)?;
    if path.file_name().is_none() {
        return Err(Error::Invalid("save path needs a filename"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let (file, mut temp) = reserve(parent)?;
    commit(file, &mut temp, path, &bytes)?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(Error::Durability)?;
    Ok(())
}

fn commit(
    mut file: File,
    temp: &mut TempFile,
    destination: &Path,
    bytes: &[u8],
) -> Result<(), Error> {
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp.0, destination)?;
    temp.0.clear();
    Ok(())
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn reserve(parent: &Path) -> io::Result<(File, TempFile)> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    for _ in 0..32 {
        let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = OsString::from(format!(".einstein-{}-{n}.tmp", std::process::id()));
        let path = parent.join(name);
        match options.open(&path) {
            Ok(file) => return Ok((file, TempFile(path))),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a save staging file",
    ))
}
