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
    /// Index into [`PROGRAMS`].
    pub program: usize,
    /// Watch-speed multiplier (1x plays a program in its `watch_secs`).
    pub watch: f64,
    pub seed: u64,
    /// The QUBO's best answer, for the scoreboard.
    pub ground: u32,
    /// Per-trade "on-ness" in 0..1, smoothed for display.
    pub activity: Vec<f32>,
    /// Per customer: the items some trade can deliver to them, with what
    /// granting that want costs the block (Goo).
    pub want_menu: Vec<Vec<(usize, f64)>>,
    /// Whose wants the picker is showing.
    pub picker_npc: usize,
}

/// A wash program: how long the drum takes to cool (the physics), and how
/// many real seconds the player spends watching it at 1x.
pub struct WashProgram {
    pub name: &'static str,
    /// Anneal length in sim time units.
    pub duration: f64,
    pub watch_secs: f64,
    /// How often the i9 finds the best set, measured with
    /// `trade_cli bench --runs 48 --time <duration> --night 1..=10` (480
    /// runs per program across 10 nights, 2026-10-05).
    pub i9_rate: &'static str,
}

pub const PROGRAMS: [WashProgram; 4] = [
    WashProgram { name: "Quick Wash", duration: 150.0, watch_secs: 12.0, i9_rate: "46%" },
    WashProgram { name: "Permanent Press", duration: 300.0, watch_secs: 16.0, i9_rate: "68%" },
    WashProgram { name: "Normal", duration: 1000.0, watch_secs: 24.0, i9_rate: "88%" },
    WashProgram { name: "Delicates", duration: 3000.0, watch_secs: 32.0, i9_rate: "99%" },
];

/// Watch speed for the manual dial at 1x (sim time units per real second).
const MANUAL_SPEED: f64 = 40.0;

/// How often the i9 reads the Hall sensors (sim time units).
const SAMPLE: f64 = 1.0;
/// Most steps per frame, so a slow frame can't snowball.
const MAX_STEPS_PER_FRAME: usize = 5_000;

impl Laundromat {
    pub fn new() -> Self {
        let physics = Physics::default();
        // A different night every launch; `UPD_SEED=<n>` replays one exactly.
        let seed = std::env::var("UPD_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() % 1_000_000)
        });
        let tc = TradeComputer::new(world::laundromat(seed), 5.0, 1.6);
        info!(
            "laundromat open: seed {seed} (UPD_SEED={seed} replays this session), {}, {} trades{}",
            tc.world.weather(),
            tc.cycles.len(),
            if tc.dropped > 0 { format!(" ({} more left off the board)", tc.dropped) } else { String::new() }
        );
        let machine = tc.machine(physics, seed).expect("build the slap-bit board");
        let ground = tc.ground_state();
        let n = tc.cycles.len();
        // Price every want once, up front, so the picker can show costs.
        let mut scratch = tc.clone();
        let want_menu = (0..tc.world.npcs.len())
            .map(|npc| {
                tc.deliverable(npc)
                    .into_iter()
                    .map(|item| {
                        scratch.want(npc, item);
                        (item, scratch.want_cost(scratch.ground_state()))
                    })
                    .collect()
            })
            .collect();
        Self {
            want_menu,
            picker_npc: 0,
            tc,
            machine,
            physics,
            anneal: Anneal::default(),
            latch: Latch::new(),
            mode: Mode::Manual,
            dial: 1.5,
            program: 2,
            watch: 1.0,
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

    /// Set or clear the want. That rewires the springs, and couplings can't
    /// change after `install`, so the board is rebuilt; the strips carry
    /// over so nothing jumps. The next Run spins with the want in place.
    pub fn set_want(&mut self, want: Option<(usize, usize)>) {
        if matches!(self.mode, Mode::Cycle { .. }) || want == self.tc.want {
            return;
        }
        match want {
            Some((npc, item)) => {
                if !self.tc.want(npc, item) {
                    return;
                }
            }
            None => self.tc.clear_want(),
        }
        let mut board = self.tc.machine(self.physics, self.seed).expect("build the slap-bit board");
        board.take_state_from(&self.machine).expect("carry the strips over");
        self.machine = board;
        self.ground = self.tc.ground_state();
        self.latch = Latch::new();
        self.mode = Mode::Manual;
        info!("want: {}", self.want_text().unwrap_or_else(|| "none (forward mode)".into()));
    }

    /// "Upddayett wants the 350 W scooter hub motor"
    pub fn want_text(&self) -> Option<String> {
        self.tc.want.map(|(npc, item)| format!("{} wants the {}", self.tc.world.npcs[npc].name, self.tc.world.items[item].name))
    }

    /// Does trade `c` move the wanted item to the person who wants it?
    pub fn carries_want(&self, c: usize) -> bool {
        self.tc.want.is_some_and(|(npc, item)| self.tc.cycles[c].delivers(npc, item))
    }

    /// Load fresh laundry and run the selected program. Every cycle starts
    /// from random strips, so it earns its answer.
    pub fn start_cycle(&mut self) {
        self.new_load();
        let p = &PROGRAMS[self.program];
        self.anneal.duration = p.duration;
        self.mode = Mode::Cycle { start: self.machine.time() };
        info!(
            "spin cycle: {} (kT x{} -> x{} over {} units; i9 finds the best set {} of the time), watching at {:.0} units/s",
            p.name,
            self.anneal.hot,
            self.anneal.cold,
            p.duration,
            p.i9_rate,
            self.speed()
        );
        if let Some(w) = self.want_text() {
            info!("  with want: {w}");
        }
    }

    /// Sim time units per real second right now.
    pub fn speed(&self) -> f64 {
        let base = match self.mode {
            Mode::Manual => MANUAL_SPEED,
            Mode::Cycle { .. } | Mode::Done => {
                let p = &PROGRAMS[self.program];
                (p.duration + self.anneal.settle) / p.watch_secs
            }
        };
        base * self.watch
    }

    /// Log what the drum settled on and what the i9 latched.
    fn log_result(&self) {
        let n = self.n();
        let mask = |b: u32| (0..n).map(|i| if (b >> i) & 1 == 1 { '#' } else { '.' }).collect::<String>();
        let (rest, rest_clash) = self.tc.evaluate(self.latch.final_bits);
        let (best, _) = self.tc.evaluate(self.latch.best_bits);
        let (ground, _) = self.tc.evaluate(self.ground);
        info!(
            "{} done: at rest {} = {rest:.0} Goo{} | i9 latched {} = {best:.0} Goo at t={:.0} | best possible {} = {ground:.0} Goo -> {}",
            PROGRAMS[self.program].name,
            mask(self.latch.final_bits),
            if rest_clash { " (clash)" } else { "" },
            mask(self.latch.best_bits),
            self.latch.best_time,
            mask(self.ground),
            if self.tc.is_optimal(self.latch.best_bits) { "BEST" } else { "missed" }
        );
        if let Some(w) = self.want_text() {
            let got = self.tc.delivers_want(self.latch.best_bits);
            info!(
                "  want ({w}): {}, cost the block {:.0} Goo",
                if got { "delivered" } else { "NOT delivered" },
                self.tc.want_cost(self.latch.best_bits)
            );
        }
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
    let steps = ((lm.speed() * real / dt).round() as usize).min(MAX_STEPS_PER_FRAME);
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
