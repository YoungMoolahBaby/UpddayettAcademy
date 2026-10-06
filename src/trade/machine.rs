//! The washing machine: one CortenForge Langevin particle per candidate trade.
//!
//! Each bit is a buckled spring-steel strip ("slap bit"), modeled as a
//! `DoubleWellPotential`. Springs between strips are `PairwiseCoupling`, the
//! per-trade bias is an `ExternalField`, and the drum's shaking is a
//! `LangevinThermostat` whose temperature follows `ctrl[0]`.

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::therm_env::generate_mjcf;
use cortenforge::sim::thermostat::{
    DoubleWellPotential, ExternalField, LangevinThermostat, PairwiseCoupling, PassiveStack, WellState,
};

use super::qubo::Ising;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Physical constants of the slap-bit board.
#[derive(Clone, Copy, Debug)]
pub struct Physics {
    /// Barrier height of each strip (clamp-screw tightness).
    pub delta_v: f64,
    /// Damping (air + clamp friction).
    pub gamma: f64,
    /// Base thermal energy; `ctrl[0]` multiplies it.
    pub k_b_t: f64,
    pub dt: f64,
}

impl Default for Physics {
    fn default() -> Self {
        Self { delta_v: 5.0, gamma: 1.0, k_b_t: 1.0, dt: 0.01 }
    }
}

/// Spin cycle: geometric cool-down from `hot` to `cold` (multipliers on kT)
/// over `duration` time units, then `settle` time units with the drum
/// stopped so every strip drops into a well.
#[derive(Clone, Copy, Debug)]
pub struct Anneal {
    pub hot: f64,
    pub cold: f64,
    pub duration: f64,
    pub settle: f64,
    /// A tripped breaker: the drum stops this far (0..1) through the
    /// cool-down and the settle starts early, so the strips freeze into
    /// whatever they had (a quench). `None`: the power holds.
    pub cut: Option<f64>,
}

impl Default for Anneal {
    fn default() -> Self {
        Self { hot: 4.0, cold: 0.35, duration: 1000.0, settle: 20.0, cut: None }
    }
}

impl Anneal {
    /// When the drum stops shaking: the end of the cool-down, or the cut.
    pub fn stop_time(&self) -> f64 {
        self.duration * self.cut.map_or(1.0, |c| c.clamp(0.0, 1.0))
    }

    pub fn total_time(&self) -> f64 {
        self.stop_time() + self.settle
    }

    /// Drum temperature multiplier at time `t`.
    pub fn temperature(&self, t: f64) -> f64 {
        if t >= self.stop_time() {
            0.0
        } else {
            self.hot * (self.cold / self.hot).powf(t / self.duration)
        }
    }
}

/// Above roughly this field a tilted quartic well has no second minimum.
/// For V = dV (x^2 - 1)^2 / x0^4 the critical tilt is 8 dV / (3 sqrt 3) / x0.
pub fn max_safe_field(delta_v: f64) -> f64 {
    8.0 * delta_v / (3.0 * 3f64.sqrt())
}

pub struct Machine {
    pub n: usize,
    pub physics: Physics,
    model: Model,
    data: Data,
}

impl Machine {
    pub fn new(ising: &Ising, physics: Physics, seed: u64) -> Result<Self, Error> {
        Self::build(ising, physics, seed, None)
    }

    /// A board with a field the i9 didn't install on top (a magnet under
    /// the counter): a second `ExternalField` in the stack, so it sums with
    /// the trade biases and the want. `stray` has one entry per strip.
    pub fn with_stray_field(ising: &Ising, physics: Physics, seed: u64, stray: &[f64]) -> Result<Self, Error> {
        Self::build(ising, physics, seed, Some(stray))
    }

    fn build(ising: &Ising, physics: Physics, seed: u64, stray: Option<&[f64]>) -> Result<Self, Error> {
        let n = ising.n;
        // One actuator slot so ctrl[0] exists; it drives temperature, not force.
        let xml = generate_mjcf(n, 1, physics.dt, (0.0, 10.0));
        let mut model = load_model(&xml)?;
        let mut data = model.make_data();
        let mut b = PassiveStack::builder();
        for i in 0..n {
            b = b.with(DoubleWellPotential::new(physics.delta_v, 1.0, i));
        }
        if !ising.edges.is_empty() {
            b = b.with(PairwiseCoupling::new(ising.j.clone(), ising.edges.clone()));
        }
        assert_eq!(ising.h.len(), n, "ExternalField needs one entry per bit");
        b = b.with(ExternalField::new(ising.h.clone()));
        if let Some(stray) = stray {
            assert_eq!(stray.len(), n, "ExternalField needs one entry per bit");
            b = b.with(ExternalField::new(stray.to_vec()));
        }
        b = b.with(
            LangevinThermostat::new(DVector::from_element(n, physics.gamma), physics.k_b_t, seed, 0)
                .with_ctrl_temperature(0),
        );
        b.build().install(&mut model);

        // Start every strip in a random well: the laundry goes in unsorted.
        let mut s = seed ^ 0x9E37_79B9_7F4A_7C15;
        for i in 0..n {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            data.qpos[i] = if s & 1 == 1 { 1.0 } else { -1.0 };
            data.qvel[i] = 0.0;
        }
        data.ctrl[0] = 1.0;
        data.forward(&model)?;
        Ok(Self { n, physics, model, data })
    }

    pub fn set_temperature(&mut self, multiplier: f64) {
        self.data.ctrl[0] = multiplier;
    }

    pub fn temperature(&self) -> f64 {
        self.data.ctrl[0]
    }

    pub fn time(&self) -> f64 {
        self.data.time
    }

    pub fn step(&mut self) -> Result<(), Error> {
        self.data.step(&self.model)?;
        Ok(())
    }

    /// Strip deflections, one per bit (about -1 or +1 inside a well).
    pub fn positions(&self) -> &[f64] {
        &self.data.qpos.as_slice()[..self.n]
    }

    pub fn velocities(&self) -> &[f64] {
        &self.data.qvel.as_slice()[..self.n]
    }

    /// Carry the strips over from another board (same bit count), e.g.
    /// after rewiring the springs, which means building a new board because
    /// couplings can't change after `install`. Time and drum setting come too.
    pub fn take_state_from(&mut self, other: &Machine) -> Result<(), Error> {
        assert_eq!(self.n, other.n, "boards differ in size");
        for i in 0..self.n {
            self.data.qpos[i] = other.data.qpos[i];
            self.data.qvel[i] = other.data.qvel[i];
        }
        self.data.time = other.data.time;
        self.data.ctrl[0] = other.data.ctrl[0];
        self.data.forward(&self.model)?;
        Ok(())
    }

    pub fn well(&self, i: usize) -> WellState {
        WellState::from_position(self.data.qpos[i], 0.5)
    }

    /// Bits as a mask (strip in the right well = trade on). Strips still on
    /// the barrier read as off.
    pub fn bits(&self) -> u32 {
        (0..self.n).filter(|&i| self.well(i) == WellState::Right).fold(0, |m, i| m | 1 << i)
    }

    pub fn all_in_wells(&self) -> bool {
        (0..self.n).all(|i| self.well(i).is_in_well())
    }

    /// Stop the drum for `time` units so the strips come to rest (the
    /// damping takes a strip's ringing down by e^-15 in 30 units).
    pub fn rest(&mut self, time: f64) -> Result<(), Error> {
        self.set_temperature(0.0);
        let steps = (time / self.physics.dt).round() as usize;
        for _ in 0..steps {
            self.step()?;
        }
        Ok(())
    }

    /// The i9's idle check, on strips at rest ([`Machine::rest`]): each
    /// strip's Hall reading says where it sits, so the well's pull there is
    /// known, and it must balance the springs and the fields the i9
    /// installed (`ising`). Whatever is left over is a field it didn't
    /// install: a magnet. A strip sitting `d` off its usual rest point
    /// reads about `8 dV d` here (the well's stiffness).
    pub fn stray_field(&self, ising: &Ising) -> Vec<f64> {
        let x = self.positions();
        // V = dV (x^2 - 1)^2 with x0 = 1 (as built above); V' = 4 dV x (x^2 - 1).
        let mut f: Vec<f64> = (0..self.n).map(|i| 4.0 * self.physics.delta_v * x[i] * (x[i] * x[i] - 1.0) - ising.h[i]).collect();
        for (&(i, k), &j) in ising.edges.iter().zip(&ising.j) {
            f[i] -= j * x[k];
            f[k] -= j * x[i];
        }
        f
    }
}
