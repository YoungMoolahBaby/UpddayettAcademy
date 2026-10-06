//! The folding counter is the escrow. Every item on the market goes onto it
//! before the spin; when the drum stops, the counter carries out each chosen
//! trade whole, or not at all. A 4-way swap only works if everyone delivers
//! or nobody does, so nobody hands anything straight to anybody.

use super::cycles::Cycle;
use super::qubo;
use super::world::World;

/// What the counter did with the i9's call.
#[derive(Clone, Debug, PartialEq)]
pub struct Settlement {
    /// Who holds each item when the counter is done (indexed like `World::items`).
    pub owner: Vec<usize>,
    /// Chosen trades and gifts the counter carried out.
    pub done: Vec<usize>,
    /// Chosen ones it called off because an item was already promised to
    /// another (a clash): everyone in them got their own things back.
    pub voided: Vec<usize>,
}

/// Does item `i` go on the counter? Everything on the market does; the
/// battery's cells stay in the drum.
pub fn escrowed(w: &World, i: usize) -> bool {
    !w.items[i].held
}

/// Settle `bits` (one strip per entry of `cycles`): the most valuable chosen
/// trade claims its items first; a later one that needs a claimed item is
/// called off whole. Panics if the result doesn't conserve items (see
/// [`Settlement::check`]).
pub fn settle(w: &World, cycles: &[Cycle], bits: u32) -> Settlement {
    let mut owner: Vec<usize> = w.items.iter().map(|it| it.owner).collect();
    let mut claimed = vec![false; w.items.len()];
    let mut chosen = qubo::chosen(bits, cycles.len());
    // Stable: equal values keep board order.
    chosen.sort_by(|&a, &b| cycles[b].value().total_cmp(&cycles[a].value()));
    let (mut done, mut voided) = (vec![], vec![]);
    for c in chosen {
        let legs = &cycles[c].legs;
        if legs.iter().any(|l| claimed[l.item] || !escrowed(w, l.item)) {
            voided.push(c);
            continue;
        }
        for l in legs {
            claimed[l.item] = true;
            owner[l.item] = l.to;
        }
        done.push(c);
    }
    let s = Settlement { owner, done, voided };
    if let Err(e) = s.check(w, cycles) {
        panic!("the counter lost track: {e}");
    }
    s
}

impl Settlement {
    /// Nothing duplicated or lost: every item ends with exactly one person;
    /// an item moved only if one carried-out trade moved it, and then it went
    /// where that trade said; everything else went home; each person ends
    /// with what they had, minus what they gave, plus what they got.
    pub fn check(&self, w: &World, cycles: &[Cycle]) -> Result<(), String> {
        let n_items = w.items.len();
        if self.owner.len() != n_items {
            return Err(format!("{} owners for {n_items} items", self.owner.len()));
        }
        if let Some(i) = self.owner.iter().position(|&o| o >= w.npcs.len()) {
            return Err(format!("the {} went to nobody", w.items[i].name));
        }
        let mut moved_by = vec![None; n_items];
        let mut count: Vec<i64> = vec![0; w.npcs.len()];
        for it in &w.items {
            count[it.owner] += 1;
        }
        for &c in &self.done {
            for l in &cycles[c].legs {
                if let Some(other) = moved_by[l.item].replace(c) {
                    return Err(format!("the {} moved twice (strips {other} and {c})", w.items[l.item].name));
                }
                if w.items[l.item].owner != l.from {
                    return Err(format!("strip {c} gives the {} from someone who doesn't own it", w.items[l.item].name));
                }
                count[l.from] -= 1;
                count[l.to] += 1;
            }
        }
        for (i, it) in w.items.iter().enumerate() {
            let want = cycles_leg_to(cycles, moved_by[i], i).unwrap_or(it.owner);
            if self.owner[i] != want {
                return Err(format!("the {} ended with {} instead of {}", it.name, w.npcs[self.owner[i]].name, w.npcs[want].name));
            }
        }
        let mut held: Vec<i64> = vec![0; w.npcs.len()];
        for &o in &self.owner {
            held[o] += 1;
        }
        if held != count {
            return Err(format!("item counts per person {held:?}, expected {count:?}"));
        }
        Ok(())
    }

    /// How many items changed hands.
    pub fn moved(&self, w: &World) -> usize {
        self.owner.iter().zip(&w.items).filter(|(o, it)| **o != it.owner).count()
    }
}

/// Where carried-out strip `c` sends item `i`.
fn cycles_leg_to(cycles: &[Cycle], c: Option<usize>, i: usize) -> Option<usize> {
    cycles[c?].legs.iter().find(|l| l.item == i).map(|l| l.to)
}
