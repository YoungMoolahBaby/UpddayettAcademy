//! Gap probe for `sim-core` and `sim-mjcf` 0.9.x: one check per finding, so
//! the same run tells us what 0.9.2 fixed.
//!
//! cargo run --release --example gaps_sim_core
//!
//! Each check prints OPEN (the gap is still there), FIXED (a known gap that
//! now behaves) or ok (never a gap; kept to catch regressions), with what
//! it saw. A panic inside a check is caught and reported, never fatal. The ids
//! match the entries in docs/FINDINGS.md.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cortenforge::sim::core::{Data, GeomType, Model};
use cortenforge::sim::mjcf::{MjcfError, load_model};

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

/// One body on a vertical slide joint `j`: `head` goes before the worldbody
/// (options), `joint` onto the joint, `tail` after the worldbody (actuators,
/// keyframes).
fn slide_xml(head: &str, joint: &str, tail: &str) -> String {
    format!(
        r#"<mujoco>{head}<worldbody><body name="b"><joint name="j" type="slide" axis="0 0 1" {joint}/><geom type="sphere" size="0.1" mass="1"/></body></worldbody>{tail}</mujoco>"#
    )
}

/// [`slide_xml`], loaded.
fn slide(head: &str, joint: &str, tail: &str) -> Model {
    load_model(&slide_xml(head, joint, tail)).expect("load")
}

/// Loads `xml`, reporting a panic or an error as text.
fn try_load(xml: &str) -> Result<Model, String> {
    match run(|| load_model(xml)) {
        Ok(Ok(m)) => Ok(m),
        Ok(Err(e)) => Err(format!("error: {e}")),
        Err(p) => Err(format!("PANIC: {p}")),
    }
}

/// Counts passive-callback calls on `model`.
fn count_passive(model: &mut Model) -> Arc<AtomicUsize> {
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    model.set_passive_callback(move |_, _| {
        c.fetch_add(1, Ordering::Relaxed);
    });
    n
}

// ---------------------------------------------------------------- sim-core

/// How often the passive callback runs per call. MuJoCo-style (RK4 runs it
/// once per stage), but nothing on `CbPassive` says so; a noise source in
/// it draws 4 times per RK4 step.
fn passive_callback_calls() -> Seen {
    let mut counts = Vec::new();
    for integ in ["Euler", "RK4", "implicitfast"] {
        let mut m = slide(&format!(r#"<option integrator="{integ}"/>"#), "", "");
        let n = count_passive(&mut m);
        let mut d = m.make_data();
        d.step(&m).expect("step");
        counts.push(format!("{integ} {}", n.swap(0, Ordering::Relaxed)));
        if integ == "Euler" {
            d.forward(&m).expect("forward");
            counts.push(format!("forward() {}", n.load(Ordering::Relaxed)));
        }
    }
    Seen::Fine(format!("calls per step: {}", counts.join(", ")))
}

/// With springs and dampers both disabled, the passive stage returns before
/// the callback (as in MuJoCo), so a thermostat in it goes silent.
fn passive_callback_off_with_springs() -> Seen {
    let mut m = slide(r#"<option><flag spring="disable" damper="disable"/></option>"#, "", "");
    let n = count_passive(&mut m);
    let mut d = m.make_data();
    for _ in 0..10 {
        d.step(&m).expect("step");
    }
    Seen::Fine(format!("{} callback calls in 10 steps", n.load(Ordering::Relaxed)))
}

/// `energy_initial` is documented as set on the first `forward()`, but only
/// `forward_skip` captures it.
fn energy_initial_never_set() -> Seen {
    let m = slide(r#"<option><flag energy="enable"/></option>"#, "", "");
    let mut d = m.make_data();
    d.qpos[0] = 1.0;
    d.forward(&m).expect("forward");
    for _ in 0..100 {
        d.step(&m).expect("step");
    }
    let (e0, e) = (d.energy_initial, d.total_energy());
    if e0 == 0.0 && e != 0.0 {
        Seen::Open(format!("energy_initial {e0} after forward + 100 steps, total energy {e:.3}"))
    } else {
        Seen::Fixed(format!("energy_initial {e0:.3}, total energy {e:.3}"))
    }
}

/// Velocity after 100 Euler steps from qvel 1, no gravity.
fn coast(m: &Model) -> f64 {
    let mut d = m.make_data();
    d.qvel[0] = 1.0;
    for _ in 0..100 {
        d.step(m).expect("step");
    }
    d.qvel[0]
}

/// Damping changed after load: hinge and slide read `jnt_damping`, so
/// MuJoCo's `dof_damping` does nothing, and the cached implicit damping
/// (Euler's eulerdamp) keeps the load-time value until
/// `compute_implicit_params()`.
fn damping_change_after_load() -> Seen {
    let head = r#"<option gravity="0 0 0"/>"#;
    let want = coast(&slide(head, r#"damping="5""#, ""));
    let mut dof = slide(head, "", "");
    dof.dof_damping[0] = 5.0;
    let mut jnt = slide(head, "", "");
    jnt.jnt_damping[0] = 5.0;
    let jnt_v = coast(&jnt);
    jnt.compute_implicit_params();
    let text = format!(
        "0 -> 5, qvel after 100 steps: dof_damping {:.4}, jnt_damping {jnt_v:.4}, + compute_implicit_params {:.4}; loaded with 5: {want:.4}",
        coast(&dof),
        coast(&jnt)
    );
    if (coast(&dof) - want).abs() > 1e-9 || (jnt_v - want).abs() > 1e-9 { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `reset_to_keyframe` doesn't clear what `reset` clears (warnings among
/// them), so a divergence stays flagged after it.
fn reset_to_keyframe_partial() -> Seen {
    let m = slide("", "", r#"<keyframe><key qpos="0.1"/></keyframe>"#);
    let mut d = m.make_data();
    d.qvel[0] = f64::NAN;
    d.step(&m).expect("step");
    d.reset_to_keyframe(&m, 0).expect("keyframe");
    let after_key = d.divergence_detected();
    d.reset(&m);
    let after_reset = d.divergence_detected();
    let text = format!("divergence_detected after reset_to_keyframe {after_key}, after reset {after_reset}");
    if after_key { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// Nothing ties a Data to its Model: stepping one with another model's Data
/// panics on an index (smaller Data) or runs on garbage (larger).
fn data_model_mismatch() -> Seen {
    let one = slide("", "", "");
    let two = load_model(
        r#"<mujoco><worldbody>
            <body><joint type="slide" axis="0 0 1"/><geom type="sphere" size="0.1" mass="1"/></body>
            <body pos="1 0 0"><joint type="slide" axis="0 0 1"/><geom type="sphere" size="0.1" mass="1"/></body>
        </worldbody></mujoco>"#,
    )
    .expect("load");
    let small = run(|| one.make_data().step(&two));
    let large = run(|| two.make_data().step(&one));
    let say = |r: &Result<Result<(), _>, String>| match r {
        Ok(Ok(())) => "Ok".to_string(),
        Ok(Err(e)) => format!("Err({e})"),
        Err(p) => format!("panic ({p})"),
    };
    let text = format!("1-dof Data on a 2-dof model: {}; 2-dof Data on a 1-dof model: {}", say(&small), say(&large));
    if matches!(small, Ok(Err(_))) && matches!(large, Ok(Err(_))) { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// A ctrlrange with lo > hi loads, then `f64::clamp` panics inside `step`.
fn ctrlrange_reversed() -> Seen {
    let xml = slide_xml("", "", r#"<actuator><motor joint="j" ctrllimited="true" ctrlrange="1 -1"/></actuator>"#);
    let m = match try_load(&xml) {
        Ok(m) => m,
        Err(e) => return Seen::Fixed(format!("load refuses it: {e}")),
    };
    match run(|| m.make_data().step(&m)) {
        Err(p) => Seen::Open(format!("loads, then step panics: {p}")),
        Ok(r) => Seen::Open(format!("loads and steps ({r:?})")),
    }
}

/// With `implicitspringdamper`, `forward()` writes qvel (when no constraint
/// is active), though `forward` is documented as not changing the state.
fn implicit_forward_moves_qvel() -> Seen {
    let m = slide(r#"<option integrator="implicitspringdamper" gravity="0 0 0"/>"#, r#"stiffness="100""#, "");
    let mut d = m.make_data();
    d.qpos[0] = 0.5;
    let mut seen = vec![d.qvel[0]];
    for _ in 0..2 {
        d.forward(&m).expect("forward");
        seen.push(d.qvel[0]);
    }
    let text = format!("qvel before, after 1 and 2 forward(): {:.4} {:.4} {:.4}", seen[0], seen[1], seen[2]);
    if seen.windows(2).any(|w| w[0] != w[1]) { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// A bad state (NaN or inf) makes `step` reset everything and return Ok;
/// only a `log::warn!` and `divergence_detected()` tell.
fn autoreset_returns_ok() -> Seen {
    let m = slide("", "", r#"<actuator><motor joint="j"/></actuator>"#);
    let mut d = m.make_data();
    for _ in 0..50 {
        d.step(&m).expect("step");
    }
    d.ctrl[0] = 0.3;
    d.qvel[0] = f64::INFINITY;
    let r = d.step(&m);
    let text = format!(
        "step -> {r:?}; time {:.3} (was 0.100), ctrl {}, divergence_detected {}",
        d.time,
        d.ctrl[0],
        d.divergence_detected()
    );
    if r.is_ok() { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// `step1` runs the control callback even with actuation disabled
/// (`forward` skips it).
fn step1_control_ignores_disable() -> Seen {
    let mut m = slide(r#"<option><flag actuation="disable"/></option>"#, "", r#"<actuator><motor joint="j"/></actuator>"#);
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    m.set_control_callback(move |_, _| {
        c.fetch_add(1, Ordering::Relaxed);
    });
    let mut d = m.make_data();
    d.forward(&m).expect("forward");
    let fwd = n.swap(0, Ordering::Relaxed);
    d.step1(&m).expect("step1");
    let s1 = n.load(Ordering::Relaxed);
    let text = format!("control callback calls with actuation disabled: forward() {fwd}, step1() {s1}");
    if s1 > fwd { Seen::Open(text) } else { Seen::Fixed(text) }
}

/// Two Datas stepped on threads sharing one `&Model` match a serial run.
fn threads_share_model() -> Seen {
    let m = slide("", "", "");
    let walk = |m: &Model, z: f64| {
        let mut d = m.make_data();
        d.qpos[0] = z;
        for _ in 0..500 {
            d.step(m).expect("step");
        }
        d.qpos[0]
    };
    let serial = [walk(&m, 0.0), walk(&m, 1.0)];
    let threaded = std::thread::scope(|s| {
        let a = s.spawn(|| walk(&m, 0.0));
        let b = s.spawn(|| walk(&m, 1.0));
        [a.join().expect("a"), b.join().expect("b")]
    });
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Model>();
    send_sync::<Data>();
    let text = format!("Model, Data: Send + Sync; threaded {threaded:?} vs serial {serial:?}");
    if threaded == serial { Seen::Fine(text) } else { Seen::Open(text) }
}

/// `Data` is `Clone` (FINDINGS once said it wasn't): a clone mid-run steps
/// on identically. (It drops `plugin_data`; no plugin here.)
fn data_clone_replays() -> Seen {
    let m = slide("", "", "");
    let mut d = m.make_data();
    d.qvel[0] = 2.0;
    for _ in 0..50 {
        d.step(&m).expect("step");
    }
    let mut c = d.clone();
    for _ in 0..50 {
        d.step(&m).expect("step");
        c.step(&m).expect("step");
    }
    let text = format!("original {:.6}, clone {:.6}", d.qpos[0], c.qpos[0]);
    if d.qpos == c.qpos && d.qvel == c.qvel { Seen::Fine(text) } else { Seen::Open(text) }
}

/// `reset` restores qpos0, zeroes qvel, ctrl and time.
fn reset_restores() -> Seen {
    let m = slide("", "", r#"<actuator><motor joint="j"/></actuator>"#);
    let mut d = m.make_data();
    d.ctrl[0] = 1.0;
    for _ in 0..100 {
        d.step(&m).expect("step");
    }
    d.reset(&m);
    let text = format!("qpos {} qvel {} ctrl {} time {}", d.qpos[0], d.qvel[0], d.ctrl[0], d.time);
    if d.qpos == m.qpos0 && d.qvel[0] == 0.0 && d.ctrl[0] == 0.0 && d.time == 0.0 { Seen::Fine(text) } else { Seen::Open(text) }
}

// ---------------------------------------------------------------- sim-mjcf

/// `inner` inside a worldbody.
fn world(inner: &str) -> String {
    format!("<mujoco><worldbody>{inner}</worldbody></mujoco>")
}

/// `inner` inside one body with hinge `j` (`jattr` on the joint), with
/// `head` before and `tail` after the worldbody.
fn hinge(head: &str, jattr: &str, inner: &str, tail: &str) -> String {
    format!(r#"<mujoco>{head}<worldbody><body name="b"><joint name="j" type="hinge" {jattr}/>{inner}</body></worldbody>{tail}</mujoco>"#)
}

/// One plain geom.
const BALL: &str = r#"<geom size="0.1"/>"#;

/// A bad input and what to look for: `Some(text)` if the model shows the gap.
type BadInput = (&'static str, String, fn(&Model) -> Option<String>);

/// Loads each case: OPEN if any loads and shows its gap; refusals and
/// correct models count as fixed.
fn cases(list: Vec<BadInput>) -> Seen {
    let (mut gaps, mut good) = (Vec::new(), Vec::new());
    for (label, xml, probe) in list {
        match try_load(&xml) {
            Err(e) => good.push(format!("{label}: refused ({e})")),
            Ok(m) => match run(|| probe(&m)) {
                Ok(Some(t)) => gaps.push(format!("{label}: {t}")),
                Ok(None) => good.push(format!("{label}: ok")),
                Err(p) => gaps.push(format!("{label}: panics ({p})")),
            },
        }
    }
    if gaps.is_empty() { Seen::Fixed(good.join("; ")) } else { Seen::Open(gaps.join("; ")) }
}

/// Steps `m` 10 times, reporting what happened.
fn step10(m: &Model) -> String {
    let mut d = m.make_data();
    let r = (0..10).try_for_each(|_| d.step(m));
    format!("step {r:?}, qpos {:?}", d.qpos.as_slice())
}

/// Typo'd attributes, elements and sub-elements load without a word.
fn typos_load_silently() -> Seen {
    cases(vec![
        ("<geom typ=\"box\">", hinge("", "", r#"<geom typ="box" size="1 1 1"/>"#, ""), |m| {
            (m.geom_type[0] != GeomType::Box).then(|| format!("a {:?}", m.geom_type[0]))
        }),
        ("<gemo>", hinge("", "", r#"<gemo size="0.1"/><geom size="0.1"/>"#, ""), |m| Some(format!("loads, ngeom {}", m.ngeom))),
        ("<intvelocity>", hinge("", "", BALL, r#"<actuator><intvelocity joint="j"/></actuator>"#), |m| {
            (m.nu == 0).then(|| "nu 0".into())
        }),
        ("<jointpositon>", hinge("", "", BALL, r#"<sensor><jointpositon joint="j"/></sensor>"#), |m| {
            (m.nsensor == 0).then(|| "nsensor 0".into())
        }),
        ("free joint in worldbody", world(r#"<joint type="free"/><body><geom size="0.1"/></body>"#), |m| {
            Some(format!("loads, njnt {}", m.njnt))
        }),
    ])
}

/// Values that don't parse, or keywords it doesn't know, fall back to the
/// default instead of erroring.
fn bad_values_fall_back() -> Seen {
    cases(vec![
        ("timestep=\"0.01s\"", hinge(r#"<option timestep="0.01s"/>"#, "", BALL, ""), |m| Some(format!("timestep {}", m.timestep))),
        ("damping=\"abc\"", hinge("", r#"damping="abc""#, BALL, ""), |m| Some(format!("damping {}", m.jnt_damping[0]))),
        ("mass=\" 1\"", hinge("", "", r#"<geom size="0.1" mass=" 1"/>"#, ""), |m| {
            (m.body_mass[1] != 1.0).then(|| format!("body mass {:.3}", m.body_mass[1]))
        }),
        ("integrator=\"RK45\"", hinge(r#"<option integrator="RK45"/>"#, "", BALL, ""), |m| Some(format!("runs {:?}", m.integrator))),
        ("flag gravity=\"off\"", hinge(r#"<option><flag gravity="off"/></option>"#, "", BALL, ""), |m| {
            (m.disableflags == 0).then(|| "gravity stays on".into())
        }),
        ("limited=\"1\"", hinge("", r#"limited="1" range="-30 30""#, BALL, ""), |m| (!m.jnt_limited[0]).then(|| "not limited".into())),
    ])
}

/// `size` in `<default><geom>` isn't inherited.
fn default_geom_size_ignored() -> Seen {
    cases(vec![("default size 0.05", hinge(r#"<default><geom type="sphere" size="0.05"/></default>"#, "", "<geom/>", ""), |m| {
        (m.geom_size[0].x != 0.05).then(|| format!("radius {}", m.geom_size[0].x))
    })])
}

/// A self-closing `<body/>` is dropped.
fn self_closing_body_dropped() -> Seen {
    cases(vec![("<body mocap/>", world(r#"<body name="t" mocap="true" pos="0 0 1"/>"#), |m| {
        (m.nbody == 1).then(|| format!("nbody {}, nmocap {}", m.nbody, m.nmocap))
    })])
}

/// A second `<compiler>` or `<option>` resets the first instead of merging.
fn second_block_resets() -> Seen {
    cases(vec![
        ("two <compiler>", hinge(r#"<compiler angle="radian"/><compiler autolimits="true"/>"#, r#"range="-1 1""#, BALL, ""), |m| {
            (m.jnt_range[0].1 != 1.0).then(|| format!("range {:.4?} (read as degrees)", m.jnt_range[0]))
        }),
        ("two <option>", hinge(r#"<option timestep="0.001"/><option><flag energy="enable"/></option>"#, "", BALL, ""), |m| {
            (m.timestep != 0.001).then(|| format!("timestep {}", m.timestep))
        }),
    ])
}

/// Limits without a range, or reversed ranges, load (MuJoCo errors).
fn limits_unchecked() -> Seen {
    cases(vec![
        ("limited, no range", hinge("", r#"limited="true""#, BALL, ""), |m| Some(format!("range {:.4?}", m.jnt_range[0]))),
        ("range=\"1 -1\"", hinge("", r#"range="1 -1""#, BALL, ""), |m| Some(format!("range {:.4?}", m.jnt_range[0]))),
        ("ctrllimited, no ctrlrange", hinge("", "", BALL, r#"<actuator><motor joint="j" ctrllimited="true"/></actuator>"#), |m| {
            Some(format!("ctrlrange {:?}", m.actuator_ctrlrange[0]))
        }),
    ])
}

/// Sizes, masses and inertias that can't be right load anyway.
fn mass_and_size_unchecked() -> Seen {
    cases(vec![
        ("no size", hinge("", "", r#"<geom type="sphere"/>"#, ""), |m| Some(format!("radius {}", m.geom_size[0].x))),
        ("size=\"-0.1\"", hinge("", "", r#"<geom type="sphere" size="-0.1"/>"#, ""), |m| Some(format!("body mass {:.4}", m.body_mass[1]))),
        ("mass=\"-1\"", hinge("", "", r#"<geom size="0.1" mass="-1"/>"#, ""), |m| Some(format!("body mass {}", m.body_mass[1]))),
        ("no geom (zero mass)", hinge("", "", r#"<site name="s"/>"#, ""), |m| Some(format!("mass {}, {}", m.body_mass[1], step10(m)))),
        ("diaginertia 0 0 0", hinge("", "", r#"<inertial pos="0 0 0" mass="1" diaginertia="0 0 0"/>"#, ""), |m| {
            Some(format!("inertia {:?}, {}", m.body_inertia[1].as_slice(), step10(m)))
        }),
        ("fullinertia not PD", hinge("", "", r#"<inertial pos="0 0 0" mass="1" fullinertia="1 1 1 2 0 0"/>"#, ""), |m| {
            Some(format!("inertia {:.3?}", m.body_inertia[1].as_slice()))
        }),
        ("<inertial> without mass", hinge("", "", r#"<inertial pos="0 0 0" diaginertia="1 1 1"/>"#, ""), |m| {
            Some(format!("mass {}", m.body_mass[1]))
        }),
    ])
}

/// Geometry attributes read wrong or dropped.
fn geometry_misread() -> Seen {
    cases(vec![
        ("plane size=\"5 5 0.1\"", world(r#"<geom type="plane" size="5 5 0.1"/>"#), |m| {
            (m.geom_size[0].x != 5.0).then(|| format!("size {:?}", m.geom_size[0].as_slice()))
        }),
        ("box fromto", hinge("", "", r#"<geom type="box" fromto="0 0 0 0 0 1" size="0.1 0.2"/>"#, ""), |m| {
            (m.geom_size[0].z != 0.5).then(|| format!("size {:?} (MuJoCo 0.1 0.2 0.5)", m.geom_size[0].as_slice()))
        }),
        ("5-value fromto", hinge("", "", r#"<geom type="capsule" fromto="0 0 0 0 0" size="0.05"/>"#, ""), |m| {
            Some(format!("capsule half-length {}", m.geom_size[0].y))
        }),
        ("4-value pos", hinge("", "", r#"<geom size="0.1" pos="1 2 3 4"/>"#, ""), |m| Some(format!("pos {:?}", m.geom_pos[0].as_slice()))),
        ("body xyaxes", world(r#"<body xyaxes="0 1 0 -1 0 0"><joint type="hinge"/><geom size="0.1"/></body>"#), |m| {
            (m.body_quat[1].angle() == 0.0).then(|| "identity orientation".into())
        }),
        ("quat=\"0 0 0 0\"", world(r#"<body quat="0 0 0 0"><joint type="hinge"/><geom size="0.1"/></body>"#), |m| {
            Some(format!("quat {:?}", m.body_quat[1].coords.as_slice()))
        }),
    ])
}

/// NaN and inf in places the validator doesn't look (each case runs in a
/// child process, since one of them never returns).
fn nan_list() -> Vec<BadInput> {
    vec![
        ("body pos nan", world(r#"<body pos="nan 0 0"><joint type="hinge"/><geom size="0.1"/></body>"#), |m| {
            Some(format!("pos {:?}", m.body_pos[1].as_slice()))
        }),
        ("gravity inf", hinge(r#"<option gravity="0 0 inf"/>"#, "", BALL, ""), |m| Some(format!("gravity {:?}", m.gravity.as_slice()))),
        ("geom mass nan", hinge("", "", r#"<geom size="0.1" mass="nan"/>"#, ""), |m| Some(format!("body mass {}", m.body_mass[1]))),
        ("joint axis nan", hinge("", r#"axis="nan 0 0""#, BALL, ""), |m| Some(format!("axis {:?}", m.jnt_axis[0].as_slice()))),
        ("geom density nan", hinge("", "", r#"<geom size="0.1" density="nan"/>"#, ""), |m| Some(format!("body mass {}", m.body_mass[1]))),
        ("fullinertia off-diagonal nan", hinge("", "", r#"<inertial pos="0 0 0" mass="1" fullinertia="1 1 1 nan 0 0"/>"#, ""), |m| {
            Some(format!("inertia {:?}", m.body_inertia[1].as_slice()))
        }),
    ]
}

/// Runs this probe again with `args`; its stdout, or how it failed. A child
/// still running after `secs` is killed and reported as hanging.
fn child(args: &[&str], secs: u64) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().expect("exe");
    let mut c = Command::new(exe).args(args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("child");
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = c.try_wait().expect("wait") {
            let out = std::io::read_to_string(c.stdout.take().expect("stdout")).unwrap_or_default();
            return if status.success() { Ok(out.trim().to_string()) } else { Err(format!("process died ({status})")) };
        }
        if start.elapsed().as_secs() >= secs {
            let _ = c.kill();
            return Err(format!("load_model hangs (killed after {secs} s)"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn nan_inf_load() -> Seen {
    let mut seen = Vec::new();
    for (k, (label, _, _)) in nan_list().into_iter().enumerate() {
        seen.push(match child(&["--nan", &k.to_string()], 20) {
            Ok(t) => t,
            Err(e) => format!("{label}: {e}"),
        });
    }
    let fixed = seen.iter().all(|t| t.starts_with("FIXED"));
    let text = seen.iter().map(|t| t.trim_start_matches("FIXED ").trim_start_matches("OPEN ")).collect::<Vec<_>>().join("; ");
    if fixed { Seen::Fixed(text) } else { Seen::Open(text) }
}

/// An undefined `class=` is ignored; MuJoCo's root class "main" isn't known.
fn undefined_class_ignored() -> Seen {
    cases(vec![
        ("class=\"nope\"", hinge("", "", r#"<geom class="nope" size="0.1"/>"#, ""), |_| Some("loads".into())),
        ("class=\"main\"", hinge(r#"<default><joint damping="3"/></default>"#, r#"class="main""#, BALL, ""), |m| {
            (m.jnt_damping[0] != 3.0).then(|| format!("damping {} (main default 3)", m.jnt_damping[0]))
        }),
    ])
}

/// An explicit value equal to the built-in default is overwritten by a class
/// default (sentinel compare).
fn explicit_value_overwritten() -> Seen {
    cases(vec![(
        "gear=\"1\" with class gear 50",
        hinge(
            r#"<default><default class="c"><motor gear="50"/></default></default>"#,
            "",
            BALL,
            r#"<actuator><motor class="c" joint="j" gear="1"/></actuator>"#,
        ),
        |m| (m.actuator_gear[0][0] != 1.0).then(|| format!("gear {}", m.actuator_gear[0][0])),
    )])
}

/// Two geoms with one name load; the name map keeps one.
fn duplicate_geom_names() -> Seen {
    cases(vec![("two geoms \"g\"", hinge("", "", r#"<geom name="g" size="0.1"/><geom name="g" size="0.2"/>"#, ""), |m| {
        Some(format!("loads, \"g\" -> geom {:?}", m.geom_name_to_id.get("g")))
    })])
}

/// Joints inside a `<frame>` are invisible to `validate()`, so an actuator
/// on one fails as undefined.
fn frame_joint_undefined() -> Seen {
    let xml = r#"<mujoco><worldbody><frame pos="0 0 1"><body><joint name="j" type="hinge"/><geom size="0.1"/></body></frame></worldbody><actuator><motor joint="j"/></actuator></mujoco>"#;
    match try_load(xml) {
        Ok(m) => Seen::Fixed(format!("loads, nu {}", m.nu)),
        Err(e) => Seen::Open(e),
    }
}

/// `load_model` refuses any string containing `<include`, even a comment.
fn include_in_comment() -> Seen {
    match try_load(&format!("<!-- <include file=\"x.xml\"/> -->{}", hinge("", "", BALL, ""))) {
        Ok(_) => Seen::Fixed("loads".into()),
        Err(e) => Seen::Open(e),
    }
}

/// Build-stage errors come back as `Unsupported(String)`, so the typed
/// variants (`DuplicateBody`, ...) can't be matched.
fn errors_flattened() -> Seen {
    let xml = world(r#"<body name="a"><geom size="0.1"/></body><body name="a"><geom size="0.1"/></body>"#);
    match load_model(&xml) {
        Ok(_) => Seen::Open("two bodies \"a\" load".into()),
        Err(MjcfError::DuplicateBody(n)) => Seen::Fixed(format!("DuplicateBody({n})")),
        Err(e) => Seen::Open(format!("{e:?}")),
    }
}

/// Errors carry no line number, and often no element or attribute.
fn errors_no_location() -> Seen {
    match load_model(&hinge("", "", "\n<geom size=\"0.1 x\"/>", "")) {
        Ok(_) => Seen::Open("loads".into()),
        Err(e) => {
            let t = e.to_string();
            if t.contains("line") || t.contains("size") { Seen::Fixed(t) } else { Seen::Open(format!("\"{t}\"")) }
        }
    }
}

/// `<mesh vertex=...>` without `face=` is refused (MuJoCo builds the convex
/// hull; the lib docs show vertex-only as embedded data).
fn mesh_vertex_only() -> Seen {
    let xml = hinge(r#"<asset><mesh name="m" vertex="0 0 0 1 0 0 0 1 0 0 0 1"/></asset>"#, "", r#"<geom type="mesh" mesh="m"/>"#, "");
    match try_load(&xml) {
        Ok(m) => Seen::Fixed(format!("loads, ngeom {}", m.ngeom)),
        Err(e) => Seen::Open(e),
    }
}

/// A chain of `depth` nested bodies.
fn nested_xml(depth: usize) -> String {
    let open = r#"<body pos="0 0 0.1"><joint type="hinge"/><geom size="0.01"/>"#.repeat(depth);
    world(&format!("{open}{}", "</body>".repeat(depth)))
}

/// Deep nesting recurses: a long chain overflows the stack and kills the
/// process (each depth runs in a child process here).
fn deep_nesting() -> Seen {
    let mut seen = Vec::new();
    let mut died = false;
    for depth in [100, 150, 1000] {
        let r = child(&["--nest", &depth.to_string()], 60);
        died |= r.is_err();
        seen.push(format!("{depth}: {}", r.unwrap_or_else(|e| e)));
    }
    if died { Seen::Open(seen.join(", ")) } else { Seen::Fixed(seen.join(", ")) }
}

/// Bad input that is refused, as it should be.
fn refused_as_it_should_be() -> Seen {
    let list: Vec<(&str, String)> = vec![
        ("joint type", world(r#"<body><joint type="hing"/><geom size="0.1"/></body>"#)),
        ("geom type", world(r#"<body><geom type="sphear" size="0.1"/></body>"#)),
        ("2-value pos", world(r#"<body pos="1 2"><geom size="0.1"/></body>"#)),
        ("non-numeric vector", hinge("", "", r#"<geom size="0.1" pos="1 a 3"/>"#, "")),
        ("duplicate joint", world(r#"<body><joint name="j"/><geom size="0.1"/></body><body><joint name="j"/><geom size="0.1"/></body>"#)),
        ("undefined actuator joint", hinge("", "", BALL, r#"<actuator><motor joint="nope"/></actuator>"#)),
        ("geom size nan", hinge("", "", r#"<geom size="nan"/>"#, "")),
        ("inertial mass 0", hinge("", "", r#"<inertial pos="0 0 0" mass="0" diaginertia="1 1 1"/>"#, "")),
        ("no <mujoco>", "<worldbody/>".into()),
    ];
    let loaded: Vec<&str> = list.iter().filter(|(_, x)| try_load(x).is_ok()).map(|(l, _)| *l).collect();
    if loaded.is_empty() {
        Seen::Fine(format!("{} kinds of bad input refused with an error", list.len()))
    } else {
        Seen::Open(format!("now loads: {}", loaded.join(", ")))
    }
}

/// A ball joint and a hinge on one body is valid MuJoCo; here
/// `validate_joint_layout` asserts inside `load_model`.
fn ball_then_hinge_panics() -> Seen {
    let xml = r#"<mujoco><worldbody><body><joint type="ball"/><joint type="hinge"/><geom type="sphere" size="0.1"/></body></worldbody></mujoco>"#;
    match try_load(xml) {
        Ok(m) => Seen::Fixed(format!("loads: nq {} nv {}", m.nq, m.nv)),
        Err(e) => Seen::Open(e),
    }
}

/// Reads the locked version of `krate` from Cargo.lock.
fn locked_version(krate: &str) -> &'static str {
    let lock = include_str!("../Cargo.lock");
    let at = lock.find(&format!("name = \"{krate}\"\n")).expect("crate in Cargo.lock");
    let rest = &lock[at..];
    let v = rest.split("version = \"").nth(1).and_then(|s| s.split('"').next()).expect("version");
    Box::leak(v.to_string().into_boxed_str())
}

fn main() {
    // Child process for `deep_nesting`: load one chain and report.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--nest") {
        let depth: usize = args[2].parse().expect("depth");
        match load_model(&nested_xml(depth)) {
            Ok(m) => println!("loads, nbody {}", m.nbody),
            Err(e) => println!("error: {e}"),
        }
        return;
    }
    // Child process for `nan_inf_load`: one case.
    if args.get(1).map(String::as_str) == Some("--nan") {
        std::panic::set_hook(Box::new(|_| {}));
        let k: usize = args[2].parse().expect("case");
        match cases(vec![nan_list().swap_remove(k)]) {
            Seen::Open(t) => println!("OPEN {t}"),
            Seen::Fixed(t) | Seen::Fine(t) => println!("FIXED {t}"),
        }
        return;
    }
    // Caught panics are reported by the checks; keep the default hook quiet.
    std::panic::set_hook(Box::new(|_| {}));
    let checks: Vec<Check> = vec![
        ("passive callback calls per step", passive_callback_calls),
        ("passive callback off with springs and dampers", passive_callback_off_with_springs),
        ("energy_initial never set", energy_initial_never_set),
        ("damping changed after load", damping_change_after_load),
        ("reset_to_keyframe is a partial reset", reset_to_keyframe_partial),
        ("Data stepped with another Model", data_model_mismatch),
        ("ctrlrange lo > hi", ctrlrange_reversed),
        ("implicitspringdamper: forward() moves qvel", implicit_forward_moves_qvel),
        ("bad state: step resets and returns Ok", autoreset_returns_ok),
        ("step1 runs control with actuation disabled", step1_control_ignores_disable),
        ("threads share one Model", threads_share_model),
        ("Data clone replays", data_clone_replays),
        ("reset restores the state", reset_restores),
        ("ball + hinge on one body", ball_then_hinge_panics),
        ("typos load silently", typos_load_silently),
        ("bad values fall back to defaults", bad_values_fall_back),
        ("default geom size ignored", default_geom_size_ignored),
        ("self-closing <body/> dropped", self_closing_body_dropped),
        ("second <compiler>/<option> resets the first", second_block_resets),
        ("limits and ranges unchecked", limits_unchecked),
        ("mass, size and inertia unchecked", mass_and_size_unchecked),
        ("geometry attributes misread", geometry_misread),
        ("NaN and inf load", nan_inf_load),
        ("undefined class ignored", undefined_class_ignored),
        ("explicit value overwritten by class default", explicit_value_overwritten),
        ("duplicate geom names", duplicate_geom_names),
        ("joint inside <frame> undefined", frame_joint_undefined),
        ("<include in a comment", include_in_comment),
        ("errors flattened to Unsupported", errors_flattened),
        ("errors have no location", errors_no_location),
        ("mesh with vertices only", mesh_vertex_only),
        ("deep nesting", deep_nesting),
        ("bad input refused", refused_as_it_should_be),
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
        "\n{open} open, {fixed} fixed, {fine} fine (cortenforge-sim-core {}, cortenforge-sim-mjcf {})",
        locked_version("cortenforge-sim-core"),
        locked_version("cortenforge-sim-mjcf")
    );
}
