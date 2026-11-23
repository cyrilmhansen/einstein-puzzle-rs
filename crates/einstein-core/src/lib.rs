//! Shared domain of the native Einstein Puzzle reconstruction.
//!
//! A `Grid` is a complete assignment, not the player's candidate board.
//! Rules are predicates on assignments, independent of UI and propagation.

#![forbid(unsafe_code)]

pub mod exact;
pub mod export;
pub mod generator;
pub mod propagation;
pub mod session;

pub const SIZE: usize = 6;

/// Fixed top-to-bottom order in the historical board.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Category {
    ArabicDigits,
    Letters,
    RomanNumerals,
    Dice,
    Shapes,
    MathSymbols,
}

impl Category {
    pub const ALL: [Self; SIZE] = [
        Self::ArabicDigits,
        Self::Letters,
        Self::RomanNumerals,
        Self::Dice,
        Self::Shapes,
        Self::MathSymbols,
    ];

    /// Zero-based index into the grid's rows.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// One-based column, numbered from left to right.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Column(u8);

/// One-based card identifier within a category (not necessarily its face text).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Value(u8);

impl Column {
    pub const ALL: [Self; SIZE] = [Self(1), Self(2), Self(3), Self(4), Self(5), Self(6)];

    pub const fn new(number: u8) -> Option<Self> {
        if number >= 1 && number <= SIZE as u8 {
            Some(Self(number))
        } else {
            None
        }
    }

    pub const fn number(self) -> u8 {
        self.0
    }

    pub const fn index(self) -> usize {
        (self.0 - 1) as usize
    }
}

impl Value {
    pub const ALL: [Self; SIZE] = [Self(1), Self(2), Self(3), Self(4), Self(5), Self(6)];

    pub const fn new(number: u8) -> Option<Self> {
        if number >= 1 && number <= SIZE as u8 {
            Some(Self(number))
        } else {
            None
        }
    }

    pub const fn number(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Card {
    pub category: Category,
    pub value: Value,
}

impl Card {
    pub const fn new(category: Category, value: Value) -> Self {
        Self { category, value }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridError {
    InvalidValue {
        category: Category,
        column: Column,
        number: u8,
    },
    DuplicateValue {
        category: Category,
        value: Value,
    },
}

impl std::fmt::Display for GridError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::InvalidValue {
                category,
                column,
                number,
            } => write!(
                output,
                "invalid value {number} in {category:?}, column {}",
                column.number()
            ),
            Self::DuplicateValue { category, value } => {
                write!(output, "duplicate value {} in {category:?}", value.number())
            }
        }
    }
}

impl std::error::Error for GridError {}

/// Immutable assignment; each row is independently a permutation of 1…6.
/// No uniqueness constraint is imposed on a column across categories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Grid {
    rows: [[Value; SIZE]; SIZE],
}

impl Grid {
    pub fn from_rows(rows: [[Value; SIZE]; SIZE]) -> Result<Self, GridError> {
        for category in Category::ALL {
            let mut used = 0u8;
            for value in rows[category.index()] {
                let bit = 1 << (value.number() - 1);
                if used & bit != 0 {
                    return Err(GridError::DuplicateValue { category, value });
                }
                used |= bit;
            }
        }
        Ok(Self { rows })
    }

    pub const fn row(&self, category: Category) -> &[Value; SIZE] {
        &self.rows[category.index()]
    }

    pub const fn card_at(&self, category: Category, column: Column) -> Card {
        Card::new(category, self.row(category)[column.index()])
    }

    pub fn column_of(&self, card: Card) -> Column {
        for column in Column::ALL {
            if self.card_at(card.category, column) == card {
                return column;
            }
        }
        unreachable!("each checked row contains each value exactly once")
    }
}

impl TryFrom<[[u8; SIZE]; SIZE]> for Grid {
    type Error = GridError;

    fn try_from(numbers: [[u8; SIZE]; SIZE]) -> Result<Self, Self::Error> {
        let mut rows = [Value::ALL; SIZE];
        for category in Category::ALL {
            for column in Column::ALL {
                let number = numbers[category.index()][column.index()];
                rows[category.index()][column.index()] =
                    Value::new(number).ok_or(GridError::InvalidValue {
                        category,
                        column,
                        number,
                    })?;
            }
        }
        Self::from_rows(rows)
    }
}

/// `Near`, `SameColumn`, and the endpoints of `Between` have symmetric truth
/// values. Their stored order is significant for generation and presentation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Rule {
    SameColumn(Card, Card),
    Near(Card, Card),
    LeftOf { left: Card, right: Card },
    Between { middle: Card, ends: [Card; 2] },
    Reveal { card: Card, column: Column },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RulePlacement {
    Vertical,
    Horizontal,
    InitialReveal,
}

impl Rule {
    pub const fn placement(self) -> RulePlacement {
        match self {
            Self::SameColumn(..) => RulePlacement::Vertical,
            Self::Near(..) | Self::LeftOf { .. } | Self::Between { .. } => {
                RulePlacement::Horizontal
            }
            Self::Reveal { .. } => RulePlacement::InitialReveal,
        }
    }

    /// Test the rule on a complete grid. No pruning and no mutation.
    pub fn holds(self, grid: &Grid) -> bool {
        self.holds_with(|card| grid.column_of(card).number())
    }

    pub(crate) fn holds_with(self, position: impl Fn(Card) -> u8) -> bool {
        match self {
            Self::SameColumn(a, b) => position(a) == position(b),
            Self::Near(a, b) => position(a).abs_diff(position(b)) == 1,
            Self::LeftOf { left, right } => position(left) < position(right),
            Self::Between {
                middle,
                ends: [a, b],
            } => {
                let (a, middle, b) = (position(a), position(middle), position(b));
                (a + 1 == middle && middle + 1 == b) || (b + 1 == middle && middle + 1 == a)
            }
            Self::Reveal { card, column } => position(card) == column.number(),
        }
    }
}
