//! Feasibility probe for the drum tumbler (PLAN Backlog item 1).
//!
//! The drum is a `cf_design` Solid (a closed can with three lifter paddles),
//! hinged to the world on a horizontal axis and spun at a prescribed rate; a
//! handful of trade items (phone, Goo can, kale, battery, hub) ride inside as
//! free rigid bodies. `Mechanism::to_model` turns it into a sim-core Model with
//! SDF collision geoms, so every contact is SDF vs SDF (the drum is concave).
//!
//! Modes (`cargo run --release --example probe_drum -- <mode>`):
//! - `drop`   : drum still, items fall in and settle (penetration, rest)
//! - `tumble` : 0.6x critical speed, items get lifted and fall (escapes, depth)
//! - `spin`   : 2x critical speed, items pin to the wall
//! - `res`    : SDF cell size sweep (build time, step speed) at tumble speed
//! - `all`    : drop + tumble + spin
//!
//! Units are mm, kg, s (cf_design's convention: gravity 9810 mm/s^2, dt 0.5 ms).

use cortenforge::cf_design::{JointDef, JointKind, Material, Mechanism, Part, Solid};
use cortenforge::sim::core::{Data, Model};
use nalgebra::{Point3, UnitQuaternion, Vector3};
use std::time::Instant;

/// Drum inner radius and inner half-depth, mm.
const R: f64 = 250.0;
const HALF_DEPTH: f64 = 150.0;
const WALL: f64 = 10.0;

/// The speed at which an item on the wall stops falling off: w^2 R = g.
fn critical() -> f64 {
    (9810.0 / R).sqrt()
}

/// A closed can (axis z) with three lifter paddles, rotated so the axis is y.
fn drum() -> Solid {
    let can = Solid::cylinder(R + WALL, HALF_DEPTH + WALL)
        .subtract(Solid::cylinder(R, HALF_DEPTH));
    let mut s = can;
    for k in 0..3 {
        let a = k as f64 * std::f64::consts::TAU / 3.0;
        // 50 mm tall radially, 24 mm thick, full depth, sunk 5 mm into the wall.
        let paddle = Solid::cuboid(Vector3::new(25.0, 12.0, HALF_DEPTH))
            .translate(Vector3::new(R - 20.0, 0.0, 0.0))
            .rotate(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), a));
        s = s.union(paddle);
    }
    s.rotate(UnitQuaternion::from_axis_angle(
        &Vector3::x_axis(),
        std::f64::consts::FRAC_PI_2,
    ))
}

/// (name, solid, density kg/m^3)
fn items() -> Vec<(&'static str, Solid, f64)> {
    vec![
        ("phone", Solid::cuboid(Vector3::new(36.0, 75.0, 5.0)), 2000.0),
        ("goo_can", Solid::cylinder(33.0, 61.0), 1050.0),
        ("kale", Solid::sphere(55.0), 300.0),
        ("battery", Solid::capsule(9.0, 24.0), 2700.0),
        ("hub", Solid::cuboid(Vector3::new(40.0, 25.0, 12.0)), 1400.0),
        ("goo_can_2", Solid::cylinder(33.0, 61.0), 1050.0),
    ]
}

struct Rig {
    model: Model,
    data: Data,
    drum_dof: usize,
    item_bodies: Vec<(String, usize)>,
}

fn env(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn build(sdf_cell: f64, n_items: usize) -> Rig {
    let sdf_cell = env("CELL", sdf_cell);
    let n_items = env("N", n_items as f64) as usize;
    let steel = Material::new("steel", 7800.0);
    let mut b = Mechanism::builder("drum_tumbler")
        .part(Part::new("drum", drum(), steel))
        .joint(JointDef::new(
            "drum_axle",
            "world",
            "drum",
            JointKind::Revolute,
            Point3::origin(),
            Vector3::y(),
        ));
    let all = items();
    for (i, (name, solid, rho)) in all.into_iter().take(n_items).enumerate() {
        // A 3 x 2 grid clear of the walls, the paddles and each other.
        let at = Point3::new(
            -110.0 + 110.0 * (i % 3) as f64,
            if i % 2 == 0 { -45.0 } else { 45.0 },
            if i < 3 { 60.0 } else { -60.0 },
        );
        b = b.part(Part::new(name, solid, Material::new(name, rho))).joint(JointDef::new(
            format!("{name}_free"),
            "world",
            name,
            JointKind::Free,
            at,
            Vector3::z(),
        ));
    }
    let mech = b.build();
    let t = Instant::now();
    let mut model = mech.to_model(sdf_cell, 8.0).expect("to_model");
    model.timestep = env("DT", model.timestep);
    model.sdf_maxcontact = env("MAXCON", model.sdf_maxcontact as f64) as usize;
    println!(
        "  built in {:.2} s (sdf cell {sdf_cell} mm): {} bodies, {} geoms, nv {}",
        t.elapsed().as_secs_f64(),
        model.nbody,
        model.ngeom,
        model.nv
    );
    let drum_body = body_named(&model, "drum");
    let drum_dof = model.body_dof_adr[drum_body];
    let item_bodies = (1..model.nbody)
        .filter(|&b| b != drum_body)
        .map(|b| (model.body_name[b].clone().unwrap_or_default(), b))
        .collect();
    let data = model.make_data();
    Rig { model, data, drum_dof, item_bodies }
}

fn body_named(m: &Model, name: &str) -> usize {
    (0..m.nbody)
        .find(|&b| m.body_name[b].as_deref() == Some(name))
        .expect("body")
}

#[derive(Default)]
struct Stats {
    steps: usize,
    max_depth: f64,
    max_ncon: usize,
    escapes: usize,
    max_speed: f64,
    wall_s: f64,
    lifted: usize,
}

/// Run `secs` of sim time at drum rate `omega` (rad/s), prescribed each step.
fn run(rig: &mut Rig, omega: f64, secs: f64) -> Stats {
    let mut st = Stats::default();
    let n = (secs / rig.model.timestep).round() as usize;
    let mut was_high = vec![false; rig.item_bodies.len()];
    let t = Instant::now();
    for _ in 0..n {
        rig.data.qvel[rig.drum_dof] = omega;
        rig.data.step(&rig.model).expect("step");
        st.steps += 1;
        st.max_ncon = st.max_ncon.max(rig.data.ncon);
        for c in &rig.data.contacts[..rig.data.ncon] {
            st.max_depth = st.max_depth.max(c.depth);
        }
        for (k, (_, b)) in rig.item_bodies.iter().enumerate() {
            let p = rig.data.xpos[*b];
            let r = (p.x * p.x + p.z * p.z).sqrt();
            if r > R + WALL || p.y.abs() > HALF_DEPTH + WALL {
                st.escapes += 1;
            }
            // "Lifted": the item got carried above the axle.
            let high = p.z > 50.0;
            if high && !was_high[k] {
                st.lifted += 1;
            }
            was_high[k] = high;
            let dof = rig.model.body_dof_adr[*b];
            let v = Vector3::new(
                rig.data.qvel[dof],
                rig.data.qvel[dof + 1],
                rig.data.qvel[dof + 2],
            );
            st.max_speed = st.max_speed.max(v.norm());
        }
    }
    st.wall_s = t.elapsed().as_secs_f64();
    st
}

fn report(label: &str, secs: f64, st: &Stats, rig: &Rig) {
    println!(
        "  {label}: {} steps in {:.2} s wall = {:.2}x real time ({:.0} us/step); \
         max contacts {}, deepest {:.2} mm, max item speed {:.0} mm/s, \
         escape-steps {}, lifts over the axle {}",
        st.steps,
        st.wall_s,
        secs / st.wall_s,
        st.wall_s / st.steps as f64 * 1e6,
        st.max_ncon,
        st.max_depth,
        st.max_speed,
        st.escapes,
        st.lifted
    );
    for (name, b) in &rig.item_bodies {
        let p = rig.data.xpos[*b];
        let r = (p.x * p.x + p.z * p.z).sqrt();
        let dof = rig.model.body_dof_adr[*b];
        let v = Vector3::new(rig.data.qvel[dof], rig.data.qvel[dof + 1], rig.data.qvel[dof + 2]);
        println!("    {name:>9}: r {r:5.0} y {:5.0} z {:5.0}  speed {:6.1} mm/s", p.y, p.z, v.norm());
    }
}

fn drop() {
    println!("== drop: drum still, 6 items fall in and settle (2 s)");
    let mut rig = build(5.0, 6);
    let st = run(&mut rig, 0.0, 2.0);
    report("drop", 2.0, &st, &rig);
    let st = run(&mut rig, 0.0, 0.5);
    println!(
        "  after settling, max item speed over the next 0.5 s: {:.1} mm/s, deepest {:.2} mm",
        st.max_speed, st.max_depth
    );
}

fn tumble() {
    let w = 0.6 * critical();
    println!("== tumble: {:.2} rad/s ({:.0} rpm, 0.6x critical), 6 items, 6 s", w, w * 60.0 / std::f64::consts::TAU);
    let mut rig = build(5.0, 6);
    run(&mut rig, 0.0, 1.0);
    let st = run(&mut rig, w, 6.0);
    report("tumble", 6.0, &st, &rig);
}

fn spin() {
    let w = 2.0 * critical();
    println!("== spin: {:.2} rad/s ({:.0} rpm, 2x critical), 6 items, 4 s", w, w * 60.0 / std::f64::consts::TAU);
    let mut rig = build(5.0, 6);
    run(&mut rig, 0.0, 1.0);
    // Ramp up over 1 s like a real machine.
    for k in 1..=10 {
        run(&mut rig, w * k as f64 / 10.0, 0.1);
    }
    let st = run(&mut rig, w, 3.0);
    report("spin", 3.0, &st, &rig);
}

fn res() {
    let w = 0.6 * critical();
    println!("== res: SDF cell sweep at tumble speed (2 s each)");
    for cell in [10.0, 5.0, 2.5] {
        let mut rig = build(cell, 6);
        run(&mut rig, 0.0, 1.0);
        let st = run(&mut rig, w, 2.0);
        println!(
            "  cell {cell:4} mm: {:.2}x real time, {:.0} us/step, deepest {:.2} mm, escapes {}",
            2.0 / st.wall_s,
            st.wall_s / st.steps as f64 * 1e6,
            st.max_depth,
            st.escapes
        );
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    match mode.as_str() {
        "drop" => drop(),
        "tumble" => tumble(),
        "spin" => spin(),
        "res" => res(),
        "info" => info(),
        "build" => build_time(),
        "all" => {
            drop();
            tumble();
            spin();
        }
        m => eprintln!("unknown mode {m}: drop | tumble | spin | res | all"),
    }
}

/// Which contact tier each part gets: sim-core takes the octree (Tier 2) only
/// when both shapes' intervals over their own bounds are tight enough.
fn info() {
    let mut parts: Vec<(&str, Solid)> = vec![("drum", drum())];
    parts.extend(items().into_iter().map(|(n, s, _)| (n, s)));
    for (name, s) in parts {
        let b = s.bounds().expect("bounds");
        let (lo, hi) = s.evaluate_interval(&b);
        let diag = b.diagonal();
        println!(
            "  {name:>9}: hint {:?}, interval [{lo:.0}, {hi:.0}] span {:.0} vs 10 x diagonal {:.0} -> {}",
            s.shape_hint(),
            hi - lo,
            10.0 * diag,
            if hi - lo > 10.0 * diag { "grid (Tier 3)" } else { "octree (Tier 2)" }
        );
    }
}

/// Where `to_model`'s build time goes, for the drum alone.
fn build_time() {
    let d = drum();
    let t = Instant::now();
    let g = d.sdf_grid_at(10.0);
    println!("  sdf grid at 10 mm: {:.2} s ({:?})", t.elapsed().as_secs_f64(), g.map(|g| (g.width(), g.height(), g.depth())));
    let t = Instant::now();
    let m = d.mesh(8.0);
    println!("  visual mesh at 8 mm: {:.2} s ({} faces)", t.elapsed().as_secs_f64(), m.geometry.faces.len());
    let b = d.bounds().expect("bounds");
    let n = (b.size().x * b.size().y * b.size().z) as u64;
    let t = Instant::now();
    let mut inside = 0u64;
    let s = 4.0;
    let (nx, ny, nz) = ((b.size().x / s) as i64, (b.size().y / s) as i64, (b.size().z / s) as i64);
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                let p = b.min + Vector3::new(i as f64 * s, j as f64 * s, k as f64 * s);
                if d.evaluate(&p) < 0.0 {
                    inside += 1;
                }
            }
        }
    }
    let per = t.elapsed().as_secs_f64() / (nx * ny * nz) as f64;
    println!(
        "  one evaluate: {:.0} ns, so a 1 mm mass grid ({} cells) costs ~{:.1} s ({inside} inside at 4 mm)",
        per * 1e9,
        n,
        per * n as f64
    );
}
