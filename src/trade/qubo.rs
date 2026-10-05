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

pub fn chosen(bits: u32, n: usize) -> Vec<usize> {
    (0..n).filter(|&i| (bits >> i) & 1 == 1).collect()
}

/// `masks[i]` has bit k set when cycles i and k conflict (move the same item).
pub fn conflict_masks(cycles: &[Cycle]) -> Vec<u32> {
    assert!(cycles.len() <= 32, "trade sets are u32 masks");
    (0..cycles.len())
        .map(|i| (0..cycles.len()).filter(|&k| k != i && cycles[i].conflicts(&cycles[k])).fold(0, |m, k| m | 1 << k))
        .collect()
}

/// Calls `f` with every valid trade set (no two trades share an item),
/// including the empty set. There are far fewer of these than 2^n states:
/// thousands, not a million, at 20 bits.
pub fn for_each_valid_set(masks: &[u32], mut f: impl FnMut(u32)) {
    fn go(i: usize, set: u32, banned: u32, masks: &[u32], f: &mut dyn FnMut(u32)) {
        if i == masks.len() {
            f(set);
            return;
        }
        go(i + 1, set, banned, masks, f);
        if banned & (1 << i) == 0 {
            go(i + 1, set | 1 << i, banned | masks[i], masks, f);
        }
    }
    go(0, 0, 0, masks, &mut f);
}

/// Best valid trade set for per-cycle `weights` (exact).
pub fn best_valid_set(weights: &[f64], masks: &[u32]) -> (u32, f64) {
    let mut best = (0u32, 0.0f64);
    for_each_valid_set(masks, |s| {
        let v: f64 = chosen(s, weights.len()).iter().map(|&i| weights[i]).sum();
        if v > best.1 + 1e-12 {
            best = (s, v);
        }
    });
    best
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
        let w = world::laundromat(1);
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
        let w = world::laundromat(1);
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
}
