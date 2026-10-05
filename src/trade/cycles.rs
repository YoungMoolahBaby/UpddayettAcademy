//! Candidate trades: closed giving loops where everybody comes out ahead.

use super::world::World;

/// `from` hands `item` to `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leg {
    pub from: usize,
    pub to: usize,
    pub item: usize,
}

/// A 2- to 4-way swap. Each participant gives one item and receives one.
#[derive(Clone, Debug)]
pub struct Cycle {
    pub legs: Vec<Leg>,
    /// Gain per leg's *recipient*, in the world's value units.
    pub gains: Vec<f64>,
}

impl Cycle {
    pub fn value(&self) -> f64 {
        self.gains.iter().sum()
    }

    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }

    pub fn delivers(&self, npc: usize, item: usize) -> bool {
        self.legs.iter().any(|l| l.to == npc && l.item == item)
    }

    /// Two cycles conflict when they move the same item.
    pub fn conflicts(&self, other: &Cycle) -> bool {
        self.legs.iter().any(|a| other.legs.iter().any(|b| a.item == b.item))
    }

    pub fn describe(&self, w: &World) -> String {
        let parts: Vec<String> = self
            .legs
            .iter()
            .map(|l| format!("{} gives the {} to {}", w.npcs[l.from].name, w.items[l.item].name, w.npcs[l.to].name))
            .collect();
        parts.join(", ")
    }

    /// "Upddayett -> Vape Lady -> Upddayett"
    pub fn short(&self, w: &World) -> String {
        let mut s = w.npcs[self.legs[0].from].name.to_string();
        for l in &self.legs {
            s.push_str(" -> ");
            s.push_str(w.npcs[l.to].name);
        }
        s
    }
}

/// Enumerates every giving loop of 2..=max_len people in which each
/// participant values what they receive more than what they give.
pub fn enumerate(w: &World, max_len: usize) -> Vec<Cycle> {
    let mut out = vec![];
    for start in 0..w.npcs.len() {
        let mut path = vec![];
        dfs(w, start, start, max_len, &mut path, &mut out);
    }
    out.sort_by(|a, b| b.value().partial_cmp(&a.value()).unwrap());
    out
}

fn dfs(w: &World, start: usize, at: usize, max_len: usize, path: &mut Vec<Leg>, out: &mut Vec<Cycle>) {
    for (item, it) in w.items.iter().enumerate() {
        if it.owner != at {
            continue;
        }
        for to in 0..w.npcs.len() {
            // Canonical form: `start` is the lowest-numbered participant.
            if to < start || to == at || w.value[to][item] <= 0.0 {
                continue;
            }
            if to != start && path.iter().any(|l| l.from == to) {
                continue;
            }
            path.push(Leg { from: at, to, item });
            if to == start {
                if path.len() >= 2
                    && let Some(c) = score(w, path)
                {
                    out.push(c);
                }
            } else if path.len() < max_len {
                dfs(w, start, to, max_len, path, out);
            }
            path.pop();
        }
    }
}

fn score(w: &World, legs: &[Leg]) -> Option<Cycle> {
    let n = legs.len();
    let mut gains = Vec::with_capacity(n);
    for k in 0..n {
        let got = legs[k];
        let gave = legs[(k + 1) % n];
        debug_assert_eq!(got.to, gave.from);
        let g = w.value[got.to][got.item] - w.value[gave.from][gave.item];
        if g <= 0.0 {
            return None;
        }
        gains.push(g);
    }
    Some(Cycle { legs: legs.to_vec(), gains })
}
