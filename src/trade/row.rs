//! A row of washers (Step 6): parallel tempering, also called replica
//! exchange, on tonight's board.
//!
//! `K` washers hold copies of the same board. Each drum is fixed at one
//! setting on a geometric ladder from `cold` to `hot`. Every `swap` time
//! units, neighbors offer to trade loads and accept by the replica-exchange
//! rule, so a load stuck in a bad set on a cold washer can ride up the row,
//! get shaken loose, and come back down. The i9 reads every washer's Hall
//! sensors and latches the best set any of them shows.
//!
//! sim-opt's `Pt` runs parallel tempering over a policy's params (a
//! trainer, like CEM); it can't swap board states, so the row is built here
//! on [`Machine`]s.

use super::machine::{Anneal, Error, Machine, Physics};
use super::{Ising, Latch, TradeComputer};

/// A row of washers.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub washers: usize,
    /// Drum settings (multiples of kT) at the two ends of the ladder.
    pub cold: f64,
    pub hot: f64,
    /// Time units between swap offers.
    pub swap: f64,
    /// How long every washer shakes, before the settle.
    pub duration: f64,
    /// How long the drums stop at the end so every strip drops into a well.
    pub settle: f64,
}

/// Washers in the row, and the ladder's ends. Tuned on nights 1-20 (24
/// spins each, equal compute): a warm cold end beats a cold one, because the
/// i9 latches whatever any washer shows and every washer quenches in the
/// settle. Cold end 0.35 (Normal's): 84.6% with 6 washers; 0.7: 88.3%;
/// 1.4: 95.4%; 4 washers at 1.4: 96.5% (Normal 83.1%). PLAN "Step 6".
pub const WASHERS: usize = 4;
pub const COLD: f64 = 1.4;
pub const HOT: f64 = 4.0;

impl Default for Row {
    fn default() -> Self {
        Self::equal_compute(WASHERS)
    }
}

impl Row {
    /// `k` washers sharing one Normal cycle's compute: each shakes 1/k of
    /// the time, settle included (strip-steps match one Normal spin).
    pub fn equal_compute(k: usize) -> Self {
        let a = Anneal::default();
        let k = k.max(1);
        let settle = a.settle;
        let duration = ((a.duration + settle) / k as f64 - settle).max(settle);
        Self { washers: k, cold: COLD, hot: HOT, swap: 1.0, duration, settle }
    }

    /// `k` washers that each run a whole Normal cycle (k times the compute).
    pub fn full(k: usize) -> Self {
        Self { duration: Anneal::default().duration, ..Self::equal_compute(k) }
    }

    /// Washer `i`'s drum setting: geometric from cold (0) to hot (k - 1).
    pub fn ladder(&self) -> Vec<f64> {
        let k = self.washers.max(1);
        if k == 1 {
            return vec![self.cold];
        }
        (0..k).map(|i| self.cold * (self.hot / self.cold).powf(i as f64 / (k - 1) as f64)).collect()
    }

    pub fn total_time(&self) -> f64 {
        self.duration + self.settle
    }
}

/// How the swaps went, per neighbor pair (pair `i` is washers i and i+1).
#[derive(Clone, Debug, Default)]
pub struct RowStats {
    pub offers: Vec<usize>,
    pub accepts: Vec<usize>,
    /// Swaps that brought a load all the way from the hot end to the cold
    /// end (a round trip's down leg): the row really moving loads.
    pub trips: usize,
}

impl RowStats {
    /// Accept rate of pair `i`.
    pub fn rate(&self, i: usize) -> f64 {
        self.accepts[i] as f64 / self.offers[i].max(1) as f64
    }
}

/// Small deterministic PRNG for the swap coin (xorshift64*).
struct Coin(u64);

impl Coin {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A row of washers in motion: the ladder, the swap coin, and where the
/// loads are. [`TradeComputer::spin_row`] runs one to the end; the game
/// steps one a frame at a time.
pub struct RowSpin {
    /// Each washer's drum setting (multiplier) and kT.
    ladder: Vec<f64>,
    kt: Vec<f64>,
    swap_steps: usize,
    steps: usize,
    coin: Coin,
    round: usize,
    pub stats: RowStats,
    /// Which load each washer holds (loads keep their number as they move).
    pub load: Vec<usize>,
    /// Whether each load last touched the hot end (for counting trips).
    from_hot: Vec<bool>,
}

impl RowSpin {
    /// Sets every washer's drum to its rung of the ladder (coldest first).
    pub fn new(row: &Row, washers: &mut [&mut Machine], seed: u64) -> Self {
        let k = washers.len();
        assert_eq!(k, row.washers, "one machine per washer");
        let ladder = row.ladder();
        let kt: Vec<f64> = ladder.iter().map(|t| t * washers[0].physics.k_b_t).collect();
        for (m, &t) in washers.iter_mut().zip(&ladder) {
            m.set_temperature(t);
        }
        let dt = washers[0].physics.dt;
        let swap_steps = if row.swap.is_finite() { ((row.swap / dt).round() as usize).max(1) } else { usize::MAX };
        Self {
            ladder,
            kt,
            swap_steps,
            steps: 0,
            coin: Coin::new(seed),
            round: 0,
            stats: RowStats { offers: vec![0; k.saturating_sub(1)], accepts: vec![0; k.saturating_sub(1)], trips: 0 },
            load: (0..k).collect(),
            from_hot: vec![false; k],
        }
    }

    /// Washer `i`'s drum setting.
    pub fn setting(&self, i: usize) -> f64 {
        self.ladder[i]
    }

    /// One sim step of every washer, then (when due) a round of swap
    /// offers, even pairs and odd pairs in turn. Returns the pairs that
    /// traded loads this step.
    pub fn step(&mut self, ising: &Ising, washers: &mut [&mut Machine]) -> Result<Vec<usize>, Error> {
        let k = washers.len();
        for m in washers.iter_mut() {
            m.step()?;
        }
        self.steps += 1;
        let mut swapped = vec![];
        if self.steps % self.swap_steps == 0 && k > 1 {
            for i in (self.round % 2..k - 1).step_by(2) {
                self.stats.offers[i] += 1;
                let (ui, uj) = (washers[i].potential(ising), washers[i + 1].potential(ising));
                let arg = (1.0 / self.kt[i] - 1.0 / self.kt[i + 1]) * (ui - uj);
                if arg >= 0.0 || self.coin.uniform() < arg.exp() {
                    let (a, b) = washers.split_at_mut(i + 1);
                    // Each drum keeps its setting: only the loads move.
                    Machine::swap_loads(a[i], b[0])?;
                    self.load.swap(i, i + 1);
                    self.stats.accepts[i] += 1;
                    swapped.push(i);
                }
            }
            self.round += 1;
            self.from_hot[self.load[k - 1]] = true;
            if self.from_hot[self.load[0]] {
                self.from_hot[self.load[0]] = false;
                self.stats.trips += 1;
            }
        }
        Ok(swapped)
    }
}

impl TradeComputer {
    /// The row's washers, fresh from the laundry basket (each its own seed).
    pub fn row_machines(&self, row: &Row, physics: Physics, seed: u64) -> Result<Vec<Machine>, Error> {
        (0..row.washers).map(|k| self.machine(physics, seed.wrapping_add(k as u64 * 104_729))).collect()
    }

    /// Runs a row of washers on tonight's board. The i9 reads every washer
    /// every `sample` time units and latches the best set; `final_bits` is
    /// the coldest washer's set after the settle. `tick` fires every
    /// `every` time units with the washers, coldest first.
    pub fn spin_row(
        &self,
        washers: &mut [Machine],
        row: &Row,
        seed: u64,
        sample: f64,
        every: f64,
        mut tick: impl FnMut(&[Machine], &Latch),
    ) -> Result<(Latch, RowStats), Error> {
        let dt = washers[0].physics.dt;
        let every_steps = |t: f64| if t.is_finite() { ((t / dt).round() as usize).max(1) } else { usize::MAX };
        let (sample_steps, tick_steps) = (every_steps(sample), every_steps(every));
        let steps = (row.duration / dt).round() as usize;
        let mut spin = RowSpin::new(row, &mut washers.iter_mut().collect::<Vec<_>>(), seed);
        let mut latch = Latch::new();
        for s in 1..=steps {
            spin.step(&self.ising, &mut washers.iter_mut().collect::<Vec<_>>())?;
            if s % sample_steps == 0 {
                for m in washers.iter() {
                    latch.observe(m, &self.problem.qubo);
                }
            }
            if s % tick_steps == 0 {
                tick(washers, &latch);
            }
        }
        for m in washers.iter_mut() {
            m.rest(row.settle)?;
            latch.observe(m, &self.problem.qubo);
        }
        latch.final_bits = washers[0].bits();
        Ok((latch, spin.stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world;

    #[test]
    fn ladder_is_geometric_cold_first() {
        let r = Row { washers: 3, cold: 0.5, hot: 2.0, ..Row::default() };
        let l = r.ladder();
        assert!((l[0] - 0.5).abs() < 1e-12 && (l[1] - 1.0).abs() < 1e-12 && (l[2] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn equal_compute_matches_one_normal_spin() {
        let a = Anneal::default();
        let r = Row::equal_compute(5);
        assert!((r.washers as f64 * r.total_time() - a.total_time()).abs() < 1e-9);
    }

    /// Swapping twice puts every strip back, and the energy moves with the load.
    #[test]
    fn swap_moves_the_load() {
        let tc = TradeComputer::new(world::laundromat(1), 5.0, 1.6);
        let mut ms = tc.row_machines(&Row::equal_compute(2), Physics::default(), 3).unwrap();
        for _ in 0..200 {
            for m in ms.iter_mut() {
                m.step().unwrap();
            }
        }
        let (u0, u1) = (ms[0].potential(&tc.ising), ms[1].potential(&tc.ising));
        let x0 = ms[0].positions().to_vec();
        let (a, b) = ms.split_at_mut(1);
        Machine::swap_loads(&mut a[0], &mut b[0]).unwrap();
        assert!((ms[0].potential(&tc.ising) - u1).abs() < 1e-12 && (ms[1].potential(&tc.ising) - u0).abs() < 1e-12);
        let (a, b) = ms.split_at_mut(1);
        Machine::swap_loads(&mut a[0], &mut b[0]).unwrap();
        assert_eq!(ms[0].positions(), &x0[..]);
    }

    /// The crate's energies agree with the formulas their docs give:
    /// dV (x^2 - 1)^2, -J x_i x_k and -h x.
    #[test]
    fn potential_matches_the_formulas() {
        let tc = TradeComputer::new(world::laundromat(4), 5.0, 1.6);
        let mut m = tc.machine(Physics::default(), 9).unwrap();
        for _ in 0..300 {
            m.step().unwrap();
        }
        let x = m.positions();
        let dv = m.physics.delta_v;
        let wells: f64 = x.iter().map(|&x| dv * (x * x - 1.0).powi(2)).sum();
        let fields: f64 = x.iter().zip(&tc.ising.h).map(|(x, h)| -h * x).sum();
        let springs: f64 = tc.ising.edges.iter().zip(&tc.ising.j).map(|(&(i, k), j)| -j * x[i] * x[k]).sum();
        assert!((m.potential(&tc.ising) - (wells + fields + springs)).abs() < 1e-9);
    }

    /// A short row on night 1 finds the best set and actually swaps.
    #[test]
    fn row_finds_night_one() {
        let tc = TradeComputer::new(world::laundromat(1), 5.0, 1.6);
        let row = Row::equal_compute(4);
        let mut ms = tc.row_machines(&row, Physics::default(), 1).unwrap();
        let (latch, stats) = tc.spin_row(&mut ms, &row, 1, 1.0, f64::INFINITY, |_, _| {}).unwrap();
        assert!(stats.accepts.iter().sum::<usize>() > 0, "no swaps: {stats:?}");
        assert!(tc.is_optimal(latch.best_bits));
    }
}
