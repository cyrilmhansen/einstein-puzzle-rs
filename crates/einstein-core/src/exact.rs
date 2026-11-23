//! Exact, node-bounded classification. Not part of historical generation.
//!
//! Variables are cards; each domain is the set of possible columns for that
//! card. Explicit relation tuples and row permutations remove unsupported
//! values. Search then partitions the remaining assignments by a card's column.

use std::sync::OnceLock;

use crate::{Card, Category, Grid, Rule, SIZE, Value};

const CARD_COUNT: usize = SIZE * SIZE;
const FULL: u8 = (1 << SIZE) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Multiplicity {
    Zero,
    One,
    AtLeastTwo,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Count {
    pub multiplicity: Multiplicity,
    pub witnesses: Vec<Grid>,
    pub nodes: u64,
}

/// Stops when two solutions are found, the search exhausts the domain, or a
/// node cannot be visited within the budget. One witness by itself is never
/// reported as unique while any branch is still unvisited.
pub fn count_solutions(rules: &[Rule], node_limit: u64) -> Count {
    let tables = rules.iter().copied().map(Table::new).collect::<Vec<_>>();
    let mut search = Search {
        tables: &tables,
        witnesses: Vec::new(),
        nodes: 0,
        node_limit,
        capped: false,
    };
    search.visit([FULL; CARD_COUNT]);
    let multiplicity = match search.witnesses.len() {
        2 => Multiplicity::AtLeastTwo,
        _ if search.capped => Multiplicity::Indeterminate,
        1 => Multiplicity::One,
        _ => Multiplicity::Zero,
    };
    Count {
        multiplicity,
        witnesses: search.witnesses,
        nodes: search.nodes,
    }
}

type Domains = [u8; CARD_COUNT];

fn index(card: Card) -> usize {
    SIZE * card.category.index() + usize::from(card.value.number() - 1)
}

struct Table {
    vars: [usize; 3],
    len: usize,
    supports: Vec<[u8; 3]>,
}

impl Table {
    fn new(rule: Rule) -> Self {
        let mut vars = Vec::with_capacity(3);
        let mut add = |card| {
            if !vars.contains(&card) {
                vars.push(card);
            }
        };
        match rule {
            Rule::Reveal { card, .. } => add(card),
            Rule::SameColumn(a, b) | Rule::Near(a, b) | Rule::LeftOf { left: a, right: b } => {
                add(a);
                add(b);
            }
            Rule::Between { middle, ends } => {
                add(ends[0]);
                add(middle);
                add(ends[1]);
            }
        }
        // Enumerating columns only over distinct cards handles repeated cards
        // without accepting tuples assigning one card different positions.
        let mut supports = Vec::new();
        for code in 0..SIZE.pow(vars.len() as u32) {
            let mut columns = [0u8; 3];
            let mut rest = code;
            for col in &mut columns[..vars.len()] {
                *col = (rest % SIZE + 1) as u8;
                rest /= SIZE;
            }
            if rule.holds_with(|card| columns[vars.iter().position(|&v| v == card).unwrap()]) {
                supports.push(columns.map(|col| if col == 0 { 0 } else { 1 << (col - 1) }));
            }
        }
        Self {
            vars: std::array::from_fn(|i| vars.get(i).map_or(0, |&card| index(card))),
            len: vars.len(),
            supports,
        }
    }

    fn propagate(&self, domains: &mut Domains) -> bool {
        let mut supported = [0u8; 3];
        let vars = &self.vars[..self.len];
        let mut wanted = [0u8; 3];
        for (i, &var) in vars.iter().enumerate() {
            wanted[i] = domains[var];
        }
        for tuple in &self.supports {
            if (0..vars.len()).all(|i| wanted[i] & tuple[i] != 0) {
                for i in 0..vars.len() {
                    supported[i] |= tuple[i];
                }
                if supported == wanted {
                    break;
                }
            }
        }
        for (i, &var) in vars.iter().enumerate() {
            if supported[i] == 0 {
                return false;
            }
            domains[var] = supported[i];
        }
        true
    }
}

fn permutations() -> &'static [[u8; SIZE]] {
    static PERMUTATIONS: OnceLock<Vec<[u8; SIZE]>> = OnceLock::new();
    PERMUTATIONS.get_or_init(|| {
        fn visit(i: usize, row: &mut [u8; SIZE], out: &mut Vec<[u8; SIZE]>) {
            if i == SIZE {
                out.push(*row);
                return;
            }
            for j in i..SIZE {
                row.swap(i, j);
                visit(i + 1, row, out);
                row.swap(i, j);
            }
        }
        let mut perms = Vec::with_capacity(720);
        visit(0, &mut std::array::from_fn(|i| 1 << i), &mut perms);
        perms
    })
}

fn enforce_permutation(domains: &mut [u8]) -> bool {
    if domains == [FULL; SIZE] {
        return true;
    }
    let mut supported = [0u8; SIZE];
    for perm in permutations() {
        if domains
            .iter()
            .zip(perm)
            .all(|(domain, col)| domain & col != 0)
        {
            for (allow, col) in supported.iter_mut().zip(perm) {
                *allow |= col;
            }
            if supported == domains {
                return true;
            }
        }
    }
    if supported[0] == 0 {
        return false;
    }
    domains.copy_from_slice(&supported);
    true
}

fn prune(domains: &mut Domains, tables: &[Table]) -> bool {
    loop {
        let before = *domains;
        for table in tables {
            if !table.propagate(domains) {
                return false;
            }
        }
        for row in domains.as_chunks_mut::<SIZE>().0 {
            if !enforce_permutation(row) {
                return false;
            }
        }
        if *domains == before {
            return true;
        }
    }
}

struct Search<'a> {
    tables: &'a [Table],
    witnesses: Vec<Grid>,
    nodes: u64,
    node_limit: u64,
    capped: bool,
}

impl Search<'_> {
    fn visit(&mut self, mut domains: Domains) {
        if self.nodes == self.node_limit {
            self.capped = true;
            return;
        }
        self.nodes += 1;
        if !prune(&mut domains, self.tables) {
            return;
        }
        let next = domains
            .iter()
            .enumerate()
            .filter(|&(_, domain)| domain.count_ones() > 1)
            .min_by_key(|&(i, domain)| (domain.count_ones(), i))
            .map(|(i, &domain)| (i, domain));
        if let Some((i, domain)) = next {
            for col in 0..SIZE {
                let bit = 1 << col;
                if domain & bit != 0 {
                    let mut child = domains;
                    child[i] = bit;
                    self.visit(child);
                    if self.capped || self.witnesses.len() == 2 {
                        return;
                    }
                }
            }
        } else {
            let mut rows = [Value::ALL; SIZE];
            for category in Category::ALL {
                for value in Value::ALL {
                    let col = domains[index(Card::new(category, value))].trailing_zeros() as usize;
                    rows[category.index()][col] = value;
                }
            }
            self.witnesses.push(
                Grid::from_rows(rows).expect("permutation filtering ensures a valid complete row"),
            );
        }
    }
}
