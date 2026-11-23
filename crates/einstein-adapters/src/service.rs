use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use einstein_core::session::{Command, Effect, HintId, HintView, Session, Snapshot, Status};
use einstein_core::{Card, Category, Column, Rule, RulePlacement, Value as CardValue};
use serde_json::{Map, Value, json};

use crate::Options;

#[derive(Clone, Copy)]
pub(crate) enum Action {
    State,
    New,
    Restart,
    End,
    Select,
    Exclude,
    ToggleHint,
    SwitchHints,
    Pause,
    Resume,
    Save,
    Load,
}

impl Action {
    pub(crate) const ALL: [Self; 12] = [
        Self::State,
        Self::New,
        Self::Restart,
        Self::End,
        Self::Select,
        Self::Exclude,
        Self::ToggleHint,
        Self::SwitchHints,
        Self::Pause,
        Self::Resume,
        Self::Save,
        Self::Load,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::New => "new",
            Self::Restart => "restart",
            Self::End => "end",
            Self::Select => "select",
            Self::Exclude => "exclude",
            Self::ToggleHint => "toggle_hint",
            Self::SwitchHints => "switch_hints",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Save => "save",
            Self::Load => "load",
        }
    }

    pub(crate) const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::New => &["seed"],
            Self::Select | Self::Exclude => &["category", "column", "value"],
            Self::ToggleHint => &["id"],
            Self::Save | Self::Load => &["slot"],
            _ => &[],
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::State => "Read candidates, clues and status, without the hidden answer.",
            Self::New => {
                "Generate a new puzzle from a decimal Rust seed (not an original SDL seed)."
            }
            Self::Restart => "Reset the current puzzle to its opening board.",
            Self::End => "End the current game.",
            Self::Select => "Select a remaining candidate in an unresolved cell.",
            Self::Exclude => "Exclude a remaining candidate in an unresolved cell.",
            Self::ToggleHint => {
                "Dismiss or restore a clue by slot, depending on the active clue view."
            }
            Self::SwitchHints => "Switch between active and dismissed clues.",
            Self::Pause => "Pause the game and stopwatch.",
            Self::Resume => "Resume a paused game.",
            Self::Save => "Save to a slot in the host-configured directory.",
            Self::Load => "Load and validate a slot before replacing the current game.",
        }
    }

    pub(crate) fn tool(self) -> Value {
        let properties = self.fields().iter().map(|&s| (s.to_owned(), match s {
            "seed" => json!({"type":"string","pattern":"^[0-9]{1,20}$","description":"Unsigned decimal seed, 0 to 18446744073709551615."}),
            "slot" => json!({"type":"integer","minimum":0,"maximum":9}),
            "id" => json!({"type":"integer","minimum":0,"maximum":255,"description":"Stable clue identifier from state."}),
            _ => json!({"type":"integer","minimum":1,"maximum":6}),
        })).collect::<Map<_,_>>();
        json!({"name":format!("einstein_{}",self.name()), "description":self.description(),
            "inputSchema":{"type":"object","properties":properties,"required":self.fields(),"additionalProperties":false}})
    }

    pub(crate) fn parse(self, args: &Map<String, Value>) -> Result<Request, &'static str> {
        if args.len() != self.fields().len()
            || !self.fields().iter().all(|&name| args.contains_key(name))
        {
            return Err("unexpected or missing arguments");
        }
        fn number(
            args: &Map<String, Value>,
            key: &str,
            lo: u64,
            hi: u64,
        ) -> Result<u64, &'static str> {
            args.get(key)
                .and_then(Value::as_u64)
                .filter(|n| (lo..=hi).contains(n))
                .ok_or("integer argument is missing or outside its allowed range")
        }
        Ok(Request::Play(match self {
            Self::State => return Ok(Request::State),
            Self::New => {
                let seed = args
                    .get("seed")
                    .and_then(Value::as_str)
                    .filter(|s| {
                        !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())
                    })
                    .and_then(|s| s.parse().ok())
                    .ok_or("seed must be a decimal u64 string")?;
                Command::New { seed }
            }
            Self::Select | Self::Exclude => {
                let category = Category::ALL[(number(args, "category", 1, 6)? - 1) as usize];
                let column = Column::ALL[(number(args, "column", 1, 6)? - 1) as usize];
                let value = CardValue::ALL[(number(args, "value", 1, 6)? - 1) as usize];
                let card = Card::new(category, value);
                if matches!(self, Self::Select) {
                    Command::Select { card, column }
                } else {
                    Command::Exclude { card, column }
                }
            }
            Self::ToggleHint => Command::ToggleHint {
                id: HintId(number(args, "id", 0, 255)? as usize),
            },
            Self::SwitchHints => Command::SwitchHints,
            Self::Restart => Command::Restart,
            Self::End => Command::End,
            Self::Pause => Command::Pause,
            Self::Resume => Command::Resume,
            Self::Save => return Ok(Request::Save(number(args, "slot", 0, 9)? as u8)),
            Self::Load => return Ok(Request::Load(number(args, "slot", 0, 9)? as u8)),
        }))
    }
}

pub(crate) enum Request {
    State,
    Play(Command),
    Save(u8),
    Load(u8),
}

pub(crate) struct Service {
    session: Session,
    save_dir: Option<PathBuf>,
    last_clock: Option<Instant>,
}

impl Service {
    pub(crate) fn new(options: Options) -> Self {
        Self {
            session: Session::new(),
            save_dir: options.save_dir,
            last_clock: (!options.no_clock).then(Instant::now),
        }
    }

    fn clock(&mut self) {
        if let Some(last) = &mut self.last_clock {
            let now = Instant::now();
            let ms = now.duration_since(*last).as_millis().min(u64::MAX.into()) as u64;
            *last += Duration::from_millis(ms);
            if ms != 0 {
                let _ = self.session.apply(Command::AdvanceClock { millis: ms });
            }
        }
    }

    pub(crate) fn run(&mut self, request: Request) -> Value {
        self.clock();
        let resets_clock = matches!(
            request,
            Request::Play(Command::New { .. } | Command::Restart | Command::Resume)
                | Request::Load(_)
        );
        let result = match request {
            Request::Play(command) => self
                .session
                .apply(command)
                .map_err(|err| err.to_string())
                .map(effect_name),
            Request::State => Ok("unchanged"),
            Request::Save(slot) => self.store(slot, true).map(|()| "saved"),
            Request::Load(slot) => self.store(slot, false).map(|()| "loaded"),
        };
        if resets_clock && result.is_ok() && self.last_clock.is_some() {
            self.last_clock = Some(Instant::now());
        }
        let state = self.session.snapshot().map(|snap| snapshot(&snap));
        match result {
            Ok(effect) => json!({"ok":true,"effect":effect,"state":state}),
            Err(error) => json!({"ok":false,"error":error,"state":state}),
        }
    }

    fn store(&mut self, slot: u8, saving: bool) -> Result<(), String> {
        let dir = self
            .save_dir
            .as_deref()
            .ok_or("storage is disabled; start with --save-dir")?;
        if saving {
            if self.session.snapshot().is_none() {
                return Err("no game to save".into());
            }
            create_private_dir(dir).map_err(|err| err.to_string())?;
        }
        let path = dir.join(format!("slot-{slot}.epz"));
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() => return Err("slot is not a regular file".into()),
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.to_string()),
            _ => {}
        }
        let result = if saving {
            einstein_storage::save(&self.session, path)
        } else {
            einstein_storage::load_into(&mut self.session, path)
        };
        result.map_err(|err| err.to_string())
    }
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut dir = fs::DirBuilder::new();
    dir.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        dir.mode(0o700);
    }
    dir.create(path)
}

fn effect_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Unchanged => "unchanged",
        Effect::Changed => "changed",
        Effect::Failed => "failed",
        Effect::Won => "won",
    }
}

fn snapshot(state: &Snapshot) -> Value {
    let status = match state.status {
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Failed => "failed",
        Status::Won => "won",
    };
    let view = if state.hint_view == HintView::Active {
        "active"
    } else {
        "dismissed"
    };
    let candidates = Category::ALL.map(|cat| {
        Column::ALL.map(|col| {
            state
                .candidates
                .values(cat, col)
                .map(CardValue::number)
                .collect::<Vec<_>>()
        })
    });
    let hints = state.hints.iter().map(|hint| {
        let rule = hint.rule;
        let (kind, cards) = match rule {
            Rule::SameColumn(a,b) => ("same_column", vec![a,b]), Rule::Near(a,b) => ("near", vec![a,b]),
            Rule::LeftOf {left,right} => ("left_of", vec![left,right]),
            Rule::Between {middle,ends} => ("between", vec![ends[0],middle,ends[1]]),
            Rule::Reveal {..} => unreachable!("snapshots exclude reveal rules"),
        };
        let cards = cards.iter().map(|card| [card.category.index() + 1, card.value.number() as usize]).collect::<Vec<_>>();
        let placement = if rule.placement() == RulePlacement::Vertical { "vertical" } else { "horizontal" };
        json!({"id":hint.id.0,"kind":kind,"cards":cards,"placement":placement,"dismissed":hint.dismissed,"visible":hint.visible(state.hint_view)})
    }).collect::<Vec<_>>();
    json!({"status":status,"candidates":candidates,"hints":hints,"hint_view":view,"elapsed_ms":state.elapsed_ms,"score_eligible":state.score_eligible})
}
