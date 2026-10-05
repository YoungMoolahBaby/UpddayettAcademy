//! Probe: can cortenforge's thermodynamic crates power a "Spin Cycle"
//! probabilistic-computer game level?
//!
//! Run: cargo run --release --example probe_therm [section...]
//! Sections: kramers scale and invert multiply temp (default: all)

use std::time::Instant;

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::therm_env::generate_mjcf;
use cortenforge::sim::thermostat::ising::{exact_distribution, tv_distance};
use cortenforge::sim::thermostat::{
    DoubleWellPotential, ExternalField, GibbsSampler, LangevinThermostat, PairwiseCoupling,
    PassiveStack, WellState,
};

const THREADS: usize = 12;

/// N slide-joint particles of mass 1, no gravity/contact, Euler, timestep dt.
fn make_model(n: usize, dt: f64) -> Model {
    let xml = generate_mjcf(n, 0, dt, (0.0, 10.0));
    load_model(&xml).expect("mjcf load")
}

/// Ising-ish circuit description (spin units: x = ±x0 = ±1).
#[derive(Clone)]
struct Circuit {
    n: usize,
    edges: Vec<(usize, usize)>,
    j: Vec<f64>,
    h: Vec<f64>,
}

struct Phys {
    delta_v: f64,
    gamma: f64,
    k_b_t: f64,
    dt: f64,
}

fn build(c: &Circuit, p: &Phys, seed: u64) -> (Model, Data) {
    let mut model = make_model(c.n, p.dt);
    let mut data = model.make_data();
    let mut b = PassiveStack::builder();
    for i in 0..c.n {
        b = b.with(DoubleWellPotential::new(p.delta_v, 1.0, i));
    }
    if !c.edges.is_empty() {
        b = b.with(PairwiseCoupling::new(c.j.clone(), c.edges.clone()));
    }
    if c.h.iter().any(|&x| x != 0.0) {
        b = b.with(ExternalField::new(c.h.clone()));
    }
    b = b.with(LangevinThermostat::new(
        DVector::from_element(c.n, p.gamma),
        p.k_b_t,
        seed,
        0,
    ));
    b.build().install(&mut model);
    for i in 0..c.n {
        data.qpos[i] = if (seed >> (i % 60)) & 1 == 1 { 1.0 } else { -1.0 };
        data.qvel[i] = 0.0;
    }
    data.forward(&model).expect("forward");
    (model, data)
}

// ───────────────────────── 1. Kramers ─────────────────────────

fn kramers_case(delta_v: f64, gamma: f64, k_b_t: f64, dt: f64, steps_per_thread: usize) {
    let c = Circuit { n: 1, edges: vec![], j: vec![], h: vec![0.0] };
    let p = Phys { delta_v, gamma, k_b_t, dt };
    let t0 = Instant::now();
    let counts: Vec<usize> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|t| {
                let p = &p;
                let c = &c;
                s.spawn(move || {
                    let (model, mut data) = build(c, p, 1000 + t as u64);
                    for _ in 0..10_000 {
                        data.step(&model).unwrap();
                    }
                    let mut last = WellState::from_position(data.qpos[0], 0.5);
                    let mut n = 0usize;
                    for _ in 0..steps_per_thread {
                        data.step(&model).unwrap();
                        let cur = WellState::from_position(data.qpos[0], 0.5);
                        if cur.is_in_well() {
                            if last.is_in_well() && cur != last {
                                n += 1;
                            }
                            last = cur;
                        }
                    }
                    n
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = t0.elapsed().as_secs_f64();
    let total: usize = counts.iter().sum();
    let t_sim = THREADS as f64 * steps_per_thread as f64 * dt;
    let k_meas = total as f64 / t_sim;
    let well = DoubleWellPotential::new(delta_v, 1.0, 0);
    let k_kgh = well.kramers_rate(gamma, 1.0, k_b_t);
    let k_turn = well.kramers_rate_turnover(gamma, 1.0, k_b_t);
    let rel_err = 1.0 / (total as f64).sqrt();
    println!(
        "  dV={delta_v} gamma={gamma} kT={k_b_t} dt={dt}: switches={total} over T={t_sim:.0} \
         -> k_meas={k_meas:.5} (+-{:.1}%)  k_KGH={k_kgh:.5} (ratio {:.3})  k_turnover={k_turn:.5} \
         (ratio {:.3})  wall={wall:.1}s [{:.1} Msteps/s/thread]",
        100.0 * rel_err,
        k_meas / k_kgh,
        k_meas / k_turn,
        steps_per_thread as f64 / wall / 1e6
    );
}

fn section_kramers() {
    println!("\n== 1. Single bistable element vs Kramers ==");
    // crate's own Gate-A central parameters
    kramers_case(3.0, 10.0, 1.0, 0.001, 20_000_000);
    // underdamped / near-turnover, where the plain KGH formula should overestimate
    kramers_case(3.0, 1.0, 1.0, 0.001, 10_000_000);
    kramers_case(3.0, 0.2, 1.0, 0.001, 10_000_000);
    // coarser timestep (game-friendly): does it bias the rate?
    kramers_case(3.0, 10.0, 1.0, 0.01, 4_000_000);
    kramers_case(3.0, 1.0, 1.0, 0.01, 4_000_000);
}

// ───────────────────────── 2. Scaling ─────────────────────────

fn rand_circuit(n: usize, all_to_all: bool, seed: u64) -> Circuit {
    let mut s = seed;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s % 10_000) as f64 / 10_000.0 - 0.5
    };
    let mut edges = vec![];
    if all_to_all {
        for i in 0..n {
            for k in i + 1..n {
                edges.push((i, k));
            }
        }
    } else {
        // 2D-ish sparse: chain + skip-4 edges (degree ~4, like a p-bit circuit)
        for i in 0..n {
            if i + 1 < n {
                edges.push((i, i + 1));
            }
            if i + 4 < n {
                edges.push((i, i + 4));
            }
        }
    }
    let scale = if all_to_all { 1.0 / (n as f64).sqrt() } else { 0.5 };
    let j = edges.iter().map(|_| rnd() * scale).collect();
    let h = (0..n).map(|_| rnd() * 0.2).collect();
    Circuit { n, edges, j, h }
}

fn section_scale() {
    println!("\n== 2. Scaling: coupled array, single thread, steps/sec ==");
    println!("  N   topology  edges   steps/s     us/step  sim-time/wall-s @dt=0.001  @dt=0.01  (sim switches/elem/wall-s @dt=0.01, gamma=1)");
    let well = DoubleWellPotential::new(3.0, 1.0, 0);
    let k1 = well.kramers_rate_turnover(1.0, 1.0, 1.0);
    for &n in &[4usize, 8, 16, 32, 64, 128] {
        for &a2a in &[false, true] {
            let c = rand_circuit(n, a2a, 99 + n as u64);
            let p = Phys { delta_v: 3.0, gamma: 1.0, k_b_t: 1.0, dt: 0.001 };
            let (model, mut data) = build(&c, &p, 7);
            for _ in 0..2000 {
                data.step(&model).unwrap();
            }
            let steps = (4_000_000 / n).max(20_000);
            let t0 = Instant::now();
            for _ in 0..steps {
                data.step(&model).unwrap();
            }
            let w = t0.elapsed().as_secs_f64();
            let sps = steps as f64 / w;
            println!(
                "  {n:<4}{:<10}{:<8}{sps:<12.0}{:<9.2}{:<27.1}{:<10.1}{:.1}",
                if a2a { "all2all" } else { "sparse" },
                c.edges.len(),
                1e6 / sps,
                sps * 0.001,
                sps * 0.01,
                sps * 0.01 * k1
            );
        }
    }
}

// ───────────────────── 3. Invertible logic ─────────────────────

/// Penalty-function (QUBO) builder in 0/1 variables, converted to the crate's
/// Ising convention H = -sum J s s - sum h s with s = 2x - 1.
struct Qubo {
    n: usize,
    lin: Vec<f64>,
    quad: std::collections::BTreeMap<(usize, usize), f64>,
}
impl Qubo {
    fn new(n: usize) -> Self {
        Self { n, lin: vec![0.0; n], quad: Default::default() }
    }
    fn q(&mut self, i: usize, k: usize, v: f64) {
        if i == k {
            self.lin[i] += v;
        } else {
            *self.quad.entry((i.min(k), i.max(k))).or_default() += v;
        }
    }
    /// z = x AND y : xy - 2xz - 2yz + 3z
    fn and(&mut self, x: usize, y: usize, z: usize, w: f64) {
        self.q(x, y, w);
        self.q(x, z, -2.0 * w);
        self.q(y, z, -2.0 * w);
        self.lin[z] += 3.0 * w;
    }
    /// x + y = s + 2c (half adder): (x + y - s - 2c)^2
    fn half_adder(&mut self, x: usize, y: usize, s: usize, c: usize, w: f64) {
        let terms = [(x, 1.0), (y, 1.0), (s, -1.0), (c, -2.0)];
        for (a, (i, ci)) in terms.iter().enumerate() {
            self.lin[*i] += w * ci * ci;
            for (k, ck) in terms.iter().skip(a + 1) {
                self.q(*i, *k, 2.0 * w * ci * ck);
            }
        }
    }
    fn to_ising(&self, beta: f64) -> Circuit {
        let mut h: Vec<f64> = self.lin.iter().map(|a| -beta * a / 2.0).collect();
        let mut edges = vec![];
        let mut j = vec![];
        for (&(i, k), &b) in &self.quad {
            edges.push((i, k));
            j.push(-beta * b / 4.0);
            h[i] -= beta * b / 4.0;
            h[k] -= beta * b / 4.0;
        }
        Circuit { n: self.n, edges, j, h }
    }
}

/// Langevin-sampled configuration distribution (only snapshots where every
/// element is committed to a well). Runs THREADS independent trajectories.
fn langevin_dist(c: &Circuit, p: &Phys, sim_time_per_thread: f64, every: usize) -> (Vec<(u32, f64)>, f64, f64) {
    let steps = (sim_time_per_thread / p.dt) as usize;
    let t0 = Instant::now();
    let hists: Vec<(Vec<u64>, u64, u64)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|t| {
                s.spawn(move || {
                    let (model, mut data) = build(c, p, 0xC0FFEE + 7919 * t as u64);
                    let burn = (20.0 / p.dt) as usize;
                    for _ in 0..burn {
                        data.step(&model).unwrap();
                    }
                    let mut hist = vec![0u64; 1 << c.n];
                    let (mut taken, mut tried) = (0u64, 0u64);
                    for k in 0..steps {
                        data.step(&model).unwrap();
                        if k % every == 0 {
                            tried += 1;
                            let mut cfg = 0u32;
                            let mut ok = true;
                            for i in 0..c.n {
                                match WellState::from_position(data.qpos[i], 0.5) {
                                    WellState::Right => cfg |= 1 << i,
                                    WellState::Left => {}
                                    WellState::Barrier => {
                                        ok = false;
                                        break;
                                    }
                                }
                            }
                            if ok {
                                hist[cfg as usize] += 1;
                                taken += 1;
                            }
                        }
                    }
                    (hist, taken, tried)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = t0.elapsed().as_secs_f64();
    let mut tot = vec![0u64; 1 << c.n];
    let (mut taken, mut tried) = (0, 0);
    for (h, a, b) in hists {
        for (i, v) in h.iter().enumerate() {
            tot[i] += v;
        }
        taken += a;
        tried += b;
    }
    let sum: u64 = tot.iter().sum();
    let dist = tot.iter().enumerate().map(|(i, &v)| (i as u32, v as f64 / sum.max(1) as f64)).collect();
    (dist, wall, taken as f64 / tried as f64)
}

fn exact(c: &Circuit, kt: f64) -> Vec<(u32, f64)> {
    exact_distribution(c.n, &c.edges, &c.j, &c.h, kt)
}

fn gibbs(c: &Circuit, kt: f64, sweeps: usize) -> Vec<(u32, f64)> {
    let mut g = GibbsSampler::new(c.n, &c.edges, &c.j, &c.h, kt, 42);
    g.sample(1000, sweeps)
}

fn bits3(cfg: u32) -> String {
    // order A B C (bit0=A, bit1=B, bit2=C)
    format!("{}{}{}", cfg & 1, (cfg >> 1) & 1, (cfg >> 2) & 1)
}

fn print_and_table(label: &str, ex: &[(u32, f64)], gi: &[(u32, f64)], lv: &[(u32, f64)]) {
    println!("  {label}:  ABC   exact   gibbs   langevin");
    for cfg in 0..8u32 {
        let valid = ((cfg & 1) & ((cfg >> 1) & 1)) == ((cfg >> 2) & 1);
        println!(
            "         {} {}  {:.3}   {:.3}   {:.3}",
            bits3(cfg),
            if valid { "*" } else { " " },
            ex[cfg as usize].1,
            gi[cfg as usize].1,
            lv[cfg as usize].1
        );
    }
    println!(
        "         TV(gibbs,exact)={:.4}  TV(langevin,exact)={:.4}",
        tv_distance(gi, ex),
        tv_distance(lv, ex)
    );
}

fn and_gate(beta: f64) -> Circuit {
    let mut q = Qubo::new(3);
    q.and(0, 1, 2, 1.0);
    q.to_ising(beta)
}

fn section_invert() {
    println!("\n== 3. Invertible AND gate (A=s0, B=s1, C=s2; C = A AND B) ==");
    let base = and_gate(4.0);
    println!(
        "  Ising (beta=4, i.e. Camsari J=[-1,2,2], h=[1,1,-2]): edges={:?} J={:?} h={:?}",
        base.edges, base.j, base.h
    );
    for &(beta, dv) in &[(1.0, 3.0), (2.0, 3.0), (4.0, 6.0)] {
        let c = and_gate(beta);
        let p = Phys { delta_v: dv, gamma: 1.0, k_b_t: 1.0, dt: 0.005 };
        println!("\n  -- free-running, beta={beta} (valid-state gap = {beta} kT), barrier dV={dv}");
        let ex = exact(&c, p.k_b_t);
        let gi = gibbs(&c, p.k_b_t, 200_000);
        let (lv, wall, frac) = langevin_dist(&c, &p, 20_000.0, 20);
        print_and_table("free", &ex, &gi, &lv);
        println!("         langevin wall={wall:.1}s (12 thr x 20000 time units), in-well fraction={frac:.2}");

        for &(clamp, hc) in &[(1, 4.0), (0, -4.0)] {
            let mut cc = c.clone();
            cc.h[2] += hc;
            let ex = exact(&cc, p.k_b_t);
            let gi = gibbs(&cc, p.k_b_t, 200_000);
            let (lv, wall, _) = langevin_dist(&cc, &p, 10_000.0, 20);
            println!("  -- clamp C={clamp} via ExternalField h_C += {hc}");
            print_and_table(&format!("C={clamp}"), &ex, &gi, &lv);
            println!("         langevin wall={wall:.1}s");
        }
    }
}

// ─────────────────── 3b. Invertible 2x2 multiplier ───────────────────

fn section_multiply() {
    println!("\n== 3b. Invertible 2-bit x 2-bit multiplier (factoring) ==");
    // vars: a0 a1 b0 b1 | p0 p1 p2 p3 | q10 q01 q11 c1
    let (a0, a1, b0, b1, p0, p1, p2, p3, q10, q01, q11, c1) = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
    let mut q = Qubo::new(12);
    q.and(a0, b0, p0, 1.0);
    q.and(a1, b0, q10, 1.0);
    q.and(a0, b1, q01, 1.0);
    q.and(a1, b1, q11, 1.0);
    q.half_adder(q10, q01, p1, c1, 1.0);
    q.half_adder(q11, c1, p2, p3, 1.0);
    let decode = |cfg: u32| -> (u32, u32, u32) {
        let bit = |i: usize| (cfg >> i) & 1;
        (bit(a0) | bit(a1) << 1, bit(b0) | bit(b1) << 1, bit(p0) | bit(p1) << 1 | bit(p2) << 2 | bit(p3) << 3)
    };
    for &beta in &[2.0, 3.0] {
        let base = q.to_ising(beta);
        println!(
            "  beta={beta}: 12 spins, {} couplings, max|J|={:.2}, max|h|={:.2}",
            base.edges.len(),
            base.j.iter().fold(0.0f64, |m, x| m.max(x.abs())),
            base.h.iter().fold(0.0f64, |m, x| m.max(x.abs()))
        );
        for &target in &[6u32, 9, 4] {
            let mut c = base.clone();
            for (k, &pi) in [p0, p1, p2, p3].iter().enumerate() {
                c.h[pi] += if (target >> k) & 1 == 1 { 5.0 } else { -5.0 };
            }
            let ex = exact(&c, 1.0);
            let gi = gibbs(&c, 1.0, 200_000);
            let p = Phys { delta_v: 5.0, gamma: 1.0, k_b_t: 1.0, dt: 0.005 };
            let (lv, wall, frac) = langevin_dist(&c, &p, 20_000.0, 20);
            let summarize = |d: &[(u32, f64)]| {
                let mut agg = std::collections::BTreeMap::<(u32, u32, u32), f64>::new();
                for &(cfg, pr) in d {
                    *agg.entry(decode(cfg)).or_default() += pr;
                }
                let correct: f64 = agg.iter().filter(|((a, b, pp), _)| a * b == target && *pp == target).map(|(_, v)| v).sum();
                let mut v: Vec<_> = agg.into_iter().collect();
                v.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
                let top: Vec<String> = v.iter().take(4).map(|((a, b, pp), pr)| format!("{a}x{b}(p={pp}):{pr:.2}")).collect();
                (correct, top.join(" "))
            };
            let (ce, te) = summarize(&ex);
            let (cg, _) = summarize(&gi);
            let (cl, tl) = summarize(&lv);
            println!(
                "   clamp p={target}: P(valid factorization) exact={ce:.3} gibbs={cg:.3} langevin={cl:.3}  TV(lang,exact)={:.3}  wall={wall:.1}s in-well={frac:.2}",
                tv_distance(&lv, &ex)
            );
            println!("      exact top: {te}");
            println!("      langevin top: {tl}");
        }
    }
    // time-to-solution: how long until a single trajectory first hits a valid factorization of 6
    let mut c = q.to_ising(3.0);
    for (k, &pi) in [p0, p1, p2, p3].iter().enumerate() {
        c.h[pi] += if (6u32 >> k) & 1 == 1 { 5.0 } else { -5.0 };
    }
    let p = Phys { delta_v: 5.0, gamma: 1.0, k_b_t: 1.0, dt: 0.005 };
    let mut times = vec![];
    let t0 = Instant::now();
    for trial in 0..48u64 {
        let (model, mut data) = build(&c, &p, 31337 + trial * 101);
        let mut steps = 0usize;
        loop {
            data.step(&model).unwrap();
            steps += 1;
            let s = |i: usize| data.qpos[i] > 0.5;
            let a = s(a0) as u32 | (s(a1) as u32) << 1;
            let b = s(b0) as u32 | (s(b1) as u32) << 1;
            let all_in = (0..12).all(|i| data.qpos[i].abs() > 0.5);
            if all_in && a * b == 6 || steps > 2_000_000 {
                break;
            }
        }
        times.push(steps as f64 * p.dt);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "  time-to-first-correct-factorization of 6 (beta=3, 48 trials): median={:.1} p90={:.1} max={:.1} sim time units; total wall={:.2}s",
        times[24],
        times[43],
        times[47],
        t0.elapsed().as_secs_f64()
    );
}

// ───────────────────── 4. Temperature sweep ─────────────────────

fn section_temp() {
    println!("\n== 4. Temperature (spin-cycle speed) sweep: 8-spin ferromagnetic ring, J=0.5, dV=3 ==");
    let n = 8;
    let edges: Vec<(usize, usize)> = (0..n).map(|i| (i, (i + 1) % n)).collect();
    let c = Circuit { n, j: vec![0.5; n], edges, h: vec![0.0; n] };
    println!("  kT    flips/elem/time  |m| langevin  |m| exact-ising  P(all-aligned) lang / exact");
    for &kt in &[0.3, 0.5, 0.75, 1.0, 1.5, 2.5, 5.0] {
        let p = Phys { delta_v: 3.0, gamma: 1.0, k_b_t: kt, dt: 0.005 };
        let (model, mut data) = build(&c, &p, 5);
        let steps = 400_000usize;
        let mut last: Vec<WellState> = (0..n).map(|i| WellState::from_position(data.qpos[i], 0.5)).collect();
        let mut flips = 0usize;
        let mut m_acc = 0.0;
        let mut m_n = 0usize;
        let mut aligned = 0usize;
        for _ in 0..steps {
            data.step(&model).unwrap();
            let mut all_in = true;
            let mut m = 0.0;
            for i in 0..n {
                let cur = WellState::from_position(data.qpos[i], 0.5);
                if cur.is_in_well() {
                    if last[i].is_in_well() && cur != last[i] {
                        flips += 1;
                    }
                    last[i] = cur;
                    m += cur.spin();
                } else {
                    all_in = false;
                }
            }
            if all_in {
                m_acc += (m / n as f64).abs();
                m_n += 1;
                if (m.abs() - n as f64).abs() < 0.1 {
                    aligned += 1;
                }
            }
        }
        let ex = exact(&c, kt);
        let m_ex: f64 = ex
            .iter()
            .map(|&(cfg, pr)| pr * ((2.0 * cfg.count_ones() as f64 - n as f64) / n as f64).abs())
            .sum();
        let al_ex = ex[0].1 + ex[(1 << n) - 1].1;
        println!(
            "  {kt:<6}{:<17.4}{:<13.3}{:<15.3}{:.3} / {:.3}",
            flips as f64 / n as f64 / (steps as f64 * p.dt),
            m_acc / m_n.max(1) as f64,
            m_ex,
            aligned as f64 / m_n.max(1) as f64,
            al_ex
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let want = |s: &str| args.is_empty() || args.iter().any(|a| a == s);
    let t0 = Instant::now();
    if want("scale") {
        section_scale();
    }
    if want("kramers") {
        section_kramers();
    }
    if want("invert") {
        section_invert();
    }
    if want("multiply") {
        section_multiply();
    }
    if want("temp") {
        section_temp();
    }
    println!("\ntotal wall: {:.1}s", t0.elapsed().as_secs_f64());
}
