//! Headless laundromat trade computer.
//!
//! cargo run --release --example trade_cli -- [run|bench|cycles] [options]
//!
//!   run              one spin cycle with a live readout (default)
//!   bench            many seeded spin cycles vs the exact answer
//!   cycles           list the candidate trades and the best set
//!   wants [--bench] [--costly]  every want the picker can offer, its cost and (bench) hit
//!                    rate; --costly keeps only wants that cost the block Goo
//!   night            tonight's conditions, and what every want is worth and means
//!   nights           how much 200 nights vary (board size, best value)
//!   magnets          sweep a magnet's strength and position: hit rate vs the clean best set,
//!                    Goo lost, the i9's idle check, and the calibration load
//!   cuts             sweep when the breaker trips, and the battery
//!   smart            the smart Salties' aimed coil per program: cost, spin-check alarms, shield
//!
//!   --night N        which night (default 1)
//!
//!   --want WHO:WHAT  backward mode, e.g. --want upd:hub
//!   --give WHO:WHAT  WHO gives WHAT away tonight (a gift strip), e.g. --give upd:phone
//!   --magnet POS:S   a magnet under strip POS pushing S x the flattening field (+ on, - off)
//!   --shield         the hard-drive steel over the magnet   --cut F  the breaker trips F (0..1) in
//!   --salty          whatever the Salties roll tonight (magnet, coil, cut)
//!   --coil           the smart Salties' coil, aimed at tonight's board (on only while the drum shakes)
//!   --clamp C        want clamp, x the well-flattening field (0 = off)  --margin M
//!   --seed N  --runs N
//!   --beta B  --penalty P  --dv DV  --gamma G  --dt DT
//!   --hot T  --cold T  --time T  --settle T

use std::time::Instant;

use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::salties::SpinCheck;
use cortenforge_play::trade::{self, Anneal, Latch, Machine, Magnet, Physics, Sabotage, TradeComputer, qubo, salties};

#[derive(Clone)]
struct Opts {
    mode: String,
    bench: bool,
    costly: bool,
    margin: Option<f64>,
    clamp: Option<f64>,
    gifts: usize,
    chain: usize,
    want: Option<String>,
    give: Option<String>,
    magnet: Option<Magnet>,
    /// The magnet is the smart Salties' coil: on only while the drum shakes.
    coil: bool,
    shield: bool,
    salty: bool,
    seed: u64,
    night: u64,
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
        costly: false,
        margin: None,
        clamp: None,
        gifts: trade::MAX_BITS,
        chain: trade::cycles::MAX_CHAIN,
        want: None,
        give: None,
        magnet: None,
        coil: false,
        shield: false,
        salty: false,
        seed: 1,
        night: 1,
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
            "--give" => o.give = Some(val()),
            "--magnet" => {
                let v = val();
                let (p, s) = v.split_once(':').expect("--magnet POS:STRENGTH");
                o.magnet = Some(Magnet { pos: num(p.into()), strength: num(s.into()) });
            }
            "--shield" => o.shield = true,
            "--coil" => o.coil = true,
            "--salty" => o.salty = true,
            "--cut" => o.anneal.cut = Some(num(val())),
            "--bench" => o.bench = true,
            "--costly" => o.costly = true,
            "--margin" => o.margin = Some(num(val())),
            "--clamp" => o.clamp = Some(num(val())),
            "--gifts" => o.gifts = num(val()) as usize,
            "--chain" => o.chain = num(val()) as usize,
            "--seed" => o.seed = num(val()) as u64,
            "--night" => o.night = num(val()) as u64,
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
    let mut w = trade::world::laundromat(o.night);
    if let Some(g) = &o.give {
        let (who, what) = g.split_once(':').expect("--give WHO:WHAT");
        let npc = w.find_npc(who).unwrap_or_else(|| panic!("nobody called {who}"));
        let item = w.find_item(what).unwrap_or_else(|| panic!("no item like {what}"));
        assert_eq!(w.items[item].owner, npc, "{} doesn't have the {}", w.npcs[npc].name, w.items[item].name);
        w.set_gift(item, true);
        println!("{} gives away the {}.", w.npcs[npc].name, w.items[item].name);
    }
    let mut tc = TradeComputer::with_board(w, o.beta, o.penalty, o.gifts, o.chain);
    tc.delta_v = o.physics.delta_v;
    if let Some(c) = o.clamp {
        tc.want_clamp = c;
    }
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

/// "+7 Goo" for a trade; "+14 Goo, 19.5 Karma" for a gift chain.
fn worth(c: &trade::Cycle) -> String {
    if c.is_gift() { format!("+{:.0} Goo, {:.1} Karma", c.goo(), c.karma()) } else { format!("+{:.0} Goo", c.goo()) }
}

fn print_cycles(tc: &TradeComputer) {
    let gifts = tc.cycles.iter().filter(|c| c.is_gift()).count();
    println!(
        "{} candidates on the board: {} trades, {gifts} gift chains ({} more left off). bit: drum score, who:",
        tc.cycles.len(),
        tc.cycles.len() - gifts,
        tc.dropped
    );
    for (i, c) in tc.cycles.iter().enumerate() {
        let biased = if tc.problem.bias[i] > 0.0 { "  <- wanted" } else { "" };
        let kind = if c.is_gift() { "gift " } else { "trade" };
        println!("  {i:>2}: {:>5.1} {kind}  {}  ({}){biased}", c.value(), c.short(&tc.world), worth(c));
    }
}

fn print_chain(tc: &TradeComputer, bits: u32) {
    let (_, clash) = tc.evaluate(bits);
    for i in qubo::chosen(bits, tc.cycles.len()) {
        let c = &tc.cycles[i];
        let kind = if c.is_gift() { format!("gift to {}", c.len()) } else { format!("{}-way", c.len()) };
        println!("  * {kind} ({}): {}.", worth(c), c.describe(&tc.world));
    }
    let t = tc.tally(bits);
    println!(
        "  Total: +{:.0} Goo and nobody loses; Karma {:.1} (relief {:.1}, flourishing {:.1}){}.",
        t.goo,
        t.karma,
        t.relief,
        t.flourishing,
        if clash { ", BUT two of them fight over the same item" } else { "" }
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
        None => println!("Exact solver skipped: over {} bits, or it overflowed to NaN (no max-energy shift).", trade::MAX_EXACT_BITS),
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
    report_sabotage(&tc, o);
    let mut m = board(&tc, o, o.seed);
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

/// The magnet as it reaches the strips (through the shield, if any).
fn magnet(o: &Opts) -> Option<Magnet> {
    o.magnet.map(|m| if o.shield { m.shielded() } else { m })
}

/// A fresh board, with the magnet under it if there is one.
fn board(tc: &TradeComputer, o: &Opts, seed: u64) -> Machine {
    match magnet(o) {
        Some(m) if o.coil => Machine::with_component(&tc.ising, o.physics, seed, salties::Coil(m.field(tc.ising.n, o.physics.delta_v))),
        Some(m) => tc.tampered_machine(o.physics, seed, &m),
        None => tc.machine(o.physics, seed),
    }
    .expect("machine")
}

/// The i9's idle check on a fresh board: the biggest stray field it reads
/// (x the flattening field) and on which strip.
fn idle_check(tc: &TradeComputer, o: &Opts) -> (f64, usize) {
    let mut m = board(tc, o, o.seed);
    m.rest(30.0).expect("rest");
    let flat = trade::machine::max_safe_field(o.physics.delta_v);
    m.stray_field(&tc.ising).iter().enumerate().fold((0.0, 0), |b, (i, x)| if x.abs() / flat > b.0 { (x.abs() / flat, i) } else { b })
}

/// Hit rate vs the clean best set, and mean Goo lost, over `o.runs` spins.
fn hit_and_loss(tc: &TradeComputer, o: &Opts) -> (f64, f64, f64) {
    let r = spin_many(tc, o);
    let n = r.len() as f64;
    let best = tc.evaluate(tc.ground_state()).0;
    let hits = r.iter().filter(|x| tc.is_optimal(x.0.best_bits)).count() as f64 / n;
    let lost = r.iter().map(|x| best - tc.evaluate(x.0.best_bits).0).sum::<f64>() / n;
    let clashes = r.iter().filter(|x| tc.evaluate(x.0.best_bits).1).count() as f64 / n;
    (hits, lost, clashes)
}

/// Tonight's sabotage into the options (`--salty`).
fn apply_salties(o: &mut Opts) {
    match trade::world::laundromat(o.night).night.sabotage {
        Some(Sabotage::Magnet(m)) => o.magnet = Some(m),
        Some(Sabotage::PowerCut(at)) => o.anneal.cut = Some(at),
        Some(Sabotage::QuietCut(at)) => o.anneal.cut = Some(at),
        Some(Sabotage::Coil) => o.coil = true,
        Some(Sabotage::Emp) => println!("The Salties try their \"EMP\". It made popcorn."),
        None => {}
    }
}

/// Where the smart Salties put the coil tonight (scouted on the plain
/// night's board), and the Goo it should cost.
fn aim_coil(o: &Opts) -> (Magnet, f64) {
    let tc = TradeComputer::new(trade::world::laundromat(o.night), o.beta, o.penalty);
    salties::aim(&tc, o.physics.delta_v)
}

/// `o.runs` seeded spin cycles from fresh random strips, on all cores.
/// Returns each run's latch, wall time and spin check.
fn spin_many(tc: &TradeComputer, o: &Opts) -> Vec<(Latch, f64, SpinCheck)> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    (t..o.runs)
                        .step_by(threads)
                        .map(|r| {
                            let mut m = board(tc, o, o.seed + r as u64 * 7919);
                            let t = Instant::now();
                            let mut check = SpinCheck::default();
                            let l = tc.spin(&mut m, &o.anneal, SAMPLE, SAMPLE, |m, _| check.observe(m, &tc.ising)).expect("spin");
                            (l, t.elapsed().as_secs_f64(), check)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

/// How many runs' spin checks raised the alarm, and the strip it pointed at
/// most often.
fn spin_alarms(results: &[(Latch, f64, SpinCheck)], o: &Opts) -> (usize, usize) {
    let flat = trade::machine::max_safe_field(o.physics.delta_v);
    let mut votes = [0usize; trade::MAX_BITS];
    let mut alarms = 0;
    for (_, _, check) in results {
        if let Some((k, x)) = check.strongest()
            && x.abs() >= salties::SPIN_ALARM * flat
        {
            alarms += 1;
            votes[k] += 1;
        }
    }
    let strip = (0..votes.len()).max_by_key(|&k| votes[k]).unwrap_or(0);
    (alarms, strip)
}

/// What the Salties did to this run, and what the i9's idle check sees.
fn report_sabotage(tc: &TradeComputer, o: &Opts) {
    if let Some(m) = o.magnet {
        let (stray, strip) = idle_check(tc, o);
        println!(
            "Sabotage: {}{}. The i9's idle check reads a stray field of {stray:.2}x flattening on strip {strip}. Scored against the clean best set.",
            if o.coil { format!("a coil under strip {:.0} ({:.1}x, pushing {}, on only while the drum spins)", m.pos, m.strength.abs(), if m.strength > 0.0 { "on" } else { "off" }) } else { Sabotage::Magnet(m).describe() },
            if o.shield { format!(", behind the shield ({:.2}x gets through)", m.shielded().strength.abs()) } else { String::new() }
        );
    }
    if let Some(at) = o.anneal.cut {
        println!("Sabotage: {}; the drum stops at t={:.0}.", Sabotage::PowerCut(at).describe(), o.anneal.stop_time());
    }
}

fn bench(o: &Opts) -> Result<(), trade::Error> {
    let tc = setup(o);
    let ground = report_problem(&tc, o);
    let (gv, _) = tc.evaluate(ground);
    report_sabotage(&tc, o);
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
    let (alarms, strip) = spin_alarms(&results, o);
    println!(
        "spin check: alarm on {} of runs (push over {:.1}x the flattening field), most often on strip {strip}",
        pct(alarms),
        salties::SPIN_ALARM
    );
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
    let mut o = parse();
    if o.salty {
        apply_salties(&mut o);
    }
    if o.coil && o.magnet.is_none() {
        o.magnet = Some(aim_coil(&o).0);
    }
    match o.mode.as_str() {
        "run" => run(&o),
        "bench" => bench(&o),
        "wants" => {
            // Every want the picker can offer: what it costs the block, whether
            // its bias pins the strip (the "thumb clamp"), and with --bench, how
            // reliably the machine finds it.
            let mut tc = setup(&o);
            let forward = tc.score_text(tc.forward_best);
            let (mut bests, mut dels, mut fields) = (vec![], vec![], vec![]);
            println!("Forward best: {forward}. Deliverable wants{}:", if o.bench { format!(" ({} runs each)", o.runs) } else { String::new() });
            for npc in 0..tc.world.npcs.len() {
                for item in tc.deliverable(npc) {
                    tc.want(npc, item);
                    let best = tc.ground_state();
                    if o.costly && tc.want_cost(best) < 0.5 {
                        continue;
                    }
                    let (field, flat) = tc.field_headroom(&o.physics);
                    fields.push(field);
                    let mut line = format!(
                        "  {:<18} wants {:<38} best {}, costs {:>2.0}, field {field:>4.1}{}",
                        tc.world.npcs[npc].name,
                        tc.world.items[item].name,
                        tc.score_text(best),
                        tc.want_cost(best),
                        if field > flat { " pinned" } else { "" }
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
                    "SUMMARY clamp {:.2} margin {:.2}: i9 best mean {:.0}% min {:.0}% | delivered mean {:.0}% min {:.0}% | field max {:.1}",
                    tc.want_clamp, tc.want_margin, mean(&bests), min(&bests), mean(&dels), min(&dels), fields.iter().copied().fold(0.0, f64::max)
                );
            }
            Ok(())
        }
        "night" => {
            // Tonight's conditions and what every want is worth and means.
            let tc = setup(&o);
            let w = &tc.world;
            println!("Night {}: {}. {} trades on the board ({} left off).", o.night, w.weather(), tc.cycles.len(), tc.dropped);
            match w.night.sabotage {
                Some(s) => println!("  The Salties: {} (--salty replays it).", s.describe()),
                None => println!("  No Salties tonight."),
            }
            for (k, npc) in w.npcs.iter().enumerate() {
                let c = w.conditions(k);
                let covered = if w.needs_covered(k) { "needs covered" } else { "needs NOT covered" };
                println!("  {:<18} {:<34} {covered}", npc.name, if c.is_empty() { "fine".to_string() } else { c.join(", ") });
                for want in w.wants.iter().filter(|x| x.npc == k) {
                    println!(
                        "      wants {:<40} {:>4.1} Goo  {:?}",
                        w.items[want.item].name,
                        w.value[k][want.item],
                        w.tag(k, want.item).unwrap()
                    );
                }
            }
            Ok(())
        }
        "nights" => {
            // How much the nights vary, and where the gift goes.
            let (mut trades, mut gifts, mut dropped, mut karma) = (vec![], vec![], vec![], vec![]);
            let (mut gift_won, mut fed_hungry, mut gift_to_fed) = (0, 0, 0);
            for night in 1..=200u64 {
                let mut oo = Opts { night, ..o.clone() };
                oo.want = None;
                let tc = setup(&oo);
                let g = tc.cycles.iter().filter(|c| c.is_gift()).count();
                trades.push(tc.cycles.len() - g);
                gifts.push(g);
                dropped.push(tc.dropped);
                let best = tc.forward_best;
                karma.push(tc.tally(best).karma);
                // Who got the food first, and were they hungry?
                let food = |c: &&trade::Cycle| c.is_gift() && tc.world.npcs[c.legs[0].from].business;
                if let Some(c) = qubo::chosen(best, tc.cycles.len()).into_iter().map(|i| &tc.cycles[i]).find(food) {
                    gift_won += 1;
                    if tc.world.night.hungry[c.legs[0].to] {
                        fed_hungry += 1;
                    } else if (0..tc.world.npcs.len()).any(|k| tc.world.night.hungry[k] && tc.world.value[k][c.legs[0].item] > 0.0) {
                        gift_to_fed += 1;
                    }
                }
            }
            let range = |v: &[usize]| format!("{}-{}", v.iter().min().unwrap(), v.iter().max().unwrap());
            let kmin = karma.iter().copied().fold(f64::INFINITY, f64::min);
            let kmax = karma.iter().copied().fold(0.0, f64::max);
            println!(
                "200 nights: board = {} trades + {} gift chains ({} candidates left off); best set uses a gift on {gift_won}, Karma {kmin:.0}-{kmax:.0}",
                range(&trades),
                range(&gifts),
                range(&dropped)
            );
            println!(
                "  the food goes first to someone hungry on {fed_hungry}; to someone fed while a hungry person wanted it on {gift_to_fed}"
            );
            Ok(())
        }
        "magnets" => {
            // A magnet's strength (at mid-board) and position, open and
            // shielded: hit rate vs the clean best set, Goo lost, what the
            // idle check reads, and how often the calibration load misses.
            let tc = setup(&o);
            let calib = salties::calibration_load();
            let (best, _) = tc.evaluate(tc.ground_state());
            let mid = (tc.cycles.len() as f64 - 1.0) / 2.0;
            let clean = hit_and_loss(&tc, &Opts { magnet: None, ..o.clone() });
            let calib_clean = hit_and_loss(&calib, &Opts { magnet: None, ..o.clone() });
            println!(
                "Night {}: {} strips, best set {best:.1}. No magnet: best set {:.0}%, calibration load missed {:.0}%. {} runs each.",
                o.night,
                tc.cycles.len(),
                100.0 * clean.0,
                100.0 * (1.0 - calib_clean.0),
                o.runs
            );
            println!("  magnet                       | open: best  lost  clash  idle  calib-miss | shielded: best  lost  idle  calib-miss");
            let mut configs: Vec<Magnet> = [-1.5, -1.0, -0.6, -0.3, 0.3, 0.6, 1.0, 1.5].iter().map(|&s| Magnet { pos: mid, strength: s }).collect();
            configs.extend([3.0, 6.0, 12.0].iter().map(|&d| Magnet { pos: mid + d, strength: -1.0 }));
            for m in configs {
                let mut row = format!("  {:<28}", Sabotage::Magnet(m).describe().replace("a magnet under ", ""));
                for shield in [false, true] {
                    let oo = Opts { magnet: Some(m), shield, ..o.clone() };
                    let (hit, lost, clash) = hit_and_loss(&tc, &oo);
                    let (idle, _) = idle_check(&tc, &oo);
                    let (cal, _, _) = hit_and_loss(&calib, &oo);
                    row += &if shield {
                        format!(" |           {:>3.0}% {lost:>5.1} {idle:>5.2}  {:>5.0}%", 100.0 * hit, 100.0 * (1.0 - cal))
                    } else {
                        format!(" |       {:>3.0}% {lost:>5.1}  {:>4.0}% {idle:>5.2}  {:>5.0}%", 100.0 * hit, 100.0 * clash, 100.0 * (1.0 - cal))
                    };
                }
                println!("{row}");
            }
            Ok(())
        }
        "smart" => {
            // The smart Salties' aimed coil against each program: what it
            // costs, whether the idle check (no) and the spin check (yes) see
            // it, and the shield over the strip the spin check names.
            let tc = setup(&o);
            let (coil, expect) = aim_coil(&o);
            let k = coil.strip(tc.cycles.len());
            println!(
                "Night {}: they aim a coil under strip {k} ({}), pushing {} at {:.1}x: on paper it costs {expect:.0} Goo. {} runs each.",
                o.night,
                tc.cycles[k].short(&tc.world),
                if coil.strength > 0.0 { "on" } else { "off" },
                coil.strength.abs(),
                o.runs
            );
            println!("  program          | clean: best  alarms | coil: best  lost  alarms (on strip {k}) | shield on {k}: best  lost");
            for (name, time) in [("Quick Wash", 150.0), ("Permanent Press", 300.0), ("Normal", 1000.0)] {
                let base = Opts { anneal: Anneal { duration: time, ..o.anneal }, magnet: None, coil: false, ..o.clone() };
                let clean = spin_many(&tc, &base);
                let coiled = Opts { magnet: Some(coil), coil: true, ..base.clone() };
                let hit = spin_many(&tc, &coiled);
                let shielded = Opts { magnet: Some(coil.through(Some(k as f64))), ..coiled.clone() };
                let (sh, sl, _) = hit_and_loss(&tc, &shielded);
                let best = tc.evaluate(tc.ground_state()).0;
                let rate = |r: &[(Latch, f64, SpinCheck)]| 100.0 * r.iter().filter(|x| tc.is_optimal(x.0.best_bits)).count() as f64 / r.len() as f64;
                let lost = hit.iter().map(|x| best - tc.evaluate(x.0.best_bits).0).sum::<f64>() / hit.len() as f64;
                let on_k = hit
                    .iter()
                    .filter(|x| x.2.strongest().is_some_and(|(s, v)| s == k && v.abs() >= salties::SPIN_ALARM * trade::machine::max_safe_field(o.physics.delta_v)))
                    .count();
                let pc = |a: usize, r: usize| 100.0 * a as f64 / r as f64;
                println!(
                    "  {name:<16} |       {:>3.0}%   {:>4.0}% |      {:>3.0}% {lost:>5.1}  {:>4.0}% ({:>3.0}%)        |          {:>3.0}% {sl:>5.1}",
                    rate(&clean),
                    pc(spin_alarms(&clean, &base).0, clean.len()),
                    rate(&hit),
                    pc(spin_alarms(&hit, &coiled).0, hit.len()),
                    pc(on_k, hit.len()),
                    100.0 * sh
                );
            }
            let (idle, _) = idle_check(&tc, &Opts { magnet: Some(coil), coil: true, ..o.clone() });
            println!("  idle check with the coil in place: {idle:.3}x (the coil is off while the drum is stopped)");
            Ok(())
        }
        "cuts" => {
            // When the breaker trips, and the battery that finishes the cycle.
            let tc = setup(&o);
            let (best, _) = tc.evaluate(tc.ground_state());
            println!("Night {}: best set {best:.1}, {} runs each, {:.0}-unit cool-down.", o.night, o.runs, o.anneal.duration);
            for cut in [Some(0.15), Some(0.3), Some(0.5), Some(0.7), Some(0.85), None] {
                let oo = Opts { anneal: Anneal { cut, ..o.anneal }, ..o.clone() };
                let (hit, lost, clash) = hit_and_loss(&tc, &oo);
                let what = cut.map_or("no cut (battery)".to_string(), |c| format!("cut {:.0}% in (kT x{:.2})", 100.0 * c, o.anneal.temperature(c * o.anneal.duration)));
                println!("  {what:<26} best set {:>3.0}%, lost {lost:>4.1} Goo, clashes {:>3.0}%", 100.0 * hit, 100.0 * clash);
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
