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
pub struct TradeComputer {
    pub world: World,
    pub cycles: Vec<Cycle>,
    pub problem: TradeProblem,
    pub ising: Ising,
    /// QUBO units -> kT: how hard the springs pull compared to the shaking.
    pub beta: f64,
    /// Conflict penalty as a multiple of the largest normalized cycle value.
    pub penalty: f64,
    /// Backward mode target, if any.
    pub want: Option<(usize, usize)>,
    bias: Option<Vec<f64>>,
}

impl TradeComputer {
    pub fn new(world: World, beta: f64, penalty: f64) -> Self {
        let cycles = cycles::enumerate(&world, MAX_LOOP);
        let problem = qubo::build(&cycles, None, penalty * qubo::MAX_VALUE);
        let ising = problem.qubo.to_ising(beta);
        Self { world, cycles, problem, ising, beta, penalty, want: None, bias: None }
    }

    fn rebuild(&mut self) {
        self.problem = qubo::build(&self.cycles, self.bias.as_deref(), self.penalty * qubo::MAX_VALUE);
        self.ising = self.problem.qubo.to_ising(self.beta);
    }

    /// Backward mode: make `npc` end up with `item`. Returns `false` (and
    /// changes nothing) if no candidate trade delivers it.
    pub fn want(&mut self, npc: usize, item: usize) -> bool {
        let Some(bias) = qubo::want_bias(&self.cycles, npc, item) else {
            return false;
        };
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
