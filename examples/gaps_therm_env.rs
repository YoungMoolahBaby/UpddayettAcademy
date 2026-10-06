//! Gap probe for `sim-therm-env` 0.9.x: one check per finding, so the same
//! run tells us what 0.9.2 fixed.
//!
//! cargo run --release --example gaps_therm_env
//!
//! Each check prints OPEN (the gap is still there), FIXED (a known gap that
//! now behaves) or ok (never a gap; kept to catch regressions), with what
//! it saw. A panic inside a check is caught and reported, never fatal. The ids
//! match the entries in docs/FINDINGS.md.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::ml_chassis::{Environment, Tensor, VecEnv};
use cortenforge::sim::therm_env::{ThermCircuitEnv, ThermCircuitEnvBuilder, generate_mjcf};
use cortenforge::sim::thermostat::{DoubleWellPotential, ExternalField, PassiveComponent};

/// A named check.
type Check = (&'static str, fn() -> Seen);

/// A named bad builder.
type Case = (&'static str, fn() -> ThermCircuitEnvBuilder);

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

/// The smallest builder that builds: `n` free particles in the bath.
fn builder(n: usize) -> ThermCircuitEnvBuilder {
    ThermCircuitEnv::builder(n).seed(7).reward(|_, _| 0.0)
}

/// Steps a single env `steps` times with a constant action; returns the final qpos.
fn walk(env: &mut ThermCircuitEnv, steps: usize, action: &[f32]) -> Vec<f64> {
    let act = Tensor::from_slice(action, &[action.len()]);
    for _ in 0..steps {
        env.step(&act).expect("step");
    }
    env.data().qpos.iter().copied().collect()
}

/// Steps a vec env `steps` times with zero actions; returns env `i`'s qpos.
fn walk_vec(v: &mut VecEnv, steps: usize, i: usize) -> Vec<f64> {
    let n = v.n_envs();
    let act = Tensor::zeros(&[n, 0]);
    v.reset_all().expect("reset");
    for _ in 0..steps {
        v.step(&act).expect("step");
    }
    v.batch().env(i).expect("env").qpos.iter().copied().collect()
}

/// `build_vec` installs one thermostat (one counter, traj_id 0) on the shared
/// model, so every env draws from one stream: env 0's path depends on how
/// many envs run beside it.
fn build_vec_shares_one_stream() -> Seen {
    let alone = walk_vec(&mut builder(1).build_vec(1).expect("build"), 100, 0);
    let paired = walk_vec(&mut builder(1).build_vec(2).expect("build"), 100, 0);
    if alone != paired {
        Seen::Open(format!(
            "env 0 after 100 steps: {:.4} in a batch of 1, {:.4} in a batch of 2",
            alone[0], paired[0]
        ))
    } else {
        Seen::Fixed(format!("env 0 matches across batch sizes ({:.4})", alone[0]))
    }
}

/// The builder only rejects NaN and infinity (the loader catches a zero or
/// negative timestep): negative damping or temperature, an inverted ctrl range
/// and zero-step episodes all build.
fn physics_params_unchecked() -> Seen {
    let cases: [Case; 6] = [
        ("timestep 0", || builder(1).timestep(0.0)),
        ("timestep -0.001", || builder(1).timestep(-0.001)),
        ("gamma -1", || builder(1).gamma(-1.0)),
        ("k_b_t -1", || builder(1).k_b_t(-1.0)),
        ("ctrl_range(5, 1)", || builder(1).with_ctrl_temperature().ctrl_range(5.0, 1.0)),
        ("episode_steps 0", || builder(1).episode_steps(0)),
    ];
    let mut built = Vec::new();
    for (name, make) in cases {
        let said = match run(|| make().build()) {
            Err(p) => format!("build panics ({})", p.lines().next().unwrap_or("")),
            Ok(Err(e)) => {
                let _ = e;
                continue;
            }
            Ok(Ok(mut env)) => {
                let act = vec![1.0_f32; env.action_space().dim()];
                match run(|| walk(&mut env, 100, &act)) {
                    Err(p) => format!("builds, then panics ({})", p.lines().next().unwrap_or("")),
                    Ok(q) if !q[0].is_finite() => "builds, qpos NaN".into(),
                    Ok(q) => format!("builds, qpos {:.3}", q[0]),
                }
            }
        };
        built.push(format!("{name}: {said}"));
    }
    if built.is_empty() {
        Seen::Fixed("every bad parameter is an Err".into())
    } else {
        Seen::Open(built.join("; "))
    }
}

/// Landscape components are never checked against `n_particles`: a well on a
/// missing particle or a short field builds, then panics on the first step.
fn landscape_not_checked() -> Seen {
    let well = run(|| builder(1).with(DoubleWellPotential::new(1.0, 1.0, 5)).build().map(|mut e| walk(&mut e, 1, &[])));
    let field = run(|| builder(3).with(ExternalField::new(vec![0.1; 2])).build().map(|mut e| walk(&mut e, 1, &[])));
    let say = |r: &Result<Result<Vec<f64>, _>, String>| match r {
        Err(p) => format!("builds, then panics ({})", p.lines().next().unwrap_or("")),
        Ok(Err(e)) => format!("Err ({e})"),
        Ok(Ok(_)) => "runs silently".into(),
    };
    let text = format!("DoubleWell on dof 5 of 1: {}; 2-long field on 3: {} (particle 2 gets no field)", say(&well), say(&field));
    if matches!((&well, &field), (Ok(Err(_)), Ok(Err(_)))) { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// `generate_mjcf` is public but puts actuator `i` on joint `x{i}`: more ctrl
/// slots than particles gives XML that doesn't load.
fn generate_mjcf_more_ctrls_than_particles() -> Seen {
    match load_model(&generate_mjcf(1, 2, 0.001, (0.0, 10.0))) {
        Err(e) => Seen::Open(format!("generate_mjcf(1 particle, 2 ctrls) doesn't load: {e}")),
        Ok(m) => Seen::Fixed(format!("loads with {} actuators", m.nu)),
    }
}

/// `generate_mjcf` writes any timestep into the XML; the loader rejects NaN,
/// 0 and negative ones.
fn generate_mjcf_bad_timestep() -> Seen {
    let mut loads = Vec::new();
    for h in [f64::NAN, 0.0, -0.01] {
        if let Ok(m) = load_model(&generate_mjcf(1, 0, h, (0.0, 10.0))) {
            loads.push(format!("{h} loads (timestep {})", m.timestep));
        }
    }
    if loads.is_empty() { Seen::Fine("NaN, 0 and -0.01 are rejected by the loader".into()) } else { Seen::Open(loads.join(", ")) }
}

/// The thermostat clamps the ctrl multiplier to [0, 10], but `ctrl_range`
/// takes any range and doesn't say so: ctrl 20 heats to 10 kT.
fn ctrl_temperature_clamped_at_10() -> Seen {
    let mut env = builder(1).with_ctrl_temperature().ctrl_range(0.0, 20.0).build().expect("build");
    walk(&mut env, 1, &[20.0]);
    let ctrl = env.data().ctrl[0];
    let t = env.effective_temperature();
    if t < ctrl * env.config_k_b_t() {
        Seen::Open(format!("ctrl {ctrl} gives effective_temperature {t} (clamped at 10x, ctrl_range allowed 20)"))
    } else {
        Seen::Fixed(format!("ctrl {ctrl} gives {t}"))
    }
}

/// With ctrl temperature on, ctrl[0] starts at 0 after build and reset, so the
/// bath is cold (pure damping) until the first action sets it.
fn ctrl_temperature_starts_cold() -> Seen {
    let mut env = builder(1).with_ctrl_temperature().build().expect("build");
    env.reset().expect("reset");
    let t = env.effective_temperature();
    if t == 0.0 {
        Seen::Open(format!("effective_temperature after reset is {t} (k_b_t {})", env.config_k_b_t()))
    } else {
        Seen::Fixed(format!("starts at {t}"))
    }
}

/// The default truncation compares accumulated float time (`d.time >
/// max_time`): the episode runs `episode_steps` or one more, by rounding.
fn truncation_off_by_one() -> Seen {
    let mut lengths = Vec::new();
    for h in [0.001, 0.002, 0.003, 0.005, 0.01, 0.25] {
        let mut env = builder(1).timestep(h).episode_steps(10).build().expect("build");
        env.reset().expect("reset");
        let act = Tensor::zeros(&[0]);
        let mut n = 0;
        while n < 100 {
            n += 1;
            if env.step(&act).expect("step").truncated {
                break;
            }
        }
        lengths.push((h, n));
    }
    let off: Vec<String> = lengths.iter().filter(|(_, n)| *n != 10).map(|(h, n)| format!("{n} at h={h}")).collect();
    if off.is_empty() {
        Seen::Fixed("10 steps at every timestep".into())
    } else {
        Seen::Open(format!("episode_steps(10) truncates after {} (10 elsewhere)", off.join(", ")))
    }
}

/// `build_vec`'s on_reset hook drops the env index the VecEnv passes, so
/// per-env domain randomization can't tell envs apart.
fn build_vec_on_reset_loses_index() -> Seen {
    use std::sync::Mutex;
    let seen = Arc::new(Mutex::new(0_usize));
    let s = Arc::clone(&seen);
    let mut v = builder(1)
        .on_reset(move |_m: &Model, d: &mut Data| {
            *s.lock().unwrap() += 1;
            d.qpos[0] = 0.0;
        })
        .build_vec(3)
        .expect("build");
    v.reset_all().expect("reset");
    let calls = *seen.lock().unwrap();
    // The hook type is FnMut(&Model, &mut Data): there is nothing to read the
    // index from, so this check can only confirm the hook runs once per env.
    Seen::Open(format!("hook ran {calls} times for 3 envs, with no env index (FnMut(&Model, &mut Data))"))
}

/// `build_vec` returns a bare `VecEnv`: no `effective_temperature`,
/// `n_particles` or `config_k_b_t`, unlike `build`.
fn build_vec_loses_accessors() -> Seen {
    let v = builder(2).with_ctrl_temperature().build_vec(2).expect("build");
    Seen::Open(format!(
        "VecEnv of {} envs: model has {} dofs, but no effective_temperature() or n_particles()",
        v.n_envs(),
        v.model().nv
    ))
}

/// Counts passive-force calls, to see how many noise draws a step takes.
struct Calls(Arc<AtomicUsize>);

impl PassiveComponent for Calls {
    fn apply(&self, _m: &Model, _d: &Data, _q: &mut DVector<f64>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// `SimEnv::step` calls `forward()` after stepping, which runs the passive
/// stack again: the thermostat draws noise twice per step and throws one away,
/// so `build()` and `build_vec(1)` walk different paths from the same seed.
fn build_and_build_vec_differ() -> Seen {
    let one = Arc::new(AtomicUsize::new(0));
    let vec = Arc::new(AtomicUsize::new(0));
    let mut env = builder(1).with(Calls(Arc::clone(&one))).build().expect("build");
    let mut v = builder(1).with(Calls(Arc::clone(&vec))).build_vec(1).expect("build");
    env.reset().expect("reset");
    v.reset_all().expect("reset");
    let (one0, vec0) = (one.load(Ordering::Relaxed), vec.load(Ordering::Relaxed));
    let single = walk(&mut env, 100, &[]);
    let act = Tensor::zeros(&[1, 0]);
    for _ in 0..100 {
        v.step(&act).expect("step");
    }
    let batch = v.batch().env(0).expect("env").qpos[0];
    let per = |c: &AtomicUsize, at: usize| (c.load(Ordering::Relaxed) - at) as f64 / 100.0;
    let text = format!(
        "same seed, 100 steps: build() qpos {:.4} ({} passive calls a step), build_vec(1) {:.4} ({} a step)",
        single[0],
        per(&one, one0),
        batch,
        per(&vec, vec0)
    );
    if single[0] == batch { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// Same seed, same steps: two envs built alike match exactly.
fn same_seed_replays() -> Seen {
    let a = walk(&mut builder(2).build().expect("build"), 200, &[]);
    let b = walk(&mut builder(2).build().expect("build"), 200, &[]);
    if a == b { Seen::Fine(format!("qpos {:.4}, {:.4} both times", a[0], a[1])) } else { Seen::Open(format!("{a:?} vs {b:?}")) }
}

/// The observation is [qpos.., qvel..] as f32 (undocumented, but stable).
fn observation_layout() -> Seen {
    let mut env = builder(2).build().expect("build");
    walk(&mut env, 50, &[]);
    let obs = env.observe();
    let d = env.data();
    let want: Vec<f32> = d.qpos.iter().chain(d.qvel.iter()).map(|&x| x as f32).collect();
    if obs.as_slice() == want.as_slice() {
        Seen::Fine(format!("shape {:?} = [qpos x2, qvel x2] as f32", obs.shape()))
    } else {
        Seen::Open(format!("{:?} vs {want:?}", obs.as_slice()))
    }
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
        ("build_vec shares one noise stream", build_vec_shares_one_stream),
        ("physics parameters unchecked", physics_params_unchecked),
        ("landscape not checked against particles", landscape_not_checked),
        ("generate_mjcf: more ctrls than particles", generate_mjcf_more_ctrls_than_particles),
        ("generate_mjcf: bad timestep", generate_mjcf_bad_timestep),
        ("ctrl temperature clamped at 10x", ctrl_temperature_clamped_at_10),
        ("ctrl temperature starts cold", ctrl_temperature_starts_cold),
        ("truncation off by one", truncation_off_by_one),
        ("build_vec on_reset loses the env index", build_vec_on_reset_loses_index),
        ("build_vec loses the accessors", build_vec_loses_accessors),
        ("build() and build_vec(1) differ", build_and_build_vec_differ),
        ("same seed replays", same_seed_replays),
        ("observation layout", observation_layout),
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
    println!("\n{open} open, {fixed} fixed, {fine} fine (cortenforge-sim-therm-env {})", locked_version("cortenforge-sim-therm-env"));
}
