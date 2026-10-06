//! Gap probe for `sim-thermostat` 0.9.x: one check per finding, so the same
//! run tells us what 0.9.2 fixed.
//!
//! cargo run --release --example gaps_thermostat
//!
//! Each check prints OPEN (the gap is still there), FIXED (a known gap that
//! now behaves) or ok (never a gap; kept to catch regressions), with what
//! it saw. A panic inside a check is caught and reported, never fatal. The ids
//! match the entries in docs/FINDINGS.md.

use std::panic::{AssertUnwindSafe, catch_unwind};

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::therm_env::generate_mjcf;
use cortenforge::sim::thermostat::{
    Baoab1D, ColoredDriveSim, DoubleWellPotential, ExternalField, GibbsSampler, IsingLearner, IsingTarget, LangevinThermostat,
    LearnerConfig, PairwiseCoupling, PassiveComponent, PassiveStack, WellState, ising,
};

/// A named check.
type Check = (&'static str, fn() -> Seen);

/// What a check found.
enum Seen {
    /// The gap is still there.
    Open(String),
    /// A known gap that now behaves as it should.
    Fixed(String),
    /// Never was a gap: kept so a regression shows up.
    Fine(String),
}

/// A bare thermostat-ready model with `n` slide joints and one ctrl slot.
fn model(n: usize) -> (Model, Data) {
    let model = load_model(&generate_mjcf(n, 1, 0.01, (0.0, 10.0))).expect("load the generated model");
    let data = model.make_data();
    (model, data)
}

/// Runs `f`, turning a panic into its message.
fn run<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panic".into())
    })
}

/// `exact_distribution` with no log-sum-exp shift: 20 spins at low
/// temperature overflow `exp` and every probability comes back NaN.
fn exact_distribution_overflows() -> Seen {
    let n = 20;
    let edges: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
    let dist = ising::exact_distribution(n, &edges, &vec![5.0; n - 1], &vec![1.0; n], 0.1);
    let bad = dist.iter().filter(|(_, p)| !p.is_finite()).count();
    let total: f64 = dist.iter().map(|(_, p)| p).sum();
    if bad > 0 {
        Seen::Open(format!("{bad} of {} probabilities are NaN or infinite", dist.len()))
    } else {
        Seen::Fixed(format!("all finite, sum {total:.6}"))
    }
}

/// `ExternalField::new` takes any length; a field shorter than the model
/// panics mid-step, a longer one is silently ignored.
fn external_field_length_unchecked() -> Seen {
    let short = run(|| {
        let (mut m, mut d) = model(3);
        PassiveStack::builder().with(ExternalField::new(vec![1.0; 2])).build().install(&mut m);
        d.step(&m).map(|_| ())
    });
    let long = run(|| {
        let (mut m, mut d) = model(3);
        PassiveStack::builder().with(ExternalField::new(vec![1.0; 4])).build().install(&mut m);
        d.step(&m).map(|_| ())
    });
    let say = |r: &Result<Result<(), _>, String>| match r {
        Err(p) => format!("panics ({})", p.lines().next().unwrap_or("")),
        Ok(Err(e)) => format!("errors ({e})"),
        Ok(Ok(())) => "runs silently".into(),
    };
    let text = format!("2 entries for 3 joints: {}; 4 entries: {}", say(&short), say(&long));
    // Fixed means the mismatch is caught up front (at construction or
    // install), not mid-step or never.
    let caught_early = matches!(short, Err(ref p) if !p.contains("index out of bounds")) && !matches!(long, Ok(Ok(())));
    if caught_early { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// The exact solver and the Gibbs sampler stop at 20 spins (u32 masks,
/// 2^20 states), by panic.
fn exact_solver_caps_at_20() -> Seen {
    let r = run(|| ising::exact_distribution(21, &[], &[], &[0.0; 21], 1.0).len());
    match r {
        Err(p) => Seen::Open(format!("21 spins: panics ({p})")),
        Ok(len) => Seen::Fixed(format!("21 spins: {len} states")),
    }
}

/// Install one stack, then a second on the same model: does the second add
/// to the first, replace it, or refuse? (An end user splitting "physics" and
/// "sabotage" into two stacks would expect them to add.)
fn second_install_replaces_first() -> Seen {
    let (mut m, mut d) = model(1);
    PassiveStack::builder().with(ExternalField::new(vec![1.0])).build().install(&mut m);
    PassiveStack::builder().with(ExternalField::new(vec![2.0])).build().install(&mut m);
    d.qpos[0] = 0.0;
    d.qvel[0] = 0.0;
    d.forward(&m).expect("forward");
    let f = d.qfrc_passive[0];
    let text = format!("fields 1 then 2 on one joint: net passive force {f:.3}");
    if (f - 2.0).abs() < 1e-9 { Seen::Open(format!("{text}: the second stack silently replaced the first")) } else { Seen::Fixed(text) }
}

/// A ctrl temperature below zero (a dial that overshoots) should be refused
/// or clamped, not turned into NaN noise.
fn negative_ctrl_temperature() -> Seen {
    let r = run(|| {
        let (mut m, mut d) = model(1);
        let lt = LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 7, 0).with_ctrl_temperature(0);
        PassiveStack::builder().with(lt).build().install(&mut m);
        d.ctrl[0] = -1.0;
        for _ in 0..100 {
            d.step(&m).map_err(|e| e.to_string())?;
        }
        Ok::<f64, String>(d.qpos[0])
    });
    match r {
        Ok(Ok(x)) if x.is_finite() => Seen::Fine(format!("ctrl -1: position stays finite ({x:.3})")),
        Ok(Ok(x)) => Seen::Open(format!("ctrl -1: position becomes {x} with no error")),
        Ok(Err(e)) => Seen::Fine(format!("ctrl -1: step errors ({e})")),
        Err(p) => Seen::Open(format!("ctrl -1: panics mid-step ({p})")),
    }
}

/// Same seed, same trajectory, even when boards run on different threads
/// (the bench relies on it); a different traj_id must differ.
fn seeds_replay_across_threads() -> Seen {
    fn trace(seed: u64, traj: u64) -> Vec<f64> {
        let (mut m, mut d) = model(2);
        let lt = LangevinThermostat::new(DVector::from_element(2, 1.0), 1.0, seed, traj);
        PassiveStack::builder().with(lt).build().install(&mut m);
        (0..500).map(|_| {
            d.step(&m).unwrap();
            d.qpos[0]
        })
        .collect()
    }
    let here = trace(42, 0);
    let there = std::thread::spawn(|| trace(42, 0)).join().unwrap();
    let other = trace(42, 1);
    match (here == there, here != other) {
        (true, true) => Seen::Fine("same seed replays bit for bit across threads; traj_id 1 differs".into()),
        (same, differs) => Seen::Open(format!("same seed across threads identical: {same}; traj_id changes the noise: {differs}")),
    }
}

/// Components that name a joint the model doesn't have: caught at install,
/// or a panic mid-step?
fn dof_out_of_range() -> Seen {
    let well = run(|| {
        let (mut m, mut d) = model(2);
        PassiveStack::builder().with(DoubleWellPotential::new(1.0, 1.0, 5)).build().install(&mut m);
        d.step(&m).map(|_| ()).map_err(|e| e.to_string())
    });
    let spring = run(|| {
        let (mut m, mut d) = model(2);
        PassiveStack::builder().with(PairwiseCoupling::new(vec![1.0], vec![(0, 7)])).build().install(&mut m);
        d.step(&m).map(|_| ()).map_err(|e| e.to_string())
    });
    let say = |r: &Result<Result<(), String>, String>| match r {
        Err(p) => format!("panics on the first step ({})", p.lines().next().unwrap_or("")),
        Ok(Err(e)) => format!("step errors ({e})"),
        Ok(Ok(())) => "runs silently".into(),
    };
    let text = format!("double well on joint 5 of 2: {}; spring to joint 7 of 2: {}", say(&well), say(&spring));
    if matches!(well, Ok(Err(_))) && matches!(spring, Ok(Err(_))) { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// The thermostat's raw noise for `n` joints over `calls` calls to
/// `apply`, with the joints at rest (so the force is noise only).
fn noise(thermostat: &LangevinThermostat, n: usize, calls: usize) -> Vec<Vec<f64>> {
    let (m, d) = model(n);
    (0..calls)
        .map(|_| {
            let mut out = DVector::zeros(n);
            thermostat.apply(&m, &d, &mut out);
            out.iter().copied().collect()
        })
        .collect()
}

/// Noise comes in groups of 8 joints; group 1 at step s must not reuse
/// group 0's noise from step s+1 (our boards have 17-20 strips).
fn noise_reused_across_groups_of_8() -> Seen {
    let t = LangevinThermostat::new(DVector::from_element(9, 1.0), 1.0, 42, 0);
    let w = noise(&t, 9, 3);
    let same = w[0][8] == w[1][0] && w[1][8] == w[2][0];
    let text = format!("joint 8 at step 0 = {:.6}, joint 0 at step 1 = {:.6}", w[0][8], w[1][0]);
    if same { Seen::Open(format!("{text}: the same draw")) } else { Seen::Fixed(text) }
}

/// The docs promise any distinct u64 traj_id gives its own stream; the
/// high 32 bits must count.
fn traj_id_high_bits_ignored() -> Seen {
    let a = noise(&LangevinThermostat::new(DVector::from_element(2, 1.0), 1.0, 42, 0), 2, 5);
    let b = noise(&LangevinThermostat::new(DVector::from_element(2, 1.0), 1.0, 42, 1 << 32), 2, 5);
    if a == b {
        Seen::Open("traj_id 0 and 1<<32 give identical noise".into())
    } else {
        Seen::Fixed("traj_id 0 and 1<<32 differ".into())
    }
}

/// The noise is indexed by calls to `apply`, which live in the model's
/// stack: a fresh `Data` (or `Data::reset`) doesn't replay an episode.
fn reset_does_not_replay_noise() -> Seen {
    let (mut m, _) = model(1);
    PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 42, 0)).build().install(&mut m);
    let episode = |d: &mut Data| -> Vec<f64> {
        (0..100)
            .map(|_| {
                d.step(&m).unwrap();
                d.qpos[0]
            })
            .collect()
    };
    let mut d = m.make_data();
    let first = episode(&mut d);
    d.reset(&m);
    let after_reset = episode(&mut d);
    let fresh = episode(&mut m.make_data());
    match (first == after_reset, first == fresh) {
        (true, true) => Seen::Fixed("reset and a fresh Data both replay the episode".into()),
        (r, f) => Seen::Open(format!("same episode after Data::reset: {r}; with a fresh Data on the same model: {f}; no API to rewind the noise")),
    }
}

/// A cloned `Model` shares the installed stack (and its noise counter), so
/// "independent" copies draw from one stream.
fn cloned_model_shares_the_noise() -> Seen {
    let (mut m, _) = model(1);
    PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 42, 0)).build().install(&mut m);
    let copy = m.clone();
    let run = |m: &Model| -> Vec<f64> {
        let mut d = m.make_data();
        (0..50)
            .map(|_| {
                d.step(m).unwrap();
                d.qpos[0]
            })
            .collect()
    };
    let (a, b) = (run(&m), run(&copy));
    if a == b {
        Seen::Fixed("a cloned model replays the original's noise".into())
    } else {
        Seen::Open("a cloned model continues the original's noise stream (one shared counter), so clones aren't independent or replayable".into())
    }
}

/// The noise variance assumes one `apply` per step. RK4 calls forward 4
/// times per step; equipartition says <v^2> = kT/M = 1.
fn rk4_shrinks_the_noise() -> Seen {
    let v2 = |integrator: &str| -> f64 {
        let xml = generate_mjcf(1, 1, 0.01, (0.0, 10.0)).replace("integrator=\"Euler\"", &format!("integrator=\"{integrator}\""));
        let mut m = load_model(&xml).unwrap();
        PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 7, 0)).build().install(&mut m);
        let mut d = m.make_data();
        let (mut sum, mut count) = (0.0, 0);
        for k in 0..200_000 {
            d.step(&m).unwrap();
            if k > 2_000 {
                sum += d.qvel[0] * d.qvel[0];
                count += 1;
            }
        }
        sum / count as f64
    };
    let (euler, rk4) = (v2("Euler"), v2("RK4"));
    let text = format!("<v^2> (want 1.0): Euler {euler:.3}, RK4 {rk4:.3}");
    if (rk4 - 1.0).abs() > 0.15 { Seen::Open(format!("{text}: wrong temperature under RK4, silently")) } else { Seen::Fixed(text) }
}

/// A thermostat or ratchet reading `ctrl` on a model with no actuators
/// panics mid-step; a NaN ctrl becomes NaN motion.
fn ctrl_temperature_without_actuators() -> Seen {
    let no_slot = run(|| {
        let mut m = load_model(&generate_mjcf(1, 0, 0.01, (0.0, 10.0))).unwrap();
        PassiveStack::builder()
            .with(LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 7, 0).with_ctrl_temperature(0))
            .build()
            .install(&mut m);
        let mut d = m.make_data();
        d.step(&m).map_err(|e| e.to_string())
    });
    let nan = run(|| {
        let (mut m, mut d) = model(1);
        PassiveStack::builder()
            .with(LangevinThermostat::new(DVector::from_element(1, 1.0), 1.0, 7, 0).with_ctrl_temperature(0))
            .build()
            .install(&mut m);
        d.ctrl[0] = f64::NAN;
        for _ in 0..10 {
            d.step(&m).map_err(|e| e.to_string())?;
        }
        Ok::<f64, String>(d.qpos[0])
    });
    let a = match &no_slot {
        Err(p) => format!("panics on the first step ({})", p.lines().next().unwrap_or("")),
        Ok(Err(e)) => format!("step errors ({e})"),
        Ok(Ok(_)) => "runs".into(),
    };
    let b = match &nan {
        Ok(Ok(x)) => format!("position {x}"),
        Ok(Err(e)) => format!("step errors ({e})"),
        Err(p) => format!("panics ({p})"),
    };
    let text = format!("ctrl temperature with no ctrl slot: {a}; ctrl = NaN: {b}");
    let good = matches!(no_slot, Ok(Err(_))) && !matches!(nan, Ok(Ok(x)) if x.is_nan());
    if good { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// `LangevinThermostat::new` takes a negative damping and a gamma vector
/// of the wrong length.
fn thermostat_params_unchecked() -> Seen {
    let negative = run(|| {
        let (mut m, mut d) = model(1);
        PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(1, -0.1), 1.0, 7, 0)).build().install(&mut m);
        for _ in 0..50 {
            d.step(&m).map_err(|e| e.to_string())?;
        }
        Ok::<f64, String>(d.qpos[0])
    });
    let short = run(|| {
        let (mut m, mut d) = model(3);
        PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(2, 1.0), 1.0, 7, 0)).build().install(&mut m);
        for _ in 0..2000 {
            d.step(&m).map_err(|e| e.to_string())?;
        }
        Ok::<f64, String>(d.qvel[2])
    });
    let a = match &negative {
        Err(p) => format!("panics ({})", p.lines().next().unwrap_or("")),
        Ok(Ok(x)) => format!("accepted, position {x}"),
        Ok(Err(e)) => format!("step errors ({e})"),
    };
    let b = match &short {
        Ok(Ok(v)) if *v == 0.0 => "2 dampings for 3 joints: accepted, joint 2 never feels the bath".to_string(),
        Ok(Ok(v)) => format!("2 dampings for 3 joints: accepted, joint 2 velocity {v:.3}"),
        Ok(Err(e)) => format!("2 dampings for 3 joints: step errors ({e})"),
        Err(p) => format!("2 dampings for 3 joints: panics ({})", p.lines().next().unwrap_or("")),
    };
    let text = format!("gamma = -0.1: {a}; {b}");
    let refused = |r: &Result<Result<f64, String>, String>| !matches!(r, Ok(Ok(_)));
    if refused(&negative) && refused(&short) { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// A reversed duplicate edge silently doubles the coupling.
fn duplicate_edges_double_j() -> Seen {
    let (m, mut d) = model(2);
    let c = PairwiseCoupling::new(vec![0.5, 0.5], vec![(0, 1), (1, 0)]);
    d.qpos[1] = 1.0;
    let mut out = DVector::zeros(2);
    c.apply(&m, &d, &mut out);
    let text = format!("J 0.5 on (0,1) and again on (1,0): force on joint 0 is {:.2}", out[0]);
    if (out[0] - 1.0).abs() < 1e-12 { Seen::Open(format!("{text}, doubled silently")) } else { Seen::Fixed(text) }
}

/// "Hysteresis-based" classifier that's stateless, and a negative
/// threshold that makes the barrier vanish.
fn well_state_threshold_unchecked() -> Seen {
    let at_zero = WellState::from_position(0.0, -0.1);
    let text = format!("from_position(0.0, -0.1) = {at_zero:?}");
    if at_zero == WellState::Barrier { Seen::Fixed(text) } else { Seen::Open(format!("{text}: the top of the barrier reads as a well")) }
}

/// Explicit damping blows up when gamma * dt / M passes ~2, with no
/// stability note in the docs.
fn stiff_damping_diverges() -> Seen {
    let xml = generate_mjcf(1, 1, 0.001, (0.0, 10.0));
    let mut m = load_model(&xml).unwrap();
    PassiveStack::builder().with(LangevinThermostat::new(DVector::from_element(1, 3000.0), 1.0, 7, 0)).build().install(&mut m);
    let mut d = m.make_data();
    d.qvel[0] = 1.0;
    for _ in 0..200 {
        if d.step(&m).is_err() {
            break;
        }
    }
    let v = d.qvel[0];
    let text = format!("gamma 3000, dt 0.001 (gamma*dt = 3): velocity after 200 steps {v:.3e}");
    if !v.is_finite() || v.abs() > 1e3 { Seen::Open(format!("{text}: unstable, with no warning")) } else { Seen::Fixed(text) }
}

/// `depopulation_factor` loses precision at small energy loss: it should
/// approach delta, not 0.
fn depopulation_factor_underflows() -> Seen {
    let w = DoubleWellPotential::new(3.0, 1.0, 0);
    let gamma = 1e-17;
    let ups = w.depopulation_factor(gamma, 1.0, 1.0);
    let delta = gamma * w.barrier_action(1.0) / 1.0;
    let text = format!("gamma 1e-17: depopulation {ups:.3e}, delta {delta:.3e}");
    if ups == 0.0 || (ups / delta - 1.0).abs() > 0.5 { Seen::Open(format!("{text}: rate collapses to 0")) } else { Seen::Fixed(text) }
}

/// Components index `qpos` by DOF number, which only lines up when every
/// earlier joint has as many positions as velocities. Put a free body
/// first and a slide joint's double well reads a quaternion entry.
fn qpos_indexing_after_a_free_joint() -> Seen {
    let xml = r#"<mujoco><option timestep="0.01" gravity="0 0 0"><flag contact="disable"/></option><worldbody>
        <body name="float"><freejoint/><geom type="sphere" size="0.05" mass="1"/></body>
        <body name="p"><joint name="x" type="slide" axis="1 0 0"/><geom type="sphere" size="0.05" mass="1"/></body>
        </worldbody></mujoco>"#;
    let m = load_model(xml).unwrap();
    let mut d = m.make_data();
    // Free joint: 7 positions, 6 velocities; the slide is position 7, DOF 6.
    d.qpos[7] = 1.5;
    let w = DoubleWellPotential::new(1.0, 1.0, 6);
    let mut out = DVector::zeros(m.nv);
    w.apply(&m, &d, &mut out);
    let want = -4.0 * 1.5 * (1.5 * 1.5 - 1.0);
    let text = format!("slide at x = 1.5 after a free body: well force {:.3} (want {want:.3})", out[6]);
    if (out[6] - want).abs() < 1e-9 { Seen::Fixed(text) } else { Seen::Open(format!("{text}: it read qpos[6], a quaternion entry")) }
}

/// A small, fast learner setup: 2 spins, one edge.
fn learner(tweak: impl FnOnce(&mut LearnerConfig), target: Option<IsingTarget>) -> IsingLearner {
    let mut config = LearnerConfig {
        n: 2,
        edges: vec![(0, 1)],
        delta_v: 3.0,
        x_0: 1.0,
        gamma: 1.0,
        k_b_t: 1.0,
        learning_rate: 0.1,
        n_steps: 400,
        n_burn_in: 100,
        n_trajectories: 2,
        x_thresh: 0.5,
        seed_base: 1,
    };
    tweak(&mut config);
    let target = target.unwrap_or_else(|| IsingTarget::from_ising_params(2, &[(0, 1)], &[0.5], &[0.2, -0.1], 1.0));
    let m = load_model(&generate_mjcf(2, 0, 0.01, (0.0, 10.0))).unwrap();
    IsingLearner::new(config, target, m)
}

/// `n_burn_in > n_steps` underflows `n_steps - n_burn_in`: in release it
/// wraps to ~1.8e19 measurement steps and never returns.
fn learner_burn_in_longer_than_run() -> Seen {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = run(|| learner(|c| {
            c.n_steps = 5;
            c.n_burn_in = 10;
        }, None)
        .step()
        .kl_divergence);
        tx.send(r).ok();
    });
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Err(_) => Seen::Open("n_steps 5, n_burn_in 10: step() still running after 5 s (usize underflow, effectively forever)".into()),
        Ok(Err(p)) if p.contains("burn") => Seen::Fixed(format!("refused: {p}")),
        Ok(Err(p)) => Seen::Open(format!("panics without saying why ({p})")),
        Ok(Ok(kl)) => Seen::Fixed(format!("returns (KL {kl:.3})")),
    }
}

/// `n_trajectories = 0` measures nothing; the update turns J and h NaN.
fn learner_zero_trajectories() -> Seen {
    let r = run(|| {
        let mut l = learner(|c| c.n_trajectories = 0, None);
        l.step();
        l.step()
    });
    match r {
        Ok(rec) if rec.coupling_j.iter().chain(&rec.field_h).any(|x| x.is_nan()) => Seen::Open("n_trajectories 0: accepted, J and h become NaN".into()),
        Ok(_) => Seen::Fixed("n_trajectories 0: parameters stay finite".into()),
        Err(p) => Seen::Fixed(format!("n_trajectories 0: refused ({p})")),
    }
}

/// `new` doesn't check the target's distribution length; `step` panics
/// deep inside `kl_divergence`.
fn learner_target_unchecked() -> Seen {
    let bad = IsingTarget { magnetizations: vec![0.0; 2], correlations: vec![0.0], distribution: vec![] };
    let built = run(|| learner(|_| {}, Some(bad)));
    match built {
        Err(p) => Seen::Fixed(format!("refused at new ({p})")),
        Ok(mut l) => match run(move || l.step().kl_divergence) {
            Err(p) => Seen::Open(format!("empty target distribution: new accepts it, step panics ({})", p.lines().next().unwrap_or(""))),
            Ok(kl) => Seen::Open(format!("empty target distribution accepted, KL {kl}")),
        },
    }
}

/// A `LearningRecord` holds the parameters after the update but the KL
/// from before it.
fn learner_record_mixes_before_and_after() -> Seen {
    let target = IsingTarget::from_ising_params(2, &[(0, 1)], &[0.5], &[0.2, -0.1], 1.0);
    let mut l = learner(|_| {}, Some(IsingTarget::from_ising_params(2, &[(0, 1)], &[0.5], &[0.2, -0.1], 1.0)));
    let rec = l.step();
    let kl_of = |j: &[f64], h: &[f64]| ising::kl_divergence(&target.distribution, &ising::exact_distribution(2, &[(0, 1)], j, h, 1.0));
    let (before, after) = (kl_of(&[0.0], &[0.0, 0.0]), kl_of(&rec.coupling_j, &rec.field_h));
    let text = format!("record KL {:.5}; KL of the starting params {before:.5}; of the params in the record {after:.5}", rec.kl_divergence);
    if (rec.kl_divergence - before).abs() < 1e-12 && (before - after).abs() > 1e-9 {
        Seen::Open(format!("{text}: the KL describes the old params"))
    } else {
        Seen::Fixed(text)
    }
}

/// A self-loop edge (i, i) is a constant in the exact solver but makes the
/// Gibbs sampler's spin feel itself.
fn gibbs_self_loop() -> Seen {
    let exact = ising::exact_distribution(1, &[(0, 0)], &[1.0], &[0.0], 1.0);
    let gibbs = GibbsSampler::new(1, &[(0, 0)], &[1.0], &[0.0], 1.0, 1).sample(100, 50_000);
    let p = |d: &[(u32, f64)]| d.iter().find(|(b, _)| *b == 1).map_or(0.0, |x| x.1);
    let text = format!("self-loop J=1: P(up) exact {:.3}, Gibbs {:.3}", p(&exact), p(&gibbs));
    // Reading the code suggested the self-coupling biases the sampler; measured,
    // it only slows mixing (each spin sticks), so this stays as a guard.
    if (p(&exact) - p(&gibbs)).abs() > 0.05 { Seen::Open(format!("{text}: they disagree")) } else { Seen::Fine(text) }
}

/// `sample(_, 0)` divides by zero: every probability NaN.
fn gibbs_zero_samples() -> Seen {
    let r = run(|| GibbsSampler::new(2, &[], &[], &[0.0, 0.0], 1.0, 0).sample(0, 0));
    match r {
        Ok(d) if d.iter().any(|(_, p)| p.is_nan()) => Seen::Open("sample(0, 0): every probability is NaN".into()),
        Ok(_) => Seen::Fixed("sample(0, 0): finite".into()),
        Err(p) => Seen::Fixed(format!("sample(0, 0): refused ({p})")),
    }
}

/// The sampler always starts all spins up, so at low temperature a
/// ferromagnetic chain stays in the up basin and the histogram is biased.
fn gibbs_starts_all_up() -> Seen {
    let n = 4;
    let edges: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
    let j = vec![2.0; n - 1];
    let h = vec![0.0; n];
    let exact = ising::exact_distribution(n, &edges, &j, &h, 0.3);
    let gibbs = GibbsSampler::new(n, &edges, &j, &h, 0.3, 3).sample(100, 10_000);
    let down = |d: &[(u32, f64)]| d.iter().find(|(b, _)| *b == 0).map_or(0.0, |x| x.1);
    let text = format!("4-chain J=2 kT=0.3: P(all down) exact {:.3}, Gibbs {:.3}", down(&exact), down(&gibbs));
    if (down(&exact) - down(&gibbs)).abs() > 0.1 { Seen::Open(format!("{text}: stuck where it started (all up), and no way to set the start")) } else { Seen::Fixed(text) }
}

/// An edge to a spin that doesn't exist silently acts as a field.
fn ising_edge_out_of_range() -> Seen {
    let r = run(|| ising::exact_distribution(2, &[(0, 5)], &[1.0], &[0.0, 0.0], 1.0));
    match r {
        Err(p) => Seen::Fixed(format!("edge (0,5) on 2 spins: refused ({p})")),
        Ok(d) => {
            let up: f64 = d.iter().filter(|(b, _)| b & 1 == 1).map(|x| x.1).sum();
            Seen::Open(format!("edge (0,5) on 2 spins: accepted; spin 0 is up with probability {up:.3} (no edge should mean 0.5)"))
        }
    }
}

/// `tv_distance` pairs entries by position, not by configuration.
fn tv_distance_ignores_bitmasks() -> Seen {
    let p = vec![(0u32, 0.9), (1, 0.1)];
    let q = vec![(1u32, 0.1), (0, 0.9)];
    let tv = ising::tv_distance(&p, &q);
    let text = format!("the same distribution listed in another order: TV {tv:.3}");
    if tv > 1e-12 { Seen::Open(format!("{text} (should be 0)")) } else { Seen::Fixed(text) }
}

/// The 1-D integrators take parameters that turn everything NaN.
fn integrators_unchecked() -> Seen {
    let w = DoubleWellPotential::new(3.0, 1.0, 0);
    let colored = run(|| {
        let mut s = ColoredDriveSim::new(&w, 1.0, 1.0, 1.0, 0.0, 1e-3, 1, 1.0);
        s.step();
        s.velocity()
    });
    let baoab = run(|| {
        let mut s = Baoab1D::new(&w, 1.0, -0.5, 1.0, 1e-3, 1, 1.0);
        for _ in 0..10 {
            s.step();
        }
        s.position()
    });
    let say = |r: &Result<f64, String>| match r {
        Ok(x) if x.is_finite() => format!("finite ({x:.3})"),
        Ok(x) => format!("accepted, gives {x}"),
        Err(p) => format!("refused ({p})"),
    };
    let text = format!("ColoredDriveSim with tau 0: {}; Baoab1D with gamma -0.5: {}", say(&colored), say(&baoab));
    if [&colored, &baoab].iter().any(|r| matches!(r, Ok(x) if !x.is_finite())) { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// The version of `krate` this probe was built against, from Cargo.lock.
fn locked_version(krate: &str) -> &'static str {
    let lock = include_str!("../Cargo.lock");
    let at = lock.find(&format!("name = \"{krate}\"
version = \"")).expect("crate in Cargo.lock");
    let rest = &lock[at..];
    let start = rest.find("version = \"").unwrap() + 11;
    let len = rest[start..].find('"').unwrap();
    &rest[start..start + len]
}

fn main() {
    // Caught panics are reported by the checks; keep the default hook quiet.
    std::panic::set_hook(Box::new(|_| {}));
    let checks: Vec<Check> = vec![
        ("exact_distribution overflows to NaN", exact_distribution_overflows),
        ("ExternalField length unchecked", external_field_length_unchecked),
        ("exact solver caps at 20 spins", exact_solver_caps_at_20),
        ("second stack replaces the first", second_install_replaces_first),
        ("negative ctrl temperature", negative_ctrl_temperature),
        ("seeds replay across threads", seeds_replay_across_threads),
        ("component joint index out of range", dof_out_of_range),
        ("noise reused across groups of 8 joints", noise_reused_across_groups_of_8),
        ("traj_id high 32 bits ignored", traj_id_high_bits_ignored),
        ("reset doesn't replay the noise", reset_does_not_replay_noise),
        ("cloned model shares the noise", cloned_model_shares_the_noise),
        ("RK4 shrinks the noise", rk4_shrinks_the_noise),
        ("ctrl temperature without actuators", ctrl_temperature_without_actuators),
        ("thermostat parameters unchecked", thermostat_params_unchecked),
        ("duplicate edges double J", duplicate_edges_double_j),
        ("WellState threshold unchecked", well_state_threshold_unchecked),
        ("stiff damping diverges", stiff_damping_diverges),
        ("depopulation factor underflows", depopulation_factor_underflows),
        ("qpos indexing after a free joint", qpos_indexing_after_a_free_joint),
        ("learner: burn-in longer than the run", learner_burn_in_longer_than_run),
        ("learner: zero trajectories", learner_zero_trajectories),
        ("learner: target unchecked", learner_target_unchecked),
        ("learner: record mixes before and after", learner_record_mixes_before_and_after),
        ("Gibbs: self-loop edge", gibbs_self_loop),
        ("Gibbs: zero samples", gibbs_zero_samples),
        ("Gibbs: always starts all up", gibbs_starts_all_up),
        ("Ising: edge out of range", ising_edge_out_of_range),
        ("tv_distance ignores bitmasks", tv_distance_ignores_bitmasks),
        ("1-D integrators unchecked", integrators_unchecked),
    ];
    let (mut open, mut fixed, mut fine) = (0, 0, 0);
    for (name, check) in checks {
        let (tag, text) = match run(check) {
            Ok(Seen::Open(t)) => {
                open += 1;
                ("OPEN ", t)
            }
            Ok(Seen::Fixed(t)) => {
                fixed += 1;
                ("FIXED", t)
            }
            Ok(Seen::Fine(t)) => {
                fine += 1;
                ("ok   ", t)
            }
            Err(p) => {
                open += 1;
                ("OPEN ", format!("the check itself panicked: {p}"))
            }
        };
        println!("{tag} {name}: {text}");
    }
    println!("\n{open} open, {fixed} fixed, {fine} fine (cortenforge-sim-thermostat {})", locked_version("cortenforge-sim-thermostat"));
}
