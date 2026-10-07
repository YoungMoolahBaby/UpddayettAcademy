//! Yuck (DESIGN "Yuck: the one enemy"): tonight's source of it, and the
//! ghosts it leaves on the board.
//!
//! Yuck is a tax on trades ([`World::yuck`]): a yucky person needs a little
//! more before a trade is worth it to them, so some trades die. It comes
//! from a person or from a shared "pump" (hunger, the cold), and nobody is
//! ever labeled: the board shows the trades that *didn't* happen, as
//! ghosts. Where the ghosts cluster (around one person, or around everyone
//! with the same condition) is the map, as John Snow's was.
//!
//! The roll is separate from [`super::world::laundromat`], so every measured board
//! (trade_cli, PLAN) stays as it was; the game adds the yuck on top.

use super::cycles::{self, Cycle};
use super::world::World;
use super::{MAX_LOOP, TradeComputer, qubo};

/// Goo a yucky person takes off each trade (PLAN "Yuck, measured": at 2,
/// each yucky person costs the street ~5 Goo a night).
pub const TAX: f64 = 2.0;

/// Where tonight's yuck comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Clean,
    /// One or two people carry it.
    People(Vec<usize>),
    /// A shared source: `"hungry"` or `"cold"` ([`World::pump`]).
    Pump(&'static str),
}

impl Source {
    /// Tonight's yuck for `w` (from its night's seed; its own stream, so the
    /// night's weather, hunger and Salties are untouched). About a third
    /// of nights are clean, a third have one or two carriers, and a third
    /// a pump; a pump that would touch nobody tonight leaves it clean.
    pub fn roll(w: &World) -> Self {
        let mut s = w.night.seed.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0x5955_434B; // "YUCK"
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let people: Vec<usize> = (0..w.npcs.len()).filter(|&k| !w.npcs[k].business).collect();
        match next() % 3 {
            0 => Source::Clean,
            1 => {
                let a = people[(next() % people.len() as u64) as usize];
                let mut who = vec![a];
                if next() % 2 == 0 {
                    let b = people[(next() % people.len() as u64) as usize];
                    if b != a {
                        who.push(b);
                    }
                }
                who.sort_unstable();
                Source::People(who)
            }
            _ => {
                let pump = if w.night.cold && !w.pump("cold").is_empty() { "cold" } else { "hungry" };
                if w.pump(pump).is_empty() { Source::Clean } else { Source::Pump(pump) }
            }
        }
    }

    /// Who carries it tonight.
    pub fn carriers(&self, w: &World) -> Vec<usize> {
        match self {
            Source::Clean => vec![],
            Source::People(who) => who.clone(),
            Source::Pump(p) => w.pump(p),
        }
    }

    /// Put the tax on everyone who carries it.
    pub fn apply(&self, w: &mut World) {
        for k in self.carriers(w) {
            w.set_yuck(k, TAX);
        }
    }

    /// Parse `UPD_YUCK`: `clean`, `hungry`, `cold`, or names (`upd,vape`).
    pub fn parse(s: &str, w: &World) -> Option<Self> {
        match s {
            "clean" => Some(Source::Clean),
            "hungry" => Some(Source::Pump("hungry")),
            "cold" => Some(Source::Pump("cold")),
            names => {
                let who: Option<Vec<usize>> = names.split(',').map(|n| w.find_npc(n)).collect();
                who.filter(|w| !w.is_empty()).map(Source::People)
            }
        }
    }
}

/// What tonight's yuck did to the board.
#[derive(Clone, Debug, Default)]
pub struct Ghosts {
    /// Trades from the clean board's best set that the yuck killed: they
    /// aren't possible tonight at all. With what they would have paid
    /// (their clean gains).
    pub trades: Vec<Cycle>,
    /// Goo the yuck cost the street: the clean best set's Goo minus
    /// tonight's best set's Goo (killed trades, and the trimmed ones).
    pub cost: f64,
}

impl Ghosts {
    /// Compare tonight's board with the same night without the yuck.
    pub fn find(clean: &TradeComputer, tonight: &TradeComputer) -> Self {
        let possible = cycles::enumerate(&tonight.world, MAX_LOOP);
        let best = |tc: &TradeComputer| qubo::chosen(tc.ground_state(), tc.cycles.len());
        let trades: Vec<Cycle> = best(clean)
            .into_iter()
            .map(|i| &clean.cycles[i])
            .filter(|c| !c.is_gift() && !possible.iter().any(|p| p.legs == c.legs))
            .cloned()
            .collect();
        let cost = clean.tally(clean.ground_state()).goo - tonight.tally(tonight.ground_state()).goo;
        Self { trades, cost: cost.max(0.0) }
    }
}

/// A night's clean world with its yuck rolled and applied (what the
/// game opens). `override_` replaces the roll (`UPD_YUCK`).
pub fn tonight(mut w: World, override_: Option<&str>) -> (World, Source) {
    let source = override_.and_then(|s| Source::parse(s, &w)).unwrap_or_else(|| Source::roll(&w));
    source.apply(&mut w);
    (w, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world;

    #[test]
    fn about_a_third_each_and_never_a_business() {
        let (mut clean, mut people, mut pump) = (0, 0, 0);
        for night in 1..=300 {
            let w = world::laundromat(night);
            match Source::roll(&w) {
                Source::Clean => clean += 1,
                Source::People(who) => {
                    assert!(!who.is_empty() && who.len() <= 2);
                    assert!(who.iter().all(|&k| !w.npcs[k].business), "night {night}");
                    people += 1;
                }
                Source::Pump(p) => {
                    assert!(!w.pump(p).is_empty(), "night {night}: a pump touching nobody");
                    pump += 1;
                }
            }
            // The same night rolls the same yuck.
            assert_eq!(Source::roll(&w), Source::roll(&world::laundromat(night)));
        }
        for k in [clean, people, pump] {
            assert!((70..=130).contains(&k), "clean {clean}, people {people}, pump {pump}");
        }
    }

    #[test]
    fn ghosts_are_killed_best_set_trades_and_cost_what_they_cost() {
        let mut seen = 0;
        for night in 1..=40 {
            let clean_w = world::laundromat(night);
            let (yucky_w, source) = tonight(clean_w.clone(), None);
            let clean = TradeComputer::new(clean_w, 5.0, 1.6);
            let yucky = TradeComputer::new(yucky_w, 5.0, 1.6);
            let g = Ghosts::find(&clean, &yucky);
            if source == Source::Clean {
                assert!(g.trades.is_empty() && g.cost.abs() < 1e-9, "night {night}: a clean night with ghosts");
                continue;
            }
            // Every ghost touches a carrier: the yuck is what killed it.
            let carriers = source.carriers(&yucky.world);
            for t in &g.trades {
                assert!(t.legs.iter().any(|l| carriers.contains(&l.to)), "night {night}: a ghost no carrier touched");
            }
            // Killing trades from the best set costs at least nothing.
            assert!(g.cost >= 0.0);
            seen += g.trades.len();
        }
        assert!(seen > 0, "no ghosts in 40 nights");
    }
}
