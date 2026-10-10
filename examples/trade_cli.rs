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
//!   learn            train the smart wash program with CEM on --train nights (A..B or a,b,c; default 11..22),
//!                    --gens G x --pop P spins, then test it on --nights A..B (default 1..10)
//!   versus           Normal vs the smart wash (--smart = the learned one, or --params a,b,c,d[,stall,ln cool-down,ln patience]) per night
//!   rematch          is the learned wash (or --params) really better than run 3 (or --vs a,b,..)? Normal, A and B
//!                    on the same boards and seeds, then sim-opt bootstrap CIs on the difference (paired and not)
//!   row              a row of washers (parallel tempering) vs Normal per night, with swap rates (tuning)
//!   print [--part P] Upddayett's prints: each design's v1 and v2 through the print check, STLs into prints/, and
//!                    what the print does to --night N's best set
//!   rowmatch         the row vs run 5 at equal compute, and K whole-cycle washers vs the best of K run-5 washers
//!   yuck             does yuck make the street harder to compute? clean vs 2 yucky people vs a pump per night
//!                    (hit rate, Goo lost, strips, best set, glassiness), paired bootstrap CIs
//!
//!   --washers K      spin a row of K washers sharing one cycle's compute (--full: each runs the whole cycle)
//!   --swap S  --rcold T  --rhot T   swap interval and the row's ladder ends (default 1, 1.4, 4)
//!   --best-of K      keep the best of K separate washers
//!   --yuck WHO,WHO   these people are yucky tonight   --pump hungry|cold  everyone it touches is   --tax T  Goo a trade (2)
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
//!   --smart          spin with the learned smart wash instead of the anneal (bench, versus)
//!   --mix            versus / learn also run each night with one want and one give-away of Upddayett's
//!   --restarts K     the smart wash as K shorter cool-downs back to back (the latch keeps the best)
//!   --patience F     the smart wash reheats once the strips read the same set for F x a cool-down

use std::time::Instant;

use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::row::{Row, RowStats};
use cortenforge_play::trade::salties::SpinCheck;
use cortenforge_play::trade::smart::{self, SmartWash};
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
    /// The smart wash (`--smart`, `--params`) instead of the anneal.
    smart: bool,
    /// `versus` / `learn` also try one want and one give-away a night (`--mix`).
    mix: bool,
    params: Option<Vec<f64>>,
    /// `rematch`'s program B (default run 3).
    vs: Option<Vec<f64>>,
    program: Option<SmartWash>,
    /// A row of washers instead of one (`--washers K`, `--full`, `--swap S`, `--rcold T`, `--rhot T`).
    row: Option<Row>,
    washers: Option<usize>,
    full: bool,
    swap: Option<f64>,
    rcold: Option<f64>,
    rhot: Option<f64>,
    /// The best of this many separate washers (`--best-of K`).
    best_of: usize,
    /// `print --part P`: one catalog part.
    part: Option<String>,
    /// Yuck (DESIGN "Yuck: the one enemy"): these people are yucky tonight (`--yuck WHO,WHO`),
    /// or a pump makes everyone it touches yucky (`--pump hungry|cold`); each pays `--tax T` Goo a trade.
    yuck: Vec<String>,
    pump: Option<String>,
    tax: f64,
    gens: usize,
    /// Cool-downs per cycle for the smart wash (`--restarts K`).
    restarts: Option<f64>,
    /// Reheat once the strips read the same set this long, x a cool-down (`--patience F`).
    patience: Option<f64>,
    pop: usize,
    /// Nights `versus` tests on, and `learn` trains on.
    nights: (u64, u64),
    train: Vec<u64>,
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
        smart: false,
        mix: false,
        params: None,
        vs: None,
        program: None,
        row: None,
        washers: None,
        full: false,
        swap: None,
        rcold: None,
        rhot: None,
        best_of: 1,
        part: None,
        yuck: vec![],
        pump: None,
        tax: 2.0,
        gens: 60,
        restarts: None,
        patience: None,
        pop: 24,
        nights: (1, 10),
        train: (11..=22).collect(),
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
            "--smart" => o.smart = true,
            "--mix" => o.mix = true,
            "--params" => o.params = Some(val().split(',').map(|s| num(s.trim().into())).collect()),
            "--vs" => o.vs = Some(val().split(',').map(|s| num(s.trim().into())).collect()),
            "--gens" => o.gens = num(val()) as usize,
            "--washers" => o.washers = Some(num(val()) as usize),
            "--full" => o.full = true,
            "--swap" => o.swap = Some(num(val())),
            "--rcold" => o.rcold = Some(num(val())),
            "--rhot" => o.rhot = Some(num(val())),
            "--best-of" => o.best_of = num(val()) as usize,
            "--part" => o.part = Some(val()),
            "--yuck" => o.yuck = val().split(',').map(str::to_string).collect(),
            "--pump" => o.pump = Some(val()),
            "--tax" => o.tax = num(val()),
            "--restarts" => o.restarts = Some(num(val())),
            "--patience" => o.patience = Some(num(val())),
            "--pop" => o.pop = num(val()) as usize,
            "--nights" => o.nights = night_range(&val()),
            "--train" => o.train = night_list(&val()),
            m if !m.starts_with("--") => o.mode = m.to_string(),
            other => panic!("unknown option {other}"),
        }
        i += 1;
    }
    o
}

/// "1..10" (inclusive) or a single night "4".
fn night_range(s: &str) -> (u64, u64) {
    let n = |s: &str| s.trim().parse::<u64>().unwrap_or_else(|_| panic!("bad night {s}"));
    match s.split_once("..") {
        Some((a, b)) => (n(a), n(b.trim_start_matches('='))),
        None => (n(s), n(s)),
    }
}

/// "11..22" or "12,20,25" (or a mix: "1..3,9").
fn night_list(s: &str) -> Vec<u64> {
    s.split(',').flat_map(|p| {
        let (a, b) = night_range(p);
        a..=b
    }).collect()
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
    apply_yuck(o, &mut w, true);
    let mut tc = board_for(o, w);
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

/// The trade computer for world `w` with the options' board settings.
fn board_for(o: &Opts, w: trade::world::World) -> TradeComputer {
    let mut tc = TradeComputer::with_board(w, o.beta, o.penalty, o.gifts, o.chain);
    tc.delta_v = o.physics.delta_v;
    if let Some(c) = o.clamp {
        tc.want_clamp = c;
    }
    if let Some(m) = o.margin {
        tc.want_margin = m;
    }
    tc
}

/// The boards `versus` and `learn` run for `night`: tonight's board, and
/// with `--mix` also one want (anyone's, any deliverable item) and one of
/// Upddayett's give-aways, picked by the night. Each comes with a label.
fn scenarios(o: &Opts, night: u64) -> Vec<(String, TradeComputer)> {
    let mut on = o.clone();
    on.night = night;
    let plain = setup(&on);
    let mut out = vec![("plain".to_string(), plain.clone())];
    if !o.mix {
        return out;
    }
    let mut rng = night.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut pick = |n: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % n as u64) as usize
    };
    let w = &plain.world;
    let wants: Vec<(usize, usize)> =
        (0..w.npcs.len()).flat_map(|k| plain.deliverable(k).into_iter().map(move |i| (k, i))).collect();
    if !wants.is_empty() {
        let (k, i) = wants[pick(wants.len())];
        let mut tc = plain.clone();
        tc.want(k, i);
        out.push((format!("{} wants the {}", w.npcs[k].name, w.items[i].name), tc));
    }
    let upd = w.find_npc("upd").expect("Upddayett");
    let gives = w.giveable(upd);
    if !gives.is_empty() {
        let item = gives[pick(gives.len())];
        let mut world = trade::world::laundromat(night);
        world.set_gift(item, true);
        out.push((format!("Upddayett gives the {} away", w.items[item].name), board_for(o, world)));
    }
    out
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
        println!("That's the best possible set.");
    } else {
        println!("\nThe best set was {}:", mask_string(ground, n));
        print_chain(&tc, ground);
        println!("\"We spun it too fast. The strips froze before they could agree.\"");
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
    on_all_cores(o.runs, |r| {
        let t = Instant::now();
        let (l, check, _) = spin_one(tc, o, o.seed + r as u64 * 7919);
        (l, t.elapsed().as_secs_f64(), check)
    })
}

/// `f(0..runs)` spread over every core, results in run order per thread.
fn on_all_cores<T: Send>(runs: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let f = &f;
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads).map(|t| s.spawn(move || (t..runs).step_by(threads).map(f).collect::<Vec<_>>())).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

/// One seeded spin: the anneal, the smart wash, a row of washers (`--washers`),
/// or the best of `--best-of` separate washers. The spin check watches the
/// (coldest) washer.
fn spin_one(tc: &TradeComputer, o: &Opts, seed: u64) -> (Latch, SpinCheck, Option<RowStats>) {
    let mut check = SpinCheck::default();
    if let Some(row) = &o.row {
        let mut ms: Vec<Machine> = (0..row.washers).map(|k| board(tc, o, seed.wrapping_add(k as u64 * 104_729))).collect();
        let tick = |ms: &[Machine], _: &Latch| check.observe(&ms[0], &tc.ising);
        let (l, stats) = tc.spin_row(&mut ms, row, seed, SAMPLE, SAMPLE, tick).expect("row");
        return (l, check, Some(stats));
    }
    let mut best: Option<Latch> = None;
    for k in 0..o.best_of.max(1) {
        let mut m = board(tc, o, seed.wrapping_add(k as u64 * 104_729));
        let tick = |m: &Machine, _: &Latch| check.observe(m, &tc.ising);
        let l = match &o.program {
            Some(p) => tc.spin_with(&mut m, p.total_time(), p.program(&tc.problem.qubo), SAMPLE, SAMPLE, tick),
            None => tc.spin(&mut m, &o.anneal, SAMPLE, SAMPLE, tick),
        }
        .expect("spin");
        if best.is_none_or(|b| l.best_energy < b.best_energy) {
            best = Some(l);
        }
    }
    (best.expect("one spin"), check, None)
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

/// Normal against a smart wash on each of `o.nights`: the i9's hit rate on
/// the best set, `o.runs` spins each, same cycle length.
fn versus(o: &Opts, smart: &SmartWash) {
    println!(
        "Normal vs smart wash ({} time units + settle, {:.1} cool-down(s) on the clock, patience {}), {} spins a night, params {:?}",
        smart.duration,
        smart.restarts(),
        if smart.patience() < smart.period() { format!("{:.0} units", smart.patience()) } else { "off".into() },
        o.runs,
        smart.params().iter().map(|p| (p * 1000.0).round() / 1000.0).collect::<Vec<_>>()
    );
    // Hit rates per kind of board: plain, a want, a give-away.
    let mut sums: Vec<(String, f64, f64, f64)> = vec![];
    for night in o.nights.0..=o.nights.1 {
        for (label, tc) in scenarios(o, night) {
            let mut on = o.clone();
            on.night = night;
            on.program = None;
            let normal = hit_and_loss(&tc, &on);
            on.program = Some(smart.clone());
            let learned = hit_and_loss(&tc, &on);
            println!(
                "  night {night:>3} ({:>2} strips): Normal {:>3.0}% (lost {:>4.1} Goo), smart {:>3.0}% (lost {:>4.1} Goo){}",
                tc.ising.n,
                100.0 * normal.0,
                normal.1,
                100.0 * learned.0,
                learned.1,
                if o.mix { format!("  {label}") } else { String::new() }
            );
            let kind = if label == "plain" { "plain" } else if label.starts_with("Upddayett gives") { "give-away" } else { "want" };
            match sums.iter_mut().find(|s| s.0 == kind) {
                Some(s) => (s.1, s.2, s.3) = (s.1 + normal.0, s.2 + learned.0, s.3 + 1.0),
                None => sums.push((kind.to_string(), normal.0, learned.0, 1.0)),
            }
        }
    }
    let (hn, hs, n) = sums.iter().fold((0.0, 0.0, 0.0), |a, s| (a.0 + s.1, a.1 + s.2, a.2 + s.3));
    if sums.len() > 1 {
        for (kind, hn, hs, n) in &sums {
            println!("  {kind:>9}: Normal {:.0}%, smart {:.0}% ({n} boards)", 100.0 * hn / n, 100.0 * hs / n);
        }
    }
    println!("  mean: Normal {:.0}%, smart {:.0}%", 100.0 * hn / n, 100.0 * hs / n);
}

/// Trains the smart wash with CEM on `o.train` nights, then tests it
/// against Normal on `o.nights` (held out).
fn learn(o: &Opts) -> Result<(), trade::Error> {
    let start = o.program.clone().unwrap_or_else(|| SmartWash::normal(o.anneal.duration));
    // With --mix each night gives three boards: plain, a want, a give-away.
    let nights: Vec<(u64, TradeComputer)> =
        o.train.iter().flat_map(|&n| scenarios(o, n).into_iter().map(move |(_, tc)| (n, tc))).collect();
    println!(
        "CEM: {} generations x {} candidates, training nights {:?} in turn, start {:?}",
        o.gens,
        o.pop,
        o.train,
        start.params()
    );
    let t0 = Instant::now();
    let learned = smart::train(&nights, o.physics, &start, o.gens, o.pop, o.seed, |g, night, m, p| {
        println!(
            "  gen {g:>3} night {night:>3}: mean {:.3}, elite {:.3}/step, noise {:.3}, {:.1} s; params {:?}",
            m.mean_reward,
            m.extra.get("elite_mean_reward_per_step").copied().unwrap_or(f64::NAN),
            m.extra.get("noise_std").copied().unwrap_or(f64::NAN),
            m.wall_time_ms as f64 / 1000.0,
            p.iter().map(|p| (p * 1000.0).round() / 1000.0).collect::<Vec<_>>()
        );
    })?;
    println!("trained in {:.0} s", t0.elapsed().as_secs_f64());
    println!("pub const LEARNED: Option<[f64; N_PARAMS]> = Some({:?});", learned.params());
    versus(o, &learned);
    Ok(())
}

/// Is program A really better than program B? Runs Normal, A and B on the
/// same boards with the same seeds (common random numbers), then asks
/// sim-opt's bootstrap for a 95% CI on the difference in hit rate. A is the
/// learned program (or `--params`), B is run 3 (or `--vs`). The CI comes
/// two ways: paired (on per-board differences, the right test here) and the
/// way `bootstrap_diff_means` does it (the two samples resampled apart).
fn rematch(o: &Opts, a: &SmartWash, b: &SmartWash) {
    use cortenforge::sim::opt::analysis::{bimodality_coefficient, bootstrap_diff_means};
    use rand::SeedableRng;
    println!(
        "rematch: A {:?} vs B {:?}, {} spins a board, nights {}..{}{}",
        a.params().iter().map(|p| (p * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        b.params().iter().map(|p| (p * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        o.runs,
        o.nights.0,
        o.nights.1,
        if o.mix { " (with a want and a give-away)" } else { "" }
    );
    // Per board: (Normal, A, B) hit rates.
    let mut rows: Vec<(f64, f64, f64)> = vec![];
    let t0 = Instant::now();
    for night in o.nights.0..=o.nights.1 {
        for (label, tc) in scenarios(o, night) {
            let mut on = o.clone();
            on.night = night;
            on.program = None;
            let n = hit_and_loss(&tc, &on).0;
            on.program = Some(a.clone());
            let ha = hit_and_loss(&tc, &on).0;
            on.program = Some(b.clone());
            let hb = hit_and_loss(&tc, &on).0;
            println!(
                "  night {night:>3}: Normal {:>3.0}%, A {:>3.0}%, B {:>3.0}%  ({:.0} s){}",
                100.0 * n,
                100.0 * ha,
                100.0 * hb,
                t0.elapsed().as_secs_f64(),
                if o.mix { format!("  {label}") } else { String::new() }
            );
            rows.push((n, ha, hb));
        }
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(o.seed);
    let mut report = |what: &str, rows: &[(f64, f64, f64)]| {
        let col = |f: fn(&(f64, f64, f64)) -> f64| rows.iter().map(f).collect::<Vec<f64>>();
        let (n, ha, hb) = (col(|r| r.0), col(|r| r.1), col(|r| r.2));
        let d: Vec<f64> = ha.iter().zip(&hb).map(|(x, y)| x - y).collect();
        let dn: Vec<f64> = ha.iter().zip(&n).map(|(x, y)| x - y).collect();
        let mean = |v: &[f64]| 100.0 * v.iter().sum::<f64>() / v.len() as f64;
        println!(
            "\n{what} ({} boards): Normal {:.1}%, A {:.1}%, B {:.1}%",
            rows.len(),
            mean(&n),
            mean(&ha),
            mean(&hb)
        );
        let ci = |name: &str, ci: cortenforge::sim::opt::analysis::BootstrapCi| {
            println!(
                "  {name:<34} {:+5.1} points, 95% CI [{:+5.1}, {:+5.1}] -> {:?}",
                100.0 * ci.point_estimate,
                100.0 * ci.lower,
                100.0 * ci.upper,
                ci.classify()
            );
        };
        // Paired: resample per-board differences (B side is a constant 0).
        ci("A - B, paired", bootstrap_diff_means(&d, &[0.0], &mut rng));
        ci("A - B, unpaired (diff_means as is)", bootstrap_diff_means(&ha, &hb, &mut rng));
        ci("A - Normal, paired", bootstrap_diff_means(&dn, &[0.0], &mut rng));
        println!(
            "  boards A won {}, B won {}, tied {}; bimodality of A - B {:.2} (> 0.56 = two humps)",
            d.iter().filter(|&&x| x > 0.0).count(),
            d.iter().filter(|&&x| x < 0.0).count(),
            d.iter().filter(|&&x| x == 0.0).count(),
            if d.len() >= 4 { bimodality_coefficient(&d) } else { f64::NAN }
        );
    };
    report("all boards", &rows);
    // Hard boards, picked by Normal (not by A or B, so the pick can't favor either).
    let mut hard = rows.clone();
    hard.sort_by(|x, y| x.0.total_cmp(&y.0));
    hard.truncate(rows.len().div_ceil(4));
    report("hardest quarter by Normal", &hard);
}

/// A row of `k` washers with the command line's ladder and swap interval:
/// sharing one cycle's compute, or (`full`) each running the whole cycle.
fn row_of(o: &Opts, k: usize, full: bool) -> Row {
    let mut r = if full { Row::full(k) } else { Row::equal_compute(k) };
    r.swap = o.swap.unwrap_or(r.swap);
    r.cold = o.rcold.unwrap_or(r.cold);
    r.hot = o.rhot.unwrap_or(r.hot);
    r
}

/// `row`: tune a row of washers (`--washers K`, default 4) on --nights: its hit
/// rate next to Normal's, and how the swaps went.
fn row_bench(o: &Opts) {
    let row = o.row.unwrap_or_else(|| row_of(o, trade::row::WASHERS, o.full));
    let ladder: Vec<String> = row.ladder().iter().map(|t| format!("{t:.2}")).collect();
    println!(
        "row: {} washers, ladder [{}], swap every {}, {} units each (+{} settle), {} spins a night",
        row.washers,
        ladder.join(", "),
        row.swap,
        row.duration,
        row.settle,
        o.runs
    );
    let t0 = Instant::now();
    let (mut sum_n, mut sum_r, mut nights) = (0.0, 0.0, 0.0);
    for night in o.nights.0..=o.nights.1 {
        for (label, tc) in scenarios(o, night) {
            let mut on = o.clone();
            on.night = night;
            on.row = None;
            on.program = None;
            let n = hit_and_loss(&tc, &on).0;
            on.row = Some(row);
            let runs: Vec<(Latch, RowStats)> = on_all_cores(o.runs, |r| {
                let (l, _, s) = spin_one(&tc, &on, on.seed + r as u64 * 7919);
                (l, s.expect("row stats"))
            });
            let hits = runs.iter().filter(|(l, _)| tc.is_optimal(l.best_bits)).count() as f64 / runs.len() as f64;
            let pairs = row.washers.saturating_sub(1);
            let rates: Vec<String> = (0..pairs)
                .map(|i| {
                    let (a, f) = runs.iter().fold((0, 0), |(a, f), (_, s)| (a + s.accepts[i], f + s.offers[i]));
                    format!("{:.0}", 100.0 * a as f64 / f.max(1) as f64)
                })
                .collect();
            let trips = runs.iter().map(|(_, s)| s.trips).sum::<usize>() as f64 / runs.len() as f64;
            println!(
                "  night {night:>3}: Normal {:>3.0}%, row {:>3.0}%   swaps accepted % [{}], {trips:.1} hot-to-cold trips a spin  ({:.0} s){}",
                100.0 * n,
                100.0 * hits,
                rates.join(" "),
                t0.elapsed().as_secs_f64(),
                if o.mix { format!("  {label}") } else { String::new() }
            );
            sum_n += n;
            sum_r += hits;
            nights += 1.0;
        }
    }
    println!("mean: Normal {:.1}%, row {:.1}%", 100.0 * sum_n / nights, 100.0 * sum_r / nights);
}

/// `rowmatch`: the row of washers against the smart wash, on the same
/// boards and seeds, at equal compute (K washers x 1/K of a cycle vs run 5
/// alone) and as a real row (K whole cycles vs the best of K run-5 washers),
/// with the row minus its swaps as the control.
/// Paired sim-opt bootstrap CIs on the per-board differences.
fn rowmatch(o: &Opts) {
    use cortenforge::sim::opt::analysis::bootstrap_diff_means;
    use rand::SeedableRng;
    let k = o.washers.unwrap_or(trade::row::WASHERS);
    let smart = o.program.clone().unwrap_or_else(|| SmartWash::learned(o.anneal.duration));
    let names = ["Normal", "run 5", "row (equal compute)", "run 5, best of K", "row (K cycles)", "row, no swaps"];
    let contender = |c: usize| {
        let mut on = o.clone();
        on.row = None;
        on.best_of = 1;
        on.program = None;
        match c {
            1 => on.program = Some(smart.clone()),
            2 => on.row = Some(row_of(o, k, false)),
            3 => {
                on.program = Some(smart.clone());
                on.best_of = k;
            }
            4 => on.row = Some(row_of(o, k, true)),
            // The control: the same washers and ladder, never trading loads.
            5 => on.row = Some(Row { swap: f64::INFINITY, ..row_of(o, k, false) }),
            _ => {}
        }
        on
    };
    println!("rowmatch: K = {k} washers, ladder {:?}, {} spins a board, nights {}..{}", row_of(o, k, false).ladder(), o.runs, o.nights.0, o.nights.1);
    let mut rows: Vec<[f64; 6]> = vec![];
    let t0 = Instant::now();
    for night in o.nights.0..=o.nights.1 {
        for (label, tc) in scenarios(o, night) {
            let mut r = [0.0; 6];
            for (c, x) in r.iter_mut().enumerate() {
                let mut on = contender(c);
                on.night = night;
                *x = hit_and_loss(&tc, &on).0;
            }
            println!(
                "  night {night:>3}: {}  ({:.0} s){}",
                r.iter().zip(names).map(|(x, n)| format!("{n} {:>3.0}%", 100.0 * x)).collect::<Vec<_>>().join(", "),
                t0.elapsed().as_secs_f64(),
                if o.mix { format!("  {label}") } else { String::new() }
            );
            rows.push(r);
        }
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(o.seed);
    let mut report = |what: &str, rows: &[[f64; 6]]| {
        let mean = |c: usize| 100.0 * rows.iter().map(|r| r[c]).sum::<f64>() / rows.len() as f64;
        println!(
            "\n{what} ({} boards): {}",
            rows.len(),
            names.iter().enumerate().map(|(c, n)| format!("{n} {:.1}%", mean(c))).collect::<Vec<_>>().join(", ")
        );
        for (a, b) in [(2, 1), (4, 3), (4, 1), (2, 5), (2, 0)] {
            let d: Vec<f64> = rows.iter().map(|r| r[a] - r[b]).collect();
            let ci = bootstrap_diff_means(&d, &[0.0], &mut rng);
            println!(
                "  {:<42} {:+5.1} points, 95% CI [{:+5.1}, {:+5.1}] -> {:?}; boards won {}, lost {}",
                format!("{} - {}", names[a], names[b]),
                100.0 * ci.point_estimate,
                100.0 * ci.lower,
                100.0 * ci.upper,
                ci.classify(),
                d.iter().filter(|&&x| x > 0.0).count(),
                d.iter().filter(|&&x| x < 0.0).count()
            );
        }
    };
    report("all boards", &rows);
    let mut hard = rows.clone();
    hard.sort_by(|x, y| x[0].total_cmp(&y[0]));
    hard.truncate(rows.len().div_ceil(4));
    report("hardest quarter by Normal", &hard);
}

/// `print`: Upddayett's catalog (or `--part P`): each design's v1 and v2
/// through the print check, the v2 STLs into `prints/`, and what the print
/// does to tonight's board (`--night N`).
fn print_mode(o: &Opts) {
    use cortenforge_play::trade::print::{self, CATALOG};
    for p in CATALOG.iter().filter(|p| o.part.as_deref().is_none_or(|k| k == p.key)) {
        println!("\n== {} ({})", p.item, p.key);
        for fixed in [false, true] {
            let r = print::check(&p.design(fixed), print::TOL);
            println!(
                "  v{}: {} ({:.1} s){}",
                if fixed { 2 } else { 1 },
                if r.prints() { "PRINTS" } else { "WON'T PRINT" },
                r.secs,
                if fixed { format!(" -- {}", p.fix) } else { format!(" -- {}", p.flaw) }
            );
            for part in &r.parts {
                println!(
                    "    {}: {:.0} x {:.0} x {:.0} mm{}, {:.0} cm^3{}",
                    part.part,
                    part.size.x,
                    part.size.y,
                    part.size.z,
                    if part.rotated { " (turned)" } else { "" },
                    part.volume / 1000.0,
                    if part.prints { String::new() } else { format!(", {} flaw(s)", part.flaws.len()) }
                );
                for f in part.flaws.iter().take(3) {
                    println!("      {}", if f.len() > 160 { &f[..160] } else { f });
                }
            }
            if !r.design_warnings.is_empty() {
                println!("    (cf-design validate says: {}; not in the verdict, see FINDINGS)", r.design_warnings.join("; "));
            }
            if fixed && r.prints() {
                match print::write_stls(&r, p.key, std::path::Path::new("prints")) {
                    Ok(paths) => println!("    wrote {}", paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")),
                    Err(e) => println!("    STL: {e}"),
                }
            }
        }
        // What the print does to tonight's board.
        let before = board_for(o, trade::world::laundromat(o.night));
        let mut w = trade::world::laundromat(o.night);
        let item = w.add_print(p);
        let after = board_for(o, w);
        let carry: Vec<String> = after.cycles.iter().filter(|c| c.legs.iter().any(|l| l.item == item)).map(|c| c.short(&after.world)).collect();
        let used: Vec<String> = qubo::chosen(after.forward_best, after.cycles.len())
            .into_iter()
            .filter(|&i| after.cycles[i].legs.iter().any(|l| l.item == item))
            .map(|i| after.cycles[i].short(&after.world))
            .collect();
        println!(
            "  night {}: best set {} -> {}; {} candidate trade(s) move it{}",
            o.night,
            before.score_text(before.forward_best),
            after.score_text(after.forward_best),
            carry.len(),
            if used.is_empty() { ", none in the best set".to_string() } else { format!("; the best set uses {}", used.join(", ")) }
        );
    }
}

fn main() -> Result<(), trade::Error> {
    let mut o = parse();
    if o.smart || o.params.is_some() || o.restarts.is_some() || o.patience.is_some() {
        let d = o.anneal.duration;
        let mut p = o.params.as_ref().map_or_else(|| SmartWash::learned(d), |p| SmartWash::new(d, p));
        // --restarts and --patience override the params.
        if let Some(k) = o.restarts {
            p = p.with_restarts(k);
        }
        if let Some(f) = o.patience {
            p = p.with_patience(f);
        }
        o.program = Some(p);
    }
    if let Some(k) = o.washers.filter(|_| o.mode != "rowmatch") {
        o.row = Some(row_of(&o, k, o.full));
    }
    if o.salty {
        apply_salties(&mut o);
    }
    if o.coil && o.magnet.is_none() {
        o.magnet = Some(aim_coil(&o).0);
    }
    match o.mode.as_str() {
        "run" => run(&o),
        "bench" => bench(&o),
        "learn" => learn(&o),
        "rematch" => {
            let b = o.vs.as_ref().map_or_else(|| SmartWash::new(o.anneal.duration, &smart::RUN_3), |p| SmartWash::new(o.anneal.duration, p));
            rematch(&o, &o.program.clone().unwrap_or_else(|| SmartWash::learned(o.anneal.duration)), &b);
            Ok(())
        }
        "row" => {
            row_bench(&o);
            Ok(())
        }
        "print" => {
            print_mode(&o);
            Ok(())
        }
        "rowmatch" => {
            rowmatch(&o);
            Ok(())
        }
        "yuck" => {
            yuck_mode(&o);
            Ok(())
        }
        "versus" => {
            versus(&o, &o.program.clone().unwrap_or_else(|| SmartWash::learned(o.anneal.duration)));
            Ok(())
        }
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

/// Make people yucky on `w` per the options: `--yuck WHO,..` by name and
/// `--pump P` by tonight's conditions, each taxing trades `--tax T` Goo.
fn apply_yuck(o: &Opts, w: &mut trade::world::World, say: bool) {
    let mut who: Vec<usize> = o.yuck.iter().map(|n| w.find_npc(n).unwrap_or_else(|| panic!("nobody called {n}"))).collect();
    if let Some(p) = &o.pump {
        assert!(matches!(p.as_str(), "hungry" | "cold"), "--pump hungry|cold");
        who.extend(w.pump(p));
    }
    who.sort_unstable();
    who.dedup();
    for &k in &who {
        w.set_yuck(k, o.tax);
    }
    if say && !who.is_empty() {
        let names: Vec<&str> = who.iter().map(|&k| w.npcs[k].name).collect();
        println!("Yucky tonight ({} Goo tax a trade): {}", o.tax, names.join(", "));
    }
}

/// How glassy a board is: the valid sets within 10% of the best set's value
/// that aren't tied with it (rivals), and the most strips any rival differs
/// from the best set by.
fn glass(tc: &TradeComputer) -> (usize, u32) {
    let masks = qubo::conflict_masks(&tc.cycles);
    let value = |s: u32| qubo::chosen(s, tc.cycles.len()).iter().map(|&i| tc.cycles[i].value()).sum::<f64>();
    let best_bits = tc.ground_state();
    let best = value(best_bits);
    let (mut rivals, mut far) = (0, 0);
    qubo::for_each_valid_set(&masks, |s| {
        let v = value(s);
        if v < best - 1e-9 && v >= 0.9 * best {
            rivals += 1;
            far = far.max((s ^ best_bits).count_ones());
        }
    });
    (rivals, far)
}

/// `yuck`: does yuck make the street harder to compute? On each of --nights,
/// three boards with a `--tax T` Goo yuck tax: clean, two yucky people
/// (picked by the night), and a pump (`--pump`, default hungry). For each:
/// strips, trades, the best set's Goo, how glassy it is, and the machine's
/// hit rate and Goo lost over --runs spins. Then paired sim-opt bootstrap
/// CIs against clean.
fn yuck_mode(o: &Opts) {
    use cortenforge::sim::opt::analysis::bootstrap_diff_means;
    use rand::SeedableRng;
    let pump = o.pump.clone().unwrap_or_else(|| "hungry".into());
    let labels = ["clean".to_string(), "2 people".to_string(), format!("{pump} pump")];
    println!(
        "yuck: tax {} Goo a trade, nights {}..{}, {} spins a board ({})",
        o.tax,
        o.nights.0,
        o.nights.1,
        o.runs,
        if o.program.is_some() { "smart wash" } else { "Normal" }
    );
    // Per night and board: [hit, lost, strips, trades, best Goo, rivals, far, yucky].
    let mut rows: Vec<[[f64; 8]; 3]> = vec![];
    let t0 = Instant::now();
    for night in o.nights.0..=o.nights.1 {
        let base = trade::world::laundromat(night);
        let n = base.npcs.len();
        let a = (night as usize * 7) % n;
        let b = (a + 1 + (night as usize * 3) % (n - 1)) % n;
        let who = [vec![], vec![a, b], base.pump(&pump)];
        let mut row = [[0.0; 8]; 3];
        let mut on = o.clone();
        on.night = night;
        for (k, people) in who.iter().enumerate() {
            let mut w = base.clone();
            for &p in people {
                w.set_yuck(p, o.tax);
            }
            let tc = board_for(&on, w);
            let (hit, lost, _) = hit_and_loss(&tc, &on);
            let (rivals, far) = glass(&tc);
            let trades = tc.cycles.iter().filter(|c| !c.is_gift()).count();
            let best: f64 = qubo::chosen(tc.ground_state(), tc.cycles.len()).iter().map(|&i| tc.cycles[i].value()).sum();
            row[k] = [hit, lost, tc.cycles.len() as f64, trades as f64, best, rivals as f64, far as f64, people.len() as f64];
        }
        println!(
            "  night {night:>3}: {}  ({:.0} s)",
            row.iter()
                .zip(&labels)
                .map(|(r, l)| format!(
                    "{l} [{} yucky] {:>3.0}% lost {:>4.1}, {:>2} strips {:>2} trades, best {:>5.1}, rivals {:>3} far {:>2}",
                    r[7], 100.0 * r[0], r[1], r[2], r[3], r[4], r[5], r[6]
                ))
                .collect::<Vec<_>>()
                .join(" | "),
            t0.elapsed().as_secs_f64()
        );
        rows.push(row);
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(o.seed);
    let stats = ["hit rate (points)", "Goo lost a spin", "strips", "trades", "best set Goo", "rivals", "far"];
    for k in [1, 2] {
        // A pump that touched nobody tonight is just the clean board again.
        let used: Vec<&[[f64; 8]; 3]> = rows.iter().filter(|r| r[k][7] > 0.0).collect();
        println!("\n{} - clean ({} nights where it touched anyone):", labels[k], used.len());
        if used.len() < 2 {
            continue;
        }
        for (s, name) in stats.iter().enumerate() {
            let scale = if s == 0 { 100.0 } else { 1.0 };
            let d: Vec<f64> = used.iter().map(|r| scale * (r[k][s] - r[0][s])).collect();
            let ci = bootstrap_diff_means(&d, &[0.0], &mut rng);
            let mean = |b: usize| scale * used.iter().map(|r| r[b][s]).sum::<f64>() / used.len() as f64;
            println!(
                "  {name:<18} clean {:>7.2}, yucky {:>7.2}: {:+7.2}, 95% CI [{:+7.2}, {:+7.2}] -> {}",
                mean(0),
                mean(k),
                ci.point_estimate,
                ci.lower,
                ci.upper,
                // Not `classify()`: it calls a CI wholly below zero "Null" (FINDINGS sim-opt).
                if ci.lower > 0.0 { "up" } else if ci.upper < 0.0 { "down" } else { "can't tell" }
            );
        }
    }
}
