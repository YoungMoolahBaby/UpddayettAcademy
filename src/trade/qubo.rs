//! Picking the best non-conflicting set of trades, as a QUBO and as Ising
//! couplings for `cortenforge::sim::thermostat`.

use std::collections::BTreeMap;

use super::cycles::Cycle;

/// Minimize `sum lin_i x_i + sum quad_ik x_i x_k` over x in {0, 1}^n.
#[derive(Clone, Debug)]
pub struct Qubo {
    pub n: usize,
    pub lin: Vec<f64>,
    pub quad: BTreeMap<(usize, usize), f64>,
}

/// Ising form in the crate's convention: `H = -sum J s_i s_k - sum h s_i`,
/// spin `s = 2x - 1` (bit set = spin +1 = particle in the right well).
#[derive(Clone, Debug)]
pub struct Ising {
    pub n: usize,
    pub edges: Vec<(usize, usize)>,
    pub j: Vec<f64>,
    pub h: Vec<f64>,
}

impl Qubo {
    pub fn new(n: usize) -> Self {
        Self { n, lin: vec![0.0; n], quad: BTreeMap::new() }
    }

    pub fn add(&mut self, i: usize, k: usize, v: f64) {
        if i == k {
            self.lin[i] += v;
        } else {
            *self.quad.entry((i.min(k), i.max(k))).or_default() += v;
        }
    }

    pub fn energy(&self, bits: u32) -> f64 {
        let x = |i: usize| ((bits >> i) & 1) as f64;
        let mut e: f64 = (0..self.n).map(|i| self.lin[i] * x(i)).sum();
        for (&(i, k), &v) in &self.quad {
            e += v * x(i) * x(k);
        }
        e
    }

    /// `H_ising = beta * E_qubo + const`. `beta` sets how many kT one unit of
    /// QUBO energy is worth.
    pub fn to_ising(&self, beta: f64) -> Ising {
        let mut h: Vec<f64> = self.lin.iter().map(|a| -beta * a / 2.0).collect();
        let mut edges = vec![];
        let mut j = vec![];
        for (&(i, k), &b) in &self.quad {
            edges.push((i, k));
            j.push(-beta * b / 4.0);
            h[i] -= beta * b / 4.0;
            h[k] -= beta * b / 4.0;
        }
        Ising { n: self.n, edges, j, h }
    }

    /// Brute-force ground state. Fine up to ~24 bits.
    pub fn brute_force(&self) -> (u32, f64) {
        assert!(self.n <= 24, "brute force over {} bits is too slow", self.n);
        (0..1u32 << self.n)
            .map(|b| (b, self.energy(b)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap()
    }
}

/// A trade-selection problem ready for the machine.
#[derive(Clone, Debug)]
pub struct TradeProblem {
    pub qubo: Qubo,
    /// Value units -> QUBO units (largest cycle value maps to `MAX_VALUE`).
    pub scale: f64,
    pub penalty: f64,
    /// Extra reward per cycle in QUBO units (backward mode), 0 otherwise.
    pub bias: Vec<f64>,
}

/// Largest normalized cycle value.
pub const MAX_VALUE: f64 = 1.5;

/// Maximum-weight independent set: reward each cycle by its value, penalize
/// every pair of cycles that move the same item by `penalty` (QUBO units).
/// For the right answer the penalty has to beat the smaller reward in each
/// pair: above `MAX_VALUE`, plus the bias when both cycles carry it. `bias`
/// (world value units, one per cycle) adds extra reward, used by backward mode.
pub fn build(cycles: &[Cycle], bias: Option<&[f64]>, penalty: f64) -> TradeProblem {
    let n = cycles.len();
    let vmax = cycles.iter().map(Cycle::value).fold(0.0, f64::max);
    let scale = MAX_VALUE / vmax;
    let bias: Vec<f64> = match bias {
        Some(b) => b.iter().map(|x| x * scale).collect(),
        None => vec![0.0; n],
    };
    let mut q = Qubo::new(n);
    for (i, c) in cycles.iter().enumerate() {
        q.add(i, i, -(c.value() * scale + bias[i]));
        for (k, d) in cycles.iter().enumerate().skip(i + 1) {
            if c.conflicts(d) {
                // Two biased cycles would otherwise collect the bias twice.
                q.add(i, k, penalty + bias[i].min(bias[k]));
            }
        }
    }
    TradeProblem { qubo: q, scale, penalty, bias }
}

/// Bias for backward mode: enough extra reward on every cycle that delivers
/// `item` to `npc` that taking one always beats whatever it displaces.
/// Returns `None` if no cycle delivers it.
pub fn want_bias(cycles: &[Cycle], npc: usize, item: usize) -> Option<Vec<f64>> {
    let hits: Vec<usize> = (0..cycles.len()).filter(|&i| cycles[i].delivers(npc, item)).collect();
    if hits.is_empty() {
        return None;
    }
    let displaced = |i: usize| -> f64 {
        let lost: f64 = cycles.iter().filter(|d| d.conflicts(&cycles[i])).map(Cycle::value).sum();
        // `conflicts` matches the cycle itself: take its value back out, then
        // once more because the cycle's own value already pays for part.
        lost - cycles[i].value() * 2.0
    };
    let need = hits.iter().map(|&i| displaced(i)).fold(0.0, f64::max);
    let vmin = cycles.iter().map(Cycle::value).fold(f64::INFINITY, f64::min);
    let b = need + vmin;
    Some((0..cycles.len()).map(|i| if hits.contains(&i) { b } else { 0.0 }).collect())
}

pub fn chosen(bits: u32, n: usize) -> Vec<usize> {
    (0..n).filter(|&i| (bits >> i) & 1 == 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::{cycles, world};

    fn ising_energy(i: &Ising, bits: u32) -> f64 {
        let s = |k: usize| if (bits >> k) & 1 == 1 { 1.0 } else { -1.0 };
        let pair: f64 = i.edges.iter().zip(&i.j).map(|(&(a, b), j)| j * s(a) * s(b)).sum();
        let field: f64 = (0..i.n).map(|k| i.h[k] * s(k)).sum();
        -pair - field
    }

    #[test]
    fn ising_is_beta_times_qubo_plus_constant() {
        let w = world::laundromat_tuesday();
        let c = cycles::enumerate(&w, 4);
        let p = build(&c, None, 2.4);
        let beta = 5.0;
        let ising = p.qubo.to_ising(beta);
        let offset = ising_energy(&ising, 0) - beta * p.qubo.energy(0);
        for bits in [1u32, 0b1011, 0x2A5A, 0x3FFF, 0x1234] {
            let d = ising_energy(&ising, bits) - beta * p.qubo.energy(bits);
            assert!((d - offset).abs() < 1e-9, "bits {bits:#x}: {d} vs {offset}");
        }
    }

    #[test]
    fn ground_state_is_a_clash_free_trade_set() {
        let w = world::laundromat_tuesday();
        let c = cycles::enumerate(&w, 4);
        assert!(c.iter().all(|cy| cy.gains.iter().all(|&g| g > 0.0)));
        let (bits, _) = build(&c, None, 2.4).qubo.brute_force();
        let on = chosen(bits, c.len());
        for (a, &i) in on.iter().enumerate() {
            for &k in &on[a + 1..] {
                assert!(!c[i].conflicts(&c[k]));
            }
        }
    }

    #[test]
    fn want_bias_makes_the_ground_state_deliver() {
        let w = world::laundromat_tuesday();
        let c = cycles::enumerate(&w, 4);
        let upd = w.find_npc("upddayett").unwrap();
        for item in ["hub motor", "18650", "soldering iron", "wi-fi"] {
            let item = w.find_item(item).unwrap();
            let bias = want_bias(&c, upd, item).unwrap();
            let (bits, _) = build(&c, Some(&bias), 2.4).qubo.brute_force();
            assert!(chosen(bits, c.len()).iter().any(|&i| c[i].delivers(upd, item)));
            let on = chosen(bits, c.len());
            assert!(on.iter().enumerate().all(|(a, &i)| on[a + 1..].iter().all(|&k| !c[i].conflicts(&c[k]))));
        }
    }
}
