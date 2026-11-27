//! Deterministic game-like puzzle generator shared by benchmarks and tests.
use breach_core::solver::Puzzle;

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random matrix; every daemon is a rule-legal walk through it, so each is solvable on its own.
pub fn puzzle(rng: &mut Rng, n: usize, codes: usize, lens: &[usize], buffer: usize) -> Puzzle {
    let cells: Vec<u8> = (0..n * n).map(|_| rng.below(codes) as u8).collect();
    let mut seqs = Vec::new();
    for &l in lens {
        loop {
            let mut used = vec![false; n * n];
            let mut is_row = rng.below(2) == 0;
            let mut line = rng.below(n);
            let mut s = Vec::new();
            let mut ok = true;
            for _ in 0..l {
                let free: Vec<usize> = (0..n)
                    .filter(|&k| !used[if is_row { line * n + k } else { k * n + line }])
                    .collect();
                if free.is_empty() {
                    ok = false;
                    break;
                }
                let k = free[rng.below(free.len())];
                let cell = if is_row { line * n + k } else { k * n + line };
                used[cell] = true;
                s.push(cells[cell]);
                line = k;
                is_row = !is_row;
            }
            if ok {
                seqs.push(s);
                break;
            }
        }
    }
    Puzzle::square(n, cells, seqs, buffer)
}
