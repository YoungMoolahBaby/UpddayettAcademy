//! Candidates for the drum: trades (closed giving loops where everybody comes
//! out ahead) and gift chains (a donation passed along an open path).

use super::world::{Tag, World};

/// `from` hands `item` to `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leg {
    pub from: usize,
    pub to: usize,
    pub item: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A 2- to 4-way swap: each participant gives one item and gets one.
    Trade,
    /// A donor gives something away; each recipient may pass something of
    /// theirs on (only if they still come out ahead); the last one keeps it.
    Gift,
}

/// One candidate: one strip on the board.
#[derive(Clone, Debug)]
pub struct Cycle {
    pub kind: Kind,
    pub legs: Vec<Leg>,
    /// Gain per leg's *recipient*, in Goo.
    pub gains: Vec<f64>,
    /// Gifts only: Karma for needs met (gains on need legs, 1x).
    pub relief: f64,
    /// Gifts only: Karma for purpose or pleasure (1.5x when the person's
    /// needs are covered tonight, else 1x).
    pub flourishing: f64,
}

/// Flourishing weight for someone whose basic needs are covered tonight.
pub const FLOURISH: f64 = 1.5;

/// Longest gift chain, in recipients. 1 = direct gifts: the drum decides who
/// gets the donation (the routing). Pay-it-forward chains (2-3 recipients,
/// like kidney-exchange chains) work but make a glassy problem: near-equal
/// sets many strip-flips apart. Measured on Normal / Delicates across 10
/// nights: 1 recipient 81% / 94%, 2: 54% / 86%, 3: 42% / 65%. Longer chains
/// are a later unlock (better board or program).
pub const MAX_CHAIN: usize = 1;

impl Cycle {
    /// What the drum maximizes: Goo for a trade, Karma for a gift.
    pub fn value(&self) -> f64 {
        match self.kind {
            Kind::Trade => self.goo(),
            Kind::Gift => self.karma(),
        }
    }

    /// Total Goo everyone in it gains.
    pub fn goo(&self) -> f64 {
        self.gains.iter().sum()
    }

    pub fn karma(&self) -> f64 {
        self.relief + self.flourishing
    }

    pub fn is_gift(&self) -> bool {
        self.kind == Kind::Gift
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

    /// Two candidates conflict when they move the same item.
    pub fn conflicts(&self, other: &Cycle) -> bool {
        self.legs.iter().any(|a| other.legs.iter().any(|b| a.item == b.item))
    }

    pub fn describe(&self, w: &World) -> String {
        let parts: Vec<String> = self
            .legs
            .iter()
            .map(|l| format!("{} gives the {} to {}", w.npcs[l.from].name, w.items[l.item].name, w.npcs[l.to].name))
            .collect();
        let s = parts.join(", ");
        match self.kind {
            Kind::Trade => s,
            Kind::Gift => format!("{s}, who keeps it"),
        }
    }

    /// What each person comes out ahead, in Goo: "Upddayett +4, Vape Lady +3".
    pub fn gains_text(&self, w: &World) -> String {
        let parts: Vec<String> =
            self.legs.iter().zip(&self.gains).map(|(l, g)| format!("{} +{g:.0}", w.npcs[l.to].name)).collect();
        parts.join(", ")
    }

    /// "Upddayett's cracked Android phone for Vape Lady's six 18650 cells"
    /// (2-way swaps only; longer loops and gifts use [`Cycle::describe`]).
    pub fn swap_text(&self, w: &World) -> Option<String> {
        let ([a, b], Kind::Trade) = (self.legs.as_slice(), self.kind) else {
            return None;
        };
        Some(format!(
            "{}'s {} for {}'s {}",
            w.npcs[a.from].name, w.items[a.item].name, w.npcs[b.from].name, w.items[b.item].name
        ))
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

/// Karma weight for `npc` getting `item` tonight: needs count 1x (relief);
/// purpose and pleasure count [`FLOURISH`]x once their needs are covered.
pub fn karma_weight(w: &World, npc: usize, item: usize) -> (Tag, f64) {
    match w.tag(npc, item) {
        Some(Tag::Need) => (Tag::Need, 1.0),
        Some(t) => (t, if w.needs_covered(npc) { FLOURISH } else { 1.0 }),
        None => (Tag::Pleasure, 0.0),
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
        if it.owner != at || it.gift {
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
        // A yucky recipient needs the tax on top before the trade is worth it.
        let g = w.value[got.to][got.item] - w.value[gave.from][gave.item] - w.yuck_tax(got.to);
        if g <= 0.0 {
            return None;
        }
        gains.push(g);
    }
    Some(Cycle { kind: Kind::Trade, legs: legs.to_vec(), gains, relief: 0.0, flourishing: 0.0 })
}

/// Enumerates gift chains of 1..=max_people recipients from every gift item,
/// sorted by Karma. A recipient may pass one of their own things on only if
/// they still come out ahead; nobody appears twice; the donor gets nothing.
pub fn enumerate_gifts(w: &World, max_people: usize) -> Vec<Cycle> {
    let mut out = vec![];
    for (gift, it) in w.items.iter().enumerate() {
        if !it.gift {
            continue;
        }
        for to in 0..w.npcs.len() {
            if to != it.owner && w.value[to][gift] > 0.0 {
                let mut path = vec![Leg { from: it.owner, to, item: gift }];
                chain(w, max_people, &mut path, &mut out);
            }
        }
    }
    out.sort_by(|a, b| b.value().partial_cmp(&a.value()).unwrap());
    out
}

fn chain(w: &World, max_people: usize, path: &mut Vec<Leg>, out: &mut Vec<Cycle>) {
    // Stop here: the last recipient keeps what they got.
    if let Some(c) = score_chain(w, path) {
        out.push(c);
    }
    if path.len() == max_people {
        return;
    }
    let last = *path.last().unwrap();
    let holder = last.to;
    for (item, it) in w.items.iter().enumerate() {
        if it.owner != holder || it.gift || w.value[holder][last.item] <= w.value[holder][item] {
            continue;
        }
        for to in 0..w.npcs.len() {
            let seen = to == path[0].from || path.iter().any(|l| l.to == to);
            if seen || w.value[to][item] <= 0.0 {
                continue;
            }
            path.push(Leg { from: holder, to, item });
            chain(w, max_people, path, out);
            path.pop();
        }
    }
}

fn score_chain(w: &World, legs: &[Leg]) -> Option<Cycle> {
    let (mut gains, mut relief, mut flourishing) = (Vec::with_capacity(legs.len()), 0.0, 0.0);
    for (k, got) in legs.iter().enumerate() {
        let gave = legs.get(k + 1).map_or(0.0, |next| w.value[got.to][next.item]);
        let g = w.value[got.to][got.item] - gave;
        if g <= 0.0 {
            return None;
        }
        gains.push(g);
        match karma_weight(w, got.to, got.item) {
            (Tag::Need, wt) => relief += wt * g,
            (_, wt) => flourishing += wt * g,
        }
    }
    Some(Cycle { kind: Kind::Gift, legs: legs.to_vec(), gains, relief, flourishing })
}
