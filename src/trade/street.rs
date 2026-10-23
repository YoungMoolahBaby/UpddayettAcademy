//! Nights that carry over (DESIGN "Nights that carry over"): tonight's
//! outcome is tomorrow's start. What changed hands stays changed: the new
//! owner keeps it or uses it up, and the old owner goes without until they
//! find another ([`Item::back_in`](super::world::Item::back_in)). Whoever
//! ate isn't hungry tomorrow, fed pigeons aren't either, and a phone you
//! were given is your phone. Goo and Karma add up night after night.
//!
//! The weather and the hunger coins still roll from the night's seed, so a
//! street where nothing ever changes hands sees the same nights as before.

use super::cycles::{self, Cycle, Kind};
use super::qubo;
use super::tab::Tab;
use super::world::{self, Use, World};

/// Something a person was given and kept.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub item: &'static str,
    /// What they wanted it for.
    pub use_: Use,
    /// What it was worth to them the night they got it, in Goo.
    pub value: f64,
    /// Nights they've had it.
    pub age: u32,
}

/// What the street remembers from one night to the next.
#[derive(Clone, Debug)]
pub struct Street {
    /// Tonight's night number (its seed).
    pub night: u64,
    /// Nights each of the regulars' things stays away (0 = here tonight).
    pub away: Vec<u32>,
    /// Who ate last night, so isn't hungry tonight.
    pub fed: Vec<bool>,
    /// Whose animals ate last night.
    pub animals_fed: Vec<bool>,
    /// What each person was given and kept.
    pub kept: Vec<Vec<Kept>>,
    /// Goo each person has gained, all nights so far.
    pub goo: Vec<f64>,
    /// Karma each person's gifts brought them, all nights so far.
    pub karma: Vec<f64>,
    /// Nights a kept thing lasts before it wears out or gets lost, and its
    /// keeper wants another (`None`: forever).
    pub wear: Option<u32>,
    /// How slowly things come back: each item's `back_in` times this (1 = as listed).
    pub restock: f64,
    /// The counter's tab ([`super::tab`]), if it's open.
    pub tab: Option<Tab>,
    /// Goo of debt the tab's rings have cancelled, all nights so far.
    pub cleared: f64,
}

impl Street {
    /// The street on night `first`, before anything has changed hands.
    pub fn new(first: u64) -> Self {
        let w = world::laundromat(first);
        let n = w.npcs.len();
        Self {
            night: first,
            away: vec![0; w.items.len()],
            fed: vec![false; n],
            animals_fed: vec![false; n],
            kept: vec![vec![]; n],
            goo: vec![0.0; n],
            karma: vec![0.0; n],
            wear: None,
            restock: 1.0,
            tab: None,
            cleared: 0.0,
        }
    }

    /// Tonight's laundromat: the night's roll, with what last night left.
    pub fn world(&self) -> World {
        let mut w = world::laundromat(self.night);
        self.carry_into(&mut w);
        w
    }

    /// Bring what the street remembers into `w`, a freshly rolled night of
    /// the same cast.
    pub fn carry_into(&self, w: &mut World) {
        let mut night = w.night.clone();
        for k in 0..w.npcs.len() {
            night.hungry[k] &= !self.fed[k];
            night.animals_hungry[k] &= !self.animals_fed[k];
            if self.kept[k].iter().any(|kept| kept.use_ == Use::Lifeline) {
                w.npcs[k].has_phone = true;
            }
        }
        // Nobody wants a second one of what they kept.
        let (kept, items) = (&self.kept, &w.items);
        w.wants.retain(|x| !kept[x.npc].iter().any(|k| k.item == items[x.item].name));
        for (item, &nights) in self.away.iter().enumerate() {
            w.items[item].away = nights > 0;
        }
        w.tab = self.tab.clone();
        w.set_night(night);
    }

    /// Tonight ends with the set `bits` of `cycles` (on `w`, tonight's
    /// world): move what changed hands, add up Goo and Karma, and roll on to
    /// tomorrow.
    pub fn settle(&mut self, w: &World, cycles: &[Cycle], bits: u32) {
        for nights in &mut self.away {
            *nights = nights.saturating_sub(1);
        }
        self.fed.fill(false);
        self.animals_fed.fill(false);
        let wear = self.wear;
        for kept in &mut self.kept {
            kept.iter_mut().for_each(|x| x.age += 1);
            kept.retain(|x| wear.is_none_or(|w| x.age < w));
        }
        for i in qubo::chosen(bits, cycles.len()) {
            let c = &cycles[i];
            for (leg, &gain) in c.legs.iter().zip(&c.gains) {
                let it = &w.items[leg.item];
                match (c.kind, c.tab.first()) {
                    // Paying back: the one owed gets what the thing is worth
                    // to them less the debt it settles; the payer gets the
                    // debt less what the thing was worth to them.
                    (Kind::PayBack, Some(x)) => {
                        let settled = -x.amount;
                        self.goo[leg.to] += w.value[leg.to][leg.item] - settled;
                        self.goo[leg.from] += settled - w.value[leg.from][leg.item];
                    }
                    _ => self.goo[leg.to] += gain,
                }
                if c.is_gift() {
                    self.karma[leg.to] += cycles::karma_weight(w, leg.to, leg.item).1 * gain;
                }
                let use_ = w.wants.iter().find(|x| x.npc == leg.to && x.item == leg.item).map_or(it.owner_use, |x| x.use_);
                match use_ {
                    Use::Eat => self.fed[leg.to] = true,
                    Use::FeedAnimals => self.animals_fed[leg.to] = true,
                    _ => {}
                }
                if it.keeps {
                    self.kept[leg.to].push(Kept { item: it.name, use_, value: w.value[leg.to][leg.item], age: 0 });
                }
                if let (Some(nights), Some(back_in)) = (self.away.get_mut(leg.item), it.back_in) {
                    *nights = (back_in as f64 * self.restock).round() as u32;
                }
            }
            if let Some(tab) = &mut self.tab {
                tab.apply(c);
            }
            self.cleared += c.cleared();
        }
        self.night += 1;
    }

    /// Each person's net worth tonight, in Goo: what they hold on `w` (see
    /// [`super::spring::net_worth`]) plus what they were given and kept.
    pub fn worth(&self, w: &World) -> Vec<f64> {
        let mut nw = super::spring::net_worth(w);
        for (k, kept) in self.kept.iter().enumerate() {
            nw[k] += kept.iter().map(|x| x.value).sum::<f64>();
        }
        nw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::TradeComputer;

    fn board(w: World) -> TradeComputer {
        TradeComputer::new(w, 5.0, 1.6)
    }

    #[test]
    fn a_street_where_nothing_changes_hands_sees_the_same_nights() {
        let mut s = Street::new(1);
        for night in 1..15 {
            let w = s.world();
            let fresh = world::laundromat(night);
            assert_eq!(w.night, fresh.night, "night {night}");
            assert_eq!(w.value, fresh.value, "night {night}");
            s.settle(&w, &[], 0);
        }
    }

    #[test]
    fn what_changed_hands_stays_changed() {
        let mut s = Street::new(1);
        for _ in 1..30 {
            let w = s.world();
            let tc = board(w);
            let best = tc.ground_state();
            let moved: Vec<_> = qubo::chosen(best, tc.cycles.len()).iter().flat_map(|&i| tc.cycles[i].legs.clone()).collect();
            s.settle(&tc.world, &tc.cycles, best);
            let next = s.world();
            for l in &moved {
                let it = &tc.world.items[l.item];
                let gone = it.back_in.unwrap() > 0;
                assert_eq!(next.items[l.item].away, gone, "{}", it.name);
                if gone {
                    assert!(next.value.iter().all(|v| v[l.item] == 0.0), "{} is away but worth something", it.name);
                }
                if tc.world.wants.iter().any(|x| x.npc == l.to && x.item == l.item && x.use_ == Use::Eat) {
                    assert!(!next.night.hungry[l.to], "{} ate but is hungry", tc.world.npcs[l.to].name);
                }
                if it.keeps {
                    assert!(next.wants.iter().all(|x| !(x.npc == l.to && x.item == l.item)), "{} wants another", it.name);
                }
            }
        }
    }

    #[test]
    fn goo_and_karma_add_up_to_the_nights_tallies() {
        let mut s = Street::new(3);
        let (mut goo, mut karma) = (0.0, 0.0);
        for _ in 0..20 {
            let tc = board(s.world());
            let best = tc.ground_state();
            let t = tc.tally(best);
            goo += t.goo;
            karma += t.karma;
            s.settle(&tc.world, &tc.cycles, best);
        }
        assert!((s.goo.iter().sum::<f64>() - goo).abs() < 1e-9);
        assert!((s.karma.iter().sum::<f64>() - karma).abs() < 1e-9);
    }

    /// Everyone on the same footing (user, 2026-10-23): on paper each
    /// person's things draw about what they want, everyone has something back
    /// within two nights, and Amir's food can reach anyone; over 60 nights
    /// with the tab, every person both owes and is owed, often.
    #[test]
    fn everyone_is_on_the_same_footing() {
        let w = world::laundromat(1);
        let people: Vec<usize> = (0..w.npcs.len()).filter(|&k| !w.npcs[k].business).collect();
        for (k, f) in w.footing().into_iter().enumerate().filter(|(k, _)| people.contains(k)) {
            let name = w.npcs[k].name;
            assert!((0.9..=1.1).contains(&(f.drawn / f.wants)), "{name}: draws {} for wants {}", f.drawn, f.wants);
            assert!(f.soonest.is_some_and(|n| n <= 2), "{name}: nothing back soon");
            assert!(f.fed, "{name}: Amir's food can't reach them");
        }
        let mut s = Street::new(1);
        s.tab = Some(Tab::new(w.npcs.len(), 10.0));
        let (mut owing, mut owed) = (vec![0; w.npcs.len()], vec![0; w.npcs.len()]);
        for _ in 0..60 {
            let tc = board(s.world());
            let best = tc.ground_state();
            s.settle(&tc.world, &tc.cycles, best);
            let tab = s.tab.as_ref().unwrap();
            for &k in &people {
                owing[k] += usize::from(tab.owed[k].iter().sum::<f64>() > 0.0);
                owed[k] += usize::from(tab.owed_to(k) > 0.0);
            }
        }
        for &k in &people {
            assert!(owing[k] >= 20 && owed[k] >= 20, "{}: owing {} nights, owed {} of 60", w.npcs[k].name, owing[k], owed[k]);
        }
    }

    #[test]
    fn the_tab_stays_under_its_limit_both_ways_and_goo_still_adds_up() {
        let mut s = Street::new(1);
        let n = s.goo.len();
        s.tab = Some(Tab::new(n, 10.0));
        let (mut goo, mut used) = (0.0, 0);
        for _ in 0..40 {
            let tc = board(s.world());
            let best = tc.ground_state();
            goo += tc.tally(best).goo;
            used += qubo::chosen(best, tc.cycles.len()).iter().filter(|&&i| !tc.cycles[i].tab.is_empty()).count();
            s.settle(&tc.world, &tc.cycles, best);
            let tab = s.tab.as_ref().unwrap();
            // The limit runs both ways: nobody owes, or is owed, past it.
            assert!((0..n).all(|k| tab.owes(k) <= 10.0 + 1e-9 && tab.credit(k) <= 10.0 + 1e-9));
        }
        assert!(used > 0, "the tab never got used");
        assert!((s.goo.iter().sum::<f64>() - goo).abs() < 1e-9);
    }
}
