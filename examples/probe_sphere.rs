//! Probe: the sphere (DESIGN "The sphere: everyone's own lane"). Everyone on
//! the street gets their own direction out from the middle, and grows their
//! wealth along it. Lanes never cross, but they spread apart: two people
//! an angle apart at radius r are 2 r sin(angle/2) apart, and a trade costs
//! each recipient `--cost` Goo per unit of that distance. Does the trade
//! machine still find trades as people grow?
//!
//! Run: cargo run --release --example probe_sphere -- [thomson|together|alone|friends] [options]
//!
//!   thomson    the placement itself: charges on ball joints, shaken by
//!              CortenForge's Langevin thermostat and cooled, vs the known
//!              Thomson-problem minimum energies (N = 5, 8, 12)
//!   together   everyone at radius r, swept: trades left, the best set,
//!              who still trades, and the machine's hit rate
//!   alone      everyone at radius 1, Upddayett alone at r
//!   friends    `together`, with trading partners pulled together while
//!              the street settles (a spring per pair, x what they want of
//!              each other)
//!   gap        distance as the wealth gap instead (user, 2026-10-13: don't
//!              grow too far away or fall too far behind): a trade costs
//!              `--cost` Goo per doubling between two people's net worth,
//!              past `--free` doublings. Upddayett's net worth is scaled
//!              1/16x..16x; everyone growing together costs nothing
//!
//!   --nights A..B   nights to average (default 1..10)
//!   --runs N        spins a board for the hit rate (default 24; 0 skips the machine)
//!   --cost C        Goo per unit of distance (default 1)
//!   --pull K        friends: spring per Goo wanted (default 0.05)
//!   --free F        gap: doublings that cost nothing (default 1, within 2x)
//!   --seed S        placement seed (default 1)

use std::time::Instant;

use cortenforge::sim::core::{mj_jac, DVector, Data, Model, Vector3};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::thermostat::{LangevinThermostat, PassiveComponent, PassiveStack};
use cortenforge_play::trade::{self, world::World, Anneal, Physics, TradeComputer};

/// The Thomson problem's minimum energies on the unit sphere, unit charges
/// (Wales et al., Cambridge Cluster Database).
const THOMSON: [(usize, f64); 3] = [(5, 6.474_691_495), (8, 19.675_287_861), (12, 49.165_253_058)];

/// Radii swept by `together` and `friends` (and Upddayett's in `alone`).
const RADII: [f64; 9] = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0];

#[derive(Clone)]
struct Opts {
    mode: String,
    nights: (u64, u64),
    runs: usize,
    cost: f64,
    pull: f64,
    free: f64,
    seed: u64,
}

fn parse() -> Opts {
    let mut o = Opts { mode: "all".into(), nights: (1, 10), runs: 24, cost: 1.0, pull: 0.05, free: 1.0, seed: 1 };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| panic!("{a} needs a value"))
        };
        match a {
            "--nights" => {
                let v = val();
                let (x, y) = v.split_once("..").expect("--nights A..B");
                o.nights = (x.parse().unwrap(), y.parse().unwrap());
            }
            "--runs" => o.runs = val().parse().unwrap(),
            "--cost" => o.cost = val().parse().unwrap(),
            "--pull" => o.pull = val().parse().unwrap(),
            "--free" => o.free = val().parse().unwrap(),
            "--seed" => o.seed = val().parse().unwrap(),
            m if !m.starts_with("--") => o.mode = m.into(),
            other => panic!("unknown option {other}"),
        }
        i += 1;
    }
    o
}

// ───────────────────────── the placement ─────────────────────────

/// Like charges on the sphere push apart (Coulomb, 1/d), and friends pull
/// together (`pull[i][j]`, a spring along the chord). Each person is a unit
/// mass at the end of a unit rod on a ball joint at the middle, so the
/// sphere is a real constraint, not a penalty; `mj_jac` turns each push on
/// the tip into joint torques.
struct Street {
    n: usize,
    pull: Vec<Vec<f64>>,
}

impl Street {
    fn tip(d: &Data, i: usize) -> Vector3<f64> {
        d.geom_xpos[i]
    }

    fn force(&self, d: &Data, i: usize) -> Vector3<f64> {
        let p = Self::tip(d, i);
        let mut f = Vector3::zeros();
        for j in 0..self.n {
            if j != i {
                let r = p - Self::tip(d, j);
                let d2 = r.norm_squared();
                f += r / (d2 * d2.sqrt()) - r * self.pull[i][j];
            }
        }
        f
    }
}

impl PassiveComponent for Street {
    fn apply(&self, model: &Model, data: &Data, qfrc: &mut DVector<f64>) {
        for i in 0..self.n {
            let (jacp, _) = mj_jac(model, data, i + 1, &Self::tip(data, i));
            *qfrc += jacp.transpose() * self.force(data, i);
        }
    }
}

/// Coulomb energy (the Thomson problem's) and the friends' spring energy.
fn energies(dirs: &[Vector3<f64>], pull: &[Vec<f64>]) -> (f64, f64) {
    let (mut e, mut s) = (0.0, 0.0);
    for i in 0..dirs.len() {
        for j in i + 1..dirs.len() {
            let d = (dirs[i] - dirs[j]).norm();
            e += 1.0 / d;
            s += 0.5 * pull[i][j] * d * d;
        }
    }
    (e, s)
}

fn xorshift(s: &mut u64) -> f64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    (*s >> 11) as f64 / (1u64 << 53) as f64
}

/// Settles `n` people on the unit sphere: random start, shaken hot by the
/// thermostat, cooled geometrically to nothing, then left to stop. Returns
/// each person's direction (their normal).
fn settle(n: usize, pull: Vec<Vec<f64>>, seed: u64) -> Vec<Vector3<f64>> {
    const DT: f64 = 0.002;
    const COOL: f64 = 150.0;
    const REST: f64 = 50.0;
    let mut xml = format!(
        r#"<mujoco model="street-sphere">
  <option timestep="{DT}" gravity="0 0 0" integrator="Euler"><flag contact="disable"/></option>
  <worldbody>"#
    );
    // The ball is big (size 0.5) on purpose: the thermostat's damping is
    // explicit, and a point-like mass has almost no inertia about its own
    // rod, so gamma dt / I runs past 2 and the street blows up to NaN
    // (FINDINGS sim-thermostat, "stiff damping diverges").
    for i in 0..n {
        xml += &format!(
            r#"
    <body name="p{i}"><joint name="b{i}" type="ball" damping="0"/><geom type="sphere" pos="1 0 0" size="0.5" mass="1"/></body>"#
        );
    }
    // The dial: a slide joint whose actuator slot carries the temperature.
    xml += r#"
    <body name="dial"><joint name="dial" type="slide" axis="1 0 0"/><geom type="sphere" size="0.02" mass="1"/></body>
  </worldbody>
  <actuator><general joint="dial" gainprm="0" biasprm="0 0 0" ctrllimited="true" ctrlrange="0 10"/></actuator>
</mujoco>"#;
    let mut model = load_model(&xml).expect("street model");
    let mut data = model.make_data();
    let nv = model.nv;
    assert_eq!(nv, 3 * n + 1, "three dofs a ball joint, one for the dial");
    PassiveStack::builder()
        .with(Street { n, pull })
        .with(LangevinThermostat::new(DVector::from_element(nv, 1.0), 1.0, seed, 0).with_ctrl_temperature(0))
        .build()
        .install(&mut model);
    // Random start: a random unit quaternion per person (Shoemake).
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for i in 0..n {
        let (u1, u2, u3) = (xorshift(&mut s), xorshift(&mut s), xorshift(&mut s));
        let tau = std::f64::consts::TAU;
        let (a, b) = ((1.0 - u1).sqrt(), u1.sqrt());
        let q = [a * (tau * u2).sin(), a * (tau * u2).cos(), b * (tau * u3).sin(), b * (tau * u3).cos()];
        data.qpos.as_mut_slice()[4 * i..4 * i + 4].copy_from_slice(&q);
    }
    data.forward(&model).expect("forward");
    let (hot, cold) = (0.5_f64, 1e-4_f64);
    let steps = ((COOL + REST) / DT) as usize;
    for k in 0..steps {
        let t = k as f64 * DT;
        data.ctrl[0] = if t < COOL { hot * (cold / hot).powf(t / COOL) } else { 0.0 };
        data.step(&model).expect("step");
    }
    data.forward(&model).expect("forward");
    (0..n).map(|i| Street::tip(&data, i)).collect()
}

fn section_thomson() {
    println!("\n== The placement: charges on ball joints vs the Thomson minimum ==");
    for (n, known) in THOMSON {
        let t0 = Instant::now();
        let mut worst: f64 = 0.0;
        let mut off_sphere: f64 = 0.0;
        for seed in 1..=4 {
            let dirs = settle(n, vec![vec![0.0; n]; n], seed);
            off_sphere = dirs.iter().fold(off_sphere, |m, d| m.max((d.norm() - 1.0).abs()));
            let (e, _) = energies(&dirs, &vec![vec![0.0; n]; n]);
            worst = worst.max((e - known) / known);
        }
        println!(
            "  N {n:>2}: known {known:.6}, worst of 4 seeds {:+.1e} relative, off the sphere {off_sphere:.1e}  ({:.1} s)",
            worst,
            t0.elapsed().as_secs_f64()
        );
    }
}

// ───────────────────────── the street on the sphere ─────────────────────────

/// What the people at `pos` pay to reach each other: `cost` Goo per unit
/// of distance.
fn reach(pos: &[Vector3<f64>], cost: f64) -> Vec<Vec<f64>> {
    pos.iter().map(|a| pos.iter().map(|b| cost * (a - b).norm()).collect()).collect()
}

/// How much `a` and `b` want each other's things on night 1 (a friends'
/// spring per Goo).
fn wanting(w: &World) -> Vec<Vec<f64>> {
    let n = w.npcs.len();
    let mut m = vec![vec![0.0; n]; n];
    for (id, it) in w.items.iter().enumerate() {
        for (k, wants) in w.value.iter().enumerate() {
            if k != it.owner && !it.gift {
                m[k][it.owner] += wants[id];
                m[it.owner][k] += wants[id];
            }
        }
    }
    m
}

/// One board: trade strips, the best set's Goo and Karma, how many people
/// trade in it, and the machine's hit rate (NaN when skipped).
fn board(w: World, runs: usize, seed: u64) -> [f64; 5] {
    let mut tc = TradeComputer::with_board(w, 5.0, 1.6, trade::MAX_BITS, trade::cycles::MAX_CHAIN);
    let physics = Physics::default();
    tc.delta_v = physics.delta_v;
    let best = tc.ground_state();
    let tally = tc.tally(best);
    let trades = tc.cycles.iter().filter(|c| !c.is_gift()).count();
    let mut traders = vec![false; tc.world.npcs.len()];
    for i in trade::qubo::chosen(best, tc.cycles.len()) {
        if !tc.cycles[i].is_gift() {
            for l in &tc.cycles[i].legs {
                traders[l.from] = true;
            }
        }
    }
    let hit = if runs == 0 || tc.cycles.is_empty() {
        f64::NAN
    } else {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let tc = &tc;
        let hits: usize = std::thread::scope(|s| {
            let hs: Vec<_> = (0..threads)
                .map(|t| {
                    s.spawn(move || {
                        (t..runs)
                            .step_by(threads)
                            .filter(|&r| {
                                let mut m = tc.machine(physics, seed + r as u64 * 7919).expect("machine");
                                let l = tc.spin(&mut m, &Anneal::default(), 1.0, 1.0, |_, _| {}).expect("spin");
                                tc.is_optimal(l.best_bits)
                            })
                            .count()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).sum()
        });
        hits as f64 / runs as f64
    };
    [trades as f64, tally.goo, tally.karma, traders.iter().filter(|&&t| t).count() as f64, hit]
}

/// Sweep `radii`: `place(r)` gives everyone's position for radius r.
fn sweep(o: &Opts, label: &str, place: impl Fn(f64) -> Vec<Vector3<f64>>) {
    let nights = o.nights.0..=o.nights.1;
    let count = nights.clone().count() as f64;
    println!("  {label}: {count} nights, {} Goo per unit of distance, {} spins a board", o.cost, o.runs);
    println!("  radius  trade strips  best Goo  Karma  people trading  hit rate   nearest  ({:>5})", "s");
    let t0 = Instant::now();
    for r in RADII {
        let pos = place(r);
        let mut sum = [0.0; 5];
        for night in nights.clone() {
            let mut w = trade::world::laundromat(night);
            w.set_reach(reach(&pos, o.cost));
            let b = board(w, o.runs, o.seed + night * 104_729);
            for (s, x) in sum.iter_mut().zip(b) {
                *s += x / count;
            }
        }
        let nearest = (0..pos.len())
            .flat_map(|i| (0..pos.len()).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| (pos[i] - pos[j]).norm())
            .fold(f64::INFINITY, f64::min);
        let hit = if sum[4].is_nan() { "    -".into() } else { format!("{:>4.0}%", 100.0 * sum[4]) };
        println!(
            "  {r:>6.1}  {:>12.1}  {:>8.1}  {:>5.1}  {:>14.1}  {hit:>8}  {nearest:>8.2}  ({:>5.0})",
            sum[0],
            sum[1],
            sum[2],
            sum[3],
            t0.elapsed().as_secs_f64()
        );
    }
}

fn print_neighbors(w: &World, dirs: &[Vector3<f64>]) {
    for (i, n) in w.npcs.iter().enumerate() {
        let mut near: Vec<(f64, usize)> = (0..dirs.len()).filter(|&j| j != i).map(|j| ((dirs[i] - dirs[j]).norm(), j)).collect();
        near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let names: Vec<String> = near.iter().take(3).map(|(d, j)| format!("{} {d:.2}", w.npcs[*j].name)).collect();
        println!("    {:<24} nearest: {}", n.name, names.join(", "));
    }
}

/// Each person's net worth tonight: what everything they hold is worth to
/// them, in Goo.
fn net_worth(w: &World) -> Vec<f64> {
    let mut nw = vec![0.0; w.npcs.len()];
    for (id, it) in w.items.iter().enumerate() {
        nw[it.owner] += w.value[it.owner][id];
    }
    nw
}

/// The gap between two net worths, in doublings (1 Goo floor, so nobody
/// is infinitely far behind).
fn doublings(a: f64, b: f64) -> f64 {
    ((a.max(1.0)) / (b.max(1.0))).log2().abs()
}

/// `gap`: Upddayett's net worth scaled from 1/16x to 16x; every trade leg
/// costs `--cost` Goo per doubling between the two people past `--free`.
fn section_gap(o: &Opts) {
    println!(
        "\n== The wealth gap: {} Goo a trade per doubling past {} free ==",
        o.cost, o.free
    );
    let w1 = trade::world::laundromat(o.nights.0);
    let nw = net_worth(&w1);
    let line: Vec<String> = w1.npcs.iter().zip(&nw).map(|(p, v)| format!("{} {v:.0}", p.name)).collect();
    println!("  net worth on night {}: {}", o.nights.0, line.join(", "));
    let upd = w1.find_npc("Upddayett").unwrap();
    let nights = o.nights.0..=o.nights.1;
    let count = nights.clone().count() as f64;
    println!("  Upddayett x  trade strips  best Goo  Karma  people trading  hit rate  his trades  ({:>5})", "s");
    let t0 = Instant::now();
    let scales = [1.0 / 16.0, 1.0 / 8.0, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
    // No cost at all, for comparison.
    for scale in std::iter::once(None).chain(scales.iter().map(|&s| Some(s))) {
        let mut sum = [0.0; 6];
        for night in nights.clone() {
            let mut w = trade::world::laundromat(night);
            if let Some(s) = scale {
                let mut nw = net_worth(&w);
                nw[upd] *= s;
                let n = nw.len();
                let reach = (0..n)
                    .map(|a| (0..n).map(|b| o.cost * (doublings(nw[a], nw[b]) - o.free).max(0.0)).collect())
                    .collect();
                w.set_reach(reach);
            }
            let his = trade::cycles::enumerate(&w, trade::MAX_LOOP).iter().filter(|c| c.legs.iter().any(|l| l.from == upd)).count();
            let b = board(w, o.runs, o.seed + night * 104_729);
            for (s, x) in sum.iter_mut().zip(b.iter().copied().chain([his as f64])) {
                *s += x / count;
            }
        }
        let label = scale.map_or("no cost".into(), |s| if s < 1.0 { format!("1/{:.0}", 1.0 / s) } else { format!("{s:.0}") });
        let hit = if sum[4].is_nan() { "    -".into() } else { format!("{:>4.0}%", 100.0 * sum[4]) };
        println!(
            "  {label:>11}  {:>12.1}  {:>8.1}  {:>5.1}  {:>14.1}  {hit:>8}  {:>10.1}  ({:>5.0})",
            sum[0],
            sum[1],
            sum[2],
            sum[3],
            sum[5],
            t0.elapsed().as_secs_f64()
        );
    }
}

fn main() {
    let o = parse();
    let street = trade::world::laundromat(1);
    let n = street.npcs.len();
    let all = o.mode == "all";
    if all || o.mode == "thomson" {
        section_thomson();
    }
    if all || o.mode == "gap" {
        section_gap(&o);
    }
    if all || o.mode == "together" {
        println!("\n== Everyone grows together ==");
        let dirs = settle(n, vec![vec![0.0; n]; n], o.seed);
        print_neighbors(&street, &dirs);
        sweep(&o, "everyone at radius r", |r| dirs.iter().map(|d| d * r).collect());
    }
    if all || o.mode == "alone" {
        println!("\n== Upddayett grows alone ==");
        let dirs = settle(n, vec![vec![0.0; n]; n], o.seed);
        let upd = street.find_npc("Upddayett").unwrap();
        sweep(&o, "everyone at 1, Upddayett at r", |r| {
            dirs.iter().enumerate().map(|(i, d)| if i == upd { d * r } else { *d }).collect()
        });
    }
    if all || o.mode == "friends" {
        println!("\n== Friends settle near friends (pull {} per Goo wanted) ==", o.pull);
        let pull: Vec<Vec<f64>> = wanting(&street).iter().map(|row| row.iter().map(|g| o.pull * g).collect()).collect();
        let dirs = settle(n, pull.clone(), o.seed);
        let (e, s) = energies(&dirs, &pull);
        println!("    push {e:.3} (alone it would be {:.3}), pull {s:.3}", THOMSON[1].1);
        print_neighbors(&street, &dirs);
        sweep(&o, "everyone at radius r", |r| dirs.iter().map(|d| d * r).collect());
    }
}
