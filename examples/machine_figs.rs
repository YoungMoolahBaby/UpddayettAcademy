//! Figures for `docs/MACHINE.md` ("How the machine works") and the site's
//! lessons (`site/`, upddayettacademy.com), plotted from the real machine:
//! every curve and dot is a CortenForge run or the night's real board, not a
//! drawing (the settlement figure is a labeled model).
//!
//! `cargo run --release --example machine_figs -- [all|pipeline|strip|board|landscape|spin|freeze|programs|fees|noise|settle|waits|collapse|ladder|blowup|penalties|magnets|idle|spincheck] [--night N]`
//! writes the machine figures to `docs/machine/*.svg` (and copies to
//! `site/figs/machine/`), the lessons to `site/figs/lessons/`. Without `--night` it picks the first night
//! (from 1) where grabbing the best trade first loses Goo, so the board figure
//! has something to show.

use std::fmt::Write as _;
use std::sync::Mutex;

use cortenforge::sim::thermostat::{DoubleWellPotential, WellState};
use cortenforge_play::trade::{self, Anneal, Cycle, Ising, Machine, Magnet, Physics, TradeComputer, qubo};

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
        "Labels: who hands to whom (U = Upddayett, R =",
        "Ranchelle, AP = Amir's Persian Kitchen, ...).",
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
    s.text((80.0, 420.0), "wherever they happen to be (a quench). Cool slowly and they keep finding better dips on the way down (an anneal).", 11.5, DIM, "start", "normal");
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
    if all || what == "ladder" {
        ladder();
    }
    if all || what == "blowup" {
        blowup();
    }
    if all || what == "penalties" {
        penalties();
    }
    if all || what == "magnets" {
        magnet_map();
    }
    if all || what == "idle" {
        idle(night);
    }
    if all || what == "spincheck" {
        spincheck();
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

// ---- Lesson 5: optimization (greedy vs global) ----

/// `f` over `items` on 12 threads, results in order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let out = Mutex::new(vec![]);
    std::thread::scope(|sc| {
        for (c, chunk) in items.chunks(items.len().div_ceil(12).max(1)).enumerate() {
            let (out, f) = (&out, &f);
            sc.spawn(move || {
                for (k, it) in chunk.iter().enumerate() {
                    let r = f(it);
                    out.lock().unwrap().push((c, k, r));
                }
            });
        }
    });
    let mut v = out.into_inner().unwrap();
    v.sort_by_key(|x| (x.0, x.1));
    v.into_iter().map(|x| x.2).collect()
}

/// Smarter greedy: take the trade worth the most per trade it knocks out
/// (value / (1 + collisions left)), drop it and its neighbors, repeat.
fn greedy_per_collision(cycles: &[Cycle], masks: &[u32]) -> u32 {
    let (mut set, mut left) = (0u32, (1u32 << cycles.len()) - 1);
    while left != 0 {
        let pick = (0..cycles.len())
            .filter(|&i| left >> i & 1 == 1)
            .max_by(|&a, &b| {
                let r = |i: usize| cycles[i].value() / (1.0 + (masks[i] & left).count_ones() as f64);
                r(a).total_cmp(&r(b))
            })
            .unwrap();
        set |= 1 << pick;
        left &= !(masks[pick] | 1 << pick);
    }
    set
}

/// Hill climbing from greedy: try adding one trade not in the set (or, with
/// `pairs`, two that don't collide with each other), dropping whatever they
/// collide with; keep the best move that gains, until no move gains. Stops
/// on the first hilltop.
fn hill_climb(cycles: &[Cycle], masks: &[u32], start: u32, pairs: bool) -> u32 {
    let n = cycles.len();
    let mut set = start;
    loop {
        let now = set_value(cycles, set);
        let outside: Vec<usize> = (0..n).filter(|&i| set >> i & 1 == 0).collect();
        let mut moves: Vec<u32> = outside.iter().map(|&i| 1u32 << i).collect();
        if pairs {
            for (a, &i) in outside.iter().enumerate() {
                moves.extend(outside[a + 1..].iter().filter(|&&k| masks[i] >> k & 1 == 0).map(|&k| 1u32 << i | 1 << k));
            }
        }
        let best = moves
            .into_iter()
            .map(|add| {
                let drop = qubo::chosen(add, n).iter().fold(0, |m, &i| m | masks[i]);
                (set & !drop) | add
            })
            .max_by(|a, b| set_value(cycles, *a).total_cmp(&set_value(cycles, *b)));
        match best {
            Some(next) if set_value(cycles, next) > now + 1e-9 => set = next,
            _ => return set,
        }
    }
}

const LADDER_NIGHTS: u64 = 200;
const LADDER_SPINS: u64 = 4;

/// Greedy, greedy per collision, hill climbing, the machine and the exact
/// answer on nights 1-200.
fn ladder() {
    let nights: Vec<u64> = (1..=LADDER_NIGHTS).collect();
    // Per night: (strips, best, greedy, per-collision, hill, [machine latch per seed]).
    let rows = par_map(&nights, |&n| {
        let tc = computer(n);
        let c = &tc.cycles;
        let masks = qubo::conflict_masks(c);
        let best = set_value(c, tc.ground_state());
        let g = greedy(c, &masks);
        let spins: Vec<f64> = (1..=LADDER_SPINS)
            .map(|seed| {
                let mut m = tc.machine(Physics::default(), seed).expect("machine");
                let l = tc.spin(&mut m, &Anneal::default(), 1.0, f64::INFINITY, |_, _| {}).expect("spin");
                let (v, clash) = tc.evaluate(l.best_bits);
                if clash { 0.0 } else { v }
            })
            .collect();
        (c.len(), best, set_value(c, g), set_value(c, greedy_per_collision(c, &masks)), set_value(c, hill_climb(c, &masks, g, false)), set_value(c, hill_climb(c, &masks, g, true)), spins)
    });
    let k = rows.len() as f64;
    type Row = (usize, f64, f64, f64, f64, f64, Vec<f64>);
    // (share of tries at the best, Goo + Karma left on the table per try, worst)
    let stats = |got: &dyn Fn(&Row) -> Vec<f64>| {
        let (mut at, mut tries, mut short, mut worst) = (0usize, 0usize, 0.0, 0.0f64);
        for r in &rows {
            for v in got(r) {
                tries += 1;
                if v >= r.1 - 1e-9 {
                    at += 1;
                }
                short += r.1 - v;
                worst = worst.max(r.1 - v);
            }
        }
        (at as f64 / tries as f64, short / tries as f64, worst)
    };
    println!("  {} nights, mean best {:.1}, mean strips {:.1}", rows.len(), rows.iter().map(|r| r.1).sum::<f64>() / k, rows.iter().map(|r| r.0 as f64).sum::<f64>() / k);
    let ways: [(&str, String, &str, (f64, f64, f64)); 6] = [
        ("Biggest trade first (greedy)", "what a person does by eye".into(), GOLD, stats(&|r| vec![r.2])),
        ("Greedy, then hill-climb", "swap in 1 trade while it helps".into(), DIM, stats(&|r| vec![r.4])),
        ("Greedy, then hill-climb", "swap in 2 trades while it helps".into(), DIM, stats(&|r| vec![r.5])),
        ("Most value per collision first", "worth ÷ (1 + trades it knocks out)".into(), PURPLE, stats(&|r| vec![r.3])),
        ("The machine (Normal)", format!("{} spins a night, the i9's latch", LADDER_SPINS), ICE, stats(&|r| r.6.clone())),
        ("Check every valid set", "exact; about 0.1 ms a night here".into(), GOO, (1.0, 0.0, 0.0)),
    ];
    for w in &ways {
        println!("  {:<32} best {:>5.1}%   left on the table {:.2} a night   worst {:.1}", format!("{} ({})", w.0, w.1), w.3.0 * 100.0, w.3.1, w.3.2);
    }
    let mut hist = std::collections::BTreeMap::new();
    for r in &rows {
        *hist.entry((r.1 - r.2).round() as i64).or_insert(0usize) += 1;
    }
    println!("  greedy gap histogram: {hist:?}");
    // The exact search on each real board, timed one night at a time.
    let boards: Vec<(Vec<f64>, Vec<u32>)> = nights
        .iter()
        .map(|&n| {
            let tc = computer(n);
            (tc.cycles.iter().map(Cycle::value).collect(), qubo::conflict_masks(&tc.cycles))
        })
        .collect();
    let t = std::time::Instant::now();
    for (weights, masks) in &boards {
        std::hint::black_box(qubo::best_valid_set(weights, masks));
    }
    println!("  exact search: {:.0} us a night", t.elapsed().as_secs_f64() / k * 1e6);
    let mut s = Svg::new(1000.0, 470.0);
    s.text((20.0, 28.0), &format!("Six ways to pick the night's trades, tried on {} nights", rows.len()), 15.0, INK, "start", "bold");
    let (bx, bw, top, rh) = (290.0, 300.0, 78.0, 54.0);
    s.text((bx, top - 18.0), "nights it finds the best set", 12.0, INK, "start", "bold");
    s.text((bx + bw + 140.0, top - 18.0), "left on the table", 12.0, INK, "end", "bold");
    for (q, label) in [(0.0, "0"), (0.5, "50%"), (1.0, "100%")] {
        let x = bx + bw * q;
        s.line((x, top - 8.0), (x, top + rh * ways.len() as f64 - 14.0), GRID, 1.0, None);
        s.text((x, top + rh * ways.len() as f64), label, 11.0, DIM, "middle", "normal");
    }
    for (k, (name, how, color, (share, left, _))) in ways.iter().enumerate() {
        let y = top + k as f64 * rh;
        s.text((20.0, y + 13.0), name, 13.0, INK, "start", "bold");
        s.text((20.0, y + 30.0), how, 11.5, DIM, "start", "normal");
        s.rect((bx, y), (bw * share, 24.0), 2.0, color, color);
        s.text((bx + bw * share + 6.0, y + 17.0), &format!("{:.0}%", share * 100.0), 13.0, INK, "start", "bold");
        s.text((bx + bw + 140.0, y + 17.0), &format!("{left:.1} a night"), 13.0, INK, "end", "normal");
    }
    // How much greedy leaves, night by night.
    let hx = 800.0;
    let ymax = *hist.values().max().unwrap() as f64;
    let a = Axes::new((hx, 90.0, 180.0, 230.0), (-1.0, 7.0), (0.0, ymax * 1.15));
    let yt: Vec<(f64, String)> = (0..=(ymax / 20.0) as usize).map(|k| ((k * 20) as f64, (k * 20).to_string())).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a.draw(&mut s, &[(0.0, "0"), (2.0, "2"), (4.0, "4"), (6.0, "6")], &yt, "Goo + Karma greedy misses", "nights");
    for (&gap, &count) in &hist {
        let (x, y) = a.p(gap as f64, count as f64);
        let c = if gap == 0 { GOO } else { GOLD };
        s.rect((x - 16.0, y), (32.0, a.fy(0.0) - y), 1.0, c, c);
        s.text((x, y - 5.0), &count.to_string(), 11.0, INK, "middle", "normal");
    }
    s.text((hx - 46.0, 70.0), "What greedy misses, night by night", 12.5, INK, "start", "bold");
    s.text(
        (20.0, 430.0),
        &format!(
            "Each night's real board: {:.1} candidate trades and gift chains on average, best set worth {:.1} Goo + Karma. \"Left on the table\": the best set's",
            rows.iter().map(|r| r.0 as f64).sum::<f64>() / k,
            rows.iter().map(|r| r.1).sum::<f64>() / k
        ),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.text((20.0, 446.0), "worth minus what the method picked, averaged over nights (and spins, for the machine). Hill-climbing starts from greedy's answer.", 11.0, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "ladder");
}

/// Gift chains up to this many recipients (the `--chain` experiment) grow
/// the board past the game's ~17 strips, so "check everything" can be timed
/// on real boards up to 30.
const BLOWUP_CHAIN: usize = 2;

/// A night's candidates with longer gift chains, best first, at most 32.
fn big_board(night: u64) -> Vec<Cycle> {
    let w = trade::world::laundromat(night);
    let mut c = trade::cycles::enumerate(&w, trade::MAX_LOOP);
    c.extend(trade::cycles::enumerate_gifts(&w, BLOWUP_CHAIN));
    c.sort_by(|a, b| b.value().total_cmp(&a.value()));
    c.truncate(32);
    c
}

fn count_valid(masks: &[u32]) -> u64 {
    let mut valid = 0;
    qubo::for_each_valid_set(masks, |_| valid += 1);
    valid
}

/// How fast "check everything" grows: night 1's board cut to its best k
/// candidates, every 2^k on/off pattern vs the valid sets among them, and
/// both searches timed.
fn blowup() {
    let c = big_board(1);
    let counts: Vec<(usize, u64)> = (1..=c.len()).map(|k| (k, count_valid(&qubo::conflict_masks(&c[..k])))).collect();
    // Check every pattern for a collision, or search only valid sets (skip a
    // trade once a colliding one is in). One size at a time, so the timings
    // don't share the CPU.
    let ks: Vec<usize> = (10..=c.len().min(28)).step_by(2).collect();
    let timed: Vec<_> = ks.iter().map(|&k| {
        let masks = qubo::conflict_masks(&c[..k]);
        let t = std::time::Instant::now();
        let all = (0..1u64 << k).filter(|&b| qubo::chosen(b as u32, k).iter().all(|&i| masks[i] & b as u32 == 0)).count() as u64;
        let every = t.elapsed().as_secs_f64();
        let reps = 200;
        let t = std::time::Instant::now();
        let mut valid = 0;
        for _ in 0..reps {
            valid = count_valid(&masks);
        }
        assert_eq!(all, valid, "both searches count the same valid sets");
        (k, valid, every, t.elapsed().as_secs_f64() / reps as f64)
    }).collect();
    for t in &timed {
        println!("  night 1, {} strips: every pattern {:.3} s, valid-only search {:.1} us ({} valid sets)", t.0, t.2, t.3 * 1e6, t.1);
    }
    // The same cut on 30 nights, for the text.
    let nights: Vec<u64> = (1..=30).collect();
    let at = par_map(&nights, |&n| {
        let c = big_board(n);
        (c.len() >= 28).then(|| count_valid(&qubo::conflict_masks(&c[..28])))
    });
    let at: Vec<u64> = at.into_iter().flatten().collect();
    println!("  28 strips on {} of 30 nights: valid sets {}-{} (of {} patterns)", at.len(), at.iter().min().unwrap(), at.iter().max().unwrap(), 1u64 << 28);

    let kmax = c.len() as f64;
    let mut s = Svg::new(1000.0, 470.0);
    s.text((80.0, 28.0), "Check everything? Night 1, with the board cut to its best 1, 2, 3, ... candidates", 15.0, INK, "start", "bold");
    let pow = |e: i32| 10f64.powi(e);
    // Left: how many.
    let a = Axes::new((80.0, 60.0, 360.0, 300.0), (0.0, kmax + 1.0), (1.0, 1e10)).logs(false, true);
    let yt: Vec<(f64, String)> = [(0, "1"), (2, "100"), (4, "10⁴"), (6, "10⁶"), (8, "10⁸"), (10, "10¹⁰")].iter().map(|&(e, l)| (pow(e), l.to_string())).collect();
    let yt: Vec<(f64, &str)> = yt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    let xt: Vec<(f64, String)> = (0..=c.len()).step_by(5).map(|k| (k as f64, k.to_string())).collect();
    let xt: Vec<(f64, &str)> = xt.iter().map(|(v, s)| (*v, s.as_str())).collect();
    a.draw(&mut s, &xt, &yt, "candidate trades on the board", "how many sets to look at");
    let all: Vec<_> = (1..=c.len()).map(|k| a.p(k as f64, 2f64.powi(k as i32))).collect();
    s.path(&all, RED, 2.5, 1.0, None);
    let valid: Vec<_> = counts.iter().map(|&(k, v)| a.p(k as f64, v as f64)).collect();
    s.path(&valid, GOO, 2.5, 1.0, None);
    for q in &valid {
        s.circle(*q, 3.0, GOO, "#ffffff", 1.0);
    }
    let end = *all.last().unwrap();
    s.text((end.0 - 6.0, end.1 - 10.0), "every on/off pattern: doubles per trade", 12.0, RED, "end", "bold");
    s.text((a.x + a.w - 8.0, a.fy(6.0)), &format!("valid sets only: {} at {}", counts.last().unwrap().1, c.len()), 12.0, GOO, "end", "bold");
    // The game's own board for night 1 (gift chains of 1, ~17 candidates).
    let tc = computer(1);
    let gm = qubo::conflict_masks(&tc.cycles);
    let game: Vec<_> = (1..=tc.cycles.len()).map(|k| (k, count_valid(&qubo::conflict_masks(&tc.cycles[..k])))).collect();
    let reps = 2000;
    let t = std::time::Instant::now();
    for _ in 0..reps {
        std::hint::black_box(count_valid(&gm));
    }
    println!("  the game's night 1 board: {} candidates, {} valid sets, valid-only search {:.1} us", tc.cycles.len(), game.last().unwrap().1, t.elapsed().as_secs_f64() / reps as f64 * 1e6);
    let gp: Vec<_> = game.iter().map(|&(k, v)| a.p(k as f64, v as f64)).collect();
    s.path(&gp, GOO, 1.5, 0.6, Some("5 3"));
    let gl = *gp.last().unwrap();
    s.circle(gl, 4.0, "#ffffff", GOO, 2.0);
    s.text((gl.0 + 8.0, gl.1 - 10.0), &format!("the game's board: {}", game.last().unwrap().1), 11.5, GOO, "start", "normal");
    // Right: how long.
    let b = Axes::new((560.0, 60.0, 380.0, 300.0), (8.0, 29.0), (1e-7, 1e3)).logs(false, true);
    let yt = [(1e-6, "1 µs"), (1e-3, "1 ms"), (1.0, "1 s"), (60.0, "1 min")];
    b.draw(&mut s, &[(10.0, "10"), (15.0, "15"), (20.0, "20"), (25.0, "25")], &yt, "candidate trades on the board", "time to find the best set");
    let every: Vec<_> = timed.iter().map(|t| b.p(t.0 as f64, t.2.max(1e-7))).collect();
    let fast: Vec<_> = timed.iter().map(|t| b.p(t.0 as f64, t.3)).collect();
    s.path(&every, RED, 2.0, 1.0, None);
    s.path(&fast, GOO, 2.0, 1.0, None);
    for q in &every {
        s.circle(*q, 4.5, RED, "#ffffff", 1.3);
    }
    for q in &fast {
        s.circle(*q, 4.5, GOO, "#ffffff", 1.3);
    }
    let last = timed.last().unwrap();
    let le = *every.last().unwrap();
    s.text((le.0 - 8.0, le.1 - 10.0), &format!("check every pattern: {:.0} s at {}", last.2, last.0), 12.0, RED, "end", "bold");
    let lf = *fast.last().unwrap();
    s.text((lf.0 - 8.0, lf.1 - 12.0), &format!("skip what collides: {:.0} µs", last.3 * 1e6), 12.0, GOO, "end", "bold");
    s.text(
        (80.0, 420.0),
        &format!(
            "Night 1's trades plus gift chains of up to {BLOWUP_CHAIN} people (the --chain {BLOWUP_CHAIN} experiment), so the board grows past the game's 17. Both searches find the same valid sets;"
        ),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.text((80.0, 436.0), "the fast one never looks at a set with a collision in it. Times on one core of a Ryzen 5 5600. The street's trades collide a lot, so valid sets are rare:", 11.0, DIM, "start", "normal");
    s.text((80.0, 452.0), "on a board where nothing collides, every pattern is valid and the green line would climb as steeply as the red.", 11.0, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "blowup");
}

/// The collision penalty: too soft and cheating sets win; too stiff and the
/// strips get stuck. Exact QUBO ground state and Normal's latch per penalty.
fn penalties() {
    let ps = [0.25, 0.5, 0.75, 1.0, 1.25, 1.6, 2.0, 2.5, 3.0, 4.0];
    let nights: Vec<u64> = (1..=20).collect();
    let spins = 16u64;
    let jobs: Vec<(f64, u64)> = ps.iter().flat_map(|&p| nights.iter().map(move |&n| (p, n))).collect();
    // (penalty, ground state is a best set, ground clashes, [latch: 0 best, 1 valid worse, 2 clash])
    let rows = par_map(&jobs, |&(p, n)| {
        let tc = TradeComputer::new(trade::world::laundromat(n), BETA, p);
        let gs = tc.qubo_ground_state();
        let ground_best = tc.is_optimal(gs);
        let ground_clash = tc.evaluate(gs).1;
        let outcomes: Vec<u8> = (1..=spins)
            .map(|seed| {
                let mut m = tc.machine(Physics::default(), seed).expect("machine");
                let l = tc.spin(&mut m, &Anneal::default(), 1.0, f64::INFINITY, |_, _| {}).expect("spin");
                if tc.is_optimal(l.best_bits) {
                    0
                } else if tc.evaluate(l.best_bits).1 {
                    2
                } else {
                    1
                }
            })
            .collect();
        (p, ground_best, ground_clash, outcomes)
    });
    // Per penalty: (penalty, nights whose lowest energy is a best set, nights, [best, worse, clash] shares).
    let summary: Vec<(f64, usize, usize, [f64; 3])> = ps
        .iter()
        .map(|&p| {
            let r: Vec<_> = rows.iter().filter(|r| r.0 == p).collect();
            let gb = r.iter().filter(|r| r.1).count();
            let gc = r.iter().filter(|r| r.2).count();
            let all: Vec<u8> = r.iter().flat_map(|r| r.3.iter().copied()).collect();
            let share = |o: u8| all.iter().filter(|&&x| x == o).count() as f64 / all.len() as f64;
            let sh = [share(0), share(1), share(2)];
            println!(
                "  penalty {p:.2}: ground best {gb}/{} (clash {gc}); latch best {:.1}%, worse {:.1}%, clash {:.1}% of {}",
                r.len(),
                sh[0] * 100.0,
                sh[1] * 100.0,
                sh[2] * 100.0,
                all.len()
            );
            (p, gb, r.len(), sh)
        })
        .collect();
    let mut s = Svg::new(1000.0, 480.0);
    s.text((80.0, 28.0), "How hard to punish a collision: too soft and the strips cheat, too stiff and they get stuck", 15.0, INK, "start", "bold");
    let a = Axes::new((80.0, 90.0, 600.0, 280.0), (-0.5, ps.len() as f64 - 0.5), (0.0, 1.0));
    let labels: Vec<String> = ps.iter().map(|p| format!("{p}")).collect();
    let xt: Vec<(f64, &str)> = labels.iter().enumerate().map(|(k, l)| (k as f64, l.as_str())).collect();
    a.draw(&mut s, &xt, &[(0.0, "0"), (0.25, "25%"), (0.5, "50%"), (0.75, "75%"), (1.0, "100%")], "", "Normal spins");
    s.text((a.x + a.w / 2.0, a.y + a.h + 50.0), "collision penalty (x the most valuable trade)", 12.0, INK, "middle", "normal");
    let colors = [GOO, GOLD, RED];
    let bw = a.w / ps.len() as f64 * 0.7;
    for (k, (p, gb, nights, sh)) in summary.iter().enumerate() {
        let mut lo = 0.0;
        for (q, c) in sh.iter().zip(colors) {
            let (x, y) = a.p(k as f64, lo + q);
            s.rect((x - bw / 2.0, y), (bw, a.fy(lo) - y), 0.0, c, c);
            lo += q;
        }
        let x = a.fx(k as f64);
        s.text((x, a.fy(sh[0]) + if sh[0] > 0.08 { 16.0 } else { -5.0 }), &format!("{:.0}%", sh[0] * 100.0), 11.5, if sh[0] > 0.08 { "#ffffff" } else { INK }, "middle", "bold");
        // Whether the energy's lowest point is a best set at all.
        let ok = gb == nights;
        s.text((x, a.y - 22.0), &format!("{gb}/{nights}"), 11.5, if ok { GOO } else { RED }, "middle", "bold");
        if (*p - PENALTY).abs() < 1e-9 {
            s.rect((x - bw / 2.0 - 5.0, a.y - 4.0), (bw + 10.0, a.h + 8.0), 4.0, "none", INK);
            s.text((x, a.y + a.h + 31.0), "the game's", 11.0, INK, "middle", "bold");
        }
    }
    s.text((a.x, a.y - 42.0), "Nights where the lowest energy is a best set (exact, every on/off pattern checked):", 11.5, INK, "start", "normal");
    let lx = 720.0;
    let legend = [
        (GOO, "The i9 latched a best set", "the right answer"),
        (GOLD, "A valid set, but not the best", "stuck in a worse dip"),
        (RED, "A set with a collision", "an item promised twice"),
    ];
    for (k, (c, t, d)) in legend.iter().enumerate() {
        let y = 110.0 + k as f64 * 50.0;
        s.rect((lx, y - 11.0), (16.0, 16.0), 2.0, c, c);
        s.text((lx + 26.0, y + 2.0), t, 12.5, INK, "start", "bold");
        s.text((lx + 26.0, y + 19.0), d, 12.0, DIM, "start", "normal");
    }
    let notes = [
        "Below 1, taking both of two colliding trades",
        "can score more than taking one, so the energy",
        "itself points at a cheat. Above about 1 the",
        "lowest point is honest, but stiffer springs",
        "make deeper walls between valid sets, and the",
        "strips stop finding their way over them.",
    ];
    for (k, l) in notes.iter().enumerate() {
        s.text((lx, 280.0 + k as f64 * 18.0), l, 12.0, INK, "start", "normal");
    }
    s.text((80.0, 455.0), &format!("Nights 1-{}, {} Normal spins each per penalty ({} spins a bar), in CortenForge's Langevin drum. Penalty in units of the night's most valuable candidate.", nights.len(), spins, nights.len() as u64 * spins), 11.0, DIM, "start", "normal");
    s.save_to(SITE_LESSONS, "penalties");
}

// ---- Lesson 6: model risk (the Salties) ----

/// The magnet's strength on the map: just under flattening, so it steers the
/// strips without pinning the one above it.
const MAP_STRENGTH: f64 = 0.8;
const MAP_SPINS: u64 = 32;
/// The map's two boards: the calibration load, and a night it doesn't cover.
const MAP_NIGHTS: [u64; 2] = [trade::salties::CALIBRATION_NIGHT, 5];

/// What the model of the model says a field costs: tilt each trade's value
/// by the Goo the field is worth to the drum (`salties::aim`'s rule), solve
/// exactly, and score that set against the clean night. (Goo lost, the set.)
fn on_paper(tc: &TradeComputer, field: &[f64]) -> (f64, u32) {
    let masks = qubo::conflict_masks(&tc.cycles);
    let per_goo = 2.0 / (tc.beta * tc.problem.scale);
    let tilted: Vec<f64> = tc.cycles.iter().zip(field).map(|(c, h)| c.value() + h * per_goo).collect();
    let (set, _) = qubo::best_valid_set(&tilted, &masks);
    (tc.evaluate(tc.ground_state()).0 - tc.evaluate(set).0, set)
}

/// Spins on a board with a hidden field on top (a magnet, or with `coil` one
/// that runs only while the drum shakes).
struct Spins {
    /// The i9 latched the clean best set.
    best: f64,
    /// The strips came to rest on it.
    rest: f64,
    lost: f64,
}

fn spins_with(tc: &TradeComputer, field: Option<&[f64]>, anneal: &Anneal, seeds: std::ops::RangeInclusive<u64>) -> Spins {
    let best = tc.evaluate(tc.ground_state()).0;
    let (mut hit, mut rest, mut lost, mut n) = (0, 0, 0.0, 0);
    for seed in seeds {
        let mut m = match field {
            Some(f) => Machine::with_stray_field(&tc.ising, Physics::default(), seed, f),
            None => tc.machine(Physics::default(), seed),
        }
        .expect("machine");
        let l = tc.spin(&mut m, anneal, 1.0, f64::INFINITY, |_, _| {}).expect("spin");
        hit += tc.is_optimal(l.best_bits) as usize;
        rest += tc.is_optimal(l.final_bits) as usize;
        lost += best - tc.evaluate(l.best_bits).0;
        n += 1;
    }
    Spins { best: hit as f64 / n as f64, rest: rest as f64 / n as f64, lost: lost / n as f64 }
}

/// `a` to `b` (both "#rrggbb") at `t` in 0..1.
fn mix(a: &str, b: &str, t: f64) -> String {
    let c = |s: &str, k: usize| u8::from_str_radix(&s[1 + 2 * k..3 + 2 * k], 16).unwrap() as f64;
    let ch = |k: usize| (c(a, k) + (c(b, k) - c(a, k)) * t.clamp(0.0, 1.0)).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ch(0), ch(1), ch(2))
}

/// A best-set share as a cell color: pale red at 0, pale gold at half, pale green at 1.
fn share_color(x: f64) -> String {
    if x < 0.5 { mix("#eba59c", "#f4dc9a", x * 2.0) } else { mix("#f4dc9a", "#acd98f", (x - 0.5) * 2.0) }
}

/// A magnet under each slot of the counter in turn, pushing on and pushing
/// off, on the calibration load and on night 5: what the drum loses, against
/// what the tilted model predicts.
fn magnet_map() {
    let dv = Physics::default().delta_v;
    let slots = trade::MAX_BITS;
    let tcs: Vec<TradeComputer> = MAP_NIGHTS.iter().map(|&n| computer(n)).collect();
    let clean = par_map(&[0usize, 1], |&b| spins_with(&tcs[b], None, &Anneal::default(), 1..=MAP_SPINS));
    let jobs: Vec<(usize, usize, f64)> = (0..2).flat_map(|b| (0..slots).flat_map(move |p| [(b, p, 1.0), (b, p, -1.0)])).collect();
    let rows = par_map(&jobs, |&(b, p, sign)| {
        let tc = &tcs[b];
        let f = Magnet { pos: p as f64, strength: sign * MAP_STRENGTH }.field(tc.cycles.len(), dv);
        (on_paper(tc, &f).0, spins_with(tc, Some(&f), &Anneal::default(), 1..=MAP_SPINS))
    });
    let cell = |b: usize, p: usize, sign: f64| &rows[(b * slots + p) * 2 + (sign < 0.0) as usize];
    for (b, tc) in tcs.iter().enumerate() {
        println!(
            "night {}: {} strips, best set {}, clean: latch {:.0}%, rest {:.0}%",
            MAP_NIGHTS[b],
            tc.cycles.len(),
            tc.evaluate(tc.ground_state()).0,
            100.0 * clean[b].best,
            100.0 * clean[b].rest
        );
        for p in 0..slots {
            let mut line = format!("  slot {p:>2}{}", if tc.ground_state() >> p & 1 == 1 { "*" } else { " " });
            for sign in [1.0, -1.0] {
                let (paper, s) = cell(b, p, sign);
                line += &format!(
                    " | {:<3} paper {paper:>4.1}: latch {:>3.0}% rest {:>3.0}% lost {:>4.1}",
                    if sign > 0.0 { "on" } else { "off" },
                    100.0 * s.best,
                    100.0 * s.rest,
                    s.lost
                );
            }
            println!("{line}");
        }
    }
    // How well the paper model calls it, and what the calibration load misses.
    let hurt = |s: &Spins, b: usize| s.best < clean[b].best - 0.2;
    let (mut agree, mut all, mut cal_pass, mut missed) = (0, 0, 0, 0);
    let (mut rest_sum, mut latch_sum) = (0.0, 0.0);
    for p in 0..slots {
        for sign in [1.0, -1.0] {
            for b in 0..2 {
                let (paper, s) = cell(b, p, sign);
                all += 1;
                agree += ((*paper > 1e-9) == hurt(s, b)) as usize;
                rest_sum += s.rest;
                latch_sum += s.best;
            }
            if !hurt(&cell(0, p, sign).1, 0) {
                cal_pass += 1;
                missed += hurt(&cell(1, p, sign).1, 1) as usize;
            }
        }
    }
    println!(
        "  paper agrees with the drum (hurt = 20 points under clean) on {agree}/{all} cells; mean at rest {:.0}%, latched {:.0}%",
        100.0 * rest_sum / all as f64,
        100.0 * latch_sum / all as f64
    );
    println!("  the calibration load passes {cal_pass} of {} magnets; {missed} of those rob night {}", 2 * slots, MAP_NIGHTS[1]);

    let mut s = Svg::new(1000.0, 530.0);
    s.text((40.0, 28.0), "A magnet under each slot of the counter: what it costs depends on where it is, and which board", 15.0, INK, "start", "bold");
    let (x0, cw, rh) = (190.0, 38.0, 34.0);
    for (b, tc) in tcs.iter().enumerate() {
        let top = 72.0 + b as f64 * 160.0;
        let n = tc.cycles.len();
        let head = if b == 0 {
            format!("Night {}, the calibration load: {} strips; clean, the i9 latches its best set {:.0}% of the time", MAP_NIGHTS[b], n, 100.0 * clean[b].best)
        } else {
            format!("Night {}, tonight: {} strips; clean, {:.0}%", MAP_NIGHTS[b], n, 100.0 * clean[b].best)
        };
        s.text((40.0, top), &head, 12.5, INK, "start", "bold");
        // The strips over each slot; green ones are in tonight's best set.
        s.text((x0 - 10.0, top + 25.0), "strip in the best set", 11.0, DIM, "end", "normal");
        for p in 0..slots {
            let x = x0 + p as f64 * cw;
            if p < n {
                let on = tc.ground_state() >> p & 1 == 1;
                s.rect((x + 4.0, top + 15.0), (cw - 8.0, 12.0), 2.0, if on { GOO } else { "#d7dbe2" }, if on { GOO } else { "#c5c9d1" });
            } else {
                s.text((x + cw / 2.0, top + 25.0), "-", 11.0, DIM, "middle", "normal");
            }
        }
        for (r, sign) in [1.0, -1.0].iter().enumerate() {
            let y = top + 36.0 + r as f64 * (rh + 4.0);
            s.text((x0 - 10.0, y + rh / 2.0 + 4.0), if *sign > 0.0 { "pushing on" } else { "pushing off" }, 12.0, INK, "end", "normal");
            for p in 0..slots {
                let (paper, sp) = cell(b, p, *sign);
                let x = x0 + p as f64 * cw;
                let fill = share_color(sp.best);
                s.rect((x + 1.0, y), (cw - 2.0, rh), 3.0, &fill, &fill);
                s.text((x + cw / 2.0, y + rh / 2.0 + 4.0), &format!("{:.0}", 100.0 * sp.best), 11.5, INK, "middle", "bold");
                if b == 1 && !hurt(&cell(0, p, *sign).1, 0) && hurt(sp, 1) {
                    s.rect((x + 1.0, y), (cw - 2.0, rh), 3.0, "none", INK);
                }
                if *paper > 1e-9 {
                    s.circle((x + cw - 6.0, y + 6.0), 2.6, INK, INK, 0.0);
                }
            }
        }
    }
    let by = 72.0 + 160.0 + 36.0 + 2.0 * (rh + 4.0);
    for p in 0..slots {
        s.text((x0 + p as f64 * cw + cw / 2.0, by + 12.0), &p.to_string(), 11.0, DIM, "middle", "normal");
    }
    s.text((x0 + slots as f64 * cw / 2.0, by + 32.0), "the slot the magnet is taped under", 12.0, INK, "middle", "normal");
    // Legend.
    let ly = by + 50.0;
    for k in 0..=20 {
        let t = k as f64 / 20.0;
        let c = share_color(t);
        s.rect((40.0 + k as f64 * 9.0, ly), (9.0, 14.0), 0.0, &c, &c);
    }
    s.text((40.0, ly + 30.0), "0%", 11.0, DIM, "start", "normal");
    s.text((229.0, ly + 30.0), "100%", 11.0, DIM, "end", "normal");
    s.text((245.0, ly + 11.0), "Normal spins where the i9 still latched the clean best set", 12.0, INK, "start", "normal");
    s.circle((249.0, ly + 33.0), 2.6, INK, INK, 0.0);
    s.text((259.0, ly + 37.0), "on paper, the best set moves (the tilted model, solved exactly)", 12.0, INK, "start", "normal");
    s.rect((640.0, ly + 25.0), (22.0, 16.0), 3.0, "none", INK);
    s.text((670.0, ly + 37.0), "night 1 passed this magnet; it robbed night 5", 12.0, INK, "start", "normal");
    s.text(
        (40.0, 495.0),
        &format!(
            "Each cell: {MAP_SPINS} Normal spins in CortenForge's Langevin drum with a magnet of {MAP_STRENGTH}x the flattening field, 1.5 strip pitches under the slot, scored against the clean night's best set."
        ),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.text(
        (40.0, 513.0),
        "Passed or robbed: within 20 points of the clean night's rate, or more than 20 under it.",
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.save_to(SITE_LESSONS, "magnets");
}

/// `2 × 10⁻⁶` for 2e-6 (one significant figure).
fn sci(x: f64) -> String {
    let mut e = x.abs().log10().floor() as i32;
    let mut m = (x / 10f64.powi(e)).round();
    if m.abs() >= 10.0 {
        (m, e) = (m / 10.0, e + 1);
    }
    let sup: String = e.to_string().chars().map(|c| match c {
        '-' => '\u{207b}',
        d => ['\u{2070}', '\u{b9}', '\u{b2}', '\u{b3}', '\u{2074}', '\u{2075}', '\u{2076}', '\u{2077}', '\u{2078}', '\u{2079}'][d.to_digit(10).unwrap() as usize],
    }).collect();
    format!("{m} \u{d7} 10{sup}")
}

/// The idle check on a board at rest: what it reads on each strip, clean,
/// with a magnet, and with the magnet behind the shield, against the field
/// the magnet really puts there.
fn idle(night: u64) {
    let tc = computer(night);
    let k = tc.cycles.len();
    let dv = Physics::default().delta_v;
    let flat = trade::machine::max_safe_field(dv);
    let read = |field: Option<&[f64]>, coil: bool| -> Vec<f64> {
        let mut m = match field {
            Some(f) if coil => Machine::with_component(&tc.ising, Physics::default(), 7, trade::salties::Coil(f.to_vec())),
            Some(f) => Machine::with_stray_field(&tc.ising, Physics::default(), 7, f),
            None => tc.machine(Physics::default(), 7),
        }
        .expect("machine");
        m.rest(30.0).expect("rest");
        m.stray_field(&tc.ising).iter().map(|x| x / flat).collect()
    };
    let mag = Magnet { pos: 5.4, strength: 0.6 };
    let truth: Vec<f64> = mag.field(k, dv).iter().map(|x| x / flat).collect();
    let truth_s: Vec<f64> = mag.shielded().field(k, dv).iter().map(|x| x / flat).collect();
    let f: Vec<f64> = truth.iter().map(|x| x * flat).collect();
    let fs: Vec<f64> = truth_s.iter().map(|x| x * flat).collect();
    let clean = read(None, false);
    let open = read(Some(&f), false);
    let shield = read(Some(&fs), false);
    let coil = read(Some(&f), true);
    let worst = |v: &[f64], t: &[f64]| v.iter().zip(t).fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    let biggest = |v: &[f64]| v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let (e_open, e_shield, floor, coil_idle) = (worst(&open, &truth), worst(&shield, &truth_s), biggest(&clean), biggest(&coil));
    let top = (0..k).max_by(|&a, &b| open[a].total_cmp(&open[b])).unwrap();
    println!(
        "  night {night}: magnet {:.1}x at {:.1}; strongest read strip {top} at {:.3}x; error open {e_open:.1e}, shielded {e_shield:.1e}; clean floor {floor:.1e}; coil at idle {coil_idle:.1e}",
        mag.strength, mag.pos, open[top]
    );

    let mut s = Svg::new(1000.0, 470.0);
    s.text((80.0, 28.0), "The idle check: stop the drum, and every strip's position has to be explained by the forces the i9 installed", 15.0, INK, "start", "bold");
    let a = Axes::new((80.0, 70.0, 600.0, 310.0), (-0.5, k as f64 - 0.5), (1e-8, 1.0)).logs(false, true);
    let xl: Vec<String> = (0..k).map(|i| i.to_string()).collect();
    let xt: Vec<(f64, &str)> = xl.iter().enumerate().map(|(i, l)| (i as f64, l.as_str())).collect();
    a.draw(
        &mut s,
        &xt,
        &[(1.0, "1"), (1e-2, "0.01"), (1e-4, "0.0001"), (1e-6, "10\u{207b}\u{2076}"), (1e-8, "10\u{207b}\u{2078}")],
        "strip",
        "",
    );
    s.vtext((a.x - 58.0, a.y + a.h / 2.0), "push nobody accounts for (x the flattening field)", 12.0, INK);
    s.line((a.fx(mag.pos), a.y), (a.fx(mag.pos), a.y + a.h), DIM, 1.0, Some("4 3"));
    s.text((a.fx(mag.pos) + 6.0, a.y + a.h - 8.0), "the magnet", 11.0, DIM, "start", "normal");
    let curve = |t: &[f64]| -> Vec<(f64, f64)> {
        // The true field between strips too, for a smooth line.
        let scale = t[0] / truth[0];
        (0..=(10 * (k - 1))).map(|j| j as f64 / 10.0).map(|x| a.p(x, scale * mag.strength * (1.5f64 * 1.5 / ((x - mag.pos).powi(2) + 2.25)).powf(1.5))).collect()
    };
    s.path(&curve(&truth), RED, 2.0, 0.55, None);
    s.path(&curve(&truth_s), GOLD, 2.0, 0.7, None);
    for i in 0..k {
        s.circle(a.p(i as f64, open[i].abs().max(1e-8)), 4.0, "#ffffff", RED, 2.0);
        s.circle(a.p(i as f64, shield[i].abs().max(1e-8)), 4.0, "#ffffff", GOLD, 2.0);
        s.circle(a.p(i as f64, clean[i].abs().max(1e-8)), 3.5, DIM, DIM, 0.0);
    }
    let lx = 720.0;
    let legend = [
        (RED, "A magnet, 0.6x, open", "line: its real field; rings: what the i9 reads"),
        (GOLD, "The same magnet behind the shield", "10% gets through, and the i9 still reads it"),
        (DIM, "A clean board", "the strips' last bit of ringing: the noise floor"),
    ];
    for (j, (c, t, d)) in legend.iter().enumerate() {
        let y = 90.0 + j as f64 * 50.0;
        s.circle((lx + 7.0, y - 4.0), 5.0, if *c == DIM { DIM } else { "#ffffff" }, c, 2.0);
        s.text((lx + 22.0, y), t, 12.5, INK, "start", "bold");
        s.text((lx + 22.0, y + 17.0), d, 11.5, DIM, "start", "normal");
    }
    let notes = [
        "Biggest gap between reading and truth:".to_string(),
        format!("  open {}, shielded {}", sci(e_open), sci(e_shield)),
        format!("Biggest reading on a clean board: {}", sci(floor)),
        String::new(),
        "The smart Salties' coil is off whenever".to_string(),
        "the drum stops. At idle the i9 reads a".to_string(),
        "board with the coil in it exactly like a".to_string(),
        format!("clean one (biggest reading {}).", sci(coil_idle)),
    ];
    for (j, l) in notes.iter().enumerate() {
        s.text((lx, 260.0 + j as f64 * 18.0), l, 12.0, INK, "start", "normal");
    }
    s.text(
        (80.0, 450.0),
        &format!("Night {night}, the strips at rest after 30 time units with the drum stopped, in CortenForge's Langevin model. The reading is the force balance on each strip."),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.save_to(SITE_LESSONS, "idle");
}

const CHECK_BOARDS: usize = 10;
const CHECK_SPINS: u64 = 48;

/// The first `count` nights with boards unlike any earlier night's (some
/// nights deal the street the same trades).
fn distinct_nights(count: usize) -> Vec<u64> {
    let mut seen: Vec<String> = vec![];
    let mut out = vec![];
    for n in 1.. {
        let tc = computer(n);
        let sig = format!("{:?} {:?}", tc.cycles.iter().map(|c| c.value()).collect::<Vec<_>>(), qubo::conflict_masks(&tc.cycles));
        if !seen.contains(&sig) {
            seen.push(sig);
            out.push(n);
            if out.len() == count {
                break;
            }
        }
    }
    out
}

/// The smart Salties' coil on 10 different boards: the idle check reads
/// nothing, the spin check (the same force balance, averaged while the drum
/// shakes) reads it, and how loud a clean spin reads for each wash length.
fn spincheck() {
    let dv = Physics::default().delta_v;
    let flat = trade::machine::max_safe_field(dv);
    let alarm = trade::salties::SPIN_ALARM;
    let programs = [("Quick Wash", 150.0), ("Permanent Press", 300.0), ("Normal", 1000.0)];
    let nights = distinct_nights(CHECK_BOARDS);
    println!("boards: nights {nights:?}");
    let boards: Vec<(TradeComputer, Magnet, f64)> = nights
        .iter()
        .map(|&n| {
            let tc = computer(n);
            let (coil, cost) = trade::salties::aim(&tc, dv);
            (tc, coil, cost)
        })
        .collect();
    let coil_machine = |tc: &TradeComputer, coil: &Magnet, seed: u64| {
        Machine::with_component(&tc.ising, Physics::default(), seed, trade::salties::Coil(coil.field(tc.ising.n, dv))).expect("machine")
    };
    let jobs: Vec<(usize, usize, bool, u64)> = (0..boards.len())
        .flat_map(|b| (0..programs.len()).flat_map(move |p| [false, true].into_iter().flat_map(move |c| (1..=CHECK_SPINS).map(move |s| (b, p, c, s)))))
        .collect();
    // (board, program, coil?, latched best?, Goo lost, loudest strip, its reading, reads, every strip's mean)
    let rows = par_map(&jobs, |&(b, p, coiled, seed)| {
        let (tc, coil, _) = &boards[b];
        let anneal = Anneal { duration: programs[p].1, ..Anneal::default() };
        let mut m = if coiled { coil_machine(tc, coil, seed) } else { tc.machine(Physics::default(), seed).expect("machine") };
        let mut check = trade::salties::SpinCheck::default();
        let l = tc.spin(&mut m, &anneal, 1.0, 1.0, |mm, _| check.observe(mm, &tc.ising)).expect("spin");
        let (k, v) = check.strongest().expect("reads");
        let lost = tc.evaluate(tc.ground_state()).0 - tc.evaluate(l.best_bits).0;
        let mean: Vec<f64> = check.mean().iter().map(|x| x / flat).collect();
        (b, p, coiled, tc.is_optimal(l.best_bits), lost, k, v.abs() / flat, check.samples(), mean)
    });
    let pick = |b: Option<usize>, p: usize, c: bool| rows.iter().filter(move |r| b.is_none_or(|b| r.0 == b) && r.1 == p && r.2 == c);
    for (b, (tc, coil, cost)) in boards.iter().enumerate() {
        let k = coil.strip(tc.cycles.len());
        let mut im = coil_machine(tc, coil, 3);
        im.rest(30.0).expect("rest");
        let idle = im.stray_field(&tc.ising).iter().fold(0.0f64, |m, x| m.max(x.abs())) / flat;
        let mut line = format!("night {:>2}: coil under {k:>2} ({:+.1}x), paper {cost:>4.1}, idle {idle:.0e} |", nights[b], coil.strength);
        for (p, (name, _)) in programs.iter().enumerate() {
            let rate = |c: bool| 100.0 * pick(Some(b), p, c).filter(|r| r.3).count() as f64 / CHECK_SPINS as f64;
            let right = pick(Some(b), p, true).filter(|r| r.6 >= alarm && r.5 == k).count();
            let fa = pick(Some(b), p, false).filter(|r| r.6 >= alarm).count();
            let lost = pick(Some(b), p, true).map(|r| r.4).sum::<f64>() / CHECK_SPINS as f64;
            line += &format!(" {}: clean {:.0}% (fa {fa}), coil {:.0}% lost {lost:.1}, right strip {right} |", &name[..5], rate(false), rate(true));
        }
        println!("{line}");
    }
    let mut stats = vec![];
    for (p, (name, _)) in programs.iter().enumerate() {
        let cl: Vec<f64> = pick(None, p, false).map(|r| r.6).collect();
        let co: Vec<f64> = pick(None, p, true).map(|r| r.6).collect();
        let reads = pick(None, p, false).map(|r| r.7).sum::<usize>() / cl.len();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let max = cl.iter().fold(0.0f64, |m, x| m.max(*x));
        let min = co.iter().fold(f64::INFINITY, |m, x| m.min(*x));
        let fa = cl.iter().filter(|x| **x >= alarm).count();
        let caught = co.iter().filter(|x| **x >= alarm).count();
        let best_clean = 100.0 * pick(None, p, false).filter(|r| r.3).count() as f64 / cl.len() as f64;
        let best_coil = 100.0 * pick(None, p, true).filter(|r| r.3).count() as f64 / co.len() as f64;
        println!(
            "{name}: ~{reads} reads; clean loudest mean {:.3} max {max:.3}, false alarms {fa}/{}; coil quietest {min:.3} mean {:.3}, caught {caught}/{}; best set clean {best_clean:.0}%, coil {best_coil:.0}%",
            mean(&cl),
            cl.len(),
            mean(&co),
            co.len()
        );
        stats.push((cl, co, reads, fa, caught));
    }

    // The figure: one coil night up close, then every spin's loudest reading.
    let (tc0, coil0, _) = &boards[0];
    let n0 = tc0.cycles.len();
    let k0 = coil0.strip(n0);
    let truth: Vec<f64> = coil0.field(n0, dv).iter().map(|x| x / flat).collect();
    let mut im = coil_machine(tc0, coil0, 3);
    im.rest(30.0).expect("rest");
    let idle_read: Vec<f64> = im.stray_field(&tc0.ising).iter().map(|x| x / flat).collect();
    let spin_read = &pick(Some(0), 2, true).next().expect("a coil spin").8;
    let mut s = Svg::new(1000.0, 520.0);
    s.text((60.0, 28.0), "The smart Salties' coil only pushes while the drum shakes: check at rest and it isn't there", 15.0, INK, "start", "bold");
    let lo = truth.iter().fold(0.0f64, |m, x| m.min(*x)).min(spin_read.iter().fold(0.0f64, |m, x| m.min(*x)));
    let a = Axes::new((80.0, 80.0, 360.0, 300.0), (-0.5, n0 as f64 - 0.5), ((lo * 1.15).min(-0.2), 0.25));
    let xl: Vec<String> = (0..n0).map(|i| if i % 2 == 0 { i.to_string() } else { String::new() }).collect();
    let xt: Vec<(f64, &str)> = xl.iter().enumerate().map(|(i, l)| (i as f64, l.as_str())).collect();
    a.draw(&mut s, &xt, &[(0.0, "0"), (-0.2, "-0.2"), (-0.4, "-0.4"), (-0.6, "-0.6"), (-0.8, "-0.8"), (0.2, "0.2")], "strip", "unexplained push (x flattening)");
    s.text((a.x, a.y - 14.0), &format!("Night {}: the coil under strip {k0}, pushing off at {:.1}x", nights[0], coil0.strength.abs()), 12.5, INK, "start", "bold");
    let bw = a.w / n0 as f64 * 0.62;
    for i in 0..n0 {
        let (x, y0) = a.p(i as f64, 0.0);
        let y1 = a.fy(truth[i]);
        s.rect((x - bw / 2.0, y0.min(y1)), (bw, (y1 - y0).abs()), 1.0, "#f6d3ce", "#f6d3ce");
        s.circle(a.p(i as f64, spin_read[i]), 4.0, RED, RED, 0.0);
        s.rect((x - 6.0, a.fy(idle_read[i]) - 1.5), (12.0, 3.0), 0.0, INK, INK);
    }
    let lg = [("#f6d3ce", "the coil's push while the drum shakes"), (RED, "spin check: averaged over one Normal spin"), (INK, "idle check: drum stopped (reads 0)")];
    for (j, (c, t)) in lg.iter().enumerate() {
        let y = a.y + a.h + 52.0 + j as f64 * 18.0;
        if j == 1 {
            s.circle((a.x + 7.0, y - 4.0), 4.0, c, c, 0.0);
        } else {
            s.rect((a.x, y - 9.0 + if j == 2 { 4.0 } else { 0.0 }), (14.0, if j == 2 { 3.0 } else { 10.0 }), 0.0, c, c);
        }
        s.text((a.x + 22.0, y), t, 11.5, INK, "start", "normal");
    }
    // Right: every spin's loudest unexplained push, clean vs coil, by wash length.
    let bx = 540.0;
    let bwid = 420.0;
    let xmax = 1.0;
    let fx = |v: f64| bx + v.min(xmax) / xmax * bwid;
    s.text((bx, a.y - 14.0), &format!("Every spin's loudest reading, {} boards x {CHECK_SPINS} spins", boards.len()), 12.5, INK, "start", "bold");
    let rowh = 92.0;
    let bins = 50usize;
    for (p, (name, _)) in programs.iter().enumerate() {
        let (cl, co, reads, fa, caught) = &stats[p];
        let top = a.y + 8.0 + p as f64 * rowh;
        let base = top + 58.0;
        s.text((bx, top + 4.0), &format!("{name}, ~{reads} reads"), 12.0, INK, "start", "bold");
        s.text(
            (bx + bwid, top + 4.0),
            &format!("false alarms {fa}/{}, caught {caught}/{}", cl.len(), co.len()),
            11.0,
            if *fa > 0 { RED } else { DIM },
            "end",
            "normal",
        );
        let hist = |v: &[f64]| {
            let mut h = vec![0usize; bins];
            for x in v {
                h[((x / xmax * bins as f64) as usize).min(bins - 1)] += 1;
            }
            h
        };
        let (hc, hk) = (hist(cl), hist(co));
        let peak = hc.iter().chain(&hk).copied().max().unwrap_or(1) as f64;
        let bwx = bwid / bins as f64;
        for (h, c) in [(&hc, DIM), (&hk, RED)] {
            for (j, &cnt) in h.iter().enumerate() {
                if cnt > 0 {
                    let hh = 44.0 * cnt as f64 / peak;
                    s.rect((bx + j as f64 * bwx + 0.5, base - hh), (bwx - 1.0, hh), 0.0, c, c);
                }
            }
        }
        s.line((bx, base), (bx + bwid, base), "#c5c9d1", 1.0, None);
        s.line((fx(alarm), top + 12.0), (fx(alarm), base + 4.0), INK, 1.5, Some("5 3"));
    }
    let bottom = a.y + 8.0 + 3.0 * rowh - 26.0;
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        s.text((fx(t), bottom + 16.0), &format!("{t}"), 11.0, DIM, "middle", "normal");
    }
    s.text((bx + bwid / 2.0, bottom + 34.0), "loudest average push on any strip (x flattening)", 12.0, INK, "middle", "normal");
    s.text((fx(alarm) + 5.0, bottom - 4.0), &format!("alarm at {alarm}"), 11.0, INK, "start", "bold");
    s.rect((bx, bottom + 50.0), (12.0, 10.0), 0.0, DIM, DIM);
    s.text((bx + 18.0, bottom + 59.0), "clean spins", 11.5, INK, "start", "normal");
    s.rect((bx + 110.0, bottom + 50.0), (12.0, 10.0), 0.0, RED, RED);
    s.text((bx + 128.0, bottom + 59.0), "spins with the coil", 11.5, INK, "start", "normal");
    s.text(
        (60.0, 505.0),
        &format!("Nights {}: the first {} different boards. The coil is aimed per board by the Salties' own scouting (0.8x the flattening field). CortenForge's Langevin drum.", nights.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "), boards.len()),
        11.0,
        DIM,
        "start",
        "normal",
    );
    s.save_to(SITE_LESSONS, "spincheck");
}
