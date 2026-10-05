//! The laundromat's trade computer, stepped by CortenForge every frame.

use bevy::prelude::*;
use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::{Anneal, Latch, Machine, Physics, TradeComputer, world};

/// What the drum is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// The player holds the spin dial.
    Manual,
    /// A full spin cycle, started at this sim time.
    Cycle { start: f64 },
    /// Cycle finished; the drum is stopped and the i9 shows its call.
    Done,
}

#[derive(Resource)]
pub struct Laundromat {
    pub tc: TradeComputer,
    pub machine: Machine,
    pub physics: Physics,
    pub anneal: Anneal,
    pub latch: Latch,
    pub mode: Mode,
    /// Manual-mode temperature multiplier (the spin dial).
    pub dial: f64,
    /// Sim time units per real second.
    pub speed: f64,
    pub seed: u64,
    /// The QUBO's best answer, for the scoreboard.
    pub ground: u32,
    /// Per-trade "on-ness" in 0..1, smoothed for display.
    pub activity: Vec<f32>,
}

/// How often the i9 reads the Hall sensors (sim time units).
const SAMPLE: f64 = 1.0;
/// Most steps per frame, so a slow frame can't snowball.
const MAX_STEPS_PER_FRAME: usize = 5_000;

impl Laundromat {
    pub fn new() -> Self {
        let tc = TradeComputer::new(world::laundromat_tuesday(), 5.0, 1.6);
        let physics = Physics::default();
        let seed = 1;
        let machine = tc.machine(physics, seed).expect("build the slap-bit board");
        let ground = tc.ground_state();
        let n = tc.cycles.len();
        Self {
            tc,
            machine,
            physics,
            anneal: Anneal::default(),
            latch: Latch::new(),
            mode: Mode::Manual,
            dial: 1.5,
            speed: 40.0,
            seed,
            ground,
            activity: vec![0.0; n],
        }
    }

    pub fn n(&self) -> usize {
        self.machine.n
    }

    /// Fresh laundry: rebuild the board with new random strip positions.
    pub fn new_load(&mut self) {
        self.seed += 1;
        self.machine = self.tc.machine(self.physics, self.seed).expect("build the slap-bit board");
        self.latch = Latch::new();
        self.mode = Mode::Manual;
        info!("new load: seed {}, strips re-randomized", self.seed);
    }

    pub fn start_cycle(&mut self) {
        self.latch = Latch::new();
        self.mode = Mode::Cycle { start: self.machine.time() };
        info!(
            "spin cycle started at sim t={:.0}: kT x{} -> x{} over {} units, speed {:.0} units/s",
            self.machine.time(),
            self.anneal.hot,
            self.anneal.cold,
            self.anneal.duration,
            self.speed
        );
    }

    /// Log what the drum settled on and what the i9 latched.
    fn log_result(&self) {
        let n = self.n();
        let mask = |b: u32| (0..n).map(|i| if (b >> i) & 1 == 1 { '#' } else { '.' }).collect::<String>();
        let (rest, rest_clash) = self.tc.evaluate(self.latch.final_bits);
        let (best, _) = self.tc.evaluate(self.latch.best_bits);
        let (ground, _) = self.tc.evaluate(self.ground);
        info!(
            "drum stopped: at rest {} = {rest:.0} Goo{} | i9 latched {} = {best:.0} Goo at t={:.0} | best possible {} = {ground:.0} Goo -> {}",
            mask(self.latch.final_bits),
            if rest_clash { " (clash)" } else { "" },
            mask(self.latch.best_bits),
            self.latch.best_time,
            mask(self.ground),
            if self.latch.best_bits == self.ground { "BEST" } else { "missed" }
        );
    }

    /// Drum temperature multiplier right now.
    pub fn temperature(&self) -> f64 {
        self.machine.temperature()
    }

    /// 0..1 through the current spin cycle.
    pub fn progress(&self) -> f64 {
        match self.mode {
            Mode::Cycle { start } => ((self.machine.time() - start) / self.anneal.total_time()).clamp(0.0, 1.0),
            Mode::Done => 1.0,
            Mode::Manual => 0.0,
        }
    }

    /// Is trade `i` locked in by the i9 (cycle finished)?
    pub fn locked(&self, i: usize) -> bool {
        self.mode == Mode::Done && (self.latch.best_bits >> i) & 1 == 1
    }

    pub fn well(&self, i: usize) -> WellState {
        self.machine.well(i)
    }
}

pub fn step_sim(time: Res<Time>, mut lm: ResMut<Laundromat>) {
    let lm = &mut *lm;
    let dt = lm.physics.dt;
    let real = time.delta_secs_f64().min(0.1);
    let steps = ((lm.speed * real / dt).round() as usize).min(MAX_STEPS_PER_FRAME);
    let sample_steps = (SAMPLE / dt).round() as u64;
    for _ in 0..steps {
        match lm.mode {
            Mode::Manual => lm.machine.set_temperature(lm.dial),
            Mode::Done => lm.machine.set_temperature(0.0),
            Mode::Cycle { start } => {
                let t = lm.machine.time() - start;
                if t >= lm.anneal.total_time() {
                    lm.latch.final_bits = lm.machine.bits();
                    lm.mode = Mode::Done;
                    lm.log_result();
                    continue;
                }
                lm.machine.set_temperature(lm.anneal.temperature(t));
            }
        }
        lm.machine.step().expect("sim step");
        if matches!(lm.mode, Mode::Cycle { .. }) && ((lm.machine.time() / dt).round() as u64).is_multiple_of(sample_steps) {
            lm.latch.observe(&lm.machine, &lm.tc.problem.qubo);
        }
    }

    // Smooth each strip's deflection into a 0..1 "trade is on" level.
    let k = 1.0 - (-8.0 * time.delta_secs()).exp();
    for i in 0..lm.n() {
        let x = lm.machine.positions()[i] as f32;
        let target = ((x + 0.6) / 1.2).clamp(0.0, 1.0);
        let target = target * target * (3.0 - 2.0 * target);
        lm.activity[i] += (target - lm.activity[i]) * k;
    }
}
