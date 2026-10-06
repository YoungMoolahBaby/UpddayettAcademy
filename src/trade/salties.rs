//! The Salties' sabotage (DESIGN.md, "The Salties"): a magnet under the
//! counter and a tripped breaker. Only the honest physics works, so that's
//! all there is here. The dumb ones brag about it; the smart ones say
//! nothing, aim a coil that only runs while the drum spins, or trip the
//! breaker early.
//!
//! The magnet is a field the i9 didn't install: a second `ExternalField` in
//! the strips' stack ([`super::Machine::with_stray_field`]), summing with the
//! trade biases and the want. The i9 still scores with the clean night's
//! QUBO, so a run the magnet steered off the best set shows up as a loss.

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::thermostat::PassiveComponent;

use super::machine::{self, Machine};
use super::qubo::{self, Ising};
use super::world;
use super::TradeComputer;

/// How far under the strip row the magnet is taped, in strip pitches.
pub const DEPTH: f64 = 1.5;

/// A steel shield from a dead hard drive lets this much of the field through.
pub const SHIELD: f64 = 0.1;

/// The shield is a plate this many strip pitches either side of where it's
/// put. It stops the field only if the magnet is under it, so you have to
/// find the magnet first (the idle check).
pub const SHIELD_SPAN: f64 = 2.0;

/// The battery: six 18650s. They're the cells Upddayett wants for his
/// balance bot, so while they run the drum nobody trades them.
pub const BATTERY_CELLS: &str = "18650";

/// The calibration load: night 1, a board whose best set the drum finds 98%
/// of the time on Normal (PLAN 3.3d bench). When the drum misses it, suspect
/// the magnet.
pub const CALIBRATION_NIGHT: u64 = 1;

/// How often the Salties show up, and what they try: the magnet, the
/// breaker, or (the rest) the "EMP".
pub const SALTY_NIGHTS: f64 = 0.3;
pub const MAGNET_SHARE: f64 = 0.5;
pub const CUT_SHARE: f64 = 0.35;

/// Of the Salties nights, how many are the smart ones (no brag, aimed, timed).
pub const SMART_SHARE: f64 = 0.4;

/// The smart Salties' coil: strong enough to steer the drum, just under the
/// flattening field so it never pins a strip (pinned strips are easy to spot).
pub const COIL_STRENGTH: f64 = 0.8;

/// The spin check calls a push real above this (x the flattening field). A
/// clean Normal spin averages out to at most ~0.12x on some strip (the
/// shaking doesn't fully cancel in 1000 reads); shorter spins are noisier.
pub const SPIN_ALARM: f64 = 0.3;

/// A magnet under the counter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Magnet {
    /// Where along the board, in strip slots (strip i sits at slot i).
    pub pos: f64,
    /// Push on the strip right above it, as a multiple of the field that
    /// flattens a well ([`machine::max_safe_field`]). Positive pushes the
    /// strips toward "on", negative toward "off".
    pub strength: f64,
}

impl Magnet {
    /// The push on each of `n` strips. The magnet points along the strips'
    /// swing, and across the swing a dipole's field is just -m / r^3 (no
    /// sign change), so strip i gets `strength * (DEPTH / r)^3`, with `r`
    /// its distance from the magnet (`DEPTH` under the row).
    pub fn field(&self, n: usize, delta_v: f64) -> Vec<f64> {
        let top = self.strength * machine::max_safe_field(delta_v);
        (0..n)
            .map(|i| {
                let dx = i as f64 - self.pos;
                top * (DEPTH * DEPTH / (dx * dx + DEPTH * DEPTH)).powf(1.5)
            })
            .collect()
    }

    /// The same magnet behind the hard-drive steel.
    pub fn shielded(self) -> Self {
        Self { strength: self.strength * SHIELD, ..self }
    }

    /// What reaches the strips with the shield over strip `shield` (if
    /// anywhere): shielded when the plate covers the magnet, untouched
    /// otherwise.
    pub fn through(self, shield: Option<f64>) -> Self {
        match shield {
            Some(at) if (at - self.pos).abs() <= SHIELD_SPAN => self.shielded(),
            _ => self,
        }
    }

    /// The strip right above it (clamped to a board of `n`).
    pub fn strip(&self, n: usize) -> usize {
        (self.pos.round().max(0.0) as usize).min(n.saturating_sub(1))
    }
}

/// What the Salties try tonight. The dumb ones brag and try whatever they
/// heard about; the smart ones ([`Sabotage::is_smart`]) say nothing, aim, and
/// time it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sabotage {
    Magnet(Magnet),
    /// The breaker trips this far through the cool-down (0..1).
    PowerCut(f64),
    /// Their "EMP". Nothing happens; that's the joke. It made popcorn.
    Emp,
    /// Smart: an electromagnet keyed to the drum's shaking, so it's off
    /// whenever the drum is stopped (the idle check sees nothing). They
    /// scout the board and aim it ([`aim`]).
    Coil,
    /// Smart: the breaker, tripped early, when it hurts, and with no
    /// flicker (they know where the panel is).
    QuietCut(f64),
}

impl Sabotage {
    /// Uniform draws a night roll spends on the Salties, always all of
    /// them, so coins added after these stay put.
    pub const DRAWS: usize = 6;

    /// Tonight's sabotage from `DRAWS` uniform draws in 0..1.
    pub fn roll(u: [f64; Self::DRAWS]) -> Option<Self> {
        if u[0] >= SALTY_NIGHTS {
            return None;
        }
        let smart = u[5] < SMART_SHARE;
        let cut = (MAGNET_SHARE..MAGNET_SHARE + CUT_SHARE).contains(&u[1]);
        Some(match (smart, cut) {
            // Smart ones never bother with the "EMP".
            (true, false) => Sabotage::Coil,
            (true, true) => Sabotage::QuietCut(0.12 + 0.18 * u[2]),
            (false, true) => Sabotage::PowerCut(0.15 + 0.7 * u[2]),
            (false, false) if u[1] >= MAGNET_SHARE + CUT_SHARE => Sabotage::Emp,
            (false, false) => {
                // Anywhere under the 20-slot counter; 0.2-1.6x flattening, either way.
                let strength = 0.2 + 1.4 * u[3];
                let pos = -0.5 + u[2] * super::MAX_BITS as f64;
                Sabotage::Magnet(Magnet { pos, strength: if u[4] < 0.5 { strength } else { -strength } })
            }
        })
    }

    /// The smart ones: no brag, no tells before the spin.
    pub fn is_smart(&self) -> bool {
        matches!(self, Sabotage::Coil | Sabotage::QuietCut(_))
    }

    /// When the breaker trips (0..1 through the cool-down), if it does.
    pub fn cut(&self) -> Option<f64> {
        match *self {
            Sabotage::PowerCut(at) | Sabotage::QuietCut(at) => Some(at),
            _ => None,
        }
    }

    /// What they brag before the spin, if they're the bragging kind. Only
    /// the magnet brag is honest.
    pub fn brag(&self) -> Option<&'static str> {
        match self {
            Sabotage::Magnet(_) => Some("\"Magnet stuff tonight. Science, baby.\""),
            Sabotage::PowerCut(_) => Some("\"We're calling down a solar flare.\""),
            Sabotage::Emp => Some("\"EMP tonight. Your little computer's toast.\""),
            Sabotage::Coil | Sabotage::QuietCut(_) => None,
        }
    }

    /// "a magnet under strip 7 (0.8x, pushing off)", "a power cut 40% in".
    pub fn describe(&self) -> String {
        match *self {
            Sabotage::Magnet(m) => format!(
                "a magnet under strip {:.0} ({:.1}x, pushing {})",
                m.pos,
                m.strength.abs(),
                if m.strength > 0.0 { "on" } else { "off" }
            ),
            Sabotage::PowerCut(at) => format!("a power cut {:.0}% into the cool-down", 100.0 * at),
            Sabotage::Emp => "an \"EMP\" (it made popcorn)".to_string(),
            Sabotage::Coil => "an aimed coil that runs only while the drum spins (smart)".to_string(),
            Sabotage::QuietCut(at) => format!("a quiet power cut {:.0}% into the cool-down (smart)", 100.0 * at),
        }
    }
}

/// The smart Salties scout tonight's board and put the coil where it costs
/// the most: every strip, both ways, at [`COIL_STRENGTH`], scored by how much
/// worse the best set gets once the tilt is added to the trade values (a
/// field `h` on a strip is worth `2 h / (beta * scale)` Goo to the drum).
/// Returns the coil and the Goo it should cost.
pub fn aim(tc: &TradeComputer, delta_v: f64) -> (Magnet, f64) {
    let n = tc.cycles.len();
    let masks = qubo::conflict_masks(&tc.cycles);
    let best = tc.evaluate(tc.forward_best).0;
    let per_goo = 2.0 / (tc.beta * tc.problem.scale);
    let mut top = (Magnet { pos: 0.0, strength: COIL_STRENGTH }, f64::NEG_INFINITY);
    for p in 0..n {
        for sign in [1.0, -1.0] {
            let m = Magnet { pos: p as f64, strength: sign * COIL_STRENGTH };
            let tilted: Vec<f64> = tc.cycles.iter().zip(m.field(n, delta_v)).map(|(c, h)| c.value() + h * per_goo).collect();
            let (set, _) = qubo::best_valid_set(&tilted, &masks);
            let lost = best - tc.evaluate(set).0;
            if lost > top.1 + 1e-9 {
                top = (m, lost);
            }
        }
    }
    top
}

/// The coil: a field that's there only while the drum shakes (`ctrl[0] >
/// 0`). The crate has no switchable field, but `PassiveComponent` is public,
/// so it's a few lines.
pub struct Coil(pub Vec<f64>);

impl PassiveComponent for Coil {
    fn apply(&self, _model: &Model, data: &Data, qfrc_out: &mut DVector<f64>) {
        if data.ctrl[0] > 0.0 {
            for (i, h) in self.0.iter().enumerate() {
                qfrc_out[i] += h;
            }
        }
    }
}

/// The i9's spin check: the idle check's force balance, averaged over a
/// spin while the drum shakes. The shaking, the springs' ringing and the
/// strips' hops average away; a push that's only there while the drum
/// spins doesn't.
#[derive(Clone, Debug, Default)]
pub struct SpinCheck {
    sum: Vec<f64>,
    samples: usize,
}

impl SpinCheck {
    /// One Hall-sensor read, only while the drum shakes.
    pub fn observe(&mut self, m: &Machine, ising: &Ising) {
        if m.temperature() <= 0.0 {
            return;
        }
        let f = m.stray_field(ising);
        if self.sum.len() != f.len() {
            *self = Self { sum: vec![0.0; f.len()], samples: 0 };
        }
        for (s, x) in self.sum.iter_mut().zip(f) {
            *s += x;
        }
        self.samples += 1;
    }

    pub fn samples(&self) -> usize {
        self.samples
    }

    /// The average unexplained push on each strip (empty before any read).
    pub fn mean(&self) -> Vec<f64> {
        self.sum.iter().map(|s| s / self.samples.max(1) as f64).collect()
    }

    /// The strip with the biggest average push, and that push.
    pub fn strongest(&self) -> Option<(usize, f64)> {
        self.mean().into_iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
    }
}

/// The calibration board ([`CALIBRATION_NIGHT`]), clean and forward.
pub fn calibration_load() -> TradeComputer {
    TradeComputer::new(world::laundromat(CALIBRATION_NIGHT), 5.0, 1.6)
}
