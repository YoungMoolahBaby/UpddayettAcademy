//! Lesson 3, "Money Laundering (Legally)": a laundromat trade computer.
//!
//! Customers bring haves and wants ([`world`]). Every closed giving loop in
//! which everybody comes out ahead is a candidate trade ([`cycles`]). Picking
//! the best set of trades that never moves an item twice is a QUBO
//! ([`qubo`]), which becomes the springs and biases of a board of bistable
//! strips that CortenForge's thermostat shakes and cools ([`machine`]).

pub mod cycles;
pub mod machine;
pub mod qubo;
pub mod world;

pub use cycles::{Cycle, Leg};
pub use machine::{Anneal, Error, Machine, Physics};
pub use qubo::{Ising, Qubo, TradeProblem};
pub use world::World;

use cortenforge::sim::thermostat::ising::exact_distribution;

/// The longest loop the machine considers.
pub const MAX_LOOP: usize = 4;

/// The crate's exact solver enumerates all states and stops at 20 bits.
pub const MAX_EXACT_BITS: usize = 20;

/// Extra want bias beyond the bare minimum, as a fraction of the smallest
/// cycle value: the energy gap the anneal gets to find the wanted chain.
pub const WANT_MARGIN: f64 = 1.0;

/// What the i9 saw during one spin cycle.
#[derive(Clone, Copy, Debug)]
pub struct Latch {
    /// Where the strips came to rest.
    pub final_bits: u32,
    /// Lowest-energy configuration read while every strip sat in a well.
    pub best_bits: u32,
    pub best_energy: f64,
    pub best_time: f64,
}

impl Default for Latch {
    fn default() -> Self {
        Self::new()
    }
}

impl Latch {
    /// Nothing latched yet.
    pub fn new() -> Self {
        Self { final_bits: 0, best_bits: 0, best_energy: f64::INFINITY, best_time: 0.0 }
    }

    /// Whether anything has been latched.
    pub fn has_best(&self) -> bool {
        self.best_energy.is_finite()
    }

    /// One Hall-sensor read: latch the strips if they all sit in a well and
    /// beat the best so far.
    pub fn observe(&mut self, m: &Machine, q: &Qubo) {
        if !m.all_in_wells() {
            return;
        }
        let bits = m.bits();
        let e = q.energy(bits);
        if e < self.best_energy - 1e-12 {
            self.best_bits = bits;
            self.best_energy = e;
            self.best_time = m.time();
        }
    }
}

/// Everything needed to run the machine on one night's laundry.
#[derive(Clone)]
pub struct TradeComputer {
    pub world: World,
    pub cycles: Vec<Cycle>,
    pub problem: TradeProblem,
    pub ising: Ising,
    /// QUBO units -> kT: how hard the springs pull compared to the shaking.
    pub beta: f64,
    /// Conflict penalty as a multiple of the largest normalized cycle value.
    pub penalty: f64,
    /// Want bias beyond the bare minimum, as a fraction of the smallest
    /// cycle value (default [`WANT_MARGIN`]). Takes effect on the next `want`.
    pub want_margin: f64,
    /// Backward mode target, if any.
    pub want: Option<(usize, usize)>,
    /// Best trade set with no want (forward mode), for pricing a want.
    pub forward_best: u32,
    bias: Option<Vec<f64>>,
}

impl TradeComputer {
    pub fn new(world: World, beta: f64, penalty: f64) -> Self {
        let cycles = cycles::enumerate(&world, MAX_LOOP);
        let problem = qubo::build(&cycles, None, penalty * qubo::MAX_VALUE);
        let ising = problem.qubo.to_ising(beta);
        let forward_best = problem.qubo.brute_force().0;
        Self { world, cycles, problem, ising, beta, penalty, want_margin: WANT_MARGIN, want: None, forward_best, bias: None }
    }

    /// `qubo::want_bias` is a worst-case bound and often pulls far harder
    /// than needed (fields up to ~2.6x the well-flattening tilt, even for
    /// wants the forward best already delivers). Strong fields distort the
    /// soft spins, so bisect for the smallest bias whose exact ground state
    /// delivers the want, then add a margin so the anneal sees a clear gap.
    /// Delivery is monotone in the bias: it only lowers delivering sets.
    fn gentlest_bias(&self, npc: usize, item: usize, upper: &[f64]) -> Vec<f64> {
        let hits: Vec<bool> = upper.iter().map(|&b| b > 0.0).collect();
        let with = |b: f64| -> Vec<f64> { hits.iter().map(|&h| if h { b } else { 0.0 }).collect() };
        let delivers = |b: f64| {
            let p = qubo::build(&self.cycles, Some(&with(b)), self.penalty * qubo::MAX_VALUE);
            let g = p.qubo.brute_force().0;
            qubo::chosen(g, self.cycles.len()).iter().any(|&i| self.cycles[i].delivers(npc, item))
        };
        let (mut lo, mut hi) = (0.0, upper.iter().copied().fold(0.0, f64::max));
        if delivers(lo) {
            hi = lo;
        } else {
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if delivers(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
        }
        let vmin = self.cycles.iter().map(Cycle::value).fold(f64::INFINITY, f64::min);
        with(hi + self.want_margin * vmin)
    }

    /// Forward mode again: no want.
    pub fn clear_want(&mut self) {
        self.bias = None;
        self.want = None;
        self.rebuild();
    }

    /// Items some candidate trade delivers to `npc`, in item order.
    pub fn deliverable(&self, npc: usize) -> Vec<usize> {
        (0..self.world.items.len()).filter(|&item| self.cycles.iter().any(|c| c.delivers(npc, item))).collect()
    }

    /// What granting the want cost everyone, in world value units: the
    /// forward-mode best minus the value of `bits`.
    pub fn want_cost(&self, bits: u32) -> f64 {
        self.evaluate(self.forward_best).0 - self.evaluate(bits).0
    }

    fn rebuild(&mut self) {
        self.problem = qubo::build(&self.cycles, self.bias.as_deref(), self.penalty * qubo::MAX_VALUE);
        self.ising = self.problem.qubo.to_ising(self.beta);
    }

    /// Backward mode: make `npc` end up with `item`. Returns `false` (and
    /// changes nothing) if no candidate trade delivers it.
    pub fn want(&mut self, npc: usize, item: usize) -> bool {
        let Some(upper) = qubo::want_bias(&self.cycles, npc, item) else {
            return false;
        };
        let bias = if self.cycles.len() <= MAX_EXACT_BITS { self.gentlest_bias(npc, item, &upper) } else { upper };
        self.bias = Some(bias);
        self.want = Some((npc, item));
        self.rebuild();
        true
    }

    pub fn machine(&self, physics: Physics, seed: u64) -> Result<Machine, Error> {
        Machine::new(&self.ising, physics, seed)
    }

    /// Runs a spin cycle while the i9 watches: every `sample` time units it
    /// reads the Hall sensors and, if every strip is in a well, latches the
    /// configuration if it beats the best so far. `tick` fires every `every`
    /// time units for display.
    pub fn spin(
        &self,
        m: &mut Machine,
        anneal: &Anneal,
        sample: f64,
        every: f64,
        mut tick: impl FnMut(&Machine, &Latch),
    ) -> Result<Latch, Error> {
        let dt = m.physics.dt;
        let steps = (anneal.total_time() / dt).round() as usize;
        let sample_steps = ((sample / dt).round() as usize).max(1);
        let tick_steps = if every.is_finite() { ((every / dt).round() as usize).max(1) } else { usize::MAX };
        let t0 = m.time();
        let mut latch = Latch::new();
        for k in 1..=steps {
            m.set_temperature(anneal.temperature(m.time() - t0));
            m.step()?;
            if k % sample_steps == 0 || k == steps {
                latch.observe(m, &self.problem.qubo);
            }
            if k % tick_steps == 0 {
                tick(m, &latch);
            }
        }
        latch.final_bits = m.bits();
        Ok(latch)
    }

    /// Best trade set by brute force over the QUBO.
    pub fn ground_state(&self) -> u32 {
        self.problem.qubo.brute_force().0
    }

    /// Ground state according to the crate's exact Boltzmann solver, with
    /// its probability at temperature `k_b_t`. `None` above 20 bits.
    pub fn exact_ground_state(&self, k_b_t: f64) -> Option<(u32, f64)> {
        if self.ising.n > MAX_EXACT_BITS {
            return None;
        }
        let i = &self.ising;
        let dist = exact_distribution(i.n, &i.edges, &i.j, &i.h, k_b_t);
        dist.into_iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }

    /// Total value (world units) of a set of trades, and whether any two of
    /// them try to move the same item.
    pub fn evaluate(&self, bits: u32) -> (f64, bool) {
        let on = qubo::chosen(bits, self.cycles.len());
        let value = on.iter().map(|&i| self.cycles[i].value()).sum();
        let clash = on.iter().enumerate().any(|(a, &i)| on[a + 1..].iter().any(|&k| self.cycles[i].conflicts(&self.cycles[k])));
        (value, clash)
    }

    /// Is `bits` a best answer? Judged by value, not by matching the
    /// ground state bit for bit: several trade sets can tie (e.g. when
    /// Tamara wants the kale, ~70% of runs find an equally good set that
    /// isn't the brute-force one). Must also be clash-free and deliver the want.
    pub fn is_optimal(&self, bits: u32) -> bool {
        let (v, clash) = self.evaluate(bits);
        let best = self.evaluate(self.ground_state()).0;
        !clash && self.delivers_want(bits) && v >= best - 1e-9
    }

    pub fn delivers_want(&self, bits: u32) -> bool {
        match self.want {
            None => true,
            Some((npc, item)) => qubo::chosen(bits, self.cycles.len()).iter().any(|&i| self.cycles[i].delivers(npc, item)),
        }
    }

    /// Largest net field on any strip while every other strip is off (the
    /// static `h` alone overstates it: the springs cancel most of it), and
    /// the field that would flatten a well.
    pub fn field_headroom(&self, physics: &Physics) -> (f64, f64) {
        let mut f = self.ising.h.clone();
        for (&(a, b), &j) in self.ising.edges.iter().zip(&self.ising.j) {
            f[a] -= j;
            f[b] -= j;
        }
        let fmax = f.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        (fmax, machine::max_safe_field(physics.delta_v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc() -> TradeComputer {
        TradeComputer::new(world::laundromat_tuesday(), 5.0, 1.6)
    }

    #[test]
    fn every_deliverable_want_is_delivered_without_clashes() {
        let mut tc = tc();
        let mut checked = 0;
        for npc in 0..tc.world.npcs.len() {
            for item in tc.deliverable(npc) {
                assert!(tc.want(npc, item));
                let best = tc.ground_state();
                let (_, clash) = tc.evaluate(best);
                let who = tc.world.npcs[npc].name;
                let what = tc.world.items[item].name;
                assert!(tc.delivers_want(best), "{who} wants the {what}: best set doesn't deliver");
                assert!(!clash, "{who} wants the {what}: best set clashes");
                assert!(tc.want_cost(best) >= -1e-9, "a want can't beat the forward optimum");
                checked += 1;
            }
        }
        assert!(checked >= 10, "only {checked} deliverable wants in the test world");
    }

    #[test]
    fn undeliverable_wants_are_refused_and_clear_restores_forward_mode() {
        let mut tc = tc();
        let upd = tc.world.find_npc("upddayett").unwrap();
        let owned = tc.world.find_item("Mtn Goo").unwrap();
        assert!(!tc.deliverable(upd).contains(&owned));
        assert!(!tc.want(upd, owned));
        assert!(tc.want.is_none());

        let hub = tc.world.find_item("hub motor").unwrap();
        assert!(tc.want(upd, hub));
        tc.clear_want();
        assert!(tc.want.is_none());
        assert_eq!(tc.ground_state(), tc.forward_best);
    }

    #[test]
    fn rewiring_keeps_the_strips_where_they_were() {
        let mut tc = tc();
        let physics = Physics::default();
        let mut old = tc.machine(physics, 7).unwrap();
        old.set_temperature(1.5);
        for _ in 0..500 {
            old.step().unwrap();
        }
        let hub = tc.world.find_item("hub motor").unwrap();
        assert!(tc.want(0, hub));
        let mut new = tc.machine(physics, 8).unwrap();
        new.take_state_from(&old).unwrap();
        assert_eq!(new.positions(), old.positions());
        assert_eq!(new.velocities(), old.velocities());
        assert_eq!(new.time(), old.time());
        assert_eq!(new.temperature(), old.temperature());
    }
}
