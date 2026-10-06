//! The Salties' sabotage (DESIGN.md, "The Salties"): a magnet under the
//! counter and a tripped breaker. Only the honest physics works, so that's
//! all there is here.
//!
//! The magnet is a field the i9 didn't install: a second `ExternalField` in
//! the strips' stack ([`super::Machine::with_stray_field`]), summing with the
//! trade biases and the want. The i9 still scores with the clean night's
//! QUBO, so a run the magnet steered off the best set shows up as a loss.

use super::machine;
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

/// What the Salties try tonight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sabotage {
    Magnet(Magnet),
    /// The breaker trips this far through the cool-down (0..1).
    PowerCut(f64),
    /// Their "EMP". Nothing happens; that's the joke. It made popcorn.
    Emp,
}

impl Sabotage {
    /// Uniform draws a night roll spends on the Salties, always all of
    /// them, so coins added after these stay put.
    pub const DRAWS: usize = 5;

    /// Tonight's sabotage from `DRAWS` uniform draws in 0..1.
    pub fn roll(u: [f64; Self::DRAWS]) -> Option<Self> {
        if u[0] >= SALTY_NIGHTS {
            return None;
        }
        Some(if u[1] >= MAGNET_SHARE + CUT_SHARE {
            Sabotage::Emp
        } else if u[1] < MAGNET_SHARE {
            // Anywhere under the 20-slot counter; 0.2-1.6x flattening, either way.
            let strength = 0.2 + 1.4 * u[3];
            let pos = -0.5 + u[2] * super::MAX_BITS as f64;
            Sabotage::Magnet(Magnet { pos, strength: if u[4] < 0.5 { strength } else { -strength } })
        } else {
            Sabotage::PowerCut(0.15 + 0.7 * u[2])
        })
    }

    /// What they brag before the spin. Only the magnet brag is honest.
    pub fn brag(&self) -> &'static str {
        match self {
            Sabotage::Magnet(_) => "\"Magnet stuff tonight. Science, baby.\"",
            Sabotage::PowerCut(_) => "\"We're calling down a solar flare.\"",
            Sabotage::Emp => "\"EMP tonight. Your little computer's toast.\"",
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
        }
    }
}

/// The calibration board ([`CALIBRATION_NIGHT`]), clean and forward.
pub fn calibration_load() -> TradeComputer {
    TradeComputer::new(world::laundromat(CALIBRATION_NIGHT), 5.0, 1.6)
}
