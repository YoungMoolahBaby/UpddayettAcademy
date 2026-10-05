//! Headless laundromat trade computer.
//!
//! cargo run --release --example trade_cli -- [run|bench|cycles] [options]
//!
//!   run              one spin cycle with a live readout (default)
//!   bench            many seeded spin cycles vs the exact answer
//!   cycles           list the candidate trades and the best set
//!   wants [--bench]  every want the picker can offer, its cost and (bench) hit rate
//!
//!   --want WHO:WHAT  backward mode, e.g. --want upd:hub
//!   --seed N  --runs N
//!   --beta B  --penalty P  --dv DV  --gamma G  --dt DT
//!   --hot T  --cold T  --time T  --settle T

use std::time::Instant;

use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::{self, Anneal, Latch, Machine, Physics, TradeComputer, qubo};

struct Opts {
    mode: String,
    bench: bool,
    margin: Option<f64>,
    want: Option<String>,
    seed: u64,
    runs: usize,
    beta: f64,
    penalty: f64,
    physics: Physics,
    anneal: Anneal,
}

fn parse() -> Opts {
    let mut o = Opts {
        mode: "run".into(),
        bench: false,
        margin: None,
        want: None,
        seed: 1,
        runs: 48,
        beta: 5.0,
        penalty: 1.6,
        physics: Physics::default(),
        anneal: Anneal::default(),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| panic!("{a} needs a value"))
        };
        let num = |s: String| s.parse::<f64>().unwrap_or_else(|_| panic!("bad number {s}"));
        match a {
            "--want" => o.want = Some(val()),
            "--bench" => o.bench = true,
            "--margin" => o.margin = Some(num(val())),
            "--seed" => o.seed = num(val()) as u64,
            "--runs" => o.runs = num(val()) as usize,
            "--beta" => o.beta = num(val()),
            "--penalty" => o.penalty = num(val()),
            "--dv" => o.physics.delta_v = num(val()),
            "--gamma" => o.physics.gamma = num(val()),
            "--dt" => o.physics.dt = num(val()),
            "--hot" => o.anneal.hot = num(val()),
            "--cold" => o.anneal.cold = num(val()),
            "--time" => o.anneal.duration = num(val()),
            "--settle" => o.anneal.settle = num(val()),
            m if !m.starts_with("--") => o.mode = m.to_string(),
            other => panic!("unknown option {other}"),
        }
        i += 1;
    }
    o
}

fn setup(o: &Opts) -> TradeComputer {
    let mut tc = TradeComputer::new(trade::world::laundromat_tuesday(), o.beta, o.penalty);
    if let Some(m) = o.margin {
        tc.want_margin = m;
    }
    if let Some(w) = &o.want {
        let (who, what) = w.split_once(':').expect("--want WHO:WHAT");
        let npc = tc.world.find_npc(who).unwrap_or_else(|| panic!("nobody called {who}"));
        let item = tc.world.find_item(what).unwrap_or_else(|| panic!("no item like {what}"));
        if !tc.want(npc, item) {
            panic!("no trade gets the {} to {}", tc.world.items[item].name, tc.world.npcs[npc].name);
        }
        println!("Backward mode: {} wants the {}.", tc.world.npcs[npc].name, tc.world.items[item].name);
    }
    tc
}

fn bitstring(m: &Machine) -> String {
    (0..m.n)
        .map(|i| match m.well(i) {
            WellState::Right => '#',
            WellState::Left => '.',
            WellState::Barrier => '~',
        })
        .collect()
}

fn mask_string(bits: u32, n: usize) -> String {
    (0..n).map(|i| if (bits >> i) & 1 == 1 { '#' } else { '.' }).collect()
}

fn print_cycles(tc: &TradeComputer) {
    println!("{} candidate trades (bit: value, loop):", tc.cycles.len());
    for (i, c) in tc.cycles.iter().enumerate() {
        let biased = if tc.problem.bias[i] > 0.0 { "  <- wanted" } else { "" };
        println!("  {i:>2}: {:>4.1} Goo  {}{biased}", c.value(), c.short(&tc.world));
    }
}

fn print_chain(tc: &TradeComputer, bits: u32) {
    let (value, clash) = tc.evaluate(bits);
    for i in qubo::chosen(bits, tc.cycles.len()) {
        let c = &tc.cycles[i];
        println!("  * {}-way (+{:.0} Goo): {}.", c.len(), c.value(), tc.cycles[i].describe(&tc.world));
    }
    println!(
        "  Total: everyone {:.0} Goo better off{}.",
        value,
        if clash { ", BUT two trades fight over the same item" } else { "" }
    );
}

fn report_problem(tc: &TradeComputer, o: &Opts) -> u32 {
    let n = tc.cycles.len();
    let ground = tc.ground_state();
    let (hmax, hsafe) = tc.field_headroom(&o.physics);
    let i = &tc.ising;
    let jmax = i.j.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    println!(
        "{n} bits, {} springs; beta={} dV={} -> idle field max {hmax:.2} (well flattens at {hsafe:.2}), max|J|={jmax:.2}",
        i.edges.len(),
        o.beta,
        o.physics.delta_v
    );
    match tc.exact_ground_state(o.physics.k_b_t * o.anneal.cold) {
        Some((g, p)) => println!(
            "Exact solver: ground state {} {} brute force; Boltzmann weight {:.0}% at the final temperature.",
            mask_string(g, n),
            if (tc.problem.qubo.energy(g) - tc.problem.qubo.energy(ground)).abs() < 1e-9 { "agrees with" } else { "DISAGREES with" },
            100.0 * p
        ),
        None => println!("Exact solver skipped ({n} bits > {}).", trade::MAX_EXACT_BITS),
    }
    ground
}

fn run(o: &Opts) -> Result<(), trade::Error> {
    let tc = setup(o);
    print_cycles(&tc);
    let ground = report_problem(&tc, o);
    let n = tc.cycles.len();

    println!("\nSPIN CYCLE (seed {}): # = trade on, . = off, ~ = strip mid-flip; i9 = best latched so far", o.seed);
    println!("  time   drum kT  strips{}  value  i9", " ".repeat(n.saturating_sub(6)));
    let mut m = tc.machine(o.physics, o.seed)?;
    let t0 = Instant::now();
    let every = o.anneal.total_time() / 30.0;
    let latch = tc.spin(&mut m, &o.anneal, SAMPLE, every, |m, l| {
        let (v, clash) = tc.evaluate(m.bits());
        let (bv, _) = tc.evaluate(l.best_bits);
        println!(
            "  {:>5.0}  {:>6.2}   {}  {:>4.0}{}  {:>3.0}",
            m.time(),
            o.physics.k_b_t * m.temperature(),
            bitstring(m),
            v,
            if clash { "!" } else { " " },
            bv
        );
    })?;
    let wall = t0.elapsed().as_secs_f64();

    let rest = latch.final_bits;
    println!("\nThe drum stops after {wall:.2} s wall. Strips at rest: {}", mask_string(rest, n));
    let defl: Vec<String> = m.positions().iter().map(|x| format!("{x:+.2}")).collect();
    println!("  deflections: {}", defl.join(" "));
    if rest != latch.best_bits {
        let (v, clash) = tc.evaluate(rest);
        println!("  ({v:.0} Goo{}; the i9 latched something better at t={:.0})", if clash { ", with a clash" } else { "" }, latch.best_time);
    }
    println!("\nThe i9 calls it: {}", mask_string(latch.best_bits, n));
    print_chain(&tc, latch.best_bits);
    if tc.is_optimal(latch.best_bits) {
        println!("That's the best possible set. \"It's not money laundering, it's a Boltzmann machine.\"");
    } else {
        println!("\nThe best set was {}:", mask_string(ground, n));
        print_chain(&tc, ground);
        println!("\"You spun it too fast, Daddy. The strips froze before they could agree.\"");
    }
    if !tc.delivers_want(latch.best_bits) {
        println!("And nobody handed over what you wanted.");
    }
    Ok(())
}

/// How often the i9 reads the Hall sensors, in sim time units.
const SAMPLE: f64 = 1.0;

/// `o.runs` seeded spin cycles from fresh random strips, on all cores.
/// Returns each run's latch and wall time.
fn spin_many(tc: &TradeComputer, o: &Opts) -> Vec<(Latch, f64)> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    (t..o.runs)
                        .step_by(threads)
                        .map(|r| {
                            let mut m = tc.machine(o.physics, o.seed + r as u64 * 7919).expect("machine");
                            let t = Instant::now();
                            let l = tc.spin(&mut m, &o.anneal, SAMPLE, f64::INFINITY, |_, _| {}).expect("spin");
                            (l, t.elapsed().as_secs_f64())
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

fn bench(o: &Opts) -> Result<(), trade::Error> {
    let tc = setup(o);
    let ground = report_problem(&tc, o);
    let (gv, _) = tc.evaluate(ground);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let t0 = Instant::now();
    let results = spin_many(&tc, o);
    let total = t0.elapsed().as_secs_f64();
    let runs = results.len();
    let pct = |k: usize| format!("{k}/{runs} ({:.0}%)", 100.0 * k as f64 / runs as f64);
    for (label, pick) in [("at rest", 0), ("i9 latch", 1)] {
        let bits: Vec<u32> = results.iter().map(|r| if pick == 0 { r.0.final_bits } else { r.0.best_bits }).collect();
        let hits = bits.iter().filter(|&&b| tc.is_optimal(b)).count();
        let valid = bits.iter().filter(|&&b| !tc.evaluate(b).1).count();
        let delivered = bits.iter().filter(|&&b| tc.delivers_want(b)).count();
        let mean_v: f64 = bits.iter().map(|&b| tc.evaluate(b).0).sum::<f64>() / runs as f64;
        println!(
            "{label:>8}: best set {}, no clashes {}, want delivered {}, mean value {mean_v:.1} of {gv:.1} Goo",
            pct(hits),
            pct(valid),
            pct(delivered)
        );
    }
    let mut walls: Vec<f64> = results.iter().map(|r| r.1).collect();
    walls.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut found: Vec<f64> = results.iter().filter(|r| tc.is_optimal(r.0.best_bits)).map(|r| r.0.best_time).collect();
    found.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "wall per spin cycle: median {:.2} s, max {:.2} s ({threads} threads, {total:.1} s total); i9 first latched the best set at median t={:.0} of {:.0}",
        walls[walls.len() / 2],
        walls[walls.len() - 1],
        found.get(found.len() / 2).copied().unwrap_or(f64::NAN),
        o.anneal.total_time()
    );
    Ok(())
}

fn main() -> Result<(), trade::Error> {
    let o = parse();
    match o.mode.as_str() {
        "run" => run(&o),
        "bench" => bench(&o),
        "wants" => {
            // Every want the picker can offer: what it costs the block, whether
            // its bias pins a strip ("thumb clamp"), and with --bench, how
            // reliably the machine finds it.
            let mut tc = setup(&o);
            let forward = tc.evaluate(tc.forward_best).0;
            let (mut bests, mut dels, mut fields) = (vec![], vec![], vec![]);
            println!("Forward best: {forward:.0} Goo. Deliverable wants{}:", if o.bench { format!(" ({} runs each)", o.runs) } else { String::new() });
            for npc in 0..tc.world.npcs.len() {
                for item in tc.deliverable(npc) {
                    tc.want(npc, item);
                    let best = tc.ground_state();
                    let (field, flat) = tc.field_headroom(&o.physics);
                    fields.push(field);
                    let mut line = format!(
                        "  {:<18} wants {:<38} best {:>2.0} Goo, costs {:>2.0}, field {field:>4.1}{}",
                        tc.world.npcs[npc].name,
                        tc.world.items[item].name,
                        tc.evaluate(best).0,
                        tc.want_cost(best),
                        if field > flat { " THUMB" } else { "" }
                    );
                    if o.bench {
                        let r = spin_many(&tc, &o);
                        let n = r.len() as f64;
                        let pct = |f: &dyn Fn(&Latch) -> bool| 100.0 * r.iter().filter(|x| f(&x.0)).count() as f64 / n;
                        bests.push(pct(&|l| tc.is_optimal(l.best_bits)));
                        dels.push(pct(&|l| tc.delivers_want(l.best_bits)));
                        line += &format!(
                            " | i9 best {:>3.0}%, delivered {:>3.0}%",
                            pct(&|l| tc.is_optimal(l.best_bits)),
                            pct(&|l| tc.delivers_want(l.best_bits))
                        );
                    }
                    println!("{line}");
                }
            }
            println!("(well flattens at field {:.1})", trade::machine::max_safe_field(o.physics.delta_v));
            if o.bench {
                let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
                let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
                println!(
                    "SUMMARY margin {:.2}: i9 best mean {:.0}% min {:.0}% | delivered mean {:.0}% min {:.0}% | field max {:.1}",
                    tc.want_margin, mean(&bests), min(&bests), mean(&dels), min(&dels), fields.iter().copied().fold(0.0, f64::max)
                );
            }
            Ok(())
        }
        "cycles" => {
            let tc = setup(&o);
            print_cycles(&tc);
            let g = report_problem(&tc, &o);
            println!("\nBest set {}:", mask_string(g, tc.cycles.len()));
            print_chain(&tc, g);
            Ok(())
        }
        m => panic!("unknown mode {m}"),
    }
}
