//! The street's spring (DESIGN "The sphere": "The pull"): the economy pulls
//! everyone toward the street's middle. Each person sits at their net worth
//! in doublings (log2 Goo); a flat-bottomed spring ties them to the street's
//! mean there: slack within `free` doublings, pulling `k` Goo per doubling
//! squared beyond. It only steers: it never takes anything, it picks among
//! the win-win trades and routes the free gifts, so the spread energy is a
//! term in what the drum minimizes.
//!
//! The exact energy (after a set of trades and gifts) judges the best set.
//! On the board it is a force on the strips ([`Force`]), a CortenForge
//! passive component: exact at every setting of the strips, no quadratic
//! fit (a flat-bottomed spring on a log scale isn't pairwise).

use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::thermostat::PassiveComponent;

use super::cycles::Cycle;
use super::qubo;
use super::world::World;

#[derive(Clone, Debug)]
pub struct Spring {
    /// Goo per doubling squared, past the free band.
    pub k: f64,
    /// Doublings from the middle that cost nothing.
    pub free: f64,
    /// Each person's net worth before tonight, in Goo (see [`net_worth`]);
    /// businesses aren't on the spring.
    pub worth: Vec<f64>,
}

/// What everything `npc` holds is worth to them tonight, per person, in Goo.
pub fn net_worth(w: &World) -> Vec<f64> {
    let mut nw = vec![0.0; w.npcs.len()];
    for (id, it) in w.items.iter().enumerate() {
        nw[it.owner] += w.value[it.owner][id];
    }
    nw
}

/// Doublings, with a 1 Goo floor so nobody is infinitely far behind.
pub fn doublings(worth: f64) -> f64 {
    worth.max(1.0).log2()
}

/// How a candidate changes each person's net worth, in Goo: what they get,
/// at their value, minus what they hand over, at theirs.
pub fn deltas(w: &World, c: &Cycle) -> Vec<f64> {
    let mut d = vec![0.0; w.npcs.len()];
    for l in &c.legs {
        d[l.to] += w.value[l.to][l.item];
        d[l.from] -= w.value[l.from][l.item];
    }
    d
}

impl Spring {
    /// Tonight's net worths, `k` Goo per doubling squared past `free`.
    pub fn tonight(w: &World, k: f64, free: f64) -> Self {
        Self { k, free, worth: net_worth(w) }
    }

    fn people(&self, w: &World) -> Vec<usize> {
        (0..w.npcs.len()).filter(|&i| !w.npcs[i].business).collect()
    }

    /// Each person's distance from the middle, in doublings (0 for
    /// businesses), for net worths `worth`.
    pub fn offsets(&self, w: &World, worth: &[f64]) -> Vec<f64> {
        let people = self.people(w);
        let x: Vec<f64> = worth.iter().map(|&v| doublings(v)).collect();
        let mean = people.iter().map(|&i| x[i]).sum::<f64>() / people.len() as f64;
        (0..worth.len()).map(|i| if w.npcs[i].business { 0.0 } else { x[i] - mean }).collect()
    }

    /// Net worths after the set `bits` of `cycles`.
    pub fn after(&self, w: &World, cycles: &[Cycle], bits: u32) -> Vec<f64> {
        let mut worth = self.worth.clone();
        for i in qubo::chosen(bits, cycles.len()) {
            for (v, d) in worth.iter_mut().zip(deltas(w, &cycles[i])) {
                *v += d;
            }
        }
        worth
    }

    /// The spread energy, in Goo, for net worths `worth`.
    pub fn energy_of(&self, w: &World, worth: &[f64]) -> f64 {
        self.offsets(w, worth).iter().map(|y| self.k * (y.abs() - self.free).max(0.0).powi(2)).sum()
    }

    /// The spread energy after the set `bits`, exactly.
    pub fn energy(&self, w: &World, cycles: &[Cycle], bits: u32) -> f64 {
        self.energy_of(w, &self.after(w, cycles, bits))
    }

    /// The spring as a force on the strips ([`Force`]): `beta` kT per QUBO
    /// unit, `scale` QUBO units per Goo, as the board's other springs.
    pub fn force(&self, w: &World, cycles: &[Cycle], beta: f64, scale: f64) -> Force {
        let people = self.people(w);
        Force {
            k: self.k,
            free: self.free,
            worth: self.worth.clone(),
            on: (0..w.npcs.len()).map(|i| people.contains(&i)).collect(),
            deltas: cycles.iter().map(|c| deltas(w, c)).collect(),
            gain: beta * scale,
        }
    }
}

/// The street's spring on the board: a many-body force, not a pairwise
/// one. Strip c at deflection x is candidate c "p = (x + 1) / 2 of the way
/// on", so each person's net worth is `worth + sum p_c deltas_c`, exactly
/// linear in the strips. The spring's energy (in kT, `gain` x Goo) is then
/// the exact spread energy at every corner, where each strip sits in a
/// well, and smooth in between; its force on strip c is minus its slope.
pub struct Force {
    k: f64,
    free: f64,
    worth: Vec<f64>,
    /// Who's on the spring (people, not businesses).
    on: Vec<bool>,
    /// `deltas[c][i]`: what candidate c does to person i's net worth.
    deltas: Vec<Vec<f64>>,
    gain: f64,
}

impl Force {
    /// Each person's net worth with the strips at `x`.
    fn worth_at(&self, x: &[f64]) -> Vec<f64> {
        let mut worth = self.worth.clone();
        for (c, d) in self.deltas.iter().enumerate() {
            let p = (x[c] + 1.0) / 2.0;
            for (v, dv) in worth.iter_mut().zip(d) {
                *v += p * dv;
            }
        }
        worth
    }

    /// The spring's potential energy with the strips at `x`, in kT.
    pub fn energy(&self, x: &[f64]) -> f64 {
        let lx: Vec<f64> = self.worth_at(x).iter().map(|&v| doublings(v)).collect();
        let people: Vec<usize> = (0..lx.len()).filter(|&i| self.on[i]).collect();
        let mean = people.iter().map(|&i| lx[i]).sum::<f64>() / people.len() as f64;
        self.gain * people.iter().map(|&i| self.k * ((lx[i] - mean).abs() - self.free).max(0.0).powi(2)).sum::<f64>()
    }

    /// d(spread energy)/d(net worth) per person, in Goo per Goo, with the
    /// strips at `x`.
    fn slope(&self, x: &[f64]) -> Vec<f64> {
        let worth = self.worth_at(x);
        let count = self.on.iter().filter(|&&o| o).count() as f64;
        let lx: Vec<f64> = worth.iter().map(|&v| doublings(v)).collect();
        let mean = (0..lx.len()).filter(|&i| self.on[i]).map(|i| lx[i]).sum::<f64>() / count;
        let h: Vec<f64> = (0..lx.len())
            .map(|i| if self.on[i] { let y = lx[i] - mean; y.signum() * (y.abs() - self.free).max(0.0) } else { 0.0 })
            .collect();
        let hmean = h.iter().sum::<f64>() / count;
        (0..lx.len())
            .map(|i| {
                if !self.on[i] || worth[i] <= 1.0 {
                    return 0.0;
                }
                // E = k sum h^2; dE/dlog2(worth_i) = 2k (h_i - mean h); the
                // 1 Goo floor is flat.
                2.0 * self.k * (h[i] - hmean) / (worth[i] * std::f64::consts::LN_2)
            })
            .collect()
    }
}

impl PassiveComponent for Force {
    fn apply(&self, _model: &Model, data: &Data, qfrc_out: &mut DVector<f64>) {
        let n = self.deltas.len();
        let x = &data.qpos.as_slice()[..n];
        let g = self.slope(x);
        for (c, d) in self.deltas.iter().enumerate() {
            let de_dp: f64 = d.iter().zip(&g).map(|(dv, gi)| dv * gi).sum();
            qfrc_out[c] -= self.gain * de_dp / 2.0;
        }
    }
}
