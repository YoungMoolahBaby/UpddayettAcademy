//! The smart wash program (Step 4): a drum schedule that CortenForge's CEM
//! (sim-rl) learns on the real board.
//!
//! The fixed programs cool on a clock. This one reads the board as it
//! spins: every time unit it sees how far through the cool-down it is, how
//! many strips sit on the barrier (mid-flip) and how long since the i9's
//! latch last found a better set, and sets the drum's kT from them. It
//! remembers what it saw: once the strips have read the same set for a
//! while, the cool-down has nothing more to give, so it reheats (the latch
//! keeps the best). It starts as Normal's schedule and CEM bends it.
//!
//! Training wraps a night's board as an ml-chassis `VecEnv` (one copy of
//! the board per CEM candidate). The reward is what the i9's latch would
//! score: the best answer seen during the spin, against the best possible.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, RwLock};

use cortenforge::sim::core::{Data, Model};
use cortenforge::sim::ml_chassis::artifact::PolicyDescriptor;
use cortenforge::sim::ml_chassis::autograd::Activation;
use cortenforge::sim::ml_chassis::{
    ActionSpace, Algorithm, EpochMetrics, NetworkKind, ObservationSpace, Policy, TrainingBudget, VecEnv,
};
use cortenforge::sim::rl::{Cem, CemHyperparams};

use super::machine::{self, Physics};
use super::{Anneal, Error, Machine, Qubo, TradeComputer};

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

/// What the program reads: `[1, tau, tau^2, barrier, stall]`, with tau the
/// fraction of the cool-down gone, barrier the fraction of strips mid-flip,
/// and stall the time since the latch last improved, in cool-downs.
pub const N_FEATURES: usize = 5;

/// The program's params: a weight per feature (log kT is their sum), then
/// ln(cool-down / duration) and ln(patience / cool-down).
pub const N_PARAMS: usize = N_FEATURES + 2;

/// ln(patience / cool-down) that never fires: the clock alone reheats.
pub const NO_PATIENCE: f64 = 6.907_755_278_982_137; // ln 1000

/// The learned program: CEM run 5 (2026-10-06), 66 generations x 32 spins
/// over the 11 hard nights 12..58, from run 3's shape at three cool-downs
/// and a patience of 0.08. It settled on 2.7 cool-downs on the clock, a
/// reheat once the strips sit still for 38 units, and a drum a little
/// cooler the longer the latch goes without a better set. Each cool-down
/// starts at 5.9x kT. Held out (48 spins a night): 88% vs Normal's 81% on
/// nights 1-10, 95% vs 83% on fresh nights 61-80, 88% vs 63% on the six
/// hardest of those. `None` falls back to Normal.
pub const LEARNED: Option<[f64; N_PARAMS]> = Some([
    1.7711103739966765,
    -2.2574816421985116,
    0.016471262742058478,
    -0.321735857911866,
    -0.1738937841145411,
    -0.9917562019650146,
    -2.2703474031096733,
]);

/// Run 3 (2026-10-06), the clock-only program before the memory: six
/// cool-downs, no stall weight, no patience. Ties run 5 on average (95% on
/// nights 61-80) and does a little worse on the hardest nights (85%).
pub const RUN_3: [f64; N_PARAMS] = [
    1.6260607631131936,
    -2.304621695618709,
    -0.08961597289308289,
    -0.41279624618638444,
    0.0,
    -1.791_759_469_228_055, // ln(1/6): six cool-downs
    NO_PATIENCE,
];

/// A drum program: log kT = weights . features, read every [`SAMPLE`]
/// time units and held in between, cold for the settle once the cycle is
/// over. A new cool-down starts when the clock runs one out, or when the
/// strips have read the same set for the patience.
#[derive(Clone, Debug)]
pub struct SmartWash {
    /// Length of the cycle before the settle (Normal's: 1000).
    pub duration: f64,
    params: Vec<f64>,
}

impl SmartWash {
    /// A program from its params. Short lists fill in from the end of
    /// [`N_PARAMS`]: no stall weight, one cool-down, no patience (so the
    /// four shape weights alone give a clock-only program, as in run 3).
    pub fn new(duration: f64, params: &[f64]) -> Self {
        assert!((4..=N_PARAMS).contains(&params.len()), "SmartWash takes 4 to {N_PARAMS} params");
        let mut p = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, NO_PATIENCE];
        p[..params.len()].copy_from_slice(params);
        Self { duration, params: p }
    }

    /// Normal's geometric schedule (4x down to 0.35x) in this form.
    pub fn normal(duration: f64) -> Self {
        let a = Anneal::default();
        Self::new(duration, &[a.hot.ln(), (a.cold / a.hot).ln(), 0.0, 0.0])
    }

    /// The trained program, or Normal's if there is none yet.
    pub fn learned(duration: f64) -> Self {
        LEARNED.map_or_else(|| Self::normal(duration), |p| Self::new(duration, &p))
    }

    /// The same program with `k` cool-downs on the clock.
    pub fn with_restarts(mut self, k: f64) -> Self {
        self.params[N_FEATURES] = -k.max(1.0).ln();
        self
    }

    /// The same program, reheating once the strips have read the same set
    /// for `frac` of a cool-down (infinite: never).
    pub fn with_patience(mut self, frac: f64) -> Self {
        self.params[N_FEATURES + 1] = if frac.is_finite() { frac.ln() } else { NO_PATIENCE };
        self
    }

    pub fn params(&self) -> &[f64] {
        &self.params
    }

    /// One cool-down on the clock (time units): at most the whole cycle,
    /// at least 20 units.
    pub fn period(&self) -> f64 {
        (self.duration * self.params[N_FEATURES].exp()).clamp(20.0, self.duration)
    }

    /// Cool-downs on the clock per cycle.
    pub fn restarts(&self) -> f64 {
        self.duration / self.period()
    }

    /// How long the strips must read the same set before the program
    /// reheats (time units; at least two reads).
    pub fn patience(&self) -> f64 {
        (self.period() * self.params[N_FEATURES + 1].exp()).max(2.0 * SAMPLE)
    }

    /// The features: `tau` through the cool-down, the strips at `x`, and
    /// the `stall` (cool-downs since the latch improved).
    pub fn features(&self, tau: f64, x: &[f64], stall: f64) -> [f64; N_FEATURES] {
        let barrier = x.iter().filter(|x| x.abs() < 0.5).count() as f64 / x.len().max(1) as f64;
        [1.0, tau, tau * tau, barrier, stall]
    }

    /// The drum setting (a multiple of kT) for these features.
    pub fn setting(&self, f: &[f64; N_FEATURES]) -> f64 {
        let log_t: f64 = self.params.iter().zip(f).map(|(p, f)| p * f).sum();
        // The thermostat caps the multiplier at 10.
        log_t.clamp(-12.0, 10f64.ln()).exp()
    }

    /// One read of the board at `t` time units into the cycle, with the
    /// strips at `x`: updates what the program remembers and returns the
    /// drum setting to hold until the next read. `qubo` scores latched
    /// sets the way the i9 does.
    pub fn read(&self, mem: &mut Memory, t: f64, x: &[f64], qubo: &Qubo) -> f64 {
        if t < mem.last {
            // A new cycle.
            *mem = Memory::default();
        }
        mem.last = t;
        if t >= self.duration {
            return 0.0;
        }
        // The latch: a better set read with every strip in a well.
        let bits = machine::read_bits(x);
        if let Some(b) = bits {
            let e = qubo.energy(b);
            if e < mem.best - 1e-12 {
                mem.best = e;
                mem.improved = t;
            }
        }
        if bits.is_none() || bits != mem.still {
            mem.still = bits;
            mem.still_since = t;
        }
        let period = self.period();
        while t - mem.reheat >= period - 1e-9 {
            mem.reheat += period;
            mem.reheats += 1;
        }
        // Frozen: this cool-down has nothing more to give.
        if bits.is_some() && t - mem.still_since >= self.patience() - 1e-9 {
            mem.reheat = t;
            mem.still_since = t;
            mem.reheats += 1;
            mem.frozen += 1;
        }
        let tau = (t - mem.reheat) / period;
        let stall = (t - mem.improved) / period;
        self.setting(&self.features(tau, x, stall))
    }

    /// Total spin time: the cycle plus Normal's settle.
    pub fn total_time(&self) -> f64 {
        self.duration + Anneal::default().settle
    }

    /// The program as [`TradeComputer::spin_with`] takes it: it reads the
    /// board every [`SAMPLE`] units and holds the drum in between, as the
    /// trained env did.
    pub fn program<'a>(&'a self, qubo: &'a Qubo) -> impl FnMut(f64, &Machine) -> f64 + 'a {
        let mut mem = Memory::default();
        let mut next = 0.0;
        let mut held = 0.0;
        move |t, m| {
            if t >= next - 1e-9 {
                held = self.read(&mut mem, t, m.positions(), qubo);
                next += SAMPLE;
            }
            held
        }
    }
}

/// What the smart drum remembers between reads during one cycle.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    /// The last read's time (a read earlier than this starts a new cycle).
    last: f64,
    /// When the current cool-down started.
    reheat: f64,
    /// The latch: the best set's energy so far, and when it was read.
    best: f64,
    improved: f64,
    /// The set the strips last read (None: some strip mid-flip), and since when.
    still: Option<u32>,
    still_since: f64,
    /// Cool-downs started after the first, and how many of those because
    /// the strips froze.
    pub reheats: usize,
    pub frozen: usize,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            last: f64::NEG_INFINITY,
            reheat: 0.0,
            best: f64::INFINITY,
            improved: 0.0,
            still: None,
            still_since: 0.0,
            reheats: 0,
            frozen: 0,
        }
    }
}

/// The program as CEM trains it: an ml-chassis `Policy` whose observation
/// is `[time, strip positions...]` and whose action is the drum setting.
///
/// `Policy::forward` gets neither the env index nor any state, so the
/// memories live in a table keyed by the candidate's params, which CEM
/// sets just before every forward (each candidate runs on its own env).
struct Trainee {
    wash: SmartWash,
    qubo: Arc<RwLock<Qubo>>,
    memories: Arc<Mutex<HashMap<u64, Memory>>>,
}

fn params_key(p: &[f64]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for x in p {
        x.to_bits().hash(&mut h);
    }
    h.finish()
}

impl Policy for Trainee {
    fn n_params(&self) -> usize {
        N_PARAMS
    }

    fn params(&self) -> &[f64] {
        &self.wash.params
    }

    fn set_params(&mut self, params: &[f64]) {
        self.wash.params.copy_from_slice(params);
    }

    fn forward(&self, obs: &[f32]) -> Vec<f64> {
        let x: Vec<f64> = obs[1..].iter().map(|&x| f64::from(x)).collect();
        let mut memories = self.memories.lock().expect("memories");
        let mem = memories.entry(params_key(&self.wash.params)).or_default();
        let qubo = self.qubo.read().expect("board");
        vec![self.wash.read(mem, f64::from(obs[0]), &x, &qubo)]
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

/// CEM settings for the smart wash. The params are log-scale (log kT
/// weights, log cool-down, log patience), so a noise of 0.3 scales by
/// about 1.35x.
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
/// fit one board. `on_gen(gen, night, metrics, params)` reports progress.
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
    let qubo = Arc::new(RwLock::new(nights[0].1.problem.qubo.clone()));
    let memories = Arc::new(Mutex::new(HashMap::new()));
    let trainee = Trainee { wash: start.clone(), qubo: qubo.clone(), memories: memories.clone() };
    let mut cem = Cem::new(Box::new(trainee), hyperparams(start));
    for g in 0..gens {
        let k = g % envs.len();
        *qubo.write().expect("board") = nights[k].1.problem.qubo.clone();
        memories.lock().expect("memories").clear();
        let m = cem.train(&mut envs[k], TrainingBudget::Epochs(1), seed.wrapping_add(g as u64 * 7919), &|_| {});
        let params = cem.policy_artifact().params;
        on_gen(g, nights[k].0, &m[0], &params);
    }
    Ok(SmartWash::new(start.duration, &cem.policy_artifact().params))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `s` over the cycle like the game does (one read per time unit)
    /// with the strips fixed at `x`; returns the setting at each read.
    fn run(s: &SmartWash, x: &[f64], q: &Qubo) -> Vec<f64> {
        let mut mem = Memory::default();
        (0..=(s.duration as usize + 10)).map(|t| s.read(&mut mem, t as f64, x, q)).collect()
    }

    fn board(n: usize) -> Qubo {
        Qubo::new(n)
    }

    #[test]
    fn starts_as_normal() {
        let a = Anneal::default();
        let s = SmartWash::normal(a.duration);
        // A strip mid-flip: never frozen, so only the clock matters.
        let kt = run(&s, &[1.0, 0.1], &board(2));
        for t in [0, 250, 500, 999, 1000, 1010] {
            assert!((kt[t] - a.temperature(t as f64)).abs() < 1e-9, "t={t}");
        }
    }

    #[test]
    fn restarts_repeat_the_cool_down() {
        let s = SmartWash::new(1000.0, &RUN_3);
        let x = [1.0, -1.0, 0.1];
        let kt = run(&s, &x, &board(3));
        let period = s.period();
        assert!((s.restarts() - 6.0).abs() < 1e-9);
        for t in [0.0, 30.0, 120.0] {
            for k in 1..6 {
                let again = (t + k as f64 * period).ceil();
                let tau = (again - k as f64 * period) / period;
                let want = s.setting(&s.features(tau, &x, 0.0));
                assert!((kt[again as usize] - want).abs() < 1e-9, "t={t}, cool-down {k}");
            }
        }
        // Each cool-down starts hot and ends cold; the settle is cold.
        assert!(kt[0] > 3.0 * kt[period as usize - 1]);
        assert_eq!(kt[1000], 0.0);
    }

    #[test]
    fn learned_has_memory() {
        let s = SmartWash::learned(1000.0);
        assert!(s.patience() < s.period(), "the learned program reheats when the strips freeze");
        assert!(s.restarts() > 1.0);
    }

    #[test]
    fn frozen_strips_reheat() {
        // One cool-down on the clock, patience 50 units: strips that sit
        // in their wells the whole time freeze and reheat every 50.
        let s = SmartWash::normal(1000.0).with_patience(0.05);
        let x = [1.0, -1.0];
        let q = board(2);
        let kt = run(&s, &x, &q);
        assert!((s.patience() - 50.0).abs() < 1e-9);
        assert!((kt[50] - kt[0]).abs() < 1e-9, "reheated at 50");
        assert!(kt[49] < kt[0]);
        let mut mem = Memory::default();
        for t in 0..1000 {
            s.read(&mut mem, t as f64, &x, &q);
        }
        assert_eq!(mem.frozen, 19);
        // A strip mid-flip never freezes.
        let kt = run(&s, &[1.0, 0.0], &q);
        assert!(kt[50] < kt[0]);
    }
}
