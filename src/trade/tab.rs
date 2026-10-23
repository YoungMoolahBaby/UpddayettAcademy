//! The tab (DESIGN "The tab"): neighbors covering each other. When someone
//! wants a thing tonight and has nothing the owner wants, it can still go
//! to them on the laundromat counter's tab: they owe the giver what it was
//! worth to the giver, in Goo. Debts get paid back in things the one owed
//! wants, and rings of IOUs (Gravo owes Ranchelle, Ranchelle owes Brisko,
//! Brisko owes Gravo) cancel with nothing changing hands, the way
//! Slovenia's multilateral set-off clears unpaid invoices.
//!
//! Nobody is chased: a debt just sits on the tab until it's paid or
//! cleared. The only rule is the limit, the most anyone owes in all, so
//! clearing a ring frees room for tomorrow.

use super::cycles::{Cycle, Kind, Leg};
use super::world::World;

/// One line of the tab moving: `debtor` owes `creditor` `amount` Goo more
/// (negative: less).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iou {
    pub debtor: usize,
    pub creditor: usize,
    pub amount: f64,
}

/// Stands in for "anyone" in [`Iou::line`].
const ANYONE: usize = usize::MAX;

impl Iou {
    /// The lines it touches, for conflicts: taking a debt off uses that
    /// debtor-creditor line (two can't both spend it); a new debt uses the
    /// debtor's one new debt a night and the creditor's one new credit, so
    /// one night can't carry anyone past the limit either way.
    pub fn lines(&self) -> [(usize, usize); 2] {
        if self.amount > 0.0 {
            [(self.debtor, ANYONE), (ANYONE, self.creditor)]
        } else {
            [(self.debtor, self.creditor); 2]
        }
    }
}

/// The counter's tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// `owed[debtor][creditor]`, in Goo.
    pub owed: Vec<Vec<f64>>,
    /// The most anyone owes in all, in Goo.
    pub limit: f64,
    /// Rings of IOUs go on the board to cancel.
    pub rings: bool,
    /// One balance each (mutual credit, like Sardex): what you're owed by
    /// anyone counts against what you owe anyone, so the limit is on your
    /// net. Off: every debt is between two people.
    pub mutual: bool,
    /// Everyone on the same footing (user, 2026-10-23): the limit runs both
    /// ways, so nobody piles up what they're owed either, like Keynes's
    /// 1944 clearing union, which charged creditor countries as well as
    /// debtors. Off: only debts are capped.
    pub even: bool,
}

/// Longest ring of IOUs on the board (2-way debts net off by themselves).
pub const MAX_RING: usize = 4;

impl Tab {
    /// An empty tab for `n` people.
    pub fn new(n: usize, limit: f64) -> Self {
        Self { owed: vec![vec![0.0; n]; n], limit, rings: true, mutual: false, even: true }
    }

    /// What `k` is owed in all, as the limit sees it (with one balance
    /// each: net of what they owe).
    pub fn credit(&self, k: usize) -> f64 {
        let owed = self.owed_to(k);
        if self.mutual { (owed - self.owed[k].iter().sum::<f64>()).max(0.0) } else { owed }
    }

    /// Can `debtor` take on `amount` more, owed to `creditor`?
    pub fn room(&self, debtor: usize, creditor: usize, amount: f64) -> bool {
        self.owes(debtor) + amount <= self.limit && (!self.even || self.credit(creditor) + amount <= self.limit)
    }

    /// What `k` owes in all (with one balance each: net of what they're owed).
    pub fn owes(&self, k: usize) -> f64 {
        let owes: f64 = self.owed[k].iter().sum();
        if self.mutual { (owes - self.owed_to(k)).max(0.0) } else { owes }
    }

    /// What `k` is owed in all.
    pub fn owed_to(&self, k: usize) -> f64 {
        self.owed.iter().map(|row| row[k]).sum()
    }

    /// Everything on the tab.
    pub fn total(&self) -> f64 {
        self.owed.iter().flatten().sum()
    }

    /// Apply what a chosen candidate does to the tab, then net two-way debts
    /// (Gravo owes Ranchelle 5 and she owes him 3: he owes her 2).
    pub fn apply(&mut self, c: &Cycle) {
        for x in &c.tab {
            let line = &mut self.owed[x.debtor][x.creditor];
            *line = (*line + x.amount).max(0.0);
        }
        let n = self.owed.len();
        for a in 0..n {
            for b in a + 1..n {
                let m = self.owed[a][b].min(self.owed[b][a]);
                self.owed[a][b] -= m;
                self.owed[b][a] -= m;
            }
        }
    }
}

/// Tonight's tab candidates on `w` (none when the tab is closed): things on
/// the tab, paying it back, and (if `rings`) rings of IOUs. Best first.
pub fn enumerate(w: &World) -> Vec<Cycle> {
    let Some(tab) = &w.tab else {
        return vec![];
    };
    let mut out = vec![];
    let n = w.npcs.len();
    let tab_cycle = |kind, legs: Vec<Leg>, gains: Vec<f64>, ious: Vec<Iou>| Cycle { kind, legs, gains, relief: 0.0, flourishing: 0.0, tab: ious };
    for (item, it) in w.items.iter().enumerate() {
        let from = it.owner;
        let given = w.value[from][item];
        if it.gift || given <= 0.0 {
            continue;
        }
        for to in (0..n).filter(|&k| k != from) {
            let got = w.value[to][item];
            if got <= 0.0 || w.npcs[to].business {
                continue;
            }
            // On the tab: `to` owes what it was worth to the giver.
            let gain = got - given - w.yuck_tax(to) - w.reach_tax(from, to);
            if gain > 0.0 && tab.room(to, from, given) {
                let iou = Iou { debtor: to, creditor: from, amount: given };
                out.push(tab_cycle(Kind::OnTab, vec![Leg { from, to, item }], vec![gain], vec![iou]));
            }
            // Paying back: `from` owes `to` and pays with this thing. Both
            // come out ahead: the one owed gets more than the debt it
            // settles, and the payer clears more than the thing is worth to them.
            let debt = tab.owed[from][to];
            let settled = debt.min(got);
            if debt > 0.0 && settled > given {
                let iou = Iou { debtor: from, creditor: to, amount: -settled };
                out.push(tab_cycle(Kind::PayBack, vec![Leg { from, to, item }], vec![got - given], vec![iou]));
            }
        }
    }
    if tab.rings {
        for start in 0..n {
            rings(tab, start, start, &mut vec![], &mut out);
        }
    }
    out.sort_by(|a, b| b.value().partial_cmp(&a.value()).unwrap());
    out
}

/// Rings of 3..=MAX_RING IOUs through `start` (its lowest-numbered person),
/// each cancelling the smallest debt on it.
fn rings(tab: &Tab, start: usize, at: usize, path: &mut Vec<usize>, out: &mut Vec<Cycle>) {
    path.push(at);
    for to in start..tab.owed.len() {
        if tab.owed[at][to] <= 0.0 {
            continue;
        }
        if to == start && path.len() >= 3 {
            let lines: Vec<(usize, usize)> = (0..path.len()).map(|k| (path[k], path.get(k + 1).copied().unwrap_or(start))).collect();
            let m = lines.iter().map(|&(a, b)| tab.owed[a][b]).fold(f64::INFINITY, f64::min);
            let ious = lines.into_iter().map(|(debtor, creditor)| Iou { debtor, creditor, amount: -m }).collect();
            out.push(Cycle { kind: Kind::Clear, legs: vec![], gains: vec![], relief: 0.0, flourishing: 0.0, tab: ious });
        } else if to != start && !path.contains(&to) && path.len() < MAX_RING {
            rings(tab, start, to, path, out);
        }
    }
    path.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world::laundromat;

    #[test]
    fn a_ring_of_ious_cancels_and_two_way_debts_net() {
        let mut tab = Tab::new(4, 20.0);
        // 0 owes 1 5, 1 owes 2 3, 2 owes 0 4: the ring clears 3 each.
        tab.owed[0][1] = 5.0;
        tab.owed[1][2] = 3.0;
        tab.owed[2][0] = 4.0;
        let mut out = vec![];
        for s in 0..4 {
            rings(&tab, s, s, &mut vec![], &mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cleared(), 9.0);
        tab.apply(&out[0]);
        assert_eq!((tab.owed[0][1], tab.owed[1][2], tab.owed[2][0]), (2.0, 0.0, 1.0));
        // Two-way: 3 owes 0 2 and 0 owes 3 5 -> 0 owes 3 3.
        tab.owed[3][0] = 2.0;
        tab.owed[0][3] = 5.0;
        tab.apply(&Cycle { kind: Kind::Clear, legs: vec![], gains: vec![], relief: 0.0, flourishing: 0.0, tab: vec![] });
        assert_eq!((tab.owed[3][0], tab.owed[0][3]), (0.0, 3.0));
    }

    #[test]
    fn the_tab_respects_its_limit_and_one_new_debt_a_night() {
        let mut w = laundromat(1);
        w.tab = Some(Tab::new(w.npcs.len(), 5.0));
        let cands = enumerate(&w);
        assert!(cands.iter().any(|c| c.kind == Kind::OnTab));
        for c in cands.iter().filter(|c| c.kind == Kind::OnTab) {
            assert!(c.tab[0].amount <= 5.0);
            assert!(c.goo() > 0.0);
        }
        // Two on one person's tab the same night conflict.
        let gravo = w.find_npc("gravo").unwrap();
        let his: Vec<&Cycle> = cands.iter().filter(|c| c.kind == Kind::OnTab && c.tab[0].debtor == gravo).collect();
        assert!(his.len() >= 2 && his[0].conflicts(his[1]));
    }
}
