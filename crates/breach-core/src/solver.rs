//! Exact solver: depth-first branch and bound, bounded by a dynamic program over the daemons' matching automata.

use serde::{Deserialize, Serialize};

pub const MAX_CODES: usize = 8;
pub const MAX_DAEMONS: usize = 12;
pub const MAX_BUFFER: usize = 24;
const MAX_UNITS: usize = MAX_DAEMONS * 3;
/// Above this many product-automaton states the DP bound is skipped and the cheap bound is used.
const DP_STATE_LIMIT: usize = 1 << 19;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Daemon {
    /// Code ids, in order.
    pub seq: Vec<u8>,
    /// Reward weight. The solver maximises the sum of weights of uploaded daemons.
    pub weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Puzzle {
    pub rows: usize,
    pub cols: usize,
    /// Row-major code ids, `rows * cols` entries, each `< MAX_CODES`.
    pub cells: Vec<u8>,
    pub daemons: Vec<Daemon>,
    pub buffer: usize,
}

impl Puzzle {
    pub fn square(n: usize, cells: Vec<u8>, seqs: Vec<Vec<u8>>, buffer: usize) -> Self {
        let daemons = seqs
            .into_iter()
            .enumerate()
            .map(|(i, seq)| Daemon { seq, weight: default_weight(i) })
            .collect();
        Puzzle { rows: n, cols: n, cells, daemons, buffer }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.rows == 0 || self.cols == 0 || self.rows * self.cols > 128 {
            return Err(format!("unsupported matrix size {}x{}", self.rows, self.cols));
        }
        if self.cells.len() != self.rows * self.cols {
            return Err("cell count does not match matrix size".into());
        }
        if self.cells.iter().any(|&c| c as usize >= MAX_CODES) {
            return Err("code id out of range".into());
        }
        if self.daemons.len() > MAX_DAEMONS {
            return Err(format!("too many daemons ({})", self.daemons.len()));
        }
        for d in &self.daemons {
            if d.seq.is_empty() || d.seq.len() > 15 {
                return Err("daemon sequence length must be 1..=15".into());
            }
            if d.seq.iter().any(|&c| c as usize >= MAX_CODES) {
                return Err("daemon code id out of range".into());
            }
        }
        if self.buffer > MAX_BUFFER {
            return Err(format!("buffer too large ({})", self.buffer));
        }
        Ok(())
    }

    pub fn total_weight(&self) -> u32 {
        self.daemons.iter().map(|d| d.weight).sum()
    }
}

/// More daemons first; later daemons win ties.
pub fn default_weight(index: usize) -> u32 {
    100 + 10 * index as u32
}

/// How a daemon's progress is assumed to react to a pick that does not continue its sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Matcher {
    /// The buffer contains the sequence as a contiguous run.
    Substring,
    /// On a wrong pick progress resets, then the same pick is re-tested against the first code.
    Retest,
    /// On a wrong pick progress resets to zero and that pick is not re-tested.
    Strict,
    /// The daemon must be uploaded under all three rules above. Safe whatever the game does.
    Unanimous,
    /// Substring matching has been ruled out: uploaded under both rules that remain.
    NotSubstring,
    /// The re-test rule has been ruled out: uploaded under both rules that remain.
    NotRetest,
    /// The strict rule has been ruled out: uploaded under both rules that remain.
    NotStrict,
}

impl Matcher {
    fn kinds(self) -> &'static [Kind] {
        match self {
            Matcher::Substring => &[Kind::Kmp],
            Matcher::Retest => &[Kind::Retest],
            Matcher::Strict => &[Kind::Strict],
            Matcher::Unanimous => &[Kind::Kmp, Kind::Retest, Kind::Strict],
            Matcher::NotSubstring => &[Kind::Retest, Kind::Strict],
            Matcher::NotRetest => &[Kind::Kmp, Kind::Strict],
            Matcher::NotStrict => &[Kind::Kmp, Kind::Retest],
        }
    }

    /// The single rules this stands for, in the order substring, retest, strict.
    pub fn rules(self) -> [bool; 3] {
        let kinds = self.kinds();
        [kinds.contains(&Kind::Kmp), kinds.contains(&Kind::Retest), kinds.contains(&Kind::Strict)]
    }

    /// The matcher that stands for exactly these rules, if there is one.
    pub fn of(rules: [bool; 3]) -> Option<Matcher> {
        const ALL: [Matcher; 7] = [
            Matcher::Substring,
            Matcher::Retest,
            Matcher::Strict,
            Matcher::Unanimous,
            Matcher::NotSubstring,
            Matcher::NotRetest,
            Matcher::NotStrict,
        ];
        ALL.into_iter().find(|m| m.rules() == rules)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Kmp,
    Retest,
    Strict,
}

/// Full KMP transition table over states `0..=len` (state `len` = just matched).
fn kmp_table(seq: &[u8]) -> Vec<[u8; MAX_CODES]> {
    let l = seq.len();
    let mut t = vec![[0u8; MAX_CODES]; l + 1];
    // fail[s] = length of the longest proper border of seq[..s]
    let mut fail = vec![0usize; l + 1];
    for s in 2..=l {
        let mut k = fail[s - 1];
        while k > 0 && seq[k] != seq[s - 1] {
            k = fail[k];
        }
        fail[s] = if seq[k] == seq[s - 1] { k + 1 } else { 0 };
    }
    for s in 0..=l {
        for c in 0..MAX_CODES {
            t[s][c] = if s < l && seq[s] as usize == c {
                (s + 1) as u8
            } else if s == 0 {
                0
            } else {
                t[fail[s]][c]
            };
        }
    }
    t
}

/// Transition table over states `0..len`; a transition into `len` means "uploaded".
fn unit_table(seq: &[u8], kind: Kind) -> Vec<[u8; MAX_CODES]> {
    let l = seq.len();
    match kind {
        Kind::Kmp => kmp_table(seq)[..l].to_vec(),
        Kind::Retest | Kind::Strict => (0..l)
            .map(|s| {
                let mut row = [0u8; MAX_CODES];
                for (c, slot) in row.iter_mut().enumerate() {
                    *slot = if seq[s] as usize == c {
                        (s + 1) as u8
                    } else if kind == Kind::Retest && seq[0] as usize == c {
                        1
                    } else {
                        0
                    };
                }
                row
            })
            .collect(),
    }
}

struct Unit {
    daemon: usize,
    len: u8,
    next: Vec<[u8; MAX_CODES]>,
}

/// Non-sticky substring tracker for the bound: states `0..len`, `(next_state, occurrence_ended_here)`.
struct Progress {
    len: u8,
    next: Vec<[(u8, bool); MAX_CODES]>,
}

fn progress_table(seq: &[u8]) -> Progress {
    let l = seq.len();
    let full = kmp_table(seq);
    // state reached after a full match, folded back into 0..l (its longest proper border)
    let mut after = 0u8;
    for k in (1..l).rev() {
        if seq[..k] == seq[l - k..] {
            after = k as u8;
            break;
        }
    }
    let next = (0..l)
        .map(|s| {
            let mut row = [(0u8, false); MAX_CODES];
            for (c, slot) in row.iter_mut().enumerate() {
                let t = full[s][c];
                *slot = if t as usize == l { (after, true) } else { (t, false) };
            }
            row
        })
        .collect();
    Progress { len: l as u8, next }
}

/// Best reward any string of `r` more codes can still earn, ignoring the matrix.
struct Bound {
    radix: Vec<usize>,
    states: usize,
    codes: Vec<usize>,
    table: Vec<u32>, // (buffer + 1) * states, u32::MAX = not computed
}

impl Bound {
    fn new(p: &Puzzle, prog: &[Progress]) -> Option<Bound> {
        let mut radix = Vec::with_capacity(prog.len());
        let mut states = 1usize;
        for pr in prog {
            radix.push(states);
            states = states.checked_mul(pr.len as usize + 1)?;
            if states > DP_STATE_LIMIT {
                return None;
            }
        }
        let mut present = [false; MAX_CODES];
        for &c in &p.cells {
            present[c as usize] = true;
        }
        let codes = (0..MAX_CODES).filter(|&c| present[c]).collect();
        Some(Bound { radix, states, codes, table: vec![u32::MAX; (p.buffer + 1) * states] })
    }

    fn value(&mut self, p: &Puzzle, prog: &[Progress], r: usize, idx: usize) -> u32 {
        if r == 0 {
            return 0;
        }
        let slot = r * self.states + idx;
        let cached = self.table[slot];
        if cached != u32::MAX {
            return cached;
        }
        let mut best = 0u32;
        for ci in 0..self.codes.len() {
            let c = self.codes[ci];
            let mut next_idx = 0usize;
            let mut gain = 0u32;
            let mut rest = idx;
            for (d, pr) in prog.iter().enumerate() {
                let base = pr.len as usize + 1;
                let digit = rest % base;
                rest /= base;
                let nd = if digit == pr.len as usize {
                    digit
                } else {
                    let (s2, hit) = pr.next[digit][c];
                    if hit {
                        gain += p.daemons[d].weight;
                        pr.len as usize
                    } else {
                        s2 as usize
                    }
                };
                next_idx += nd * self.radix[d];
            }
            let v = gain + self.value(p, prog, r - 1, next_idx);
            if v > best {
                best = v;
            }
        }
        self.table[slot] = best;
        best
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Solution {
    /// Sum of weights of uploaded daemons.
    pub reward: u32,
    /// Cell indices (`row * cols + col`) in pick order.
    pub path: Vec<u16>,
    /// Bit `i` set = daemon `i` uploaded.
    pub uploaded: u32,
    pub nodes: u64,
    /// `false` = the node budget ran out; the result is the best found so far, not proven optimal.
    pub proven: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SolveOptions {
    pub matcher: Matcher,
    pub node_budget: u64,
    /// Use the product-automaton bound (set `false` only to measure its effect).
    pub dp_bound: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        SolveOptions { matcher: Matcher::Substring, node_budget: 20_000_000, dp_bound: true }
    }
}

struct Search<'a> {
    p: &'a Puzzle,
    units: Vec<Unit>,
    /// number of units per daemon that must finish
    need_units: Vec<u8>,
    prog: Vec<Progress>,
    bound: Option<Bound>,
    best: Solution,
    best_len: usize,
    path: [u16; MAX_BUFFER],
    nodes: u64,
    budget: u64,
}

#[derive(Clone, Copy)]
struct State {
    used: u128,
    ust: [u8; MAX_UNITS],
    udone: u64,
    dleft: [u8; MAX_DAEMONS], // unfinished units per daemon
    ddone: u32,
    kprog: [u8; MAX_DAEMONS],
    reward: u32,
}

impl<'a> Search<'a> {
    #[inline]
    fn apply(&self, st: &State, cell: usize) -> (State, bool) {
        let code = self.p.cells[cell] as usize;
        let mut n = *st;
        n.used |= 1u128 << cell;
        let mut advanced = false;
        for (u, unit) in self.units.iter().enumerate() {
            if st.udone >> u & 1 == 1 {
                continue;
            }
            let ns = unit.next[st.ust[u] as usize][code];
            if ns > st.ust[u] {
                advanced = true;
            }
            if ns == unit.len {
                n.udone |= 1 << u;
                n.ust[u] = 0;
                n.dleft[unit.daemon] -= 1;
                if n.dleft[unit.daemon] == 0 {
                    n.ddone |= 1 << unit.daemon;
                    n.reward += self.p.daemons[unit.daemon].weight;
                }
            } else {
                n.ust[u] = ns;
            }
        }
        for (d, pr) in self.prog.iter().enumerate() {
            n.kprog[d] = pr.next[st.kprog[d] as usize][code].0;
        }
        (n, advanced)
    }

    #[inline]
    fn upper(&mut self, st: &State, remaining: usize) -> u32 {
        if remaining == 0 {
            return st.reward;
        }
        if let Some(b) = self.bound.as_mut() {
            let mut idx = 0usize;
            for (d, pr) in self.prog.iter().enumerate() {
                let digit =
                    if st.ddone >> d & 1 == 1 { pr.len as usize } else { st.kprog[d] as usize };
                idx += digit * b.radix[d];
            }
            return st.reward + b.value(self.p, &self.prog, remaining, idx);
        }
        // cheap bound: every daemon whose unfinished units still fit
        let mut ub = st.reward;
        let mut fits = [true; MAX_DAEMONS];
        for (u, unit) in self.units.iter().enumerate() {
            if st.udone >> u & 1 == 0 && (unit.len - st.ust[u]) as usize > remaining {
                fits[unit.daemon] = false;
            }
        }
        for (d, dm) in self.p.daemons.iter().enumerate() {
            if st.ddone >> d & 1 == 0 && fits[d] {
                ub += dm.weight;
            }
        }
        ub
    }

    fn dfs(&mut self, depth: usize, line: usize, is_row: bool, st: &State) {
        self.nodes += 1;
        if self.nodes > self.budget {
            return;
        }
        let remaining = self.p.buffer - depth;
        if remaining == 0 {
            return;
        }
        let span = if is_row { self.p.cols } else { self.p.rows };
        // children: (bound, advanced, k, state)
        let mut kids: [(u32, bool, u8); 16] = [(0, false, 0); 16];
        let mut states: [State; 16] = [*st; 16];
        let mut nk = 0;
        for k in 0..span {
            let cell = if is_row { line * self.p.cols + k } else { k * self.p.cols + line };
            if st.used >> cell & 1 == 1 {
                continue;
            }
            let (ns, adv) = self.apply(st, cell);
            if ns.reward > self.best.reward
                || (ns.reward == self.best.reward && ns.reward > 0 && depth + 1 < self.best_len)
            {
                self.path[depth] = cell as u16;
                self.best.reward = ns.reward;
                self.best.uploaded = ns.ddone;
                self.best.path = self.path[..=depth].to_vec();
                self.best_len = depth + 1;
            }
            let ub = self.upper(&ns, remaining - 1);
            kids[nk] = (ub, adv, k as u8);
            states[nk] = ns;
            nk += 1;
        }
        // most promising first: higher bound, then picks that advance a daemon
        let mut order: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        order[..nk].sort_unstable_by(|&a, &b| {
            kids[b]
                .0
                .cmp(&kids[a].0)
                .then(kids[b].1.cmp(&kids[a].1))
                .then(kids[a].2.cmp(&kids[b].2))
        });
        for &i in &order[..nk] {
            let (ub, _, k) = kids[i];
            // a child at depth+1 can only yield solutions of length >= depth+2 beyond what was recorded above
            if ub < self.best.reward || (ub == self.best.reward && depth + 2 >= self.best_len) {
                continue;
            }
            let k = k as usize;
            let cell = if is_row { line * self.p.cols + k } else { k * self.p.cols + line };
            self.path[depth] = cell as u16;
            self.dfs(depth + 1, k, !is_row, &states[i]);
        }
    }
}

pub fn solve_with(p: &Puzzle, opt: &SolveOptions) -> Solution {
    solve_from(p, opt, &[]).unwrap_or(Solution { proven: true, ..Solution::default() })
}

/// Checks that picks follow the rules of the board: in range, no cell twice, top row first, then column and row in turn.
fn legal(p: &Puzzle, path: &[u16]) -> Result<(), String> {
    if path.len() > p.buffer {
        return Err(format!("route has {} picks, buffer holds {}", path.len(), p.buffer));
    }
    let mut seen = vec![false; p.rows * p.cols];
    for (i, &c) in path.iter().enumerate() {
        let c = c as usize;
        if c >= p.rows * p.cols {
            return Err(format!("pick {} is outside the matrix", i + 1));
        }
        if std::mem::replace(&mut seen[c], true) {
            return Err(format!("pick {} reuses a cell", i + 1));
        }
        let (r, col) = (c / p.cols, c % p.cols);
        if i == 0 {
            if r != 0 {
                return Err("first pick must be in the top row".into());
            }
        } else {
            let prev = path[i - 1] as usize;
            let (pr, pc) = (prev / p.cols, prev % p.cols);
            if i % 2 == 1 {
                if col != pc {
                    return Err(format!("pick {} must stay in the column of pick {}", i + 1, i));
                }
            } else if r != pr {
                return Err(format!("pick {} must stay in the row of pick {}", i + 1, i));
            }
        }
    }
    Ok(())
}

/// Best route that begins with the picks already made; the returned path starts with them.
pub fn solve_from(p: &Puzzle, opt: &SolveOptions, played: &[u16]) -> Result<Solution, String> {
    p.validate()?;
    if p.rows > 16 || p.cols > 16 {
        return Err(format!("unsupported matrix size {}x{}", p.rows, p.cols));
    }
    legal(p, played)?;
    let kinds = opt.matcher.kinds();
    let mut units = Vec::new();
    for (d, dm) in p.daemons.iter().enumerate() {
        for &k in kinds {
            units.push(Unit { daemon: d, len: dm.seq.len() as u8, next: unit_table(&dm.seq, k) });
        }
    }
    let prog: Vec<Progress> = p.daemons.iter().map(|d| progress_table(&d.seq)).collect();
    let bound = if opt.dp_bound { Bound::new(p, &prog) } else { None };
    let mut s = Search {
        p,
        units,
        need_units: vec![kinds.len() as u8; p.daemons.len()],
        prog,
        bound,
        best: Solution::default(),
        best_len: usize::MAX,
        path: [0; MAX_BUFFER],
        nodes: 0,
        budget: opt.node_budget,
    };
    let mut st = State {
        used: 0,
        ust: [0; MAX_UNITS],
        udone: 0,
        dleft: [0; MAX_DAEMONS],
        ddone: 0,
        kprog: [0; MAX_DAEMONS],
        reward: 0,
    };
    for (d, &n) in s.need_units.iter().enumerate() {
        st.dleft[d] = n;
    }
    for (i, &cell) in played.iter().enumerate() {
        st = s.apply(&st, cell as usize).0;
        s.path[i] = cell;
    }
    // what the picks made so far have earned stands even if nothing more can be added
    s.best = Solution {
        reward: st.reward,
        uploaded: st.ddone,
        path: played.to_vec(),
        ..Solution::default()
    };
    if st.reward > 0 {
        s.best_len = played.len();
    }
    let (line, is_row) = match played.last() {
        None => (0, true),
        Some(&last) if played.len() % 2 == 1 => (last as usize % p.cols, false),
        Some(&last) => (last as usize / p.cols, true),
    };
    s.dfs(played.len(), line, is_row, &st);
    s.best.nodes = s.nodes;
    s.best.proven = s.nodes <= opt.node_budget;
    Ok(s.best)
}

pub fn solve(p: &Puzzle, matcher: Matcher) -> Solution {
    solve_with(p, &SolveOptions { matcher, ..SolveOptions::default() })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobustSolution {
    /// The route to show. Works under every matching rule when `rule_sensitive` is false.
    pub recommended: Solution,
    /// Best route if the game does plain substring matching (may equal `recommended`).
    pub optimistic: Solution,
    /// True when the optimistic route earns more than any route that is safe under every rule.
    pub rule_sensitive: bool,
}

/// Route that does not depend on the unconfirmed matching rule whenever that costs nothing.
pub fn solve_robust(p: &Puzzle, node_budget: u64) -> RobustSolution {
    let optimistic =
        solve_with(p, &SolveOptions { matcher: Matcher::Substring, node_budget, dp_bound: true });
    let safe =
        solve_with(p, &SolveOptions { matcher: Matcher::Unanimous, node_budget, dp_bound: true });
    let rule_sensitive = safe.reward < optimistic.reward;
    RobustSolution { recommended: safe, optimistic, rule_sensitive }
}

/// What happened at each pick when a route is replayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replay {
    pub uploaded: u32,
    pub reward: u32,
    /// For each pick, the daemons uploaded by that pick (bit mask).
    pub uploaded_at: Vec<u32>,
}

/// Independent replay of a route against the rules. Used by tests and before anything is displayed.
pub fn verify(p: &Puzzle, path: &[u16], matcher: Matcher) -> Result<Replay, String> {
    p.validate()?;
    legal(p, path)?;
    let kinds = matcher.kinds();
    let tables: Vec<Vec<Vec<[u8; MAX_CODES]>>> =
        p.daemons.iter().map(|d| kinds.iter().map(|&k| unit_table(&d.seq, k)).collect()).collect();
    let mut state = vec![vec![0u8; kinds.len()]; p.daemons.len()];
    let mut unit_done = vec![vec![false; kinds.len()]; p.daemons.len()];
    let mut uploaded = 0u32;
    let mut reward = 0u32;
    let mut uploaded_at = Vec::with_capacity(path.len());
    for &c in path {
        let code = p.cells[c as usize] as usize;
        let mut now = 0u32;
        for d in 0..p.daemons.len() {
            if uploaded >> d & 1 == 1 {
                continue;
            }
            let len = p.daemons[d].seq.len() as u8;
            for k in 0..kinds.len() {
                if unit_done[d][k] {
                    continue;
                }
                let ns = tables[d][k][state[d][k] as usize][code];
                if ns == len {
                    unit_done[d][k] = true;
                    state[d][k] = 0;
                } else {
                    state[d][k] = ns;
                }
            }
            if unit_done[d].iter().all(|&x| x) {
                uploaded |= 1 << d;
                reward += p.daemons[d].weight;
                now |= 1 << d;
            }
        }
        uploaded_at.push(now);
    }
    Ok(Replay { uploaded, reward, uploaded_at })
}

/// The daemons a run of codes in the buffer uploads under a rule, as a bit mask; the cells the codes came from play no part.
pub fn uploaded_by(sequences: &[Vec<u8>], codes: &[u8], matcher: Matcher) -> u32 {
    let kinds = matcher.kinds();
    let mut uploaded = 0u32;
    for (d, seq) in sequences.iter().enumerate().take(32) {
        if seq.is_empty() || seq.len() > u8::MAX as usize {
            continue;
        }
        let done = kinds.iter().all(|&kind| {
            let table = unit_table(seq, kind);
            let mut state = 0usize;
            codes.iter().any(|&code| {
                state = table[state].get(code as usize).copied().unwrap_or(0) as usize;
                state == seq.len()
            })
        });
        uploaded |= (done as u32) << d;
    }
    uploaded
}

/// A route whose outcome differs between the matching rules, so following it in game tells them apart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub path: Vec<u16>,
    /// Daemons uploaded under each rule (bit masks), in the order substring, retest, strict.
    pub substring: u32,
    pub retest: u32,
    pub strict: u32,
}

impl Probe {
    /// Number of different outcomes among the three rules (2 or 3).
    pub fn outcomes(&self) -> usize {
        self.outcomes_among([true; 3])
    }

    /// Number of different outcomes among the rules still in play (substring, retest, strict).
    pub fn outcomes_among(&self, rules: [bool; 3]) -> usize {
        distinct([self.substring, self.retest, self.strict], rules)
    }
}

/// How many different values the outcomes of the rules in play take.
fn distinct(outcomes: [u32; 3], rules: [bool; 3]) -> usize {
    let mut count = 0;
    for i in 0..3 {
        if rules[i] && !(0..i).any(|j| rules[j] && outcomes[j] == outcomes[i]) {
            count += 1;
        }
    }
    count
}

/// Most informative probe: three distinct outcomes beat two, then fewer picks; `None` if no route separates the rules.
pub fn rule_probe(p: &Puzzle, node_budget: u64) -> Option<Probe> {
    rule_probe_among(p, node_budget, [true; 3])
}

/// A route whose outcome differs between the rules still in play (substring, retest, strict), as many ways as possible.
pub fn rule_probe_among(p: &Puzzle, node_budget: u64, rules: [bool; 3]) -> Option<Probe> {
    if p.validate().is_err() || p.daemons.is_empty() {
        return None;
    }
    let in_play = rules.iter().filter(|r| **r).count();
    let kinds = [Kind::Kmp, Kind::Retest, Kind::Strict];
    let tables: Vec<[Vec<[u8; MAX_CODES]>; 3]> = p
        .daemons
        .iter()
        .map(|d| {
            [
                unit_table(&d.seq, kinds[0]),
                unit_table(&d.seq, kinds[1]),
                unit_table(&d.seq, kinds[2]),
            ]
        })
        .collect();
    struct Walk<'a> {
        p: &'a Puzzle,
        tables: &'a [[Vec<[u8; MAX_CODES]>; 3]],
        path: Vec<u16>,
        best: Option<Probe>,
        nodes: u64,
        budget: u64,
        rules: [bool; 3],
        in_play: usize,
    }
    impl Walk<'_> {
        fn go(&mut self, line: usize, is_row: bool, used: u128, st: &[[u8; 3]], done: [u32; 3]) {
            self.nodes += 1;
            if self.nodes > self.budget || self.path.len() == self.p.buffer {
                return;
            }
            // a probe that already separates every rule in play cannot be beaten by going deeper
            if let Some(b) = &self.best {
                if b.outcomes_among(self.rules) == self.in_play
                    && self.path.len() + 1 >= b.path.len()
                {
                    return;
                }
            }
            let span = if is_row { self.p.cols } else { self.p.rows };
            for k in 0..span {
                let cell = if is_row { line * self.p.cols + k } else { k * self.p.cols + line };
                if used >> cell & 1 == 1 {
                    continue;
                }
                let code = self.p.cells[cell] as usize;
                let mut st2 = st.to_vec();
                let mut done2 = done;
                for (d, t) in self.tables.iter().enumerate() {
                    let len = self.p.daemons[d].seq.len() as u8;
                    for r in 0..3 {
                        if done[r] >> d & 1 == 1 {
                            continue;
                        }
                        let ns = t[r][st[d][r] as usize][code];
                        if ns == len {
                            done2[r] |= 1 << d;
                            st2[d][r] = 0;
                        } else {
                            st2[d][r] = ns;
                        }
                    }
                }
                self.path.push(cell as u16);
                if distinct(done2, self.rules) > 1 {
                    let cand = Probe {
                        path: self.path.clone(),
                        substring: done2[0],
                        retest: done2[1],
                        strict: done2[2],
                    };
                    let rank =
                        |x: &Probe| (x.outcomes_among(self.rules), std::cmp::Reverse(x.path.len()));
                    let better = match &self.best {
                        None => true,
                        Some(b) => rank(&cand) > rank(b),
                    };
                    if better {
                        self.best = Some(cand);
                    }
                }
                self.go(k, !is_row, used | (1u128 << cell), &st2, done2);
                self.path.pop();
            }
        }
    }
    let mut w = Walk {
        p,
        tables: &tables,
        path: Vec::new(),
        best: None,
        nodes: 0,
        budget: node_budget,
        rules,
        in_play,
    };
    w.go(0, true, 0, &vec![[0u8; 3]; p.daemons.len()], [0; 3]);
    w.best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pz(rows: &[&[u8]], seqs: &[&[u8]], buffer: usize) -> Puzzle {
        Puzzle::square(
            rows.len(),
            rows.iter().flat_map(|r| r.iter().copied()).collect(),
            seqs.iter().map(|s| s.to_vec()).collect(),
            buffer,
        )
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Exhaustive enumeration scored by replay: shares nothing with the solver except `verify`.
    fn brute(p: &Puzzle, m: Matcher) -> (u32, usize) {
        fn rec(
            p: &Puzzle,
            m: Matcher,
            path: &mut Vec<u16>,
            line: usize,
            is_row: bool,
            best: &mut (u32, usize),
        ) {
            if !path.is_empty() {
                let r = verify(p, path, m).unwrap().reward;
                if r > best.0 || (r == best.0 && r > 0 && path.len() < best.1) {
                    *best = (r, path.len());
                }
            }
            if path.len() == p.buffer {
                return;
            }
            let span = if is_row { p.cols } else { p.rows };
            for k in 0..span {
                let cell = (if is_row { line * p.cols + k } else { k * p.cols + line }) as u16;
                if path.contains(&cell) {
                    continue;
                }
                path.push(cell);
                rec(p, m, path, k, !is_row, best);
                path.pop();
            }
        }
        let mut best = (0u32, usize::MAX);
        rec(p, m, &mut Vec::new(), 0, true, &mut best);
        best
    }

    /// Every route that begins with `played`, tried one by one: the best reward and the fewest picks that earn it.
    fn brute_from(p: &Puzzle, m: Matcher, played: &[u16]) -> (u32, usize) {
        fn rec(p: &Puzzle, m: Matcher, path: &mut Vec<u16>, best: &mut (u32, usize)) {
            if !path.is_empty() {
                let r = verify(p, path, m).unwrap().reward;
                if r > best.0 || (r == best.0 && r > 0 && path.len() < best.1) {
                    *best = (r, path.len());
                }
            }
            if path.len() == p.buffer {
                return;
            }
            for cell in 0..(p.rows * p.cols) as u16 {
                path.push(cell);
                if legal(p, path).is_ok() {
                    rec(p, m, path, best);
                }
                path.pop();
            }
        }
        let mut best = (0u32, usize::MAX);
        rec(p, m, &mut played.to_vec(), &mut best);
        best
    }

    /// A legal start of `len` picks, chosen at random.
    fn random_start(p: &Puzzle, rng: &mut Rng, len: usize) -> Vec<u16> {
        let mut path: Vec<u16> = Vec::new();
        while path.len() < len {
            let open: Vec<u16> = (0..(p.rows * p.cols) as u16)
                .filter(|&c| {
                    path.push(c);
                    let ok = legal(p, &path).is_ok();
                    path.pop();
                    ok
                })
                .collect();
            if open.is_empty() {
                break;
            }
            path.push(open[rng.below(open.len())]);
        }
        path
    }

    #[test]
    fn resuming_matches_exhaustive_search_from_any_legal_start() {
        let mut rng = Rng(0x0BAD_5EED_1357_9BDF);
        for case in 0..300 {
            let n = 3 + rng.below(2);
            let codes = 2 + rng.below(3);
            let nd = 1 + rng.below(4);
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(codes) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..nd)
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(codes) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells, seqs, 3 + rng.below(4));
            let len = rng.below(p.buffer + 1);
            let played = random_start(&p, &mut rng, len);
            for m in [Matcher::Substring, Matcher::Retest, Matcher::Strict, Matcher::Unanimous] {
                let want = brute_from(&p, m, &played);
                let opt = SolveOptions { matcher: m, node_budget: u64::MAX, dp_bound: true };
                let got = solve_from(&p, &opt, &played).unwrap();
                assert!(got.proven && got.path.starts_with(&played), "case {case} {m:?}");
                let len = if got.reward == 0 { usize::MAX } else { got.path.len() };
                assert_eq!((got.reward, len), want, "case {case} {m:?} played {played:?} {p:?}");
                if !got.path.is_empty() {
                    let rp = verify(&p, &got.path, m).unwrap();
                    assert_eq!((rp.reward, rp.uploaded), (got.reward, got.uploaded));
                }
            }
        }
    }

    #[test]
    fn resuming_from_nothing_is_solving_and_an_illegal_start_is_refused() {
        let p = pz(&[&[0, 1, 2], &[1, 2, 0], &[2, 0, 1]], &[&[0, 1], &[2, 2]], 4);
        let opt = SolveOptions::default();
        assert_eq!(solve_from(&p, &opt, &[]).unwrap().path, solve_with(&p, &opt).path);
        assert!(solve_from(&p, &opt, &[4]).is_err(), "a first pick outside the top row");
        assert!(
            solve_from(&p, &opt, &[0, 1]).is_err(),
            "a second pick outside the first one's column"
        );
    }

    #[test]
    fn matches_exhaustive_search_under_every_matcher() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for case in 0..400 {
            let n = 3 + rng.below(2);
            let codes = 2 + rng.below(3);
            let nd = 1 + rng.below(4);
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(codes) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..nd)
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(codes) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells, seqs, 3 + rng.below(4));
            for m in [
                Matcher::Substring,
                Matcher::Retest,
                Matcher::Strict,
                Matcher::Unanimous,
                Matcher::NotSubstring,
                Matcher::NotRetest,
                Matcher::NotStrict,
            ] {
                let want = brute(&p, m);
                for dp in [true, false] {
                    let got = solve_with(
                        &p,
                        &SolveOptions { matcher: m, node_budget: u64::MAX, dp_bound: dp },
                    );
                    assert!(got.proven);
                    let len = if got.reward == 0 { usize::MAX } else { got.path.len() };
                    assert_eq!(
                        (got.reward, len),
                        want,
                        "case {case} matcher {m:?} dp {dp} puzzle {p:?}"
                    );
                    if got.reward > 0 {
                        let rp = verify(&p, &got.path, m).unwrap();
                        assert_eq!((rp.reward, rp.uploaded), (got.reward, got.uploaded));
                    }
                }
            }
        }
    }

    // Expected shortest full-solution lengths for known puzzles.
    #[test]
    fn known_shortest_solutions() {
        let cases: Vec<(Puzzle, usize)> = vec![
            (
                pz(
                    &[
                        &[0, 1, 0, 0, 0],
                        &[4, 1, 0, 3, 3],
                        &[3, 1, 3, 3, 4],
                        &[4, 1, 1, 1, 1],
                        &[4, 0, 1, 0, 0],
                    ],
                    &[&[0, 1], &[1, 3], &[3, 4, 0]],
                    6,
                ),
                6,
            ),
            (
                pz(
                    &[
                        &[0, 1, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 4, 3],
                        &[0, 2, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                    ],
                    &[&[1, 2], &[3, 4]],
                    5,
                ),
                5,
            ),
            (
                pz(
                    &[
                        &[0, 1, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                        &[0, 2, 0, 0, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                        &[0, 3, 0, 0, 0, 4],
                        &[0, 0, 0, 0, 0, 0],
                    ],
                    &[&[1, 2], &[3, 4]],
                    7,
                ),
                7,
            ),
            (
                pz(
                    &[
                        &[1, 0, 0, 0, 0, 0],
                        &[2, 2, 0, 0, 0, 0],
                        &[0, 0, 2, 0, 0, 0],
                        &[0, 0, 3, 4, 0, 0],
                        &[0, 0, 0, 1, 0, 0],
                        &[0, 0, 0, 0, 0, 0],
                    ],
                    &[&[1, 2, 2], &[2, 3], &[3, 4, 1]],
                    8,
                ),
                8,
            ),
        ];
        for (p, len) in cases {
            let s = solve(&p, Matcher::Substring);
            assert_eq!(s.reward, p.total_weight());
            assert_eq!(s.path.len(), len);
        }
        // needs 6 picks with a buffer of 5: the best subset now, everything at 6
        let p = pz(
            &[
                &[1, 0, 0, 1, 0],
                &[0, 0, 0, 1, 3],
                &[1, 3, 4, 1, 0],
                &[3, 3, 1, 0, 0],
                &[0, 0, 1, 1, 0],
            ],
            &[&[0, 3], &[1, 0, 0], &[0, 0, 1]],
            5,
        );
        let s = solve(&p, Matcher::Substring);
        assert!(s.reward > 0 && s.reward < p.total_weight());
        let p6 = Puzzle { buffer: 6, ..p };
        assert_eq!(solve(&p6, Matcher::Substring).reward, p6.total_weight());
    }

    #[test]
    fn matchers_disagree_exactly_where_expected() {
        let feed = |seq: &[u8], kind: Kind, input: &[usize]| {
            let t = unit_table(seq, kind);
            let mut s = 0u8;
            for &c in input {
                s = t[s as usize][c];
                if s as usize == seq.len() {
                    return true;
                }
            }
            false
        };
        // A A B fed A A A B: a substring, but both reset rules lose it
        assert!(feed(&[0, 0, 1], Kind::Kmp, &[0, 0, 0, 1]));
        assert!(!feed(&[0, 0, 1], Kind::Retest, &[0, 0, 0, 1]));
        assert!(!feed(&[0, 0, 1], Kind::Strict, &[0, 0, 0, 1]));
        // A B fed A A B: re-test catches it, strict does not
        assert!(feed(&[0, 1], Kind::Retest, &[0, 0, 1]));
        assert!(!feed(&[0, 1], Kind::Strict, &[0, 0, 1]));
        // A A B fed A A A A A B: strict catches it and re-test does not, so neither rule contains the other
        assert!(feed(&[0, 0, 1], Kind::Strict, &[0, 0, 0, 0, 0, 1]));
        assert!(!feed(&[0, 0, 1], Kind::Retest, &[0, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn codes_alone_upload_what_their_route_uploads() {
        let mut rng = Rng(0x5EED_C0DE_5EED_C0DE);
        let all = [
            Matcher::Substring,
            Matcher::Retest,
            Matcher::Strict,
            Matcher::Unanimous,
            Matcher::NotSubstring,
            Matcher::NotRetest,
            Matcher::NotStrict,
        ];
        let mut uploads = 0;
        for _ in 0..300 {
            let n = 4 + rng.below(2);
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(3) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..2 + rng.below(2))
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(3) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells.clone(), seqs.clone(), 5 + rng.below(4));
            for m in all {
                let path = solve(&p, m).path;
                let codes: Vec<u8> = path.iter().map(|&c| cells[c as usize]).collect();
                let replay = verify(&p, &path, m).unwrap();
                assert_eq!(
                    uploaded_by(&seqs, &codes, m),
                    replay.uploaded,
                    "{m:?} {codes:?} {seqs:?}"
                );
                // and the same codes under another rule are judged by that rule
                for other in all {
                    let there = verify(&p, &path, other).unwrap().uploaded;
                    assert_eq!(uploaded_by(&seqs, &codes, other), there, "{other:?} {codes:?}");
                }
                uploads += replay.uploaded.count_ones();
            }
        }
        assert!(uploads > 1000, "{uploads}");
    }

    #[test]
    fn robust_route_is_valid_under_every_rule() {
        let mut rng = Rng(0xDEAD_BEEF_0BAD_F00D);
        let mut sensitive = 0;
        for _ in 0..300 {
            let n = 4 + rng.below(2);
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(3) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..3)
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(3) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells, seqs, 4 + rng.below(4));
            let r = solve_robust(&p, u64::MAX);
            if r.recommended.reward > 0 {
                for m in [Matcher::Substring, Matcher::Retest, Matcher::Strict] {
                    let rp = verify(&p, &r.recommended.path, m).unwrap();
                    assert_eq!(
                        rp.uploaded & r.recommended.uploaded,
                        r.recommended.uploaded,
                        "{m:?} {p:?}"
                    );
                }
            }
            assert!(r.recommended.reward <= r.optimistic.reward);
            sensitive += r.rule_sensitive as u32;
        }
        assert!(sensitive > 0, "expected some rule-sensitive puzzles with 3 codes");
    }

    #[test]
    fn probe_routes_really_separate_the_rules() {
        let mut rng = Rng(0x0F0F_1234_5678_9ABC);
        let (mut found, mut three) = (0, 0);
        for _ in 0..200 {
            let n = 5;
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(4) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..3)
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(4) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells, seqs, 6);
            if let Some(probe) = rule_probe(&p, 2_000_000) {
                found += 1;
                three += (probe.outcomes() == 3) as u32;
                // the claimed outcomes are what an independent replay gives
                assert_eq!(
                    verify(&p, &probe.path, Matcher::Substring).unwrap().uploaded,
                    probe.substring
                );
                assert_eq!(
                    verify(&p, &probe.path, Matcher::Retest).unwrap().uploaded,
                    probe.retest
                );
                assert_eq!(
                    verify(&p, &probe.path, Matcher::Strict).unwrap().uploaded,
                    probe.strict
                );
                assert!(probe.outcomes() >= 2);
            }
        }
        assert!(found > 20, "probes found in only {found} of 200 puzzles");
        let _ = three;
    }

    #[test]
    fn a_probe_among_two_rules_separates_exactly_those() {
        let mut rng = Rng(0x7777_1234_ABCD_0001);
        let mut found = 0;
        for _ in 0..200 {
            let n = 5;
            let cells: Vec<u8> = (0..n * n).map(|_| rng.below(4) as u8).collect();
            let seqs: Vec<Vec<u8>> = (0..3)
                .map(|_| (0..2 + rng.below(3)).map(|_| rng.below(4) as u8).collect())
                .collect();
            let p = Puzzle::square(n, cells, seqs, 6);
            for pair in [Matcher::NotSubstring, Matcher::NotRetest, Matcher::NotStrict] {
                let rules = pair.rules();
                if let Some(probe) = rule_probe_among(&p, 2_000_000, rules) {
                    found += 1;
                    assert_eq!(probe.outcomes_among(rules), 2);
                    let seen = [Matcher::Substring, Matcher::Retest, Matcher::Strict]
                        .map(|m| verify(&p, &probe.path, m).unwrap().uploaded);
                    assert_eq!(seen, [probe.substring, probe.retest, probe.strict]);
                }
            }
        }
        assert!(found > 20, "probes found in only {found} of 600 searches");
        assert_eq!(Matcher::of([true, true, false]), Some(Matcher::NotStrict));
        assert_eq!(Matcher::of([false, false, false]), None);
        assert_eq!(Matcher::of(Matcher::Unanimous.rules()), Some(Matcher::Unanimous));
    }

    #[test]
    fn verify_rejects_illegal_routes() {
        let p = pz(&[&[0, 1, 2], &[1, 2, 0], &[2, 0, 1]], &[&[0, 1]], 4);
        assert!(verify(&p, &[3], Matcher::Substring).is_err()); // not top row
        assert!(verify(&p, &[0, 1], Matcher::Substring).is_err()); // second pick must share the column
        assert!(verify(&p, &[0, 3, 5, 3], Matcher::Substring).is_err()); // reuse
        assert!(verify(&p, &[0, 3, 4, 1, 2], Matcher::Substring).is_err()); // over buffer
        let ok = verify(&p, &[0, 3], Matcher::Substring).unwrap();
        assert_eq!(ok.uploaded, 1);
        assert_eq!(ok.uploaded_at, vec![0, 1]);
    }

    #[test]
    fn budget_returns_best_so_far() {
        let mut rng = Rng(42);
        let cells: Vec<u8> = (0..49).map(|_| rng.below(6) as u8).collect();
        let p = Puzzle::square(
            7,
            cells,
            vec![vec![0, 1, 2, 3], vec![4, 5, 4, 5], vec![1, 1, 1, 1]],
            10,
        );
        let s = solve_with(
            &p,
            &SolveOptions { matcher: Matcher::Substring, node_budget: 50, dp_bound: true },
        );
        assert!(!s.proven || s.nodes <= 50);
        if s.reward > 0 {
            assert_eq!(verify(&p, &s.path, Matcher::Substring).unwrap().reward, s.reward);
        }
    }
}
