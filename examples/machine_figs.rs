//! Figures for `docs/MACHINE.md` ("How the machine works") and the site's
//! lessons (`site/`, upddayettacademy.com), plotted from the real machine:
//! every curve and dot is a CortenForge run or the night's real board, not a
//! drawing (the settlement figure is a labeled model).
//!
//! `cargo run --release --example machine_figs -- [all|pipeline|strip|board|landscape|spin|freeze|programs|fees|noise|settle|waits|collapse] [--night N]`
//! writes the machine figures to `docs/machine/*.svg` (and copies to
//! `site/figs/machine/`), the lessons to `site/figs/lessons/`. Without `--night` it picks the first night
//! (from 1) where grabbing the best trade first loses Goo, so the board figure
//! has something to show.

use std::fmt::Write as _;
use std::sync::Mutex;

use cortenforge::sim::thermostat::{DoubleWellPotential, WellState};
use cortenforge_play::trade::{self, Anneal, Cycle, Ising, Machine, Physics, TradeComputer, qubo};

const OUT: &str = "docs/machine";
/// The site (upddayettacademy.com) keeps its own copies.
const SITE_MACHINE: &str = "site/figs/machine";
const SITE_LESSONS: &str = "site/figs/lessons";
const BETA: f64 = 5.0;
const PENALTY: f64 = 1.6;

// ---- colors (the game's, darkened to read on white) ----
const INK: &str = "#1d2026";
const DIM: &str = "#7a808c";
const GRID: &str = "#e6e8ec";
const GOO: &str = "#3f9a12";
const GOLD: &str = "#d99a00";
const GIFT: &str = "#e8761a";
const RED: &str = "#d8392b";
const ICE: &str = "#3d8fd6";
const PURPLE: &str = "#8a4fc2";

// ---- a tiny SVG kit ----

struct Svg {
    w: f64,
    h: f64,
    body: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Svg {
    fn new(w: f64, h: f64) -> Self {
        Self { w, h, body: String::new() }
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), color: &str, width: f64, dash: Option<&str>) {
        let dash = dash.map(|d| format!(r#" stroke-dasharray="{d}""#)).unwrap_or_default();
        let _ = writeln!(
            self.body,
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="{width}"{dash}/>"#,
            a.0, a.1, b.0, b.1
        );
    }

    fn path(&mut self, pts: &[(f64, f64)], color: &str, width: f64, opacity: f64, dash: Option<&str>) {
        if pts.len() < 2 {
            return;
        }
        let mut d = String::new();
        for (k, (x, y)) in pts.iter().enumerate() {
            let _ = write!(d, "{}{:.1},{:.1} ", if k == 0 { "M" } else { "L" }, x, y);
        }
        let dash = dash.map(|d| format!(r#" stroke-dasharray="{d}""#)).unwrap_or_default();
        let _ = writeln!(
            self.body,
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" stroke-opacity="{opacity}" stroke-linejoin="round"{dash}/>"#
        );
    }

    fn circle(&mut self, c: (f64, f64), r: f64, fill: &str, stroke: &str, width: f64) {
        let _ = writeln!(
            self.body,
            r#"<circle cx="{:.1}" cy="{:.1}" r="{r:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{width}"/>"#,
            c.0, c.1
        );
    }

    fn rect(&mut self, at: (f64, f64), size: (f64, f64), radius: f64, fill: &str, stroke: &str) {
        let _ = writeln!(
            self.body,
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{radius}" fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>"#,
            at.0, at.1, size.0, size.1
        );
    }

    /// `anchor`: "start", "middle" or "end".
    fn text(&mut self, at: (f64, f64), s: &str, size: f64, color: &str, anchor: &str, weight: &str) {
        let _ = writeln!(
            self.body,
            r#"<text x="{:.1}" y="{:.1}" font-size="{size}" fill="{color}" text-anchor="{anchor}" font-weight="{weight}">{}</text>"#,
            at.0,
            at.1,
            esc(s)
        );
    }

    fn vtext(&mut self, at: (f64, f64), s: &str, size: f64, color: &str) {
        let _ = writeln!(
            self.body,
            r#"<text transform="translate({:.1},{:.1}) rotate(-90)" font-size="{size}" fill="{color}" text-anchor="middle">{}</text>"#,
            at.0,
            at.1,
            esc(s)
        );
    }

    fn arrow(&mut self, a: (f64, f64), b: (f64, f64), color: &str) {
        self.line(a, b, color, 2.0, None);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (ux, uy) = (dx / l, dy / l);
        let p1 = (b.0 - 9.0 * ux + 4.5 * uy, b.1 - 9.0 * uy - 4.5 * ux);
        let p2 = (b.0 - 9.0 * ux - 4.5 * uy, b.1 - 9.0 * uy + 4.5 * ux);
        let _ = writeln!(
            self.body,
            r#"<path d="M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z" fill="{color}"/>"#,
            b.0, b.1, p1.0, p1.1, p2.0, p2.1
        );
    }

    /// A machine figure: for `docs/MACHINE.md` and the site's machine page.
    fn save(&self, name: &str) {
        self.save_to(OUT, name);
        self.save_to(SITE_MACHINE, name);
    }

    fn save_to(&self, dir: &str, name: &str) {
        std::fs::create_dir_all(dir).expect("figure dir");
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" \
             font-family=\"Segoe UI, Helvetica, Arial, sans-serif\">\n<rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>\n{}</svg>\n",
            self.body,
            w = self.w,
            h = self.h
        );
        let path = format!("{dir}/{name}.svg");
        std::fs::write(&path, svg).expect("write svg");
        println!("wrote {path}");
    }
}

/// A plot area: data ranges mapped onto a pixel rectangle.
#[derive(Clone, Copy)]
struct Axes {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    log_x: bool,
    log_y: bool,
}

impl Axes {
    fn new(rect: (f64, f64, f64, f64), xr: (f64, f64), yr: (f64, f64)) -> Self {
        Self { x: rect.0, y: rect.1, w: rect.2, h: rect.3, x0: xr.0, x1: xr.1, y0: yr.0, y1: yr.1, log_x: false, log_y: false }
    }

    fn logs(mut self, x: bool, y: bool) -> Self {
        self.log_x = x;
        self.log_y = y;
        self
    }

    fn fx(&self, v: f64) -> f64 {
        let (v, a, b) = if self.log_x { (v.ln(), self.x0.ln(), self.x1.ln()) } else { (v, self.x0, self.x1) };
        self.x + (v - a) / (b - a) * self.w
    }

    fn fy(&self, v: f64) -> f64 {
        let (v, a, b) = if self.log_y { (v.ln(), self.y0.ln(), self.y1.ln()) } else { (v, self.y0, self.y1) };
        self.y + self.h - (v - a) / (b - a) * self.h
    }

    fn p(&self, x: f64, y: f64) -> (f64, f64) {
        (self.fx(x), self.fy(y))
    }

    /// Frame, grid and tick labels.
    fn draw(&self, s: &mut Svg, xticks: &[(f64, &str)], yticks: &[(f64, &str)], xlabel: &str, ylabel: &str) {
        for &(v, label) in xticks {
            let x = self.fx(v);
            s.line((x, self.y), (x, self.y + self.h), GRID, 1.0, None);
            s.text((x, self.y + self.h + 16.0), label, 11.0, DIM, "middle", "normal");
        }
        for &(v, label) in yticks {
            let y = self.fy(v);
            s.line((self.x, y), (self.x + self.w, y), GRID, 1.0, None);
            s.text((self.x - 6.0, y + 4.0), label, 11.0, DIM, "end", "normal");
        }
        s.rect((self.x, self.y), (self.w, self.h), 0.0, "none", "#c5c9d1");
        if !xlabel.is_empty() {
            s.text((self.x + self.w / 2.0, self.y + self.h + 34.0), xlabel, 12.0, INK, "middle", "normal");
        }
        if !ylabel.is_empty() {
            s.vtext((self.x - 42.0, self.y + self.h / 2.0), ylabel, 12.0, INK);
        }
    }
}

// ---- the night ----

fn computer(night: u64) -> TradeComputer {
    TradeComputer::new(trade::world::laundromat(night), BETA, PENALTY)
}

/// Grab the best-scoring strip, then the best one that doesn't collide, and
/// so on: what a person does by eye.
fn greedy(cycles: &[Cycle], masks: &[u32]) -> u32 {
    let mut order: Vec<usize> = (0..cycles.len()).collect();
    order.sort_by(|&a, &b| cycles[b].value().total_cmp(&cycles[a].value()));
    let mut set = 0u32;
    for i in order {
        if masks[i] & set == 0 {
            set |= 1 << i;
        }
    }
    set
}

fn set_value(cycles: &[Cycle], set: u32) -> f64 {
    qubo::chosen(set, cycles.len()).iter().map(|&i| cycles[i].value()).sum()
}

fn pick_night() -> u64 {
    (1..400)
        .find(|&n| {
            let tc = computer(n);
            let masks = qubo::conflict_masks(&tc.cycles);
            let best = set_value(&tc.cycles, tc.ground_state());
            let g = set_value(&tc.cycles, greedy(&tc.cycles, &masks));
            (14..=20).contains(&tc.cycles.len()) && best - g >= 3.0
        })
        .expect("a night where greedy loses")
}

fn initials(name: &str) -> String {
    name.split([' ', '-']).filter_map(|w| w.chars().next()).take(2).collect()
}

fn chain(c: &Cycle, w: &trade::World) -> String {
    let mut s = initials(w.npcs[c.legs[0].from].name);
    for l in &c.legs {
        s.push('>');
        s.push_str(&initials(w.npcs[l.to].name));
    }
    s
}

// ---- the figures ----

/// The whole machine on one line: from the street to the counter.
fn pipeline() {
    let mut s = Svg::new(980.0, 190.0);
    let steps = [
        ("The street", "haves, wants,", "tonight's Goo", GOLD),
        ("Loops", "every 2-4 way swap", "where all gain", GOLD),
        ("Energy", "reward each loop,", "punish collisions", PURPLE),
        ("Strips", "one strip per loop,", "springs between", ICE),
        ("Spin", "shake hot, cool", "slowly (Langevin)", RED),
        ("The i9", "keeps the best set", "it reads", GOO),
        ("The counter", "hands every trade", "over at once", GIFT),
    ];
    let (bw, gap) = (122.0, 15.0);
    for (k, (title, a, b, color)) in steps.iter().enumerate() {
        let x = 12.0 + k as f64 * (bw + gap);
        s.rect((x, 40.0), (bw, 100.0), 10.0, "#f6f7f9", color);
        s.rect((x, 40.0), (bw, 8.0), 4.0, color, color);
        s.text((x + bw / 2.0, 74.0), title, 15.0, INK, "middle", "bold");
        s.text((x + bw / 2.0, 98.0), a, 11.5, DIM, "middle", "normal");
        s.text((x + bw / 2.0, 114.0), b, 11.5, DIM, "middle", "normal");
        if k + 1 < steps.len() {
            s.arrow((x + bw + 1.0, 90.0), (x + bw + gap - 1.0, 90.0), INK);
        }
    }
    s.text((12.0, 24.0), "From a laundromat full of stuff to a set of trades: what happens on every spin", 14.0, INK, "start", "bold");
    s.text((12.0, 170.0), "Gold: the trade problem.  Purple: math.  Blue and red: CortenForge's physics.  Green and orange: what the game does with the answer.", 11.5, DIM, "start", "normal");
    s.save("pipeline");
}

/// One strip's energy: two wells, the tilt a good trade gives it, and the
/// push a colliding neighbor gives it.
fn strip(tc: &TradeComputer) {
    let p = Physics::default();
    let well = DoubleWellPotential::new(p.delta_v, 1.0, 0);
    // The best strip's idle field (every other strip off) and the push it
    // gets from its strongest colliding neighbor when that one is on.
    let idle = tc.idle_fields();
    let top = (0..tc.ising.n).max_by(|&a, &b| idle[a].total_cmp(&idle[b])).unwrap();
    let h = idle[top];
    let push = tc
        .ising
        .edges
        .iter()
        .zip(&tc.ising.j)
        .filter(|((a, b), _)| *a == top || *b == top)
        .map(|(_, j)| 2.0 * j)
        .fold(0.0, f64::min);
    let e = |x: f64, field: f64| well.potential(x) - field * x;
    let mut s = Svg::new(900.0, 400.0);
    let ax = Axes::new((70.0, 50.0, 470.0, 280.0), (-1.7, 1.7), (-12.0, 16.0));
    ax.draw(
        &mut s,
        &[(-1.0, "-1  (off)"), (0.0, "0"), (1.0, "+1  (on)")],
        &[(-10.0, "-10"), (-5.0, "-5"), (0.0, "0"), (5.0, "5"), (10.0, "10"), (15.0, "15")],
        "strip position x (how far it's bent)",
        "energy (kT)",
    );
    let curve = |field: f64| -> Vec<(f64, f64)> {
        (0..=200).map(|i| -1.7 + 3.4 * i as f64 / 200.0).map(|x| ax.p(x, e(x, field))).filter(|p| p.1 >= ax.y - 2.0).collect()
    };
    s.path(&curve(0.0), DIM, 2.5, 1.0, Some("6 4"));
    s.path(&curve(h), GOO, 3.0, 1.0, None);
    s.path(&curve(h + push), RED, 3.0, 1.0, None);
    s.text((70.0, 30.0), "One strip, three situations", 15.0, INK, "start", "bold");
    let lx = 560.0;
    let legend = [
        (DIM, "Alone: two equal dips, off and on,".to_string(), format!("a hump of {:.0} kT between them.", p.delta_v)),
        (GOO, format!("Tonight's best trade, others off:"), format!("tilted toward on by a field of {h:.1}.")),
        (RED, "Same trade, a colliding trade on:".to_string(), "its spring tips it back toward off.".to_string()),
    ];
    for (k, (c, a, b)) in legend.iter().enumerate() {
        let y = 80.0 + k as f64 * 70.0;
        s.line((lx, y), (lx + 24.0, y), c, 3.0, None);
        s.text((lx + 32.0, y + 4.0), a, 12.0, INK, "start", "normal");
        s.text((lx + 32.0, y + 22.0), b, 12.0, DIM, "start", "normal");
    }
    s.text((lx, 305.0), "The drum's shaking (kT) decides how", 12.0, INK, "start", "normal");
    s.text((lx, 323.0), "often a strip gets kicked over the hump.", 12.0, INK, "start", "normal");
    s.text(
        (70.0, 380.0),
        &format!(
            "Curves: CortenForge's DoubleWellPotential (dV = {:.0}, x0 = 1) minus the field term h x. Fields from night {}'s real board.",
            p.delta_v,
            tc.world.night.seed
        ),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.save("strip");
}

/// Tonight's candidate trades as a collision graph: the best set vs grabbing
/// the biggest first.
fn board(tc: &TradeComputer) -> (f64, f64) {
    let n = tc.cycles.len();
    let masks = qubo::conflict_masks(&tc.cycles);
    let best = tc.ground_state();
    let g = greedy(&tc.cycles, &masks);
    let (bv, gv) = (set_value(&tc.cycles, best), set_value(&tc.cycles, g));
    let mut s = Svg::new(1010.0, 590.0);
    let pos = |k: usize| -> (f64, f64) {
        let a = std::f64::consts::TAU * k as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
        (340.0 + 215.0 * a.cos(), 312.0 + 215.0 * a.sin())
    };
    for i in 0..n {
        for k in i + 1..n {
            if masks[i] >> k & 1 == 1 {
                let both = (best >> i & 1) + (best >> k & 1);
                s.line(pos(i), pos(k), if both > 0 { "#c9ccd3" } else { "#e1e3e8" }, 1.2, None);
            }
        }
    }
    for (k, c) in tc.cycles.iter().enumerate() {
        let at = pos(k);
        let on = best >> k & 1 == 1;
        let greedy_on = g >> k & 1 == 1;
        let fill = if on { GOO } else { "#ffffff" };
        let ring = if c.is_gift() { GIFT } else { INK };
        if greedy_on {
            s.circle(at, 27.0, "none", GOLD, 4.0);
        }
        s.circle(at, 20.0, fill, ring, if c.is_gift() { 3.0 } else { 1.5 });
        let unit = if c.is_gift() { "K" } else { "" };
        s.text((at.0, at.1 + 5.0), &format!("{:.0}{unit}", c.value()), 14.0, if on { "#ffffff" } else { INK }, "middle", "bold");
        let a = std::f64::consts::TAU * k as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
        let lab = (340.0 + 262.0 * a.cos(), 312.0 + 255.0 * a.sin() + 4.0);
        let anchor = if a.cos() > 0.3 { "start" } else if a.cos() < -0.3 { "end" } else { "middle" };
        s.text(lab, &chain(c, &tc.world), 10.5, DIM, anchor, "normal");
    }
    s.text((20.0, 30.0), &format!("Night {}: {} strips, and every line is a collision", tc.world.night.seed, n), 15.0, INK, "start", "bold");
    let lx = 650.0;
    s.circle((lx + 10.0, 90.0), 12.0, GOO, INK, 1.5);
    s.text((lx + 32.0, 95.0), &format!("the best set: {bv:.0}"), 13.0, INK, "start", "bold");
    s.circle((lx + 10.0, 130.0), 14.0, "none", GOLD, 4.0);
    s.text((lx + 32.0, 135.0), &format!("grab-the-biggest-first: {gv:.0}"), 13.0, INK, "start", "bold");
    s.circle((lx + 10.0, 170.0), 12.0, "#ffffff", GIFT, 3.0);
    s.text((lx + 32.0, 175.0), "a gift (scored in Karma, K)", 13.0, INK, "start", "normal");
    let lines = [
        "Each circle is one candidate trade: a 2-4 way loop",
        "where everyone gains. The number is what it's worth.",
        "A line joins two trades that need the same item,",
        "so at most one of them can happen.",
        "",
        "Grabbing the biggest trade first looks smart, but it",
        "knocks out its neighbors, and they were worth more",
        "together. The machine has to weigh every trade",
        "against all the others at once.",
        "",
        "Labels: who hands to whom (U = Upddayett, VL = Vape",
        "Lady, AP = Amir's Persian Kitchen, ...).",
    ];
    for (k, l) in lines.iter().enumerate() {
        s.text((lx, 225.0 + k as f64 * 19.0), l, 12.5, if k >= 10 { DIM } else { INK }, "start", "normal");
    }
    s.save("board");
    (bv, gv)
}

/// Every valid set of trades on the night, by what it's worth: the
/// near-ties that make the puzzle hard.
fn landscape(tc: &TradeComputer) -> (usize, usize, usize) {
    let masks = qubo::conflict_masks(&tc.cycles);
    let mut values = vec![];
    qubo::for_each_valid_set(&masks, |set| values.push(set_value(&tc.cycles, set)));
    let best = values.iter().cloned().fold(0.0, f64::max);
    let near = values.iter().filter(|v| **v >= best - 2.0 - 1e-9).count();
    let top = (best.ceil() as usize) + 1;
    let mut hist = vec![0usize; top + 1];
    for v in &values {
        hist[v.round() as usize] += 1;
    }
    let ymax = *hist.iter().max().unwrap() as f64;
    let mut s = Svg::new(760.0, 420.0);
    let ax = Axes::new((80.0, 50.0, 620.0, 270.0), (-0.5, top as f64 + 0.5), (0.0, ymax * 1.08));
    let xt: Vec<(f64, String)> = (0..=top).step_by(5).map(|v| (v as f64, v.to_string())).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    let step = if ymax > 400.0 { 100.0 } else if ymax > 100.0 { 50.0 } else { 10.0 };
    let yt: Vec<(f64, String)> = (0..=(ymax / step) as usize).map(|k| (k as f64 * step, format!("{}", k as f64 * step))).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    ax.draw(&mut s, &xt, &yt, "what a set of trades is worth (Goo + Karma)", "how many valid sets");
    let bw = ax.w / (top as f64 + 1.0) * 0.8;
    for (v, &c) in hist.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let color = if v as f64 >= best - 2.0 - 1e-9 { RED } else { "#aab0bb" };
        let (x, y) = ax.p(v as f64, c as f64);
        s.rect((x - bw / 2.0, y), (bw, ax.fy(0.0) - y), 1.0, color, color);
    }
    let bx = ax.fx(best);
    s.text((bx, ax.y - 8.0), "the best", 12.0, RED, "end", "bold");
    let n = tc.cycles.len();
    s.text((80.0, 30.0), &format!("Night {}: every set of trades that doesn't collide", tc.world.night.seed), 15.0, INK, "start", "bold");
    s.text(
        (80.0, 380.0),
        &format!(
            "{n} strips make 2^{n} = {} on/off patterns; only {} of them are valid (no item moves twice).",
            1u64 << n,
            values.len()
        ),
        12.0,
        INK,
        "start",
        "normal",
    );
    s.text(
        (80.0, 400.0),
        &format!("Red: the {near} sets within 2 of the best. They look almost as good, and can be many strip flips away from it."),
        12.0,
        INK,
        "start",
        "normal",
    );
    s.save("landscape");
    (n, values.len(), near)
}

/// What one Normal spin records.
struct Trace {
    t: Vec<f64>,
    kt: Vec<f64>,
    x: Vec<Vec<f64>>,
    /// The score (Goo) of the set the strips show, when all sit in wells.
    score: Vec<Option<f64>>,
    latch: Vec<f64>,
    latch_at: f64,
    hit: bool,
    final_hit: bool,
    final_bits: u32,
}

fn spin_trace(tc: &TradeComputer, seed: u64) -> Trace {
    let anneal = Anneal::default();
    let mut m = tc.machine(Physics::default(), seed).expect("machine");
    let q = &tc.problem.qubo;
    let scale = tc.problem.scale;
    let mut tr = Trace { t: vec![], kt: vec![], x: vec![vec![]; tc.ising.n], score: vec![], latch: vec![], latch_at: 0.0, hit: false, final_hit: false, final_bits: 0 };
    let latch = tc
        .spin(&mut m, &anneal, 1.0, 1.0, |m, l| {
            tr.t.push(m.time());
            tr.kt.push(m.temperature());
            for (i, x) in m.positions().iter().enumerate() {
                tr.x[i].push(*x);
            }
            tr.score.push(m.all_in_wells().then(|| -q.energy(m.bits()) / scale));
            tr.latch.push(if l.has_best() { -l.best_energy / scale } else { f64::NAN });
        })
        .expect("spin");
    tr.latch_at = latch.best_time;
    tr.hit = tc.is_optimal(latch.best_bits);
    tr.final_hit = tc.is_optimal(latch.final_bits);
    tr.final_bits = latch.final_bits;
    tr
}

/// The hero: one real Normal spin, strip by strip.
fn spin(tc: &TradeComputer) -> (u64, usize, usize, usize) {
    // Run 48 spins (for honest numbers), then show a typical good one: it
    // comes to rest on the best set, latched at about the median time.
    let seeds: Vec<u64> = (1..=48).collect();
    let results = Mutex::new(vec![]);
    std::thread::scope(|sc| {
        for chunk in seeds.chunks(4) {
            let results = &results;
            sc.spawn(move || {
                for &seed in chunk {
                    let mut m = tc.machine(Physics::default(), seed).expect("machine");
                    let l = tc.spin(&mut m, &Anneal::default(), 1.0, f64::INFINITY, |_, _| {}).expect("spin");
                    results.lock().unwrap().push((seed, tc.is_optimal(l.best_bits), tc.is_optimal(l.final_bits), l.best_time));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);
    let hits = results.iter().filter(|r| r.1).count();
    let rest = results.iter().filter(|r| r.2).count();
    let mut times: Vec<f64> = results.iter().filter(|r| r.1).map(|r| r.3).collect();
    times.sort_by(f64::total_cmp);
    let median = times.get(times.len() / 2).copied().unwrap_or(0.0);
    let seed = results
        .iter()
        .filter(|r| r.1 && r.2)
        .min_by(|a, b| (a.3 - median).abs().total_cmp(&(b.3 - median).abs()))
        .or(results.iter().find(|r| r.1))
        .map_or(1, |r| r.0);
    let tr = spin_trace(tc, seed);
    let best = tc.ground_state();
    // Night 1 has exact ties: the spin may rest on another set worth the
    // same, so the lanes mark the set this spin found.
    let found = tr.final_bits;
    let masks = qubo::conflict_masks(&tc.cycles);
    let bestv0 = set_value(&tc.cycles, best);
    let mut ties = 0;
    qubo::for_each_valid_set(&masks, |set| ties += usize::from((set_value(&tc.cycles, set) - bestv0).abs() < 1e-9));
    let end = *tr.t.last().unwrap();
    let anneal = Anneal::default();
    let n = tc.ising.n;

    let lane = 15.0;
    let lanes_h = n as f64 * lane;
    let (y1, y2) = (50.0, 50.0 + 90.0 + 30.0);
    let y3 = y2 + lanes_h + 30.0;
    let y4 = y3 + 100.0 + 30.0;
    let height = y4 + 150.0 + 80.0;
    let mut s = Svg::new(1010.0, height);
    s.text(
        (70.0, 28.0),
        &format!("One real spin: the Normal program on night {}, every strip, every time unit (seed {seed})", tc.world.night.seed),
        15.0,
        INK,
        "start",
        "bold",
    );
    let xt: Vec<(f64, String)> = (0..=10).map(|k| (k as f64 * 100.0, format!("{}", k * 100))).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    let no_x: [(f64, &str); 0] = [];

    // 1: the drum.
    let a1 = Axes::new((70.0, y1, 860.0, 90.0), (0.0, end), (0.3, 6.0)).logs(false, true);
    a1.draw(&mut s, &no_x, &[(0.35, "0.35"), (1.0, "1"), (4.0, "4")], "", "drum kT");
    let pts: Vec<_> = tr.t.iter().zip(&tr.kt).filter(|(_, k)| **k > 0.0).map(|(t, k)| a1.p(*t, *k)).collect();
    s.path(&pts, RED, 2.5, 1.0, None);
    s.text((a1.fx(20.0), a1.fy(4.0) + 34.0), "hot: the strips hop freely", 11.5, RED, "start", "bold");
    s.text((a1.fx(anneal.duration) - 6.0, a1.fy(0.35) - 8.0), "cold: frozen", 11.5, RED, "end", "bold");

    // 2: every strip, one lane each: green on, grey off, gold on the hump.
    let a2 = Axes::new((70.0, y2, 860.0, lanes_h), (0.0, end), (0.0, 1.0));
    s.text((70.0, y2 - 8.0), "Each row is one strip (one candidate trade): green = on, grey = off, gold = on the hump, mid-flip", 12.0, INK, "start", "bold");
    for i in 0..n {
        let top = y2 + i as f64 * lane;
        let state = |x: f64| match WellState::from_position(x, 0.5) {
            WellState::Right => 2u8,
            WellState::Left => 0,
            _ => 1,
        };
        let xs = &tr.x[i];
        let mut k = 0;
        while k < xs.len() {
            let st = state(xs[k]);
            let mut e = k + 1;
            while e < xs.len() && state(xs[e]) == st {
                e += 1;
            }
            let x0 = a2.fx(tr.t[k] - 1.0);
            let x1 = a2.fx(tr.t[e - 1]);
            let color = match st {
                2 => GOO,
                1 => GOLD,
                _ => "#dfe2e7",
            };
            let _ = writeln!(
                s.body,
                r#"<rect x="{x0:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{color}"/>"#,
                top + 1.5,
                (x1 - x0).max(0.6),
                lane - 3.0
            );
            k = e;
        }
        let c = &tc.cycles[i];
        let unit = if c.is_gift() { "K" } else { "" };
        s.text((64.0, top + lane - 4.0), &format!("{:.0}{unit}", c.value()), 10.5, if c.is_gift() { GIFT } else { DIM }, "end", "normal");
        if found >> i & 1 == 1 {
            s.circle((a2.x + a2.w + 12.0, top + lane / 2.0), 4.5, GOO, "none", 0.0);
        }
    }
    s.rect((a2.x, a2.y), (a2.w, a2.h), 0.0, "none", "#c5c9d1");
    s.text((a2.x + a2.w + 22.0, y2 + 10.0), "where", 10.5, GOO, "start", "bold");
    s.text((a2.x + a2.w + 22.0, y2 + 23.0), "it ends", 10.5, GOO, "start", "bold");
    s.vtext((30.0, y2 + lanes_h / 2.0), "worth (Goo, K = Karma)", 11.0, INK);

    // 3: one strip close up: the busiest strip of the best set.
    let flips = |xs: &[f64]| xs.windows(2).filter(|w| (w[0] > 0.0) != (w[1] > 0.0)).count();
    let pick = (0..n).filter(|&i| found >> i & 1 == 1).max_by_key(|&i| flips(&tr.x[i])).unwrap_or(0);
    let a3 = Axes::new((70.0, y3, 860.0, 100.0), (0.0, end), (-2.0, 2.0));
    a3.draw(&mut s, &no_x, &[(-1.0, "off"), (1.0, "on")], "", "");
    let pts: Vec<_> = tr.t.iter().zip(&tr.x[pick]).map(|(t, x)| a3.p(*t, x.clamp(-2.0, 2.0))).collect();
    s.path(&pts, ICE, 1.3, 1.0, None);
    s.text(
        (70.0, y3 - 8.0),
        &format!("Close up: the strip for {} ({:.0} {}), its real position. It rattles in its dip and hops while the drum is hot.", chain(&tc.cycles[pick], &tc.world), tc.cycles[pick].value(), if tc.cycles[pick].is_gift() { "Karma" } else { "Goo" }),
        12.0,
        INK,
        "start",
        "bold",
    );

    // 4: the score the i9 reads, and what it keeps.
    let bestv = set_value(&tc.cycles, best);
    let lo = tr.score.iter().flatten().cloned().fold(bestv, f64::min).max(-bestv).min(0.0);
    let a4 = Axes::new((70.0, y4, 860.0, 150.0), (0.0, end), (lo - 2.0, bestv + 6.0));
    let yt: Vec<(f64, String)> = [lo, bestv].iter().map(|v| (*v, format!("{v:.0}"))).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a4.draw(&mut s, &xt, &yt, "time (units): Normal cools for 1000, then the drum stops for 20", "score (Goo)");
    s.text((70.0, y4 - 8.0), "What the i9 reads through the Hall sensors, and what it keeps", 12.0, INK, "start", "bold");
    s.line(a4.p(0.0, bestv), a4.p(end, bestv), GOO, 1.5, Some("6 4"));
    s.text((a4.x + a4.w - 6.0, a4.fy(bestv) + 16.0), &format!("the best possible: {bestv:.0}"), 11.5, GOO, "end", "bold");
    for (t, v) in tr.t.iter().zip(&tr.score) {
        if let Some(v) = v
            && *v >= lo - 2.0
        {
            s.circle(a4.p(*t, *v), 1.7, "#9aa1ad", "none", 0.0);
        }
    }
    let pts: Vec<_> = tr.t.iter().zip(&tr.latch).filter(|(_, v)| v.is_finite()).map(|(t, v)| a4.p(*t, v.max(lo - 2.0))).collect();
    s.path(&pts, GOLD, 2.5, 1.0, None);
    s.circle(a4.p(tr.latch_at, bestv), 6.0, "none", GOLD, 3.0);
    s.text((a4.fx(tr.latch_at) + 10.0, a4.fy(bestv) - 8.0), &format!("the i9 latches the best set at t = {:.0}", tr.latch_at), 11.5, GOLD, "start", "bold");
    s.text(
        (70.0, height - 30.0),
        "Grey dots: the score of whatever the strips show (read when every strip sits in a dip; clashes score below zero). Gold: the best the i9 has seen so far.",
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.text(
        (70.0, height - 13.0),
        &format!(
            "Night {}, 48 spins: the latch got a best set {hits} times (median t = {median:.0}); the strips came to rest on one {rest} times. {ties} sets tie for the best tonight.",
            tc.world.night.seed
        ),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.save("spin");
    (seed, hits, rest, results.len())
}


/// Freeze-out: how often a strip hops, against how hard the drum shakes.
/// Left: lone strips (no springs, no fields) against CortenForge's own
/// Kramers formulas. Right: the night's board during real Normal spins.
fn freeze(tc: &TradeComputer) {
    let p = Physics::default();
    let well = DoubleWellPotential::new(p.delta_v, 1.0, 0);
    // Left: 20 lone strips per temperature, flips counted every step.
    let kts = [0.7, 0.85, 1.0, 1.25, 1.6, 2.0, 2.5];
    let n = 20;
    let lone = Ising { n, edges: vec![], j: vec![], h: vec![0.0; n] };
    let measured: Vec<(f64, f64)> = std::thread::scope(|sc| {
        let hs: Vec<_> = kts
            .iter()
            .enumerate()
            .map(|(k, &kt)| {
                let (lone, well) = (&lone, &well);
                sc.spawn(move || {
                    let rate = well.kramers_rate_turnover(p.gamma, 1.0, kt);
                    let time = (500.0 / (n as f64 * rate)).clamp(2000.0, 60000.0);
                    let mut m = Machine::new(lone, p, 100 + k as u64).expect("machine");
                    m.set_temperature(kt);
                    let mut side: Vec<i8> = m.positions().iter().map(|x| if *x > 0.0 { 1 } else { -1 }).collect();
                    let mut flips = 0usize;
                    for _ in 0..(time / p.dt) as usize {
                        m.step().expect("step");
                        for (i, x) in m.positions().iter().enumerate() {
                            let s = match WellState::from_position(*x, 0.5) {
                                WellState::Right => 1,
                                WellState::Left => -1,
                                _ => continue,
                            };
                            if s != side[i] {
                                side[i] = s;
                                flips += 1;
                            }
                        }
                    }
                    (kt, flips as f64 / (n as f64 * time))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Right: the board during 12 Normal spins, binned by drum setting.
    let bins: Vec<f64> = (0..=12).map(|k| 0.35 * (4.0f64 / 0.35).powf(k as f64 / 12.0)).collect();
    let tally = Mutex::new(vec![(0usize, 0.0f64); bins.len() - 1]);
    std::thread::scope(|sc| {
        for seed in 1..=12u64 {
            let (tally, bins) = (&tally, &bins);
            sc.spawn(move || {
                let mut local = vec![(0usize, 0.0f64); bins.len() - 1];
                let mut m = tc.machine(p, 500 + seed).expect("machine");
                let nn = tc.ising.n;
                let mut side: Vec<i8> = m.positions().iter().map(|x| if *x > 0.0 { 1 } else { -1 }).collect();
                let anneal = Anneal::default();
                let _ = tc.spin_with(&mut m, anneal.duration, |t, _| anneal.temperature(t), 1.0, p.dt, |m, _| {
                    let kt = m.temperature();
                    let Some(b) = bins.windows(2).position(|w| kt >= w[0] && kt < w[1]) else { return };
                    local[b].1 += p.dt * nn as f64;
                    for (i, x) in m.positions().iter().enumerate() {
                        let s = match WellState::from_position(*x, 0.5) {
                            WellState::Right => 1,
                            WellState::Left => -1,
                            _ => continue,
                        };
                        if s != side[i] {
                            side[i] = s;
                            local[b].0 += 1;
                        }
                    }
                });
                let mut t = tally.lock().unwrap();
                for (a, b) in t.iter_mut().zip(local) {
                    a.0 += b.0;
                    a.1 += b.1;
                }
            });
        }
    });
    let board: Vec<(f64, f64)> = bins
        .windows(2)
        .zip(tally.into_inner().unwrap())
        .filter(|(_, (f, t))| *t > 0.0 && *f > 0)
        .map(|(w, (f, t))| ((w[0] * w[1]).sqrt(), f as f64 / t))
        .collect();

    let mut s = Svg::new(980.0, 430.0);
    s.text((70.0, 28.0), "Freeze-out: a strip hops over its hump less and less as the drum cools", 15.0, INK, "start", "bold");
    let xt = [(0.35, "0.35"), (0.5, "0.5"), (1.0, "1"), (2.0, "2"), (4.0, "4")];
    let yt = [(1e-4, "0.0001"), (1e-3, "0.001"), (1e-2, "0.01"), (1e-1, "0.1"), (1.0, "1")];
    let a = Axes::new((80.0, 60.0, 380.0, 280.0), (0.35, 4.0), (5e-5, 2.0)).logs(true, true);
    a.draw(&mut s, &xt, &yt, "drum kT (the hump is 5)", "hops per strip per time unit");
    let curve = |f: &dyn Fn(f64) -> f64| -> Vec<(f64, f64)> {
        (0..=80).map(|i| 0.35 * (4.0f64 / 0.35).powf(i as f64 / 80.0)).filter(|kt| f(*kt) >= 5e-5).map(|kt| a.p(kt, f(kt).min(2.0))).collect()
    };
    s.path(&curve(&|kt| well.kramers_rate(p.gamma, 1.0, kt)), PURPLE, 1.8, 1.0, Some("6 4"));
    s.path(&curve(&|kt| well.kramers_rate_turnover(p.gamma, 1.0, kt)), PURPLE, 2.5, 1.0, None);
    for (kt, r) in &measured {
        s.circle(a.p(*kt, *r), 5.0, ICE, "#ffffff", 1.5);
    }
    s.text((a.x + 8.0, a.y + 18.0), "A lone strip", 13.0, INK, "start", "bold");
    s.circle((a.x + 14.0, a.y + 36.0), 5.0, ICE, "#ffffff", 1.5);
    s.text((a.x + 26.0, a.y + 40.0), "measured (CortenForge Langevin)", 11.5, INK, "start", "normal");
    s.line((a.x + 6.0, a.y + 56.0), (a.x + 22.0, a.y + 56.0), PURPLE, 2.5, None);
    s.text((a.x + 26.0, a.y + 60.0), "Kramers, with turnover (the crate's formula)", 11.5, INK, "start", "normal");
    s.line((a.x + 6.0, a.y + 76.0), (a.x + 22.0, a.y + 76.0), PURPLE, 1.8, Some("6 4"));
    s.text((a.x + 26.0, a.y + 80.0), "Kramers, high-friction form", 11.5, INK, "start", "normal");

    let b = Axes::new((560.0, 60.0, 380.0, 280.0), (0.35, 4.0), (5e-5, 2.0)).logs(true, true);
    b.draw(&mut s, &xt, &yt, "drum kT during a Normal spin", "");
    s.path(&curve(&|kt| well.kramers_rate_turnover(p.gamma, 1.0, kt)).iter().map(|(x, y)| (x - a.x + b.x, *y)).collect::<Vec<_>>(), PURPLE, 1.5, 0.5, None);
    for (kt, r) in &board {
        s.circle(b.p(*kt, *r), 5.0, RED, "#ffffff", 1.5);
    }
    s.text((b.x + 8.0, b.y + 18.0), &format!("Night {}'s board, 12 spins", tc.world.night.seed), 13.0, INK, "start", "bold");
    s.text((b.x + 8.0, b.y + 36.0), "Springs and fields tilt the dips, so a", 11.5, INK, "start", "normal");
    s.text((b.x + 8.0, b.y + 52.0), "strip in its good dip hops even less.", 11.5, INK, "start", "normal");
    s.text(
        (80.0, 395.0),
        "Each tenfold drop in hopping takes only a modest drop in kT: the rate goes as e^(-hump / kT). Cool too fast and the strips freeze",
        11.5,
        DIM,
        "start",
        "normal",
    );
    s.text((80.0, 412.0), "wherever they happen to be (a quench). Cool slowly and they keep finding better dips on the way down (an anneal).", 11.5, DIM, "start", "normal");
    s.save("freeze");
    for (kt, r) in &measured {
        println!(
            "  lone strip kT {kt:.2}: measured {r:.5}, Kramers turnover {:.5}, high-friction {:.5}",
            well.kramers_rate_turnover(p.gamma, 1.0, *kt),
            well.kramers_rate(p.gamma, 1.0, *kt)
        );
    }
}

/// The wash programs: time against how often the i9 lands the best set.
/// Numbers as measured for the game's program picker (nights 1-10, 48
/// spins each; `src/game/sim.rs` PROGRAMS).
fn programs() {
    let fixed = [("Quick Wash", 150.0, 26.0), ("Permanent Press", 300.0, 46.0), ("Normal", 1000.0, 81.0), ("Delicates", 3000.0, 95.0)];
    let mut s = Svg::new(760.0, 420.0);
    s.text((80.0, 28.0), "Wash programs: a longer, gentler cool-down finds the best set more often", 15.0, INK, "start", "bold");
    let a = Axes::new((80.0, 50.0, 420.0, 290.0), (100.0, 4000.0), (0.0, 100.0)).logs(true, false);
    a.draw(
        &mut s,
        &[(150.0, "150"), (300.0, "300"), (1000.0, "1000"), (3000.0, "3000")],
        &[(0.0, "0%"), (25.0, "25%"), (50.0, "50%"), (75.0, "75%"), (100.0, "100%")],
        "drum time (units, log scale)",
        "the i9 lands the best set",
    );
    let pts: Vec<_> = fixed.iter().map(|(_, t, r)| a.p(*t, *r)).collect();
    s.path(&pts, "#aab0bb", 2.0, 1.0, None);
    for (name, t, r) in fixed {
        s.circle(a.p(t, r), 6.0, ICE, "#ffffff", 1.5);
        // The last point sits at the right edge: label it on the left.
        let (dx, anchor) = if t > 2000.0 { (-10.0, "end") } else { (9.0, "start") };
        s.text((a.fx(t) + dx, a.fy(r) + 15.0), &format!("{name} {r:.0}%"), 11.5, INK, anchor, "normal");
    }
    // Same electricity as Normal, spent smarter.
    s.circle(a.p(1000.0, 88.0), 7.0, PURPLE, "#ffffff", 1.5);
    s.text((a.fx(1000.0) - 10.0, a.fy(88.0) + 4.0), "Smart 88%", 11.5, PURPLE, "end", "bold");
    s.rect((a.fx(1000.0) - 7.0, a.fy(95.0) - 7.0), (14.0, 14.0), 2.0, GOO, "#ffffff");
    s.text((a.fx(1000.0) - 12.0, a.fy(95.0) + 4.0), "Row of 4 washers 95%", 11.5, GOO, "end", "bold");
    let lx = 530.0;
    let lines = [
        ("Blue: one washer, cooling on a clock.", INK),
        ("Every doubling of time buys less.", DIM),
        ("", DIM),
        ("Purple: the Smart program spends", PURPLE),
        ("Normal's time but watches the strips", PURPLE),
        ("and reheats when they freeze. A", PURPLE),
        ("schedule CortenForge's CEM learned.", PURPLE),
        ("", DIM),
        ("Green: four washers, each a quarter", GOO),
        ("of Normal's time, swapping loads", GOO),
        ("between hot and cold drums", GOO),
        ("(parallel tempering).", GOO),
    ];
    for (k, (l, c)) in lines.iter().enumerate() {
        s.text((lx, 70.0 + k as f64 * 19.0), l, 12.0, c, "start", "normal");
    }
    s.text((80.0, 395.0), "Measured on nights 1-10, 48 spins each, as the game's program picker shows them. Row: 4 x 235 units + settles, the same total as Normal.", 11.0, DIM, "start", "normal");
    s.save("programs");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let what = args.first().map(String::as_str).filter(|a| !a.starts_with("--")).unwrap_or("all");
    let night = args.iter().position(|a| a == "--night").and_then(|k| args.get(k + 1)).and_then(|s| s.parse().ok()).unwrap_or_else(pick_night);
    let tc = computer(night);
    println!("night {night}: {} strips ({} trades, {} gifts)", tc.cycles.len(), tc.cycles.iter().filter(|c| !c.is_gift()).count(), tc.cycles.iter().filter(|c| c.is_gift()).count());
    let all = what == "all";
    if all || what == "pipeline" {
        pipeline();
    }
    if all || what == "strip" {
        strip(&tc);
    }
    if all || what == "board" {
        let (b, g) = board(&tc);
        println!("  best set {b:.1}, greedy {g:.1}");
    }
    if all || what == "landscape" {
        let (n, valid, near) = landscape(&tc);
        println!("  {n} strips: {} patterns, {valid} valid, {near} within 2 of the best", 1u64 << n);
    }
    if all || what == "spin" {
        let (seed, hits, rest, of) = spin(&tc);
        println!("  spin seed {seed}; latch hit {hits}/{of}, at rest {rest}/{of}");
    }
    if all || what == "freeze" {
        freeze(&tc);
    }
    if all || what == "programs" {
        programs();
    }
    // The site's lessons (site/figs/lessons).
    if all || what == "fees" {
        fees();
    }
    if all || what == "noise" {
        noise();
    }
    if all || what == "settle" {
        settle();
    }
    if all || what == "waits" {
        waits();
    }
    if all || what == "collapse" {
        collapse();
    }
}

// ---- lesson figures (upddayettacademy.com) ----

/// Tonight's best trades (trades only, no gifts) with a flat fee of `fee`
/// Goo on everyone in every trade: (net Goo kept, trades, people in them).
fn best_trades(night: u64, fee: f64) -> (f64, usize, usize) {
    let mut w = trade::world::laundromat(night);
    if fee > 0.0 {
        for k in 0..w.npcs.len() {
            w.set_yuck(k, fee);
        }
    }
    // Gains come out net of the fee; a trade where anyone would lose is gone.
    let mut c = trade::cycles::enumerate(&w, trade::MAX_LOOP);
    c.sort_by(|a, b| b.goo().total_cmp(&a.goo()));
    c.truncate(trade::MAX_BITS);
    if c.is_empty() {
        return (0.0, 0, 0);
    }
    let masks = qubo::conflict_masks(&c);
    let weights: Vec<f64> = c.iter().map(Cycle::goo).collect();
    let (set, net) = qubo::best_valid_set(&weights, &masks);
    let on = qubo::chosen(set, c.len());
    (net, on.len(), on.iter().map(|&i| c[i].len()).sum())
}

/// One fee level, averaged per night: (fee, net kept, fees paid, destroyed, trades).
type FeeRow = (f64, f64, f64, f64, f64);

/// Transaction costs: a flat fee per person per trade, across nights 1-30.
fn fees() {
    let nights: Vec<u64> = (1..=30).collect();
    // Gains are whole Goo, so the picture is a staircase: sample it finely.
    let steps: Vec<f64> = (0..=70).map(|k| k as f64 * 0.05).collect();
    let k = nights.len() as f64;
    let gross0 = nights.iter().map(|&n| best_trades(n, 0.0).0).sum::<f64>() / k;
    let rows: Vec<FeeRow> = steps
        .iter()
        .map(|&f| {
            let (mut net, mut paid, mut trades) = (0.0, 0.0, 0.0);
            for &n in &nights {
                let (v, t, legs) = best_trades(n, f);
                net += v;
                paid += f * legs as f64;
                trades += t as f64;
            }
            let (net, paid, trades) = (net / k, paid / k, trades / k);
            (f, net, paid, gross0 - net - paid, trades)
        })
        .collect();
    let most = rows.iter().max_by(|a, b| a.2.total_cmp(&b.2)).unwrap();
    println!("  most fee revenue: {:.1} Goo a night at a fee of {:.2} (street keeps {:.1}, destroyed {:.1})", most.2, most.0, most.1, most.3);
    for r in rows.iter().step_by(10) {
        println!("  fee {:.1}: net {:.1}, fees {:.1}, destroyed {:.1}, trades {:.1}", r.0, r.1, r.2, r.3, r.4);
    }
    let fmax = *steps.last().unwrap();
    let top = (gross0 / 10.0).ceil() * 10.0;
    let mut s = Svg::new(1000.0, 470.0);
    s.text((80.0, 28.0), "A flat fee on every trader: where the Goo goes (nights 1-30, average per night)", 15.0, INK, "start", "bold");
    let a = Axes::new((80.0, 60.0, 520.0, 320.0), (0.0, fmax), (0.0, top + 5.0));
    let xt: Vec<(f64, String)> = (0..=(fmax * 2.0) as usize).map(|k| (k as f64 * 0.5, format!("{}", k as f64 * 0.5))).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    let yt: Vec<(f64, String)> = (0..=(top / 10.0) as usize).map(|k| ((k * 10) as f64, (k * 10).to_string())).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a.draw(&mut s, &xt, &yt, "fee per person per trade (Goo)", "Goo per night");
    // Stacked areas: net (bottom), fees, destroyed (up to the no-fee total).
    let band = |s: &mut Svg, lo: &dyn Fn(&FeeRow) -> f64, hi: &dyn Fn(&FeeRow) -> f64, color: &str| {
        let mut d = String::new();
        for (k, r) in rows.iter().enumerate() {
            let p = a.p(r.0, hi(r));
            let _ = write!(d, "{}{:.1},{:.1} ", if k == 0 { "M" } else { "L" }, p.0, p.1);
        }
        for r in rows.iter().rev() {
            let p = a.p(r.0, lo(r));
            let _ = write!(d, "L{:.1},{:.1} ", p.0, p.1);
        }
        let _ = writeln!(s.body, r#"<path d="{d}Z" fill="{color}" stroke="none"/>"#);
    };
    band(&mut s, &|_| 0.0, &|r| r.1, GOO);
    band(&mut s, &|r| r.1, &|r| r.1 + r.2, GOLD);
    band(&mut s, &|r| r.1 + r.2, &|_| gross0, "#f2b8b0");
    s.line(a.p(0.0, gross0), a.p(fmax, gross0), RED, 1.5, Some("6 4"));
    // The game's yuck tax.
    let at2 = rows.iter().find(|r| (r.0 - 2.0).abs() < 1e-9).copied().unwrap();
    s.line(a.p(2.0, 0.0), a.p(2.0, gross0), INK, 1.2, Some("3 3"));
    s.text((a.fx(2.0) + 4.0, a.y + 14.0), "fee 2: the game's yuck tax", 11.0, INK, "start", "normal");
    let lx = 630.0;
    let legend: [(&str, String, String); 3] = [
        (GOO, "Kept by the street".into(), format!("{:.1} Goo at a fee of 2 (no fee: {gross0:.1})", at2.1)),
        (GOLD, "Paid in fees".into(), format!("{:.1} Goo at a fee of 2", at2.2)),
        ("#f2b8b0", "Destroyed: trades that never happen".into(), format!("{:.1} Goo at a fee of 2", at2.3)),
    ];
    for (k, (c, a1, b1)) in legend.iter().enumerate() {
        let y = 80.0 + k as f64 * 54.0;
        s.rect((lx, y - 11.0), (16.0, 16.0), 2.0, c, c);
        s.text((lx + 26.0, y + 2.0), a1, 12.5, INK, "start", "bold");
        s.text((lx + 26.0, y + 20.0), b1, 12.0, DIM, "start", "normal");
    }
    // Trades per night, small.
    let t0 = rows[0].4;
    let b = Axes::new((lx + 30.0, 270.0, 300.0, 110.0), (0.0, fmax), (0.0, t0.ceil() + 1.0));
    let t0s = format!("{t0:.1}");
    let half = format!("{}", fmax / 2.0);
    let full = format!("{fmax}");
    b.draw(&mut s, &[(0.0, "0"), (fmax / 2.0, half.as_str()), (fmax, full.as_str())], &[(0.0, "0"), (t0, t0s.as_str())], "fee", "");
    let pts: Vec<_> = rows.iter().map(|r| b.p(r.0, r.4)).collect();
    s.path(&pts, INK, 2.0, 1.0, None);
    s.text((b.x, b.y - 8.0), "Trades per night", 12.0, INK, "start", "bold");
    s.text(
        (80.0, 430.0),
        "Exact best sets (trades only, the board's 20 best loops) for each of 30 nights at each fee. A trade survives only if every person",
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.text((80.0, 446.0), "in it still gains after paying. The pink band is value nobody gets: not the traders, and not whoever collects the fee.", 11.0, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "fees");
}

/// Whether Normal's i9 latch lands a best set on `tc`, one entry per seed.
fn hits(tc: &TradeComputer, seeds: std::ops::RangeInclusive<u64>) -> Vec<bool> {
    let seeds: Vec<u64> = seeds.collect();
    let out = Mutex::new(vec![]);
    std::thread::scope(|sc| {
        for chunk in seeds.chunks(seeds.len().div_ceil(12).max(1)) {
            let out = &out;
            sc.spawn(move || {
                for &seed in chunk {
                    let mut m = tc.machine(Physics::default(), seed).expect("machine");
                    let l = tc.spin(&mut m, &Anneal::default(), 1.0, f64::INFINITY, |_, _| {}).expect("spin");
                    out.lock().unwrap().push((seed, tc.is_optimal(l.best_bits)));
                }
            });
        }
    });
    let mut v = out.into_inner().unwrap();
    v.sort();
    v.into_iter().map(|x| x.1).collect()
}

/// Sampling noise: 480 real spins on one hard night, cut into 12-spin and
/// 48-spin "experiments".
fn noise() {
    // A night where Normal lands the best set about half the time.
    let night = (1..=60u64)
        .find(|&n| {
            let h = hits(&computer(n), 1..=24);
            let r = h.iter().filter(|x| **x).count() as f64 / 24.0;
            println!("  night {n}: {:.0}% in 24 spins", r * 100.0);
            (0.4..=0.65).contains(&r)
        })
        .expect("a middling night");
    let h = hits(&computer(night), 1001..=1480);
    let p = h.iter().filter(|x| **x).count() as f64 / h.len() as f64;
    let est = |n: usize| -> Vec<f64> { h.chunks(n).map(|c| c.iter().filter(|x| **x).count() as f64 / n as f64).collect() };
    let (e12, e48) = (est(12), est(48));
    let range = |v: &[f64]| (v.iter().cloned().fold(1.0, f64::min), v.iter().cloned().fold(0.0, f64::max));
    println!("  night {night}: {:.1}% over 480; 12-spin range {:?}, 48-spin range {:?}", p * 100.0, range(&e12), range(&e48));

    let mut s = Svg::new(1000.0, 400.0);
    s.text((80.0, 28.0), &format!("One night, 480 real spins: how much can one experiment lie? (night {night}, Normal program)"), 15.0, INK, "start", "bold");
    let a = Axes::new((190.0, 60.0, 760.0, 240.0), (0.0, 1.0), (0.0, 3.0));
    a.draw(&mut s, &[(0.0, "0%"), (0.25, "25%"), (0.5, "50%"), (0.75, "75%"), (1.0, "100%")], &[], "measured hit rate (how often the i9 latched the best set)", "");
    for (y, e, n, c) in [(2.2, &e12, 12usize, ICE), (0.8, &e48, 48usize, PURPLE)] {
        let sd = (p * (1.0 - p) / n as f64).sqrt();
        let (lo, hi) = ((p - 1.96 * sd).max(0.0), (p + 1.96 * sd).min(1.0));
        let (x0, x1) = (a.fx(lo), a.fx(hi));
        s.rect((x0, a.fy(y) - 30.0), (x1 - x0, 60.0), 4.0, "#eef0f4", "#eef0f4");
        // Stack equal estimates so every experiment shows.
        let mut seen: std::collections::HashMap<i64, usize> = Default::default();
        for v in e.iter() {
            let k = seen.entry((v * 1000.0).round() as i64).or_default();
            let dy = (*k as f64 * 9.0 - 22.0).min(26.0);
            *k += 1;
            s.circle((a.fx(*v), a.fy(y) + dy), 4.2, c, "#ffffff", 1.2);
        }
        s.text((a.x - 12.0, a.fy(y) - 4.0), &format!("{} experiments", e.len()), 12.5, INK, "end", "bold");
        s.text((a.x - 12.0, a.fy(y) + 13.0), &format!("of {n} spins each"), 12.0, DIM, "end", "normal");
        let (mn, mx) = range(e);
        s.text((a.x + a.w - 6.0, a.fy(y) - 36.0), &format!("they read anywhere from {:.0}% to {:.0}%", mn * 100.0, mx * 100.0), 11.5, c, "end", "bold");
    }
    s.line(a.p(p, 0.0), a.p(p, 3.0), RED, 2.0, None);
    s.text((a.fx(p) + 5.0, a.y + 14.0), &format!("all 480: {:.0}%", p * 100.0), 12.0, RED, "start", "bold");
    s.text((80.0, 352.0), "Each dot is one experiment: a batch of real spins and the share that found the best set. The grey band is where 95% of", 11.5, DIM, "start", "normal");
    s.text((80.0, 369.0), "experiments that size should land (binomial). Four times the spins only halves the spread.", 11.5, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "noise");
}

/// Settlement risk: a loop of k people where each fails to deliver with
/// probability `p` (a model, not a game measurement).
fn settle() {
    let mut s = Svg::new(1000.0, 432.0);
    s.text((80.0, 28.0), "Settlement risk in a swap loop: hand over in turn, or everything through the counter", 15.0, INK, "start", "bold");
    let a = Axes::new((80.0, 60.0, 520.0, 290.0), (2.0, 8.0), (0.0, 1.0));
    let xt: Vec<(f64, String)> = (2..=8).map(|k| (k as f64, k.to_string())).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a.draw(&mut s, &xt, &[(0.0, "0%"), (0.25, "25%"), (0.5, "50%"), (0.75, "75%"), (1.0, "100%")], "people in the loop", "probability");
    let p: f64 = 0.05;
    let ks: Vec<f64> = (2..=8).map(|k| k as f64).collect();
    // Everyone has to show up and deliver.
    let done: Vec<_> = ks.iter().map(|&k| a.p(k, (1.0 - p).powf(k))).collect();
    // In turn: the first person delivers, then waits on the other k - 1.
    let stranded: Vec<_> = ks.iter().map(|&k| a.p(k, (1.0 - p) * (1.0 - (1.0 - p).powf(k - 1.0)))).collect();
    let escrow: Vec<_> = ks.iter().map(|&k| a.p(k, 0.0)).collect();
    s.rect((a.fx(2.0), a.y), (a.fx(4.0) - a.fx(2.0), a.h), 0.0, "#fbf3dc", "#fbf3dc");
    s.text((a.fx(3.0), a.y + 16.0), "the game's loops (2-4)", 11.5, GOLD, "middle", "bold");
    s.path(&done, GOO, 3.0, 1.0, None);
    s.path(&stranded, RED, 3.0, 1.0, None);
    s.path(&escrow, ICE, 3.0, 1.0, Some("7 4"));
    for pts in [&done, &stranded] {
        for q in pts.iter() {
            s.circle(*q, 3.5, "#ffffff", INK, 1.2);
        }
    }
    let lx = 630.0;
    let lines: [(&str, &str, &str); 3] = [
        (GOO, "The whole loop completes", "Same either way: every extra person is one more chance it breaks."),
        (RED, "Someone is left holding nothing", "Handing over in turn: whoever goes first gives, then waits on everyone else."),
        (ICE, "...if it all goes through the counter", "Zero. If anyone fails to deliver, the counter hands everything back."),
    ];
    for (k, (c, t, d)) in lines.iter().enumerate() {
        let y = 80.0 + k as f64 * 72.0;
        s.line((lx, y), (lx + 24.0, y), c, 3.0, if k == 2 { Some("7 4") } else { None });
        s.text((lx + 32.0, y + 4.0), t, 12.5, INK, "start", "bold");
        // Wrap the explanation by hand at ~44 characters.
        let (mut line, mut row) = (String::new(), 0);
        for word in d.split(' ') {
            if line.len() + word.len() > 44 {
                s.text((lx + 32.0, y + 22.0 + row as f64 * 16.0), &line, 11.5, DIM, "start", "normal");
                line.clear();
                row += 1;
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        s.text((lx + 32.0, y + 22.0 + row as f64 * 16.0), &line, 11.5, DIM, "start", "normal");
    }
    s.text((80.0, 414.0), "A simple model, not a game measurement: each person independently fails to deliver 1 time in 20 (p = 5%).", 11.5, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "settle");
}

// ---- lesson 4: rare events (the hump) ----

/// Lone strips (no springs, no fields) shaken at `kt` for up to `time` units:
/// for each strip, the times it hopped from one dip to the other. With
/// `stop_after`, the run ends once every strip has hopped that many times, so
/// those first waits are all complete (no wait cut off by the clock).
fn hop_times(physics: Physics, n: usize, kt: f64, time: f64, seed: u64, stop_after: Option<usize>) -> Vec<Vec<f64>> {
    let lone = Ising { n, edges: vec![], j: vec![], h: vec![0.0; n] };
    let mut m = Machine::new(&lone, physics, seed).expect("machine");
    m.set_temperature(kt);
    let mut side: Vec<i8> = m.positions().iter().map(|x| if *x > 0.0 { 1 } else { -1 }).collect();
    let mut hops = vec![vec![]; n];
    for _ in 0..(time / physics.dt) as usize {
        if stop_after.is_some_and(|k| hops.iter().all(|h: &Vec<f64>| h.len() >= k)) {
            break;
        }
        m.step().expect("step");
        let t = m.time();
        for (i, x) in m.positions().iter().enumerate() {
            let s = match WellState::from_position(*x, 0.5) {
                WellState::Right => 1,
                WellState::Left => -1,
                _ => continue,
            };
            if s != side[i] {
                side[i] = s;
                hops[i].push(t);
            }
        }
    }
    hops
}

/// Waiting for a rare event: how long a lone strip sits in its dip before it
/// hops (kT 0.85, a hump of 5), against CortenForge's Kramers rate.
/// Waits per strip in [`waits`].
const PER_STRIP: usize = 10;

fn waits() {
    let p = Physics::default();
    let kt = 0.85;
    let well = DoubleWellPotential::new(p.delta_v, 1.0, 0);
    let k_theory = well.kramers_rate_turnover(p.gamma, 1.0, kt);
    // 12 boards of 20 strips, each strip's first PER_STRIP waits: every one
    // complete (a run cut off by the clock would drop the longest waits and
    // bias the mean low, right-censoring). Each wait runs from one hop (or the start, a
    // strip at rest in its dip) to the next.
    let waits: Vec<f64> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..12u64)
            .map(|k| {
                sc.spawn(move || {
                    let mut out = vec![];
                    for hops in hop_times(p, 20, kt, 40000.0, 700 + k, Some(PER_STRIP)) {
                        let mut last = 0.0;
                        for t in hops.into_iter().take(PER_STRIP) {
                            out.push(t - last);
                            last = t;
                        }
                    }
                    out
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let n = waits.len() as f64;
    let mean = waits.iter().sum::<f64>() / n;
    let k_meas = 1.0 / mean;
    let surv = |t: f64| waits.iter().filter(|w| **w > t).count() as f64 / n;
    // Memoryless: of the strips that already waited `s`, how many wait t more.
    let s0 = mean;
    let survivors: Vec<f64> = waits.iter().filter(|w| **w > s0).map(|w| w - s0).collect();
    let surv_after = |t: f64| survivors.iter().filter(|w| **w > t).count() as f64 / survivors.len() as f64;
    let mut sorted = waits.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    println!(
        "  kT {kt}: {} waits, mean {mean:.0} (rate {k_meas:.5}), median {median:.0}, Kramers {k_theory:.5} (mean {:.0}); {} waited past the mean, their mean extra wait {:.0}; longest {:.0}",
        waits.len(),
        1.0 / k_theory,
        survivors.len(),
        survivors.iter().sum::<f64>() / survivors.len() as f64,
        sorted.last().unwrap()
    );

    let tmax = (mean * 5.0 / 500.0).ceil() * 500.0;
    let mut s = Svg::new(1000.0, 440.0);
    s.text((80.0, 28.0), &format!("Waiting for a hop: {} real waits of lone strips (a hump of 5, drum at kT {kt})", waits.len()), 15.0, INK, "start", "bold");
    // Left: histogram of waits.
    let bins = 25;
    let bw = tmax / bins as f64;
    let mut hist = vec![0usize; bins];
    for w in &waits {
        if *w < tmax {
            hist[(w / bw) as usize] += 1;
        }
    }
    let hmax = *hist.iter().max().unwrap() as f64;
    let a = Axes::new((80.0, 60.0, 380.0, 280.0), (0.0, tmax), (0.0, hmax * 1.1));
    let xt: Vec<(f64, String)> = (0..=5).map(|k| (k as f64 * tmax / 5.0, format!("{:.0}", k as f64 * tmax / 5.0))).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    let step = if hmax > 200.0 { 50.0 } else if hmax > 80.0 { 25.0 } else { 10.0 };
    let yt: Vec<(f64, String)> = (0..=(hmax * 1.1 / step) as usize).map(|k| (k as f64 * step, format!("{:.0}", k as f64 * step))).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a.draw(&mut s, &xt, &yt, "wait before the hop (time units)", "how many waits");
    for (k, c) in hist.iter().enumerate() {
        let (x0, x1) = (a.fx(k as f64 * bw), a.fx((k + 1) as f64 * bw));
        let y = a.fy(*c as f64);
        s.rect((x0 + 0.5, y), (x1 - x0 - 1.0, a.fy(0.0) - y), 0.0, ICE, ICE);
    }
    // The exponential with Kramers' rate, scaled to counts per bin.
    let pts: Vec<_> = (0..=100).map(|i| tmax * i as f64 / 100.0).map(|t| a.p(t, n * bw * k_theory * (-k_theory * t).exp())).collect();
    s.path(&pts, PURPLE, 2.5, 1.0, None);
    s.line(a.p(mean, 0.0), a.p(mean, hmax * 1.1), RED, 1.5, Some("5 4"));
    s.text((a.fx(mean) + 5.0, a.y + 16.0), &format!("mean {mean:.0}"), 11.5, RED, "start", "bold");
    s.text((a.x + a.w - 8.0, a.y + 36.0), "Most waits are short,", 11.5, INK, "end", "normal");
    s.text((a.x + a.w - 8.0, a.y + 52.0), "a few are very long.", 11.5, INK, "end", "normal");

    // Right: survival on a log scale, from the start and after already waiting.
    let b = Axes::new((570.0, 60.0, 380.0, 280.0), (0.0, tmax), (0.003, 1.0)).logs(false, true);
    b.draw(&mut s, &xt, &[(0.003, "0.3%"), (0.01, "1%"), (0.1, "10%"), (1.0, "100%")], "time (units)", "still waiting");
    let line: Vec<_> = (0..=100).map(|i| tmax * i as f64 / 100.0).map(|t| b.p(t, (-k_theory * t).exp().max(0.003))).collect();
    s.path(&line, PURPLE, 2.5, 1.0, None);
    let curve = |f: &dyn Fn(f64) -> f64| -> Vec<(f64, f64)> {
        (0..=200).map(|i| tmax * i as f64 / 200.0).filter(|t| f(*t) >= 0.003).map(|t| b.p(t, f(t))).collect()
    };
    s.path(&curve(&surv), ICE, 3.0, 1.0, None);
    s.path(&curve(&surv_after), GOLD, 2.5, 1.0, Some("7 4"));
    s.text((b.x + b.w - 8.0, b.y + 18.0), "a straight line on a log scale = exponential", 11.5, INK, "end", "normal");
    let lx = b.x + 10.0;
    let ly = b.y + b.h - 70.0;
    s.line((lx, ly), (lx + 22.0, ly), ICE, 3.0, None);
    s.text((lx + 28.0, ly + 4.0), "all waits, from the start", 11.5, INK, "start", "normal");
    s.line((lx, ly + 20.0), (lx + 22.0, ly + 20.0), GOLD, 2.5, Some("7 4"));
    s.text((lx + 28.0, ly + 24.0), &format!("strips that already waited {s0:.0}"), 11.5, INK, "start", "normal");
    s.line((lx, ly + 40.0), (lx + 22.0, ly + 40.0), PURPLE, 2.5, None);
    s.text((lx + 28.0, ly + 44.0), "Kramers' rate (the crate's formula)", 11.5, INK, "start", "normal");
    s.text(
        (80.0, 398.0),
        &format!(
            "Measured in CortenForge: 12 boards of 20 lone strips, each strip's first 10 waits (all complete). Mean wait {mean:.0} units; Kramers predicts {:.0}.",
            1.0 / k_theory
        ),
        11.5,
        DIM,
        "start",
        "normal",
    );
    s.text(
        (80.0, 416.0),
        "The gold curve lies on the blue one: a strip that has already waited a long time is no closer to hopping than one that just landed.",
        11.5,
        DIM,
        "start",
        "normal",
    );
    s.save_to(SITE_LESSONS, "waits");
}

/// Data collapse: hop rates for humps of 3, 5 and 7 look unrelated against
/// kT and fall on one curve against hump / kT.
fn collapse() {
    let base = Physics::default();
    let humps = [3.0, 5.0, 7.0];
    let xs = [3.0, 4.0, 5.0, 6.0, 7.0];
    let jobs: Vec<(f64, f64)> = humps.iter().flat_map(|&dv| xs.iter().map(move |&x| (dv, x))).collect();
    // (hump, hump/kT, kT, measured rate, Kramers' rate)
    let rows: Vec<(f64, f64, f64, f64, f64)> = std::thread::scope(|sc| {
        let hs: Vec<_> = jobs
            .iter()
            .enumerate()
            .map(|(k, &(dv, x))| {
                sc.spawn(move || {
                    let p = Physics { delta_v: dv, ..base };
                    let kt = dv / x;
                    let well = DoubleWellPotential::new(dv, 1.0, 0);
                    let theory = well.kramers_rate_turnover(p.gamma, 1.0, kt);
                    let n = 20;
                    let time = (400.0 / (n as f64 * theory)).clamp(2000.0, 60000.0);
                    let hops: usize = hop_times(p, n, kt, time, 900 + k as u64, None).iter().map(Vec::len).sum();
                    (dv, x, kt, hops as f64 / (n as f64 * time), theory)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for r in &rows {
        println!("  hump {:.0}, hump/kT {:.0} (kT {:.2}): measured {:.5}, Kramers {:.5} ({:+.0}%)", r.0, r.1, r.2, r.3, r.4, (r.3 / r.4 - 1.0) * 100.0);
    }
    let colors = [ICE, PURPLE, RED];
    let mut s = Svg::new(1000.0, 450.0);
    s.text((80.0, 28.0), "Three different humps, one law: what matters is the hump measured in kT", 15.0, INK, "start", "bold");
    let yt = [(1e-4, "0.0001"), (1e-3, "0.001"), (1e-2, "0.01"), (1e-1, "0.1"), (1.0, "1")];
    // Left: rate against the drum's kT.
    let a = Axes::new((80.0, 60.0, 380.0, 280.0), (0.4, 2.5), (5e-5, 1.0)).logs(true, true);
    a.draw(&mut s, &[(0.5, "0.5"), (1.0, "1"), (2.0, "2")], &yt, "drum kT", "hops per strip per time unit");
    for (h, c) in humps.iter().zip(colors) {
        let pts: Vec<_> = rows.iter().filter(|r| r.0 == *h).map(|r| a.p(r.2, r.3)).collect();
        s.path(&pts, c, 1.5, 0.6, None);
        for q in &pts {
            s.circle(*q, 5.0, c, "#ffffff", 1.5);
        }
        let last = rows.iter().filter(|r| r.0 == *h).map(|r| a.p(r.2, r.3)).fold((0.0, 0.0), |m: (f64, f64), q| if q.0 > m.0 { q } else { m });
        s.text((last.0 + 8.0, last.1 + 4.0), &format!("hump {h:.0}"), 12.0, c, "start", "bold");
    }
    s.text((a.x + 8.0, a.y + 18.0), "Against kT: three separate curves", 12.5, INK, "start", "bold");
    // Right: rate divided by the attempt rate, against hump / kT.
    let b = Axes::new((570.0, 60.0, 380.0, 280.0), (2.5, 7.5), (3e-4, 0.1)).logs(false, true);
    b.draw(&mut s, &[(3.0, "3"), (4.0, "4"), (5.0, "5"), (6.0, "6"), (7.0, "7")], &[(1e-3, "0.001"), (1e-2, "0.01"), (1e-1, "0.1")], "hump / kT", "rate ÷ attempt rate");
    let law: Vec<_> = (0..=50).map(|i| 2.5 + 5.0 * i as f64 / 50.0).map(|x| b.p(x, (-x as f64).exp())).collect();
    s.path(&law, INK, 2.0, 1.0, Some("6 4"));
    // The three humps land on top of each other (that's the point): nested
    // rings, biggest first, so all three show.
    let radii = [8.0, 5.5, 3.2];
    for ((h, c), r0) in humps.iter().zip(colors).zip(radii) {
        for r in rows.iter().filter(|r| r.0 == *h) {
            // The attempt rate: Kramers' rate without its exponential.
            let attempt = r.4 / (-r.1).exp();
            s.circle(b.p(r.1, r.3 / attempt), r0, c, "#ffffff", 1.3);
        }
    }
    s.text((b.x + b.w - 8.0, b.y + 18.0), "Against hump / kT: one line", 12.5, INK, "end", "bold");
    let (lx, ly) = (b.x + 14.0, b.y + b.h - 78.0);
    for (k, ((h, c), r0)) in humps.iter().zip(colors).zip(radii).enumerate() {
        let y = ly + k as f64 * 18.0;
        s.circle((lx, y), r0.min(6.0), c, "#ffffff", 1.3);
        s.text((lx + 12.0, y + 4.0), &format!("hump {h:.0}"), 11.5, c, "start", "bold");
    }
    s.line((lx - 6.0, ly + 56.0), (lx + 6.0, ly + 56.0), INK, 2.0, Some("4 3"));
    s.text((lx + 12.0, ly + 60.0), "e^(-hump/kT)", 11.5, INK, "start", "normal");
    s.text(
        (80.0, 400.0),
        "Measured in CortenForge: 20 lone strips per point. The attempt rate is how often a strip rattles against the hump (Kramers' rate without",
        11.5,
        DIM,
        "start",
        "normal",
    );
    s.text((80.0, 418.0), "its exponential, from the crate). Each step of 1 in hump / kT divides the hop rate by e = 2.7.", 11.5, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "collapse");
}
