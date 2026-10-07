//! The laundromat's trade computer, stepped by CortenForge every frame.

use bevy::prelude::*;
use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::salties::SpinCheck;
use cortenforge_play::trade::{Anneal, Latch, Machine, Magnet, Physics, Sabotage, TradeComputer, escrow, machine, print, salties, world, yuck};
use cortenforge_play::trade::row::{Row, RowSpin};
use cortenforge_play::trade::smart::{self, SmartWash};

/// What the drum is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// The player holds the spin dial.
    Manual,
    /// A full spin cycle, started at this sim time.
    Cycle { start: f64 },
    /// Cycle finished; the drum is stopped and the i9 shows its call.
    Done,
}

#[derive(Resource)]
pub struct Laundromat {
    pub tc: TradeComputer,
    pub machine: Machine,
    pub physics: Physics,
    pub anneal: Anneal,
    pub latch: Latch,
    pub mode: Mode,
    /// Manual-mode temperature multiplier (the spin dial).
    pub dial: f64,
    /// Index into [`PROGRAMS`].
    pub program: usize,
    /// Watch-speed multiplier (1x plays a program in its `watch_secs`).
    pub watch: f64,
    /// Tonight (the world's seed; `UPD_SEED=<night>` replays it).
    pub night: u64,
    /// Seed for the strips' starting positions and the shaking.
    pub seed: u64,
    /// The QUBO's best answer, for the scoreboard.
    pub ground: u32,
    /// Per-trade "on-ness" in 0..1, smoothed for display.
    pub activity: Vec<f32>,
    /// Per customer: the items some trade can deliver to them, with what
    /// granting that want costs the block (Goo).
    pub want_menu: Vec<Vec<(usize, f64)>>,
    /// Whose wants the picker is showing.
    pub picker_npc: usize,
    /// What Upddayett gives away tonight, if anything.
    pub give: Option<usize>,
    /// What he could give away (someone wants it), with what it costs him
    /// (his own value for it tonight, in Goo).
    pub give_menu: Vec<(usize, f64)>,
    /// Steel shield over this strip, if placed. It stops a magnet only if it
    /// covers it ([`salties::SHIELD_SPAN`]).
    pub shield: Option<usize>,
    /// The drum runs on the vape-cell battery tonight (the cells leave the
    /// trades).
    pub battery: bool,
    /// What the battery costs the block tonight (Goo off the best set).
    pub battery_cost: f64,
    /// The last idle check: the stray field on each strip (the Hall offsets).
    pub idle: Option<Vec<f64>>,
    /// Sim time the breaker trips this cycle, on a power-cut night.
    pub cut_at: Option<f64>,
    /// Where the smart Salties aimed their coil tonight (a coil night only).
    pub coil: Option<Magnet>,
    /// The i9's spin check for the current (or last) cycle.
    pub spin_check: SpinCheck,
    /// What the counter (the escrow) did with the i9's call, once the drum stops.
    pub settled: Option<escrow::Settlement>,
    /// The trade on the scope (its strip's qpos over time), if one was clicked.
    pub scope: Option<usize>,
    /// Every strip's qpos, sampled every [`TRACE_DT`] of sim time, newest last.
    pub trace: std::collections::VecDeque<Sample>,
    /// The learned drum program (the Smart program runs it).
    pub smart: SmartWash,
    /// The smart drum: when it next reads the board, and the setting it holds.
    pub drum: (f64, f64),
    /// What the smart drum remembers this cycle (the latch, when it reheated).
    pub drum_mem: smart::Memory,
    /// The drum setting (kT) when the breaker tripped this cycle.
    pub stopped_kt: Option<f64>,
    /// The row program: the back-row washers (warmer, in ladder order; ours
    /// is the coolest) and the swaps between them, this cycle or the last.
    pub row: Vec<Machine>,
    pub row_spin: Option<RowSpin>,
    /// Sim time each washer last traded loads, ours first.
    pub swapped_at: Vec<f64>,
    /// What Upddayett printed tonight (an index into `print::CATALOG`): his
    /// item on tonight's board.
    pub printed: Option<usize>,
    /// Trades tonight's yuck killed (the ghost strips), and what it cost.
    pub ghosts: yuck::Ghosts,
    /// Tonight's yuck: its source, who caught it last night, the cures.
    pub yuck: yuck::Yuck,
    /// The player's call on the ghosts tonight (one a night), and whether it was right.
    pub call: Option<(yuck::Call, bool)>,
    /// The cure for a right call is done.
    pub cured: bool,
    /// Goo of trades the cure brought back tonight (the karma of it).
    pub restored: f64,
}

/// One scope sample: sim time, the drum's kT, every strip's deflection.
pub struct Sample {
    pub t: f64,
    pub kt: f32,
    pub x: Vec<f32>,
}

/// A wash program: how long the drum takes to cool (the physics), and how
/// many real seconds the player spends watching it at 1x.
pub struct WashProgram {
    pub name: &'static str,
    /// Anneal length in sim time units.
    pub duration: f64,
    pub watch_secs: f64,
    /// How often the i9 finds the best set, measured with
    /// `trade_cli bench --runs 48 --time <duration> --night 1..=10` (480
    /// runs per program across 10 nights, with gift strips, 2026-10-05).
    pub i9_rate: &'static str,
    /// The smart wash: the learned program sets the drum from the board
    /// ([`smart::SmartWash`]) instead of the clock.
    pub smart: bool,
    /// A row of washers trading loads (parallel tempering, [`Row`]): ours is
    /// the coolest, and three of the back row run warmer.
    pub row: bool,
}

pub const PROGRAMS: [WashProgram; 6] = [
    WashProgram { name: "Quick Wash", duration: 150.0, watch_secs: 12.0, i9_rate: "26%", smart: false, row: false },
    WashProgram { name: "Permanent Press", duration: 300.0, watch_secs: 16.0, i9_rate: "46%", smart: false, row: false },
    WashProgram { name: "Normal", duration: 1000.0, watch_secs: 24.0, i9_rate: "81%", smart: false, row: false },
    WashProgram { name: "Delicates", duration: 3000.0, watch_secs: 32.0, i9_rate: "95%", smart: false, row: false },
    WashProgram { name: "Smart (learned)", duration: 1000.0, watch_secs: 24.0, i9_rate: "88%", smart: true, row: false },
    // Four washers sharing one Normal cycle's electricity: (1000 + 20) / 4 - 20
    // units each ([`Row::equal_compute`]).
    WashProgram { name: "Row of 4 washers", duration: 235.0, watch_secs: 20.0, i9_rate: "95%", smart: false, row: true },
];

/// Upddayett, in any night's cast.
fn upddayett(w: &world::World) -> usize {
    w.find_npc("upddayett").expect("Upddayett runs the place")
}

/// Night `night`'s world without its yuck, with Upddayett giving `give`
/// away (if anything) and the battery holding its cells (if it runs).
fn clean_world(night: u64, give: Option<usize>, battery: bool, printed: Option<usize>) -> world::World {
    let mut w = world::laundromat(night);
    // The print first: it is an item Upddayett could also give away.
    if let Some(k) = printed {
        w.add_print(&print::CATALOG[k]);
    }
    if let Some(item) = give {
        w.set_gift(item, true);
    }
    if battery {
        w.set_held(w.find_item(salties::BATTERY_CELLS).expect("Ranchelle's cells"), true);
    }
    w
}

/// Night `night`'s yuck (DESIGN "Yuck: the one enemy"): rolled from the
/// night, or `UPD_YUCK=clean|hungry|cold|tv|upd,ranchelle` for checking.
fn roll_yuck(night: u64) -> yuck::Yuck {
    let w = world::laundromat(night);
    std::env::var("UPD_YUCK").ok().and_then(|s| yuck::Yuck::parse(&s, &w)).unwrap_or_else(|| yuck::Yuck::roll(&w))
}

/// The night's world with `yuck` on it. Giving something away cures
/// Upddayett's own: giving lifts the giver.
fn yucky_world(night: u64, give: Option<usize>, battery: bool, printed: Option<usize>, yuck: &yuck::Yuck) -> world::World {
    let mut w = clean_world(night, give, battery, printed);
    let mut y = yuck.clone();
    if give.is_some() {
        y.cured.push(upddayett(&w));
    }
    y.apply(&mut w);
    w
}

/// Tonight's trade computer: the night's world with its yuck.
fn board_for(night: u64, give: Option<usize>, battery: bool, printed: Option<usize>, yuck: &yuck::Yuck) -> TradeComputer {
    TradeComputer::new(yucky_world(night, give, battery, printed, yuck), 5.0, 1.6)
}

/// [`board_for`], logged, with the ghosts: the trades tonight's yuck killed,
/// against the same board without it.
fn open(night: u64, give: Option<usize>, battery: bool, printed: Option<usize>, yuck: &yuck::Yuck) -> (TradeComputer, yuck::Ghosts) {
    let tc = board_for(night, give, battery, printed, yuck);
    let ghosts = yuck::Ghosts::find(&TradeComputer::new(clean_world(night, give, battery, printed), 5.0, 1.6), &tc);
    let carriers: Vec<&str> = (0..tc.world.npcs.len()).filter(|&k| tc.world.yuck_tax(k) > 0.0).map(|k| tc.world.npcs[k].name).collect();
    info!("yuck: {:?} (carriers now {carriers:?}), {} ghost trade(s), cost {:.0} Goo", yuck.source, ghosts.trades.len(), ghosts.cost);
    let gifts = tc.cycles.iter().filter(|c| c.is_gift()).count();
    info!(
        "night {night} (UPD_SEED={night} replays it): {}{}{}{}, {} trades + {gifts} gifts{}",
        tc.world.weather(),
        give.map_or(String::new(), |item| format!(", Upddayett gives away the {}", tc.world.items[item].name)),
        tc.world.night.sabotage.map_or(String::new(), |s| format!(", the Salties try {}", s.describe())),
        if battery { ", drum on the battery" } else { "" },
        tc.cycles.len() - gifts,
        if tc.dropped > 0 { format!(" ({} more left off the board)", tc.dropped) } else { String::new() }
    );
    (tc, ghosts)
}

/// Watch speed for the manual dial at 1x (sim time units per real second).
const MANUAL_SPEED: f64 = 40.0;

/// How often the i9 reads the Hall sensors (sim time units).
const SAMPLE: f64 = 1.0;
/// Most steps per frame, so a slow frame can't snowball.
const MAX_STEPS_PER_FRAME: usize = 5_000;
/// The scope samples every strip this often (sim time units) and keeps this many.
pub const TRACE_DT: f64 = 0.5;
pub const TRACE_LEN: usize = 600;

impl Laundromat {
    pub fn new() -> Self {
        let physics = Physics::default();
        // A different night every launch; `UPD_SEED=<n>` replays one exactly.
        let night = std::env::var("UPD_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() % 1_000_000)
        });
        let yuck = roll_yuck(night);
        let (tc, ghosts) = open(night, None, false, None, &yuck);
        let machine = tc.machine(physics, night).expect("build the slap-bit board");
        let mut lm = Self {
            want_menu: vec![],
            picker_npc: 0,
            give: None,
            give_menu: vec![],
            shield: None,
            battery: false,
            battery_cost: 0.0,
            idle: None,
            cut_at: None,
            coil: None,
            spin_check: SpinCheck::default(),
            settled: None,
            scope: None,
            trace: Default::default(),
            smart: SmartWash::learned(PROGRAMS[2].duration),
            drum: (0.0, 0.0),
            drum_mem: smart::Memory::default(),
            stopped_kt: None,
            row: vec![],
            row_spin: None,
            swapped_at: vec![],
            printed: None,
            tc,
            ghosts,
            yuck,
            call: None,
            cured: false,
            restored: 0.0,
            machine,
            physics,
            anneal: Anneal::default(),
            latch: Latch::new(),
            mode: Mode::Manual,
            dial: 1.5,
            program: 2,
            watch: 1.0,
            night,
            seed: night,
            ground: 0,
            activity: vec![],
        };
        lm.price_wants();
        lm.machine = lm.board(night);
        lm
    }

    /// What the Salties try tonight, if anything.
    pub fn sabotage(&self) -> Option<Sabotage> {
        self.tc.world.night.sabotage
    }

    /// Tonight's magnet as it reaches the strips (through the shield).
    pub fn magnet(&self) -> Option<Magnet> {
        match self.sabotage() {
            Some(Sabotage::Magnet(m)) => Some(m.through(self.shield.map(|s| s as f64))),
            Some(Sabotage::Coil) => self.coil.map(|m| m.through(self.shield.map(|s| s as f64))),
            _ => None,
        }
    }

    /// A fresh board for tonight, with the magnet under it if there is one.
    fn board(&self, seed: u64) -> Machine {
        match self.magnet() {
            Some(m) if self.coil.is_some() => {
                Machine::with_component(&self.tc.ising, self.physics, seed, salties::Coil(m.field(self.n_cycles(), self.physics.delta_v)))
            }
            Some(m) => self.tc.tampered_machine(self.physics, seed, &m),
            None => self.tc.machine(self.physics, seed),
        }
        .expect("build the slap-bit board")
    }

    /// Strips on tonight's board.
    fn n_cycles(&self) -> usize {
        self.tc.cycles.len()
    }

    /// The well-flattening field, the unit the Salties' numbers are quoted in.
    pub fn flat(&self) -> f64 {
        machine::max_safe_field(self.physics.delta_v)
    }

    /// Has the breaker tripped this cycle?
    pub fn power_out(&self) -> bool {
        self.cut_at.is_some_and(|t| self.machine.time() >= t)
    }

    /// What the spin check says about the last cycle: the strip that felt
    /// an unexplained push over the alarm level, and how big (x the
    /// flattening field). `None` if nothing did, or no cycle ran yet.
    pub fn spin_alarm(&self) -> Option<(usize, f64)> {
        let (k, x) = self.spin_check.strongest()?;
        let x = x.abs() / self.flat();
        (x >= salties::SPIN_ALARM).then_some((k, x))
    }

    /// The i9's idle check: stop the drum, let the strips come to rest, and
    /// read every Hall sensor against what the springs and fields say.
    pub fn idle_check(&mut self) {
        if matches!(self.mode, Mode::Cycle { .. }) {
            return;
        }
        self.dial = 0.0;
        self.mode = Mode::Manual;
        self.machine.rest(30.0).expect("rest the strips");
        let stray = self.machine.stray_field(&self.tc.ising);
        let (i, top) = stray.iter().enumerate().fold((0, 0.0f64), |b, (i, x)| if x.abs() > b.1.abs() { (i, *x) } else { b });
        info!("idle check: largest stray field {:.2}x flattening on strip {i}", top.abs() / self.flat());
        self.idle = Some(stray);
    }

    /// Put the steel shield over strip `at` (or take it off). The field on
    /// the strips changes, so the board is rebuilt with the strips carried over.
    pub fn set_shield(&mut self, at: Option<usize>) {
        if matches!(self.mode, Mode::Cycle { .. }) || at == self.shield {
            return;
        }
        self.shield = at;
        let mut board = self.board(self.seed);
        board.take_state_from(&self.machine).expect("carry the strips over");
        self.machine = board;
        self.idle = None;
        self.spin_check = SpinCheck::default();
        self.latch = Latch::new();
        self.mode = Mode::Manual;
        info!("shield: {}", at.map_or("off".into(), |s| format!("over strip {s}")));
    }

    /// Run the drum on the vape-cell battery tonight (or not). The cells
    /// leave the trades, so the board is rebuilt.
    pub fn set_battery(&mut self, on: bool) {
        if matches!(self.mode, Mode::Cycle { .. }) || on == self.battery {
            return;
        }
        self.battery = on;
        self.reopen();
    }

    /// What running on the battery cost, for the roasts.
    fn held_text(&self) -> String {
        if self.battery_cost < 0.5 {
            ", and nobody needed them for a trade tonight".to_string()
        } else {
            format!(", but holding them back cost the block {:.0} Goo of trades", self.battery_cost)
        }
    }

    /// After the spin: what the Salties did, with the measured numbers
    /// (their brag first, then the AI's roast).
    pub fn salty_lines(&self) -> Vec<(String, String)> {
        let lost = self.tc.evaluate(self.ground).0 - self.tc.evaluate(self.latch.best_bits).0;
        let cost = if lost < 0.5 { "The drum still found the best set.".to_string() } else { format!("It cost the block {lost:.0} Goo.") };
        let mut lines = match self.sabotage() {
            Some(Sabotage::Magnet(raw)) => {
                let n = self.n();
                let k = raw.strip(n);
                let open = raw.field(n, self.physics.delta_v)[k].abs() / self.flat();
                let got = self.magnet().map_or(0.0, |m| m.field(n, self.physics.delta_v)[k].abs() / self.flat());
                let roast = if got < open {
                    format!("Their magnet pushed strip {k} at {open:.2}x the flattening field. Your shield took it to {got:.2}x. Steel: 1, Salt: 0.")
                } else if open > 1.0 {
                    format!("A magnet under strip {k}, {open:.2}x the flattening field: past it, so those strips were pinned. {cost}")
                } else {
                    format!("A magnet under strip {k}, {open:.2}x the flattening field. It quietly tilted the drum. {cost}")
                };
                vec![("\"Magnet stuff, baby.\"".into(), roast)]
            }
            Some(Sabotage::PowerCut(at)) => {
                let t = self.kt_at_cut(at);
                let roast = if self.battery {
                    format!("A flare doesn't flip breakers. A hand does: {:.0}% in. Six 18650s finished the cycle{}.", 100.0 * at, self.held_text())
                } else {
                    format!(
                        "A flare doesn't flip breakers. A hand does: {:.0}% in, at {t:.2} kT. The strips froze where they were: a quench, not an anneal. {cost}",
                        100.0 * at
                    )
                };
                vec![("\"Solar flare!\"".into(), roast)]
            }
            Some(Sabotage::Emp) => vec![("\"EMP, baby!\"".into(), "It made popcorn.".into())],
            // The smart ones don't brag; the i9's numbers are all there is.
            Some(Sabotage::Coil) => {
                let Some(aimed) = self.coil else { return vec![] };
                let n = self.n();
                let k = aimed.strip(n);
                let trade = self.tc.cycles[k].short(&self.tc.world);
                let open = aimed.strength.abs();
                let felt = self.spin_check.mean().get(k).map_or(0.0, |x| x.abs() / self.flat());
                let roast = if self.magnet().is_some_and(|m| m.strength.abs() < open) {
                    format!(
                        "No brag tonight: the smart kind. A coil under strip {k}, aimed at {trade}, live only while the drum spins. \
                         Your shield took it from {open:.2}x to {felt:.2}x. Steel: 1, Salt: 0."
                    )
                } else {
                    format!(
                        "No brag tonight: the smart kind. Strip {k} felt {felt:.2}x the flattening field while the drum spun and nothing at idle: \
                         a coil keyed to the shaking, aimed at {trade}. {cost} Shield strip {k} and spin again."
                    )
                };
                vec![(String::new(), roast)]
            }
            Some(Sabotage::QuietCut(at)) => {
                let t = self.kt_at_cut(at);
                let roast = if self.battery {
                    format!("No brag, no flicker: they knew where the panel was. Tripped {:.0}% in. Six 18650s finished the cycle{}.", 100.0 * at, self.held_text())
                } else {
                    format!(
                        "No brag, no flicker: they knew where the panel was, and when. Tripped {:.0}% in, at {t:.2} kT, while it still mattered. \
                         {cost} The battery would finish it{}.",
                        100.0 * at,
                        if self.battery_cost < 0.5 { ", and nobody needs its cells tonight".to_string() } else { format!(", for {:.0} Goo of trades", self.battery_cost) }
                    )
                };
                vec![(String::new(), roast)]
            }
            None => vec![],
        };
        if self.battery && self.battery_cost >= 0.5 && self.sabotage().and_then(|s| s.cut()).is_none() {
            lines.push((String::new(), format!("Nobody touched the breaker. The battery sat there and cost the block {:.0} Goo of trades.", self.battery_cost)));
        }
        lines
    }

    /// Board-dependent state for a fresh night: the best set, every want's
    /// price (once, up front, so the picker can show costs), the display.
    fn price_wants(&mut self) {
        let tc = &self.tc;
        let mut scratch = tc.clone();
        self.want_menu = (0..tc.world.npcs.len())
            .map(|npc| {
                tc.deliverable(npc)
                    .into_iter()
                    .map(|item| {
                        scratch.want(npc, item);
                        (item, scratch.want_cost(scratch.ground_state()))
                    })
                    .collect()
            })
            .collect();
        // What he'd give up, priced on the night as if he kept everything.
        let mut kept = tc.world.clone();
        if let Some(item) = self.give {
            kept.set_gift(item, false);
        }
        let upd = upddayett(&kept);
        self.give_menu = kept.giveable(upd).into_iter().map(|item| (item, kept.value[upd][item])).collect();
        // What the battery takes off tonight's best set.
        let best = |tc: &TradeComputer| tc.evaluate(tc.forward_best).0;
        self.battery_cost = (best(&board_for(self.night, self.give, false, self.printed, &self.yuck)) - best(&board_for(self.night, self.give, true, self.printed, &self.yuck))).max(0.0);
        self.ground = tc.ground_state();
        self.activity = vec![0.0; tc.cycles.len()];
        // The smart Salties scout the board as it stands (they don't see wants).
        self.coil = matches!(self.sabotage(), Some(Sabotage::Coil)).then(|| salties::aim(&self.tc, self.physics.delta_v).0);
    }

    /// Close up and open tomorrow: new conditions, new values, a new board.
    /// A want and a give-away carry over if they still work.
    pub fn next_night(&mut self) {
        if matches!(self.mode, Mode::Cycle { .. }) {
            return;
        }
        // Yuck spreads along tonight's trades (if the drum ran) into tomorrow.
        let caught = if self.mode == Mode::Done {
            let trades: Vec<_> = cortenforge_play::trade::qubo::chosen(self.latch.best_bits, self.n()).into_iter().map(|i| &self.tc.cycles[i]).collect();
            let carriers: Vec<usize> = (0..self.tc.world.npcs.len()).filter(|&k| self.tc.world.yuck_tax(k) > 0.0).collect();
            yuck::spread(&carriers, &trades, self.night)
        } else {
            vec![]
        };
        self.night += 1;
        // One print a night: tomorrow's board starts without it.
        self.printed = None;
        self.seed = self.night;
        let tv_on = self.yuck.tv_on;
        self.yuck = roll_yuck(self.night);
        self.yuck.spread = caught;
        self.yuck.tv_on = tv_on;
        self.call = None;
        self.cured = false;
        self.restored = 0.0;
        self.reopen();
    }

    /// The player's call on tonight's ghosts: a person, or a pump? One a
    /// night. A wrong call is yuck itself: blaming spreads it to the caller.
    pub fn call_yuck(&mut self, call: yuck::Call) {
        if matches!(self.mode, Mode::Cycle { .. }) || self.call.is_some() || self.ghosts.trades.is_empty() {
            return;
        }
        let right = self.yuck.check(call);
        if !right {
            let upd = upddayett(&self.tc.world);
            if !self.yuck.spread.contains(&upd) {
                self.yuck.spread.push(upd);
            }
        }
        self.call = Some((call, right));
        info!("yuck call {call:?}: {}", if right { "right" } else { "wrong (Upddayett caught it)" });
        self.reopen();
    }

    /// Act on a right call: treat the person kindly (no label), or fix the
    /// pump (feed everyone, open the warming room, or switch the TV off).
    pub fn cure_yuck(&mut self) {
        let (Some((call, true)), false) = (self.call, self.cured) else { return };
        if matches!(self.mode, Mode::Cycle { .. }) {
            return;
        }
        let before = self.ghosts.cost;
        match call {
            yuck::Call::Person(k) => self.yuck.cured.push(k),
            yuck::Call::Pump("tv") => self.yuck.tv_on = false,
            yuck::Call::Pump(_) => self.yuck.pump_fixed = true,
        }
        self.cured = true;
        self.reopen();
        self.restored = (before - self.ghosts.cost).max(0.0);
        info!("yuck cured: {:.0} Goo of trades came back", self.restored);
    }

    /// Switch the TV on or off. Off it pumps no yuck, and shows nothing.
    pub fn set_tv(&mut self, on: bool) {
        if matches!(self.mode, Mode::Cycle { .. }) || self.yuck.tv_on == on {
            return;
        }
        self.yuck.tv_on = on;
        self.reopen();
    }

    /// Upddayett printed catalog part `k` tonight: it joins the board as his
    /// item (one print a night), so the board is rebuilt.
    pub fn add_print(&mut self, k: usize) {
        if matches!(self.mode, Mode::Cycle { .. }) || self.printed.is_some() {
            return;
        }
        self.printed = Some(k);
        self.reopen();
        info!("Upddayett's print joins the board: the {}", print::CATALOG[k].item);
    }

    /// Upddayett gives `give` away tonight (or keeps everything). The item
    /// leaves the trades and gets a gift strip, so the board is rebuilt
    /// from scratch (it has a different number of strips).
    pub fn set_give(&mut self, give: Option<usize>) {
        if matches!(self.mode, Mode::Cycle { .. }) || give == self.give {
            return;
        }
        self.give = give;
        self.reopen();
        let what = give.map_or("nothing".to_string(), |item| format!("the {}", self.tc.world.items[item].name));
        info!("Upddayett gives away {what}");
    }

    /// Open tonight's board (for `night`, with `give`), keeping the want if
    /// something can still deliver it.
    fn reopen(&mut self) {
        let want = self.tc.want;
        let upd = upddayett(&self.tc.world);
        let mut tonight = world::laundromat(self.night);
        if let Some(k) = self.printed {
            tonight.add_print(&print::CATALOG[k]);
        }
        if self.give.is_some_and(|item| !tonight.giveable(upd).contains(&item)) {
            self.give = None;
        }
        (self.tc, self.ghosts) = open(self.night, self.give, self.battery, self.printed, &self.yuck);
        self.idle = None;
        self.cut_at = None;
        self.spin_check = SpinCheck::default();
        self.price_wants();
        self.machine = self.board(self.seed);
        self.latch = Latch::new();
        self.mode = Mode::Manual;
        if let Some((npc, item)) = want {
            self.set_want(Some((npc, item)));
            if self.tc.want.is_none() {
                let (who, what) = (self.tc.world.npcs[npc].name, self.tc.world.items[item].name);
                info!("want dropped: nothing gets {who} the {what} tonight");
            }
        }
    }

    pub fn n(&self) -> usize {
        self.machine.n
    }

    /// Fresh laundry: rebuild the board with new random strip positions.
    pub fn new_load(&mut self) {
        self.seed += 1;
        self.machine = self.board(self.seed);
        self.row.clear();
        self.row_spin = None;
        self.cut_at = None;
        self.latch = Latch::new();
        self.settled = None;
        self.mode = Mode::Manual;
        info!("new load: seed {}, strips re-randomized", self.seed);
    }

    /// What the counter did with the i9's call, for the result panel and the log.
    pub fn counter_lines(&self) -> Vec<String> {
        let Some(s) = &self.settled else { return vec![] };
        let w = &self.tc.world;
        let gifts = s.done.iter().filter(|&&c| self.tc.cycles[c].is_gift()).count();
        let trades = s.done.len() - gifts;
        let on_counter = (0..w.items.len()).filter(|&i| escrow::escrowed(w, i)).count();
        let moved = s.moved(w);
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut lines = vec![format!(
            "The counter held all {on_counter} items through the spin, then {} changed hands in {}{}; {} went home.",
            moved,
            plural(trades, "trade", "trades"),
            if gifts > 0 { format!(" and {}", plural(gifts, "gift", "gifts")) } else { String::new() },
            on_counter - moved
        )];
        for &c in &s.voided {
            let cy = &self.tc.cycles[c];
            let taken = cy.legs.iter().find(|l| s.owner[l.item] != w.items[l.item].owner).map_or("an item", |l| w.items[l.item].name);
            lines.push(format!("It called off {}: the {taken} was already promised to a bigger trade. Everyone in it got their own things back.", cy.short(w)));
        }
        lines
    }

    /// Set or clear the want. That rewires the springs, and couplings can't
    /// change after `install`, so the board is rebuilt; the strips carry
    /// over so nothing jumps. The next Run spins with the want in place.
    pub fn set_want(&mut self, want: Option<(usize, usize)>) {
        if matches!(self.mode, Mode::Cycle { .. }) || want == self.tc.want {
            return;
        }
        match want {
            Some((npc, item)) => {
                if !self.tc.want(npc, item) {
                    return;
                }
            }
            None => self.tc.clear_want(),
        }
        let mut board = self.board(self.seed);
        board.take_state_from(&self.machine).expect("carry the strips over");
        self.machine = board;
        self.ground = self.tc.ground_state();
        self.latch = Latch::new();
        self.mode = Mode::Manual;
        info!("want: {}", self.want_text().unwrap_or_else(|| "none (forward mode)".into()));
    }

    /// "Upddayett wants the 350 W scooter hub motor"
    pub fn want_text(&self) -> Option<String> {
        self.tc.want.map(|(npc, item)| format!("{} wants the {}", self.tc.world.npcs[npc].name, self.tc.world.items[item].name))
    }

    /// Does trade `c` move the wanted item to the person who wants it?
    pub fn carries_want(&self, c: usize) -> bool {
        self.tc.want.is_some_and(|(npc, item)| self.tc.cycles[c].delivers(npc, item))
    }

    /// Load fresh laundry and run the selected program. Every cycle starts
    /// from random strips, so it earns its answer.
    pub fn start_cycle(&mut self) {
        self.new_load();
        let p = &PROGRAMS[self.program];
        self.anneal.duration = p.duration;
        // The breaker trips on a power-cut night; the battery keeps the drum going.
        let cut = self.sabotage().and_then(|s| s.cut());
        self.spin_check = SpinCheck::default();
        self.anneal.cut = cut.filter(|_| !self.battery);
        self.cut_at = cut.map(|at| self.machine.time() + at * p.duration);
        self.mode = Mode::Cycle { start: self.machine.time() };
        self.drum = (0.0, 0.0);
        self.drum_mem = smart::Memory::default();
        self.stopped_kt = None;
        if p.row {
            // Three more boards of tonight's laundry, each its own load and
            // shaking (seeded as in `trade_cli rowmatch`).
            let row = Row::default();
            self.row = (1..row.washers).map(|k| self.board(self.seed.wrapping_add(k as u64 * 104_729))).collect();
            let mut washers: Vec<&mut Machine> = std::iter::once(&mut self.machine).chain(self.row.iter_mut()).collect();
            self.row_spin = Some(RowSpin::new(&row, &mut washers, self.seed));
            self.swapped_at = vec![f64::NEG_INFINITY; row.washers];
        }
        let how = if p.smart {
            format!("learned program, params {:?}", self.smart.params())
        } else if let Some(spin) = &self.row_spin {
            let ladder: Vec<String> = (0..=self.row.len()).map(|i| format!("{:.2}", spin.setting(i))).collect();
            format!("{} washers at kT x[{}], trading loads every unit,", self.row.len() + 1, ladder.join(", "))
        } else {
            format!("kT x{} -> x{}", self.anneal.hot, self.anneal.cold)
        };
        info!(
            "spin cycle: {} ({how} over {} units; i9 finds the best set {} of the time), watching at {:.0} units/s",
            p.name,
            p.duration,
            p.i9_rate,
            self.speed()
        );
        if let Some(w) = self.want_text() {
            info!("  with want: {w}");
        }
    }

    /// The Smart program's drum setting at `t` into the cycle: it reads the
    /// board every [`smart::SAMPLE`] units and holds in between, as it did
    /// in training. A tripped breaker still stops it.
    fn smart_drum(&mut self, t: f64) -> f64 {
        if t >= self.anneal.stop_time() {
            if self.anneal.cut.is_some() && self.stopped_kt.is_none() {
                self.stopped_kt = Some(self.drum.1 * self.physics.k_b_t);
            }
            return 0.0;
        }
        if t >= self.drum.0 - 1e-9 {
            let before = self.drum_mem.frozen;
            let kt = self.smart.read(&mut self.drum_mem, t, self.machine.positions(), &self.tc.problem.qubo);
            if self.drum_mem.frozen > before {
                info!("t={t:.0}: the strips froze, so the smart drum reheats");
            }
            self.drum = (self.drum.0 + smart::SAMPLE, kt);
        }
        self.drum.1
    }

    /// The drum's kT when the breaker tripped `at` (0..1) through the cycle.
    fn kt_at_cut(&self, at: f64) -> f64 {
        self.stopped_kt.unwrap_or_else(|| self.anneal.temperature(at * self.anneal.duration - 1e-9) * self.physics.k_b_t)
    }

    /// Sim time units per real second right now.
    pub fn speed(&self) -> f64 {
        let base = match self.mode {
            Mode::Manual => MANUAL_SPEED,
            Mode::Cycle { .. } | Mode::Done => {
                let p = &PROGRAMS[self.program];
                (p.duration + self.anneal.settle) / p.watch_secs
            }
        };
        base * self.watch
    }

    /// Log what the drum settled on and what the i9 latched.
    fn log_result(&self) {
        let n = self.n();
        let mask = |b: u32| (0..n).map(|i| if (b >> i) & 1 == 1 { '#' } else { '.' }).collect::<String>();
        let rest_clash = self.tc.evaluate(self.latch.final_bits).1;
        let (rest, best, ground) =
            (self.tc.score_text(self.latch.final_bits), self.tc.score_text(self.latch.best_bits), self.tc.score_text(self.ground));
        info!(
            "{} done: at rest {} = {rest}{} | i9 latched {} = {best} at t={:.0} | best possible {} = {ground} -> {}",
            PROGRAMS[self.program].name,
            mask(self.latch.final_bits),
            if rest_clash { " (clash)" } else { "" },
            mask(self.latch.best_bits),
            self.latch.best_time,
            mask(self.ground),
            if self.tc.is_optimal(self.latch.best_bits) { "BEST" } else { "missed" }
        );
        if let Some(spin) = &self.row_spin {
            let rates: Vec<String> = (0..spin.stats.offers.len()).map(|i| format!("{:.0}%", 100.0 * spin.stats.rate(i))).collect();
            info!(
                "  the row: swaps accepted [{}] between neighbors, {} loads carried from the hottest washer down to ours",
                rates.join(" "),
                spin.stats.trips
            );
        }
        for line in self.counter_lines() {
            info!("  {line}");
        }
        if let Some(w) = self.want_text() {
            let got = self.tc.delivers_want(self.latch.best_bits);
            info!(
                "  want ({w}): {}, cost the block {:.0} Goo",
                if got { "delivered" } else { "NOT delivered" },
                self.tc.want_cost(self.latch.best_bits)
            );
        }
    }

    /// Drum temperature multiplier right now.
    pub fn temperature(&self) -> f64 {
        self.machine.temperature()
    }

    /// 0..1 through the current spin cycle.
    pub fn progress(&self) -> f64 {
        match self.mode {
            Mode::Cycle { start } => ((self.machine.time() - start) / self.anneal.total_time()).clamp(0.0, 1.0),
            Mode::Done => 1.0,
            Mode::Manual => 0.0,
        }
    }

    /// Is trade `i` locked in by the i9 (cycle finished)?
    pub fn locked(&self, i: usize) -> bool {
        self.mode == Mode::Done && (self.latch.best_bits >> i) & 1 == 1
    }

    pub fn well(&self, i: usize) -> WellState {
        self.machine.well(i)
    }
}

pub fn step_sim(time: Res<Time>, mut lm: ResMut<Laundromat>) {
    let lm = &mut *lm;
    let dt = lm.physics.dt;
    let real = time.delta_secs_f64().min(0.1);
    let steps = ((lm.speed() * real / dt).round() as usize).min(MAX_STEPS_PER_FRAME);
    let sample_steps = (SAMPLE / dt).round() as u64;
    let trace_steps = (TRACE_DT / dt).round() as u64;
    // A rebuilt board can have a different number of strips: start the scope over.
    if lm.trace.back().is_some_and(|s| s.x.len() != lm.n()) {
        lm.trace.clear();
        lm.scope = None;
    }
    for _ in 0..steps {
        // The row program steps every washer itself.
        let mut stepped = false;
        match lm.mode {
            Mode::Manual => lm.machine.set_temperature(lm.dial),
            Mode::Done => lm.machine.set_temperature(0.0),
            Mode::Cycle { start } => {
                let t = lm.machine.time() - start;
                if t >= lm.anneal.total_time() {
                    lm.latch.final_bits = lm.machine.bits();
                    lm.mode = Mode::Done;
                    lm.settled = Some(escrow::settle(&lm.tc.world, &lm.tc.cycles, lm.latch.best_bits));
                    lm.log_result();
                    continue;
                }
                if let Some(spin) = lm.row_spin.as_mut() {
                    // Every washer on its rung, trading loads, until the
                    // drums stop (or the breaker trips); then all settle.
                    let mut washers: Vec<&mut Machine> = std::iter::once(&mut lm.machine).chain(lm.row.iter_mut()).collect();
                    let swapped = if t < lm.anneal.stop_time() {
                        spin.step(&lm.tc.ising, &mut washers).expect("row step")
                    } else {
                        for m in washers.iter_mut() {
                            m.set_temperature(0.0);
                            m.step().expect("sim step");
                        }
                        vec![]
                    };
                    let now = lm.machine.time();
                    for i in swapped {
                        lm.swapped_at[i] = now;
                        lm.swapped_at[i + 1] = now;
                    }
                    stepped = true;
                } else {
                    let kt = if PROGRAMS[lm.program].smart { lm.smart_drum(t) } else { lm.anneal.temperature(t) };
                    lm.machine.set_temperature(kt);
                }
            }
        }
        if !stepped {
            lm.machine.step().expect("sim step");
        }
        if matches!(lm.mode, Mode::Cycle { .. }) && ((lm.machine.time() / dt).round() as u64).is_multiple_of(sample_steps) {
            // The i9 reads every washer in the row.
            lm.latch.observe(&lm.machine, &lm.tc.problem.qubo);
            for m in &lm.row {
                lm.latch.observe(m, &lm.tc.problem.qubo);
            }
            lm.spin_check.observe(&lm.machine, &lm.tc.ising);
        }
        if ((lm.machine.time() / dt).round() as u64).is_multiple_of(trace_steps) {
            let x = lm.machine.positions().iter().map(|&x| x as f32).collect();
            let kt = (lm.machine.temperature() * lm.physics.k_b_t) as f32;
            lm.trace.push_back(Sample { t: lm.machine.time(), kt, x });
        }
    }

    while lm.trace.len() > TRACE_LEN {
        lm.trace.pop_front();
    }

    // Smooth each strip's deflection into a 0..1 "trade is on" level.
    let k = 1.0 - (-8.0 * time.delta_secs()).exp();
    for i in 0..lm.n() {
        let x = lm.machine.positions()[i] as f32;
        let target = ((x + 0.6) / 1.2).clamp(0.0, 1.0);
        let target = target * target * (3.0 - 2.0 * target);
        lm.activity[i] += (target - lm.activity[i]) * k;
    }
}
