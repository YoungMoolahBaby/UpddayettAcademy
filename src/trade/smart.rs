//! The smart wash program (Step 4): a drum schedule that CortenForge's CEM
//! (sim-rl) learns on the real board.
//!
//! The fixed programs cool on a clock. This one reads the board as it
//! spins: every time unit it sees how far through the cycle it is and how
//! many strips sit on the barrier (mid-flip), and sets the drum's kT from
//! them. It starts as Normal's schedule and CEM bends it.
//!
//! Training wraps a night's board as an ml-chassis `VecEnv` (one copy of
//! the board per CEM candidate). The reward is what the i9's latch would
//! score: the best answer seen during the spin, against the best possible.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cortenforge::sim::core::{Data, Model};
use cortenforge::sim::ml_chassis::artifact::PolicyDescriptor;
use cortenforge::sim::ml_chassis::autograd::Activation;
use cortenforge::sim::ml_chassis::{
    ActionSpace, Algorithm, EpochMetrics, NetworkKind, ObservationSpace, Policy, TrainingBudget, VecEnv,
};
use cortenforge::sim::rl::{Cem, CemHyperparams};

use super::machine::{self, Physics};
use super::{Anneal, Error, Machine, TradeComputer};

/// How often the program reads the board and resets the drum (time units).
/// The i9 latches at the same rate.
pub const SAMPLE: f64 = 1.0;

/// Reward for landing the best set, on top of its value fraction (1 at best).
pub const HIT_BONUS: f64 = 1.0;

/// Cost of heat: the reward loses this times the mean drum setting. Zero:
/// at 0.02 it only broke ties, but on easy nights every candidate ties at
/// the best set, so it drove CEM to a colder drum that then missed on hard
/// nights (run 1, 2026-10-06).
pub const HEAT_COST: f64 = 0.0;

/// What the program reads: `[1, tau, tau^2, barrier]`, with tau the fraction
/// of the cool-down gone and barrier the fraction of strips mid-flip.
pub const N_FEATURES: usize = 4;

/// The learned program: CEM run 3 (2026-10-06), 66 generations x 32 spins
/// over the 11 hard nights 12..58, from Normal's schedule, run as
/// [`LEARNED_RESTARTS`] cool-downs. Each starts hotter than Normal (5.1x
/// kT) and cools faster while many strips are mid-flip. Held out: 88% vs
/// Normal's 81% on nights 1-10 (48 spins a night), 93% vs 83% on nights
/// 61-80 (24 spins, fresh seeds). `None` falls back to Normal.
pub const LEARNED: Option<[f64; N_FEATURES]> =
    Some([1.6260607631131936, -2.304621695618709, -0.08961597289308289, -0.41279624618638444]);

/// Cool-downs per cycle for [`LEARNED`]. Six of Normal's own shape alone
/// don't help on average (82% vs 81%); CEM's shape is what makes them pay.
pub const LEARNED_RESTARTS: usize = 6;

/// A drum program: log kT = params . features, held for [`SAMPLE`] time
/// units, cold for the settle once the cool-down is over. As an ml-chassis
/// `Policy` its observation is `[time, strip positions...]` and its action
/// the drum setting.
#[derive(Clone, Debug)]
pub struct SmartWash {
    /// Length of the cool-down (Normal's: 1000).
    pub duration: f64,
    /// How many cool-downs fit in `duration` (the shape repeats; the i9
    /// latch keeps the best of them). 1 = one long cool-down, like Normal.
    pub restarts: usize,
    params: Vec<f64>,
}

impl SmartWash {
    pub fn new(duration: f64, params: &[f64]) -> Self {
        assert_eq!(params.len(), N_FEATURES, "SmartWash takes {N_FEATURES} params");
        Self { duration, restarts: 1, params: params.to_vec() }
    }

    /// Normal's geometric schedule (4x down to 0.35x) in this form: CEM's
    /// starting point.
    pub fn normal(duration: f64) -> Self {
        let a = Anneal::default();
        Self::new(duration, &[a.hot.ln(), (a.cold / a.hot).ln(), 0.0, 0.0])
    }

    /// The trained program, or Normal's if there is none yet.
    pub fn learned(duration: f64) -> Self {
        LEARNED.map_or_else(|| Self::normal(duration), |p| Self::new(duration, &p).with_restarts(LEARNED_RESTARTS))
    }

    /// The same program, run as `k` shorter cool-downs back to back.
    pub fn with_restarts(mut self, k: usize) -> Self {
        self.restarts = k.max(1);
        self
    }

    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// The features at `t` time units in, with the strips at `x`.
    pub fn features(&self, t: f64, x: &[f64]) -> [f64; N_FEATURES] {
        // Where in the current cool-down: 0 at each reheat, 1 at its end.
        let tau = (t / self.duration * self.restarts as f64).fract();
        let barrier = x.iter().filter(|x| x.abs() < 0.5).count() as f64 / x.len().max(1) as f64;
        [1.0, tau, tau * tau, barrier]
    }

    /// Drum setting (a multiple of kT) at `t` with the strips at `x`.
    pub fn temperature(&self, t: f64, x: &[f64]) -> f64 {
        if t >= self.duration {
            return 0.0;
        }
        let f = self.features(t, x);
        let log_t: f64 = self.params.iter().zip(f).map(|(p, f)| p * f).sum();
        // The thermostat caps the multiplier at 10.
        log_t.clamp(-12.0, 10f64.ln()).exp()
    }

    /// Total spin time: the cool-down plus Normal's settle.
    pub fn total_time(&self) -> f64 {
        self.duration + Anneal::default().settle
    }

    /// The program as [`TradeComputer::spin_with`] takes it: it reads the
    /// board every [`SAMPLE`] units and holds the drum in between, as the
    /// trained env did.
    pub fn program(&self) -> impl FnMut(f64, &Machine) -> f64 + '_ {
        let mut next = 0.0;
        let mut held = 0.0;
        move |t, m| {
            if t >= next - 1e-9 {
                held = self.temperature(t, m.positions());
                next += SAMPLE;
            }
            held
        }
    }
}

impl Policy for SmartWash {
    fn n_params(&self) -> usize {
        N_FEATURES
    }

    fn params(&self) -> &[f64] {
        &self.params
    }

    fn set_params(&mut self, params: &[f64]) {
        self.params.copy_from_slice(params);
    }

    fn forward(&self, obs: &[f32]) -> Vec<f64> {
        let x: Vec<f64> = obs[1..].iter().map(|&x| f64::from(x)).collect();
        vec![self.temperature(f64::from(obs[0]), &x)]
    }

    /// ml-chassis has no kind for a hand-written policy; this one is
    /// linear in its features, so it says Linear (the artifact can't
    /// rebuild it, but CEM only reads the params).
    fn descriptor(&self) -> PolicyDescriptor {
        PolicyDescriptor {
            kind: NetworkKind::Linear,
            obs_dim: N_FEATURES,
            act_dim: 1,
            hidden_dims: vec![],
            activation: Activation::Tanh,
            obs_scale: vec![1.0; N_FEATURES],
            stochastic: false,
        }
    }
}

/// How good a set is, as the reward sees it: its value as a fraction of
/// the best set's (0 if it clashes or misses the want), plus
/// [`HIT_BONUS`] if it is a best set.
pub fn quality(tc: &TradeComputer, best: f64, bits: u32) -> f64 {
    let (v, clash) = tc.evaluate(bits);
    if clash || !tc.delivers_want(bits) {
        return 0.0;
    }
    let frac = if best > 0.0 { (v / best).max(0.0) } else { 1.0 };
    frac + if tc.is_optimal(bits) { HIT_BONUS } else { 0.0 }
}

/// One env's latch inside the reward: when it last read, and the best
/// quality latched this episode.
#[derive(Clone, Copy)]
struct Seen {
    time: f64,
    best: f64,
}

/// Tonight's board as an ml-chassis `VecEnv`, one copy per CEM candidate.
///
/// Each step is [`SAMPLE`] time units. The reward pays each latch
/// improvement, so an episode's rewards add up to the latched answer's
/// [`quality`], less [`HEAT_COST`] times the mean drum setting.
pub fn wash_env(tc: &TradeComputer, physics: Physics, program: &SmartWash, n_envs: usize, seed: u64) -> Result<VecEnv, Error> {
    let n = tc.ising.n;
    let model = Arc::new(machine::board_model(&tc.ising, physics, seed, None)?);
    let obs = ObservationSpace::builder().time().all_qpos().build(&model)?;
    let act = ActionSpace::builder().all_ctrl().build(&model)?;
    let total = program.total_time();
    let steps = total / SAMPLE;
    let best = tc.evaluate(tc.ground_state()).0;
    let board = tc.clone();
    // The reward is a plain Fn(&Model, &Data) shared by every env, with no
    // env index or episode state, so the latches live in a side table keyed
    // by each env's Data (they stay put inside the batch).
    let seen: Mutex<HashMap<usize, Seen>> = Mutex::new(HashMap::new());
    let reward = move |_: &Model, d: &Data| {
        let key = std::ptr::from_ref(d) as usize;
        let mut seen = seen.lock().expect("latch table");
        let s = seen.entry(key).or_insert(Seen { time: f64::INFINITY, best: 0.0 });
        if d.time < s.time {
            // A new episode.
            s.best = 0.0;
        }
        s.time = d.time;
        let mut r = -HEAT_COST * d.ctrl[0] / steps;
        if let Some(bits) = machine::read_bits(&d.qpos.as_slice()[..n]) {
            let q = quality(&board, best, bits);
            if q > s.best {
                r += q - s.best;
                s.best = q;
            }
        }
        r
    };
    let mut resets = seed;
    let env = VecEnv::builder(model, n_envs)
        .observation_space(obs)
        .action_space(act)
        .reward(reward)
        .done(|_, _| false)
        .truncated(move |_, d| d.time >= total - 0.5 * SAMPLE)
        .sub_steps((SAMPLE / physics.dt).round() as usize)
        .on_reset(move |_, d, i| {
            resets = resets.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1 + i as u64);
            machine::load_laundry(d, n, resets);
        })
        .build()?;
    Ok(env)
}

/// CEM settings for the smart wash. The params are log-temperature
/// weights, so a noise of 0.3 scales the drum by about 1.35x.
pub fn hyperparams(program: &SmartWash) -> CemHyperparams {
    CemHyperparams {
        elite_fraction: 0.25,
        noise_std: 0.3,
        noise_decay: 0.95,
        noise_min: 0.03,
        max_episode_steps: (program.total_time() / SAMPLE).ceil() as usize + 2,
    }
}

/// Trains the smart wash: each CEM generation runs on the next training
/// night in turn (`pop` candidates, one spin each), so the program can't
/// fit one board. `on_gen(gen, night, metrics)` reports progress.
pub fn train(
    nights: &[(u64, TradeComputer)],
    physics: Physics,
    start: &SmartWash,
    gens: usize,
    pop: usize,
    seed: u64,
    on_gen: impl Fn(usize, u64, &EpochMetrics, &[f64]),
) -> Result<SmartWash, Error> {
    let mut envs = nights
        .iter()
        .enumerate()
        .map(|(k, (_, tc))| wash_env(tc, physics, start, pop, seed + k as u64 * 1009))
        .collect::<Result<Vec<_>, _>>()?;
    let mut cem = Cem::new(Box::new(start.clone()), hyperparams(start));
    for g in 0..gens {
        let k = g % envs.len();
        let m = cem.train(&mut envs[k], TrainingBudget::Epochs(1), seed.wrapping_add(g as u64 * 7919), &|_| {});
        let params = cem.policy_artifact().params;
        on_gen(g, nights[k].0, &m[0], &params);
    }
    Ok(SmartWash::new(start.duration, &cem.policy_artifact().params).with_restarts(start.restarts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_as_normal() {
        let a = Anneal::default();
        let s = SmartWash::normal(a.duration);
        for t in [0.0, 250.0, 500.0, 999.0, 1000.0, 1010.0] {
            assert!((s.temperature(t, &[1.0, -1.0]) - a.temperature(t)).abs() < 1e-9, "t={t}");
        }
    }

    #[test]
    fn restarts_repeat_the_cool_down() {
        let s = SmartWash::learned(1000.0);
        let period = 1000.0 / s.restarts as f64;
        let x = [1.0, -1.0, 0.1];
        for t in [0.0, 30.0, 120.0] {
            for k in 1..s.restarts {
                let again = s.temperature(t + k as f64 * period, &x);
                assert!((again - s.temperature(t, &x)).abs() < 1e-9, "t={t}, cool-down {k}");
            }
        }
        // Each cool-down starts hot and ends cold; the settle is cold.
        assert!(s.temperature(0.0, &x) > 3.0 * s.temperature(period - 1.0, &x));
        assert_eq!(s.temperature(1000.0, &x), 0.0);
    }
}
