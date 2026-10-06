//! Gap probe for `sim-ml-chassis` + `sim-rl` 0.9.x: one check per finding, so
//! the same run tells us what 0.9.2 fixed.
//!
//! cargo run --release --example gaps_ml_chassis
//!
//! Each check prints OPEN (the gap is still there), FIXED (a known gap that
//! now behaves) or ok (never a gap; kept to catch regressions), with what
//! it saw. A panic inside a check is caught and reported, never fatal. The ids
//! match the entries in docs/FINDINGS.md.
//!
//! The last check gives CEM a real job: learn a wash program (when to heat
//! the drum) that shakes a sock out of the shallow well into the deep one.

use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cortenforge::sim::core::{Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::ml_chassis::{
    ActionSpace, Algorithm, AutogradPolicy, DifferentiablePolicy, Environment, LinearPolicy, LinearQ, MlpPolicy, MlpQ,
    ObservationSpace, OptimizerConfig, Policy, QFunction, ReplayBuffer, SimEnv, Tape, TaskConfig, Tensor,
    TrainingBudget, TrainingCheckpoint, VecEnv, collect_episodic_rollout,
};
use cortenforge::sim::rl::{Cem, CemHyperparams};
use cortenforge::sim::therm_env::ThermCircuitEnv;
use cortenforge::sim::thermostat::{DoubleWellPotential, ExternalField};

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

/// Runs `f`, turning a panic into its message.
fn run<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panic".into())
    })
}

/// A two-link arm, held out sideways so gravity swings it, with a joint
/// sensor on the shoulder, two motors and a mocap target.
const ARM: &str = r#"
<mujoco model="arm">
  <option timestep="0.01" integrator="INTEGRATOR"/>
  <worldbody>
    <body name="upper" pos="0 0 1">
      <joint name="j0" type="hinge" axis="0 1 0" damping="0.05" STIFF/>
      <geom type="capsule" fromto="0 0 0 0.4 0 0" size="0.04" mass="1"/>
      <body name="lower" pos="0.4 0 0">
        <joint name="j1" type="hinge" axis="0 1 0" damping="0.05" STIFF/>
        <geom type="capsule" fromto="0 0 0 0.4 0 0" size="0.04" mass="1"/>
      </body>
    </body>
    <body name="target" mocap="true" pos="0.5 0 1">
      <geom type="sphere" size="0.05" contype="0" conaffinity="0"/>
    </body>
  </worldbody>
  <actuator>
    <motor name="m0" joint="j0" ctrlrange="-1 1" ctrllimited="true"/>
    <motor name="m1" joint="j1" ctrlrange="-1 1" ctrllimited="true"/>
  </actuator>
  <sensor><jointpos name="angle" joint="j0"/></sensor>
</mujoco>"#;

/// A cart on a rail with no gravity and one motor.
const CART: &str = r#"
<mujoco model="cart">
  <option timestep="0.01" gravity="0 0 0"/>
  <worldbody>
    <body name="cart"><joint name="x" type="slide" axis="1 0 0"/><geom type="sphere" size="0.1" mass="1"/></body>
  </worldbody>
  <actuator><motor joint="x" ctrlrange="-1 1" ctrllimited="true"/></actuator>
</mujoco>"#;

/// The arm under Euler.
fn arm_model() -> Model {
    load_model(&ARM.replace("INTEGRATOR", "Euler").replace("STIFF", "")).expect("arm")
}

fn arm() -> Arc<Model> {
    Arc::new(arm_model())
}

/// `[qpos.., qvel..]`.
fn obs_q(m: &Model) -> ObservationSpace {
    ObservationSpace::builder().all_qpos().all_qvel().build(m).expect("obs")
}

/// Every ctrl.
fn act_ctrl(m: &Model) -> ActionSpace {
    ActionSpace::builder().all_ctrl().build(m).expect("act")
}

/// A `SimEnv` that never ends, with reward 0.
fn sim(m: &Arc<Model>, obs: ObservationSpace, act: ActionSpace) -> SimEnv {
    SimEnv::builder(Arc::clone(m))
        .observation_space(obs)
        .action_space(act)
        .reward(|_, _| 0.0)
        .done(|_, _| false)
        .truncated(|_, _| false)
        .build()
        .expect("sim env")
}

/// A `VecEnv` of `n` that never ends, with reward 0.
fn vec_env(m: &Arc<Model>, n: usize, obs: ObservationSpace, act: ActionSpace) -> VecEnv {
    VecEnv::builder(Arc::clone(m), n)
        .observation_space(obs)
        .action_space(act)
        .reward(|_, _| 0.0)
        .done(|_, _| false)
        .truncated(|_, _| false)
        .build()
        .expect("vec env")
}

/// A one-env `VecEnv` driven by `qfrc_applied` on the shoulder.
fn vec_qfrc(m: &Arc<Model>, sub_steps: usize) -> VecEnv {
    VecEnv::builder(Arc::clone(m), 1)
        .observation_space(obs_q(m))
        .action_space(ActionSpace::builder().qfrc_applied(0..1).build(m).expect("act"))
        .reward(|_, d| d.qpos[0])
        .done(|_, _| false)
        .truncated(|_, _| false)
        .sub_steps(sub_steps)
        .build()
        .expect("vec env")
}

// ── SimEnv and VecEnv ──────────────────────────────────────────────────────

/// `VecEnv::step` doesn't call `forward()` after `step_all`, so sensors and
/// other derived fields describe the state before the last integration.
fn vec_env_sensors_stale() -> Seen {
    let m = arm();
    let obs = || ObservationSpace::builder().sensor("angle").all_qpos().build(&m).expect("obs");
    let mut v = vec_env(&m, 1, obs(), act_ctrl(&m));
    v.reset_all().expect("reset");
    let r = v.step(&Tensor::zeros(&[1, 2])).expect("step");
    let row = r.observations.row(0).to_vec();
    let mut s = sim(&m, obs(), act_ctrl(&m));
    s.reset().expect("reset");
    let o = s.step(&Tensor::zeros(&[2])).expect("step").observation;
    let so = o.as_slice();
    let text = format!(
        "after 1 step: VecEnv sensor {:.6} vs qpos {:.6}; SimEnv sensor {:.6} vs qpos {:.6}",
        row[0], row[1], so[0], so[1]
    );
    if row[0] != row[1] { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `SimEnv::step` calls `forward()` after stepping, which runs the passive
/// callback (the thermostat, in therm-env) a second time per step.
fn sim_env_passive_twice() -> Seen {
    let count = |use_vec: bool| {
        let mut model = arm_model();
        let n = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&n);
        model.set_passive_callback(move |_, _| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        let m = Arc::new(model);
        if use_vec {
            let mut v = vec_env(&m, 1, obs_q(&m), act_ctrl(&m));
            v.reset_all().expect("reset");
            n.store(0, Ordering::Relaxed);
            for _ in 0..10 {
                v.step(&Tensor::zeros(&[1, 2])).expect("step");
            }
        } else {
            let mut s = sim(&m, obs_q(&m), act_ctrl(&m));
            s.reset().expect("reset");
            n.store(0, Ordering::Relaxed);
            for _ in 0..10 {
                s.step(&Tensor::zeros(&[2])).expect("step");
            }
        }
        n.load(Ordering::Relaxed)
    };
    let (s, v) = (count(false), count(true));
    let text = format!("passive callback calls in 10 steps: SimEnv {s}, VecEnv {v}");
    if s != v { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// Steps a SimEnv and a one-env VecEnv `steps` times from the same start;
/// returns both final `[qpos, qvel]`.
fn sim_vs_vec(m: &Arc<Model>, steps: usize) -> (Vec<f64>, Vec<f64>) {
    let mut s = sim(m, obs_q(m), act_ctrl(m));
    s.reset().expect("reset");
    let mut v = vec_env(m, 1, obs_q(m), act_ctrl(m));
    v.reset_all().expect("reset");
    for _ in 0..steps {
        s.step(&Tensor::zeros(&[2])).expect("step");
        v.step(&Tensor::zeros(&[1, 2])).expect("step");
    }
    let state = |d: &Data| d.qpos.iter().chain(d.qvel.iter()).copied().collect::<Vec<f64>>();
    (state(s.data()), state(v.batch().env(0).expect("env")))
}

/// With no passive callback and Euler, the two envs walk the same path.
fn sim_and_vec_agree_euler() -> Seen {
    let (s, v) = sim_vs_vec(&arm(), 50);
    if s == v {
        Seen::Fine(format!("50 steps, bit for bit: qpos[0] {:.6}", s[0]))
    } else {
        Seen::Open(format!("SimEnv {s:?} vs VecEnv {v:?}"))
    }
}

/// Under implicitspringdamper, `forward()` itself moves qvel (see sim-core),
/// so SimEnv's extra `forward()` walks a different path from VecEnv.
fn sim_and_vec_differ_implicit() -> Seen {
    let xml = ARM.replace("INTEGRATOR", "implicitspringdamper").replace("STIFF", r#"stiffness="5""#);
    let m = Arc::new(load_model(&xml).expect("arm"));
    let (s, v) = sim_vs_vec(&m, 50);
    let text = format!("after 50 steps qvel[0]: SimEnv {:.6}, VecEnv {:.6}", s[2], v[2]);
    if s != v { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// sim-core resets a diverged Data inside `step` and returns Ok; SimEnv
/// doesn't check `divergence_detected()`, so the blow-up is invisible.
fn sim_env_hides_divergence() -> Seen {
    let m = arm();
    let act = ActionSpace::builder().qfrc_applied(0..1).build(&m).expect("act");
    let mut s = sim(&m, obs_q(&m), act);
    s.reset().expect("reset");
    for _ in 0..20 {
        s.step(&Tensor::zeros(&[1])).expect("step");
    }
    let before = s.data().time;
    match s.step(&Tensor::from_slice(&[f32::NAN], &[1])) {
        Ok(r) => {
            let text = format!(
                "NaN force at t = {before:.2}: Ok, done {}, time now {:.2}, divergence_detected {}",
                r.done,
                s.data().time,
                s.data().divergence_detected()
            );
            if r.done { Seen::Fixed(text) } else { Seen::Open(text) }
        }
        Err(e) => Seen::Fixed(format!("step returned Err: {e}")),
    }
}

/// VecEnv catches divergence, but only as a plain `done`: `errors` stays
/// None and the reward is the reset state's.
fn vec_env_divergence_is_plain_done() -> Seen {
    let m = arm();
    let mut v = vec_qfrc(&m, 4);
    v.reset_all().expect("reset");
    for _ in 0..20 {
        v.step(&Tensor::zeros(&[1, 1])).expect("step");
    }
    let r = v.step(&Tensor::from_slice(&[f32::NAN], &[1, 1])).expect("step");
    let q0 = m.qpos0[0];
    let text = format!(
        "NaN force, 4 sub-steps: done {}, truncated {}, error {:?}, terminal obs {}, reward {:.4} \
         (qpos0 is {q0:.4}: the reward comes from the restarted episode)",
        r.dones[0],
        r.truncateds[0],
        r.errors[0].as_ref().map(|e| e.to_string()),
        if r.terminal_observations[0].is_some() { "Some" } else { "None" },
        r.rewards[0]
    );
    if r.dones[0] && r.errors[0].is_none() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// sim-core checks qpos/qvel at the start of a step, so a state pushed past
/// 1e10 by the last integration is returned as a normal step first.
fn vec_env_divergence_one_step_late() -> Seen {
    let m = arm();
    let mut v = vec_qfrc(&m, 1);
    v.reset_all().expect("reset");
    {
        let d = v.batch_mut().env_mut(0).expect("env");
        d.qpos[0] = 9.99e9;
        d.qvel[0] = 5e9;
    }
    let a = Tensor::zeros(&[1, 1]);
    let r1 = v.step(&a).expect("step");
    let r2 = v.step(&a).expect("step");
    let text = format!(
        "step 1: done {}, obs qpos {:.4e}; step 2: done {}",
        r1.dones[0],
        r1.observations.row(0)[0],
        r2.dones[0]
    );
    if !r1.dones[0] && r1.observations.row(0)[0].abs() > 1e10 { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `SimEnv::step` doesn't check the action length: short panics, long is
/// cut silently. (VecEnv checks the shape.)
fn sim_env_action_length_unchecked() -> Seen {
    let m = arm();
    let mut s = sim(&m, obs_q(&m), act_ctrl(&m));
    s.reset().expect("reset");
    let short = run(|| s.step(&Tensor::zeros(&[1])).map(|_| ()));
    let mut s = sim(&m, obs_q(&m), act_ctrl(&m));
    s.reset().expect("reset");
    let long = run(|| s.step(&Tensor::zeros(&[5])).map(|_| ()));
    let say = |r: &Result<Result<(), _>, String>| match r {
        Ok(Ok(())) => "Ok".to_string(),
        Ok(Err(e)) => format!("Err({e})"),
        Err(p) => format!("panics ({p})"),
    };
    let text = format!("act dim 2: 1 value {}; 5 values {}", say(&short), say(&long));
    if matches!(long, Ok(Ok(()))) || short.is_err() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `check_flat` only tests `end <= len`, so a reversed range builds with
/// dim 0 and then panics in `extract`.
fn reversed_obs_range() -> Seen {
    let m = arm_model();
    let backwards = Range { start: 2, end: 1 };
    match ObservationSpace::builder().qpos(backwards).build(&m) {
        Ok(space) => {
            let dim = space.dim();
            let d = m.make_data();
            match run(|| space.extract(&d)) {
                Ok(t) => Seen::Open(format!("qpos(2..1) builds (dim {dim}), extract gives {:?}", t.shape())),
                Err(p) => Seen::Open(format!("qpos(2..1) builds (dim {dim}), extract panics: {p}")),
            }
        }
        Err(e) => Seen::Fixed(format!("build refuses it: {e}")),
    }
}

/// Two `ctrl` injectors over the same channel both build; the later one wins.
fn overlapping_action_injectors() -> Seen {
    let m = arm_model();
    match ActionSpace::builder().ctrl(0..2).ctrl(1..2).build(&m) {
        Ok(a) => Seen::Open(format!("ctrl(0..2).ctrl(1..2) builds with dim {} for 2 ctrls", a.dim())),
        Err(e) => Seen::Fixed(format!("refused: {e}")),
    }
}

/// `SimEnv` can return done and truncated together; `VecEnv` forces
/// truncated false when done.
fn done_and_truncated_disagree() -> Seen {
    let m = arm();
    let mut s = SimEnv::builder(Arc::clone(&m))
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, _| 0.0)
        .done(|_, _| true)
        .truncated(|_, _| true)
        .build()
        .expect("sim");
    s.reset().expect("reset");
    let r = s.step(&Tensor::zeros(&[2])).expect("step");
    let mut v = VecEnv::builder(Arc::clone(&m), 1)
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, _| 0.0)
        .done(|_, _| true)
        .truncated(|_, _| true)
        .build()
        .expect("vec");
    v.reset_all().expect("reset");
    let rv = v.step(&Tensor::zeros(&[1, 2])).expect("step");
    let text = format!(
        "both true: SimEnv (done {}, truncated {}), VecEnv (done {}, truncated {})",
        r.done, r.truncated, rv.dones[0], rv.truncateds[0]
    );
    if r.truncated != rv.truncateds[0] { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// VecEnv keeps the terminal observation on a truncation auto-reset.
fn vec_env_keeps_terminal_obs() -> Seen {
    let m = arm();
    let mut v = VecEnv::builder(Arc::clone(&m), 1)
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, _| 0.0)
        .done(|_, _| false)
        .truncated(|_, d| d.time > 0.095)
        .build()
        .expect("vec");
    v.reset_all().expect("reset");
    for k in 1..=20 {
        let r = v.step(&Tensor::zeros(&[1, 2])).expect("step");
        if r.truncateds[0] {
            let t = r.terminal_observations[0].as_ref().map(|t| t.as_slice()[0]);
            let o = r.observations.row(0)[0];
            return match t {
                Some(t) if t != o => Seen::Fine(format!("truncated at step {k}: terminal qpos {t:.4}, new obs {o:.4}")),
                _ => Seen::Open(format!("truncated at step {k}: terminal obs {t:?}, new obs {o:.4}")),
            };
        }
    }
    Seen::Open("never truncated".into())
}

/// The mocap injectors' argument is named `body_range` but takes mocap ids;
/// an all-zero quaternion action writes a NaN pose that nothing flags.
fn mocap_injectors() -> Seen {
    let m = arm();
    let by_body = ActionSpace::builder().mocap_pos(3..4).build(&m).map(|a| a.dim());
    let act = ActionSpace::builder().mocap_quat(0..1).build(&m).expect("act");
    let mut s = sim(&m, obs_q(&m), act);
    s.reset().expect("reset");
    let r = s.step(&Tensor::zeros(&[4]));
    let q = s.data().mocap_quat[0];
    let text = format!(
        "mocap_pos(3..4) by body id: {}; zero quat action: step {}, mocap_quat {:?}, divergence_detected {}",
        match &by_body {
            Ok(d) => format!("Ok (dim {d})"),
            Err(e) => format!("Err ({e})"),
        },
        if r.is_ok() { "Ok" } else { "Err" },
        q.coords.as_slice(),
        s.data().divergence_detected()
    );
    if q.coords.iter().any(|c| c.is_nan()) { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// The `energy()` observation reads 0 unless the model enables energy; the
/// builder doesn't say so.
fn energy_obs_needs_flag() -> Seen {
    let m = arm();
    let mut s = sim(&m, ObservationSpace::builder().energy().build(&m).expect("obs"), act_ctrl(&m));
    s.reset().expect("reset");
    let mut o = Tensor::zeros(&[2]);
    for _ in 0..30 {
        o = s.step(&Tensor::zeros(&[2])).expect("step").observation;
    }
    let e = o.as_slice().to_vec();
    if e.iter().all(|&x| x == 0.0) {
        Seen::Open(format!("arm swinging for 0.3 s, energy obs {e:?} (no <flag energy=\"enable\"/>)"))
    } else {
        Seen::Fixed(format!("energy obs {e:?}"))
    }
}

/// `TaskConfig::from_build_fn` checks nothing: a 1-long obs_scale for a
/// 4-wide observation, and an act_dim of 1 for a 2-ctrl env.
fn from_build_fn_unchecked() -> Seen {
    let m = arm();
    let mm = Arc::clone(&m);
    let task = TaskConfig::from_build_fn("liar", 4, 1, vec![1.0], move |n, _| {
        VecEnv::builder(Arc::clone(&mm), n)
            .observation_space(obs_q(&mm))
            .action_space(act_ctrl(&mm))
            .reward(|_, _| 0.0)
            .done(|_, _| false)
            .truncated(|_, _| false)
            .build()
    });
    match task.build_vec_env(2, 0) {
        Ok(_) => Seen::Open(format!(
            "builds: says act_dim {} and obs_scale len {}, env has 2 ctrls and obs dim 4",
            task.act_dim(),
            task.obs_scale().len()
        )),
        Err(e) => Seen::Fixed(format!("refused: {e}")),
    }
}

// ── Rollouts ───────────────────────────────────────────────────────────────

/// `collect_episodic_rollout` takes act_dim from env 0's first action: a
/// short action from another env is zero-padded, a long one panics.
fn rollout_action_length() -> Seen {
    let m = arm();
    let mut v = vec_env(&m, 3, obs_q(&m), act_ctrl(&m));
    let short = run(|| {
        collect_episodic_rollout(&mut v, &mut |i, _| if i == 1 { vec![] } else { vec![0.5, 0.5] }, 3)
            .trajectories[1]
            .actions[0]
            .clone()
    });
    let mut v = vec_env(&m, 3, obs_q(&m), act_ctrl(&m));
    let long = run(|| collect_episodic_rollout(&mut v, &mut |i, _| vec![0.5; if i == 2 { 3 } else { 2 }], 3).trajectories.len());
    let text = format!(
        "env 1 returns []: {}; env 2 returns 3 values: {}",
        match &short {
            Ok(a) => format!("runs, recorded action {a:?}"),
            Err(p) => format!("panics ({p})"),
        },
        match &long {
            Ok(_) => "runs".to_string(),
            Err(p) => format!("panics ({p})"),
        }
    );
    if short.is_ok() || long.is_err() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// A VecEnv of 0 envs builds, and `max_steps = 0` still takes a step.
fn rollout_zero_cases() -> Seen {
    let m = arm();
    let zero = VecEnv::builder(Arc::clone(&m), 0)
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, _| 0.0)
        .done(|_, _| false)
        .truncated(|_, _| false)
        .build();
    let zero_text = match zero {
        Ok(mut v) => match run(|| collect_episodic_rollout(&mut v, &mut |_, _| vec![0.0; 2], 5).trajectories.len()) {
            Ok(n) => format!("0 envs builds; rollout gives {n} trajectories"),
            Err(p) => format!("0 envs builds; rollout panics ({p})"),
        },
        Err(e) => format!("0 envs refused ({e})"),
    };
    let mut v = vec_env(&m, 1, obs_q(&m), act_ctrl(&m));
    let len = collect_episodic_rollout(&mut v, &mut |_, _| vec![0.0; 2], 0).trajectories[0].len();
    let text = format!("{zero_text}; max_steps 0 gives a {len}-step trajectory");
    if len > 0 || zero_text.starts_with("0 envs builds") { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// Envs that finish early keep stepping (and auto-resetting, and running
/// `on_reset`) until the slowest env finishes.
fn rollout_steps_finished_envs() -> Seen {
    let m = arm();
    let resets: Arc<[AtomicUsize; 2]> = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let r = Arc::clone(&resets);
    let mut v = VecEnv::builder(Arc::clone(&m), 2)
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, _| 0.0)
        .done(|_, _| false)
        .truncated(|_, d| d.time > 0.055)
        // Env 1 starts its clock 0.5 s back, so it runs 10x longer than env 0.
        .on_reset(move |_, d, i| {
            r[i].fetch_add(1, Ordering::Relaxed);
            d.time = -0.5 * i as f64;
        })
        .build()
        .expect("vec");
    let ro = collect_episodic_rollout(&mut v, &mut |_, _| vec![0.0; 2], 1000);
    let lens: Vec<usize> = ro.trajectories.iter().map(|t| t.len()).collect();
    let n0 = resets[0].load(Ordering::Relaxed);
    let text = format!("episode lengths {lens:?}; env 0's on_reset ran {n0} times in one rollout");
    if n0 > 2 { Seen::Open(text) } else { Seen::Fixed(text) }
}

// ── Policies, values, autograd, optimizer ─────────────────────────────────

/// `MlpPolicy` / `MlpQ` / `AutogradPolicy::new` start at all zeros, so the
/// hidden layer never gets a gradient: only the output bias can learn.
fn mlp_zero_init() -> Seen {
    let obs = [0.5f32, 0.2, -0.1];
    let nonzero = |g: &[f64]| g.iter().filter(|x| **x != 0.0).count();
    let mlp = MlpPolicy::new(3, 8, 1, &[1.0; 3]);
    let g = mlp.log_prob_gradient(&obs, &[0.7], 0.5);
    let q = MlpQ::new(3, 8, 1, &[1.0; 3]);
    let qa = q.action_gradient(&obs, &[0.3]);
    let ag = AutogradPolicy::new(3, &[8], 1, &[1.0; 3]);
    let ga = ag.log_prob_gradient(&obs, &[0.7], 0.5);
    let text = format!(
        "nonzero gradient entries: MlpPolicy {} of {}, AutogradPolicy {} of {}; MlpQ action_gradient {qa:?}",
        nonzero(&g),
        g.len(),
        nonzero(&ga),
        ga.len()
    );
    if nonzero(&g) <= 1 || nonzero(&ga) <= 1 { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `LinearPolicy` zips the observation with `obs_scale`: a short obs reads
/// as zeros, extra entries are dropped, no error.
fn linear_policy_obs_length() -> Seen {
    let mut p = LinearPolicy::new(2, 1, &[1.0, 1.0]);
    p.set_params(&[1.0, 1.0, 0.0]);
    let short = run(|| p.forward(&[1.0]));
    let long = run(|| p.forward(&[1.0, 0.0, 9.0]));
    let text = format!("obs_dim 2: forward([1]) {short:?}, forward([1, 0, 9]) {long:?}");
    if short.is_ok() || long.is_ok() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `LinearQ` concatenates obs and action before the weights, so a short obs
/// shifts the action into an obs weight.
fn linear_q_misaligned() -> Seen {
    let mut q = LinearQ::new(2, 1, &[1.0, 1.0]);
    q.set_params(&[1.0, 2.0, 3.0, 0.0]);
    let right = q.forward(&[1.0, 0.0], &[1.0]);
    let wrong = run(|| q.forward(&[1.0], &[1.0]));
    let text = format!("Q([1, 0], [1]) = {right}; Q([1], [1]) = {wrong:?}");
    if wrong.is_ok() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `forward_batch` drops a trailing partial row.
fn forward_batch_partial_row() -> Seen {
    let p = LinearPolicy::new(2, 1, &[1.0, 1.0]);
    match run(|| p.forward_batch(&[1.0; 5], 2)) {
        Ok(out) => Seen::Open(format!("5 floats at obs_dim 2 give {} outputs, no error", out.len())),
        Err(e) => Seen::Fixed(format!("panics: {e}")),
    }
}

/// `log_prob_gradient` with sigma 0 divides by zero.
fn sigma_zero_gradient() -> Seen {
    let p = LinearPolicy::new(2, 1, &[1.0, 1.0]);
    let g = p.log_prob_gradient(&[1.0, 0.5], &[0.3], 0.0);
    if g.iter().any(|x| !x.is_finite()) {
        Seen::Open(format!("sigma 0 gives {g:?}"))
    } else {
        Seen::Fixed(format!("{g:?}"))
    }
}

/// A second `Tape::backward` re-propagates the intermediate cotangents, so
/// it over-counts instead of accumulating.
fn tape_backward_twice() -> Seen {
    let mut t = Tape::new();
    let x = t.param(1.0);
    let two = t.constant(2.0);
    let three = t.constant(3.0);
    let a = t.mul(x, two);
    let b = t.mul(a, three);
    t.backward(b);
    let once = t.grad(x);
    t.backward(b);
    let twice = t.grad(x);
    let text = format!("b = 6x: grad after one backward {once}, after two {twice} (accumulating gives 12)");
    if twice != 2.0 * once { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// Adam takes any settings: a negative `max_grad_norm` reverses the step,
/// and beta1 = 1 gives NaN.
fn adam_settings_unchecked() -> Seen {
    let adam = |beta1: f64, clip: f64| {
        let cfg = OptimizerConfig::Adam { lr: 0.1, beta1, beta2: 0.999, eps: 1e-8, max_grad_norm: clip };
        let mut o = cfg.build(1);
        let mut p = [0.0];
        o.step_in_place(&mut p, &[1.0], true);
        p[0]
    };
    let (ok, neg, b1) = (adam(0.9, 1.0), adam(0.9, -1.0), adam(1.0, 1.0));
    let text = format!("one ascent step on gradient +1: clip 1 -> {ok:.3}, clip -1 -> {neg:.3}, beta1 1 -> {b1}");
    if neg < 0.0 || b1.is_nan() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `ReplayBuffer::new(0, ..)` builds, and the first push panics.
fn replay_buffer_zero_capacity() -> Seen {
    match run(|| {
        let mut b = ReplayBuffer::new(0, 2, 1);
        b.push(&[0.0, 0.0], &[0.0], 0.0, &[0.0, 0.0], false);
    }) {
        Ok(()) => Seen::Fixed("capacity 0 accepts a push".into()),
        Err(p) => Seen::Open(format!("capacity 0 builds, first push panics: {p}")),
    }
}

// ── CEM ────────────────────────────────────────────────────────────────────

/// CEM settings for the probes.
fn hp(elite_fraction: f64, noise_std: f64, max_episode_steps: usize) -> CemHyperparams {
    CemHyperparams { elite_fraction, noise_std, noise_decay: 0.9, noise_min: 0.05, max_episode_steps }
}

/// A reaching task on the arm: hold the shoulder at 0.5 rad, 1 s episodes.
fn reach_env(n: usize) -> VecEnv {
    let m = arm();
    VecEnv::builder(Arc::clone(&m), n)
        .observation_space(obs_q(&m))
        .action_space(act_ctrl(&m))
        .reward(|_, d| -(d.qpos[0] - 0.5).powi(2))
        .done(|_, _| false)
        .truncated(|_, d| d.time > 1.0)
        .build()
        .expect("reach")
}

fn reach_cem(h: CemHyperparams) -> Cem {
    Cem::new(Box::new(LinearPolicy::new(4, 2, &[1.0, 1.0, 0.3, 0.3])), h)
}

/// `n_elites` isn't capped at the population, so elite_fraction > 1 panics.
fn cem_elite_fraction_over_one() -> Seen {
    let mut cem = reach_cem(hp(1.5, 0.3, 100));
    match run(|| cem.train(&mut reach_env(4), TrainingBudget::Epochs(1), 0, &|_| {}).len()) {
        Ok(n) => Seen::Fixed(format!("trains {n} epoch")),
        Err(p) => Seen::Open(format!("elite_fraction 1.5 with 4 envs panics: {p}")),
    }
}

/// Nothing checks the other settings: elite_fraction 0 or NaN become one
/// elite, a NaN noise trains on silently.
fn cem_hyperparams_unchecked() -> Seen {
    let mut out = Vec::new();
    for (name, h) in
        [("elite_fraction 0", hp(0.0, 0.3, 100)), ("elite_fraction NaN", hp(f64::NAN, 0.3, 100)), ("noise_std NaN", hp(0.2, f64::NAN, 100))]
    {
        let mut cem = reach_cem(h);
        let r = run(|| cem.train(&mut reach_env(8), TrainingBudget::Epochs(2), 0, &|_| {}));
        out.push(match r {
            Ok(m) => format!(
                "{name}: trains, last mean_reward {:.1}, params[0] {:.3}",
                m.last().map_or(f64::NAN, |e| e.mean_reward),
                cem.policy_artifact().params[0]
            ),
            Err(p) => format!("{name}: panics ({p})"),
        });
    }
    let text = out.join("; ");
    if out.iter().all(|s| s.contains("trains")) { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `Steps(s)` is divided by n_envs x max_episode_steps, so a budget under
/// one full-length epoch trains nothing, silently.
fn cem_steps_budget() -> Seen {
    let mut cem = reach_cem(hp(0.2, 0.3, 300));
    let m = cem.train(&mut reach_env(10), TrainingBudget::Steps(2999), 0, &|_| {});
    let mut cem = reach_cem(hp(0.2, 0.3, 300));
    let one = cem.train(&mut reach_env(10), TrainingBudget::Steps(3000), 0, &|_| {});
    let used: usize = one.iter().map(|e| e.total_steps).sum();
    let text = format!(
        "10 envs x 300 max steps: Steps(2999) runs {} epochs; Steps(3000) runs {} that uses {used} steps (episodes truncate at 1 s)",
        m.len(),
        one.len()
    );
    if m.is_empty() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// The `noise_std` CEM reports for an epoch is the decayed value it will
/// use next, not the one it just used.
fn cem_noise_metric_is_next_epochs() -> Seen {
    let mut cem = reach_cem(hp(0.2, 0.3, 100));
    let m = cem.train(&mut reach_env(8), TrainingBudget::Epochs(1), 0, &|_| {});
    let reported = m[0].extra.get("noise_std").copied().unwrap_or(f64::NAN);
    let text = format!("epoch 0 ran at noise 0.3, reports {reported:.3}");
    if (reported - 0.3).abs() > 1e-12 { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// With the same seed on a deterministic task, CEM replays.
fn cem_same_seed_replays() -> Seen {
    let go = || {
        let mut cem = reach_cem(hp(0.25, 0.3, 100));
        let m = cem.train(&mut reach_env(8), TrainingBudget::Epochs(3), 9, &|_| {});
        (m.iter().map(|e| e.mean_reward).collect::<Vec<_>>(), cem.policy_artifact().params)
    };
    let (a, b) = (go(), go());
    if a == b {
        Seen::Fine(format!("3 epochs twice, bit for bit: mean_reward {:.4} -> {:.4}", a.0[0], a.0[2]))
    } else {
        Seen::Open(format!("{:?} vs {:?}", a.0, b.0))
    }
}

/// CEM ranks elites by reward per step but reports reward per episode: with
/// +1 per step alive, every episode ties at 1.0, so selection is by env
/// index and CEM can't learn to survive.
fn cem_per_step_fitness() -> Seen {
    let m = Arc::new(load_model(CART).expect("cart"));
    let mut survived = 0;
    let mut finals = Vec::new();
    let mut per_step0 = f64::NAN;
    let seeds = 8;
    for seed in 0..seeds {
        let mut v = VecEnv::builder(Arc::clone(&m), 16)
            .observation_space(obs_q(&m))
            .action_space(act_ctrl(&m))
            .reward(|_, _| 1.0)
            .done(|_, d| d.qpos[0] > 0.5)
            .truncated(|_, d| d.time > 3.0)
            .build()
            .expect("cart");
        let mut cem = Cem::new(Box::new(LinearPolicy::new(2, 1, &[1.0, 1.0])), hp(0.25, 0.5, 300));
        let mt = cem.train(&mut v, TrainingBudget::Epochs(10), seed, &|_| {});
        let last = mt.last().expect("epochs");
        finals.push(last.mean_reward);
        // Survivors average the full 300 steps.
        if last.mean_reward > 290.0 {
            survived += 1;
        }
        let per_step = last.extra.get("elite_mean_reward_per_step").copied().unwrap_or(f64::NAN);
        if seed == 0 {
            per_step0 = per_step;
        }
    }
    let text = format!(
        "+1 per step alive, done at x > 0.5: {survived} of {seeds} seeds end with every env surviving; \
         last mean_reward per seed {:?} (seed 0 elite reward per step {:.2})",
        finals.iter().map(|r| r.round()).collect::<Vec<_>>(),
        per_step0
    );
    if survived < seeds { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// A NaN reward ties with everything in the elite sort; with a big
/// population the sort can panic.
fn cem_nan_reward() -> Seen {
    let mut out = Vec::new();
    for n in [8, 32] {
        let m = arm();
        let mut v = VecEnv::builder(Arc::clone(&m), n)
            .observation_space(obs_q(&m))
            .action_space(act_ctrl(&m))
            .reward(|_, d| if d.ctrl[0] > 0.0 { f64::NAN } else { -d.qpos[0].abs() })
            .done(|_, _| false)
            .truncated(|_, d| d.time > 0.5)
            .build()
            .expect("vec");
        let mut cem = reach_cem(hp(0.25, 0.5, 60));
        let r = run(|| cem.train(&mut v, TrainingBudget::Epochs(3), 1, &|_| {}));
        out.push(match r {
            Ok(mt) => format!(
                "{n} envs: trains, mean_reward {:?}, best_reward {:?}",
                mt.iter().map(|e| e.mean_reward).collect::<Vec<_>>(),
                cem.checkpoint().best_reward
            ),
            Err(p) => format!("{n} envs: panics ({p})"),
        });
    }
    Seen::Open(out.join("; "))
}

/// A checkpoint holding a NaN saves fine and then won't load.
fn checkpoint_nan_round_trip() -> Seen {
    let cem = reach_cem(hp(0.2, f64::NAN, 100));
    let cp = cem.checkpoint();
    let path = std::env::temp_dir().join("gaps_ml_chassis_nan.ckpt.json");
    let saved = cp.save(&path);
    let loaded = TrainingCheckpoint::load(&path);
    let _ = std::fs::remove_file(&path);
    let text = format!(
        "noise_std NaN: save {}, load {}",
        match &saved {
            Ok(()) => "Ok".to_string(),
            Err(e) => format!("Err ({e})"),
        },
        match &loaded {
            Ok(_) => "Ok".to_string(),
            Err(e) => format!("Err ({e})"),
        }
    );
    if saved.is_ok() && loaded.is_err() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `Cem::from_checkpoint` doesn't check `algorithm_name`.
fn from_checkpoint_ignores_name() -> Seen {
    let h = hp(0.2, 0.3, 100);
    let mut cp = reach_cem(h).checkpoint();
    cp.algorithm_name = "PPO".into();
    match Cem::from_checkpoint(&cp, h) {
        Ok(_) => Seen::Open("a checkpoint named PPO loads into Cem".into()),
        Err(e) => Seen::Fixed(format!("refused: {e}")),
    }
}

// ── CEM's real job: a wash program ────────────────────────────────────────

/// Steps per wash episode (10 sub-steps of 0.01 each, so 10 s of bath).
const WASH_STEPS: usize = 100;

/// One sock in a tilted double well, starting in the shallow well at -1 (the
/// stain). The policy sets the drum temperature, 0 to 3 kT, each 0.1 s;
/// reward is +1 per step in the deep well (x > 0) minus 0.2 x the heat used.
fn wash_env(n: usize, seed: u64) -> VecEnv {
    ThermCircuitEnv::builder(1)
        .timestep(0.01)
        .gamma(1.0)
        .k_b_t(3.0)
        .seed(seed)
        .with(DoubleWellPotential::new(3.0, 1.0, 0))
        .with(ExternalField::new(vec![0.5]))
        .with_ctrl_temperature()
        .ctrl_range(0.0, 1.0)
        .sub_steps(10)
        .episode_steps(WASH_STEPS)
        .reward(|_, d| if d.qpos[0] > 0.0 { 1.0 } else { 0.0 } - 0.2 * d.ctrl[0])
        .on_reset(|_, d| d.qpos[0] = -1.0)
        .build_vec(n)
        .expect("wash env")
}

/// Mean episode reward of a linear wash program over `n` socks.
fn wash_score(params: &[f64], n: usize, seed: u64) -> f64 {
    let mut p = LinearPolicy::new(2, 1, &[1.0, 0.3]);
    p.set_params(params);
    let mut env = wash_env(n, seed);
    let ro = collect_episodic_rollout(&mut env, &mut |_, obs| p.forward(obs), WASH_STEPS);
    ro.trajectories.iter().map(|t| t.rewards.iter().sum::<f64>()).sum::<f64>() / n as f64
}

fn wash_cem() -> Cem {
    let h = CemHyperparams { elite_fraction: 0.2, noise_std: 1.0, noise_decay: 0.9, noise_min: 0.05, max_episode_steps: WASH_STEPS };
    Cem::new(Box::new(LinearPolicy::new(2, 1, &[1.0, 0.3])), h)
}

/// `best_artifact` stores the elite mean after the update, scored with the
/// reward of the population that came before it: those parameters were
/// never rolled out.
fn cem_best_never_evaluated() -> Seen {
    let mut cem = wash_cem();
    let m = cem.train(&mut wash_env(32, 1), TrainingBudget::Epochs(1), 3, &|_| {});
    let best = cem.best_artifact();
    let claimed = cem.checkpoint().best_reward.unwrap_or(f64::NAN);
    let start = wash_score(&[0.0, 0.0, 0.0], 256, 50);
    let actual = wash_score(&best.params, 256, 50);
    let text = format!(
        "after 1 epoch: best_reward {claimed:.1} (= epoch 0 mean_reward {:.1}); start params score {start:.1}, \
         the stored best params {:?} score {actual:.1}",
        m[0].mean_reward,
        best.params.iter().map(|p| (p * 100.0).round() / 100.0).collect::<Vec<_>>()
    );
    if best.params != [0.0, 0.0, 0.0] && claimed == m[0].mean_reward { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// The real job: CEM learns when to heat. Cold never frees the sock, always
/// hot frees it but keeps kicking it back; a learned program should heat
/// only while the sock is in the shallow well.
fn cem_learns_wash_program() -> Seen {
    let mut cem = wash_cem();
    let t0 = std::time::Instant::now();
    let m = cem.train(&mut wash_env(32, 1), TrainingBudget::Epochs(25), 7, &|_| {});
    let secs = t0.elapsed().as_secs_f64();
    let learned = cem.policy_artifact().params;
    let (cold, hot) = (wash_score(&[0.0, 0.0, -10.0], 256, 50), wash_score(&[0.0, 0.0, 10.0], 256, 50));
    let mut best_const = (f64::NEG_INFINITY, 0.0);
    for b in [-0.5, -0.2, 0.0, 0.2, 0.5, 1.0] {
        let s = wash_score(&[0.0, 0.0, b], 256, 50);
        if s > best_const.0 {
            best_const = (s, b);
        }
    }
    let score = wash_score(&learned, 256, 50);
    let text = format!(
        "{:.1} s, 25 epochs x 32 socks: mean_reward {:.1} -> {:.1}; learned temp = tanh({:.2} x + {:.2} v + {:.2}) \
         scores {score:.1} of {WASH_STEPS}, vs cold {cold:.1}, always hot {hot:.1}, best constant {:.1} (bias {})",
        secs,
        m[0].mean_reward,
        m.last().map_or(f64::NAN, |e| e.mean_reward),
        learned[0],
        learned[1] * 0.3,
        learned[2],
        best_const.0,
        best_const.1
    );
    if score > best_const.0 + 5.0 { Seen::Fine(text) } else { Seen::Open(text) }
}

/// Reads the locked version of `krate` from Cargo.lock.
fn locked_version(krate: &str) -> &'static str {
    let lock = include_str!("../Cargo.lock");
    let at = lock.find(&format!("name = \"{krate}\"\nversion = \"")).expect("crate in Cargo.lock");
    let rest = &lock[at..];
    let start = rest.find("version = \"").unwrap() + 11;
    let len = rest[start..].find('"').unwrap();
    &rest[start..start + len]
}

fn main() {
    // Caught panics are reported by the checks; keep the default hook quiet.
    std::panic::set_hook(Box::new(|_| {}));
    let checks: Vec<Check> = vec![
        ("VecEnv: sensors one step stale", vec_env_sensors_stale),
        ("SimEnv: passive callback runs twice per step", sim_env_passive_twice),
        ("SimEnv and VecEnv agree (Euler)", sim_and_vec_agree_euler),
        ("SimEnv and VecEnv differ (implicitspringdamper)", sim_and_vec_differ_implicit),
        ("SimEnv hides divergence", sim_env_hides_divergence),
        ("VecEnv: divergence is a plain done", vec_env_divergence_is_plain_done),
        ("VecEnv: divergence seen one step late", vec_env_divergence_one_step_late),
        ("SimEnv: action length unchecked", sim_env_action_length_unchecked),
        ("reversed obs range builds", reversed_obs_range),
        ("overlapping action injectors", overlapping_action_injectors),
        ("done and truncated disagree", done_and_truncated_disagree),
        ("VecEnv keeps the terminal obs", vec_env_keeps_terminal_obs),
        ("mocap injectors", mocap_injectors),
        ("energy obs needs the flag", energy_obs_needs_flag),
        ("from_build_fn unchecked", from_build_fn_unchecked),
        ("rollout: action length", rollout_action_length),
        ("rollout: zero envs, zero steps", rollout_zero_cases),
        ("rollout steps finished envs", rollout_steps_finished_envs),
        ("Mlp starts at zero", mlp_zero_init),
        ("LinearPolicy: obs length", linear_policy_obs_length),
        ("LinearQ: obs and action misaligned", linear_q_misaligned),
        ("forward_batch drops a partial row", forward_batch_partial_row),
        ("sigma 0 gradient", sigma_zero_gradient),
        ("Tape: second backward", tape_backward_twice),
        ("Adam settings unchecked", adam_settings_unchecked),
        ("ReplayBuffer capacity 0", replay_buffer_zero_capacity),
        ("CEM: elite_fraction > 1", cem_elite_fraction_over_one),
        ("CEM: hyperparameters unchecked", cem_hyperparams_unchecked),
        ("CEM: Steps budget", cem_steps_budget),
        ("CEM: noise_std metric", cem_noise_metric_is_next_epochs),
        ("CEM: same seed replays", cem_same_seed_replays),
        ("CEM: per-step fitness", cem_per_step_fitness),
        ("CEM: NaN reward", cem_nan_reward),
        ("checkpoint: NaN round trip", checkpoint_nan_round_trip),
        ("from_checkpoint ignores the name", from_checkpoint_ignores_name),
        ("CEM: best never evaluated", cem_best_never_evaluated),
        ("CEM learns a wash program", cem_learns_wash_program),
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
    println!(
        "\n{open} open, {fixed} fixed, {fine} fine (cortenforge-sim-ml-chassis {}, cortenforge-sim-rl {})",
        locked_version("cortenforge-sim-ml-chassis"),
        locked_version("cortenforge-sim-rl")
    );
}
