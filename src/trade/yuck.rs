//! Yuck (DESIGN "Yuck: the one enemy"): tonight's source of it, the ghosts
//! it leaves on the board, the player's call (a person, or a pump?), the
//! cures, and how it spreads along trades to the next night.
//!
//! Yuck is a tax on trades ([`World::yuck`]): a yucky person needs a little
//! more before a trade is worth it to them, so some trades die. It comes
//! from a person or from a shared "pump" (hunger, the cold, the TV), and
//! nobody is ever labeled: the board shows the trades that *didn't* happen,
//! as ghosts. Where the ghosts cluster (around one person, or around
//! everyone with the same chip) is the map, as John Snow's was.
//!
//! The roll is separate from [`super::world::laundromat`], so every
//! measured board (trade_cli, PLAN) stays as it was; the game adds the
//! yuck on top.

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
    /// A shared source: `"hungry"`, `"cold"` or `"tv"` ([`World::pump`]).
    Pump(&'static str),
}

/// The player's call on the ghosts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Call {
    Person(usize),
    Pump(&'static str),
}

/// The pumps a player can name.
pub const PUMPS: [&str; 3] = ["hungry", "cold", "tv"];

/// Tonight's yuck and what's been done about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Yuck {
    pub source: Source,
    /// People who caught it last night by trading with a carrier.
    pub spread: Vec<usize>,
    /// People treated tonight (and Upddayett, if he gave something away).
    pub cured: Vec<usize>,
    /// The pump is fixed tonight (fed, warmed, or the TV off).
    pub pump_fixed: bool,
    /// The TV is on. Off, it pumps nothing (and shows nothing).
    pub tv_on: bool,
}

impl Yuck {
    pub fn clean() -> Self {
        Self { source: Source::Clean, spread: vec![], cured: vec![], pump_fixed: false, tv_on: true }
    }

    /// Tonight's yuck for `w` (from its night's seed; its own stream, so the
    /// night's weather, hunger and Salties are untouched). About a third
    /// of nights are clean, a third have one or two carriers, and a third
    /// a pump: the TV one time in three, else the cold on a cold night,
    /// else hunger. A pump that would touch nobody leaves the night clean.
    pub fn roll(w: &World) -> Self {
        let mut s = w.night.seed.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0x5955_434B; // "YUCK"
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let people: Vec<usize> = (0..w.npcs.len()).filter(|&k| !w.npcs[k].business).collect();
        let source = match next() % 3 {
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
                let pump = match next() % 3 {
                    0 => "tv",
                    _ if w.night.cold && !w.pump("cold").is_empty() => "cold",
                    _ => "hungry",
                };
                if w.pump(pump).is_empty() { Source::Clean } else { Source::Pump(pump) }
            }
        };
        Self { source, ..Self::clean() }
    }

    /// Parse `UPD_YUCK`: `clean`, `hungry`, `cold`, `tv`, or names (`upd,vape`).
    pub fn parse(s: &str, w: &World) -> Option<Self> {
        let source = match s {
            "clean" => Source::Clean,
            "hungry" => Source::Pump("hungry"),
            "cold" => Source::Pump("cold"),
            "tv" => Source::Pump("tv"),
            names => {
                let who: Option<Vec<usize>> = names.split(',').map(|n| w.find_npc(n)).collect();
                Source::People(who.filter(|w| !w.is_empty())?)
            }
        };
        Some(Self { source, ..Self::clean() })
    }

    /// Whether the source is a pump that's still running tonight.
    fn pumping(&self) -> Option<&'static str> {
        match self.source {
            Source::Pump(p) if !self.pump_fixed && (p != "tv" || self.tv_on) => Some(p),
            _ => None,
        }
    }

    /// Who carries it tonight, before any cure: the source's carriers and
    /// whoever caught it last night.
    pub fn exposed(&self, w: &World) -> Vec<usize> {
        let mut who = match &self.source {
            Source::Clean => vec![],
            Source::People(p) => p.clone(),
            Source::Pump(p) => w.pump(p),
        };
        who.extend(&self.spread);
        who.sort_unstable();
        who.dedup();
        who
    }

    /// Who carries it now: the exposed, minus a fixed pump's people (unless
    /// they also caught it some other way) and minus the cured.
    pub fn carriers(&self, w: &World) -> Vec<usize> {
        let mut who = match (&self.source, self.pumping()) {
            (Source::People(p), _) => p.clone(),
            (Source::Pump(_), Some(p)) => w.pump(p),
            _ => vec![],
        };
        who.extend(&self.spread);
        who.sort_unstable();
        who.dedup();
        who.retain(|k| !self.cured.contains(k));
        who
    }

    /// Put the tax on everyone who carries it.
    pub fn apply(&self, w: &mut World) {
        for k in self.carriers(w) {
            w.set_yuck(k, TAX);
        }
    }

    /// Is the call right? A person is right if they carry it as a person
    /// (from the source, or caught last night). A pump is right only if it
    /// *is* the pump: blaming someone who is just hungry with everyone else
    /// is the classic mistake (the Cohen-Cole and Fletcher rebuttal).
    pub fn check(&self, call: Call) -> bool {
        match (call, &self.source) {
            (Call::Pump(p), Source::Pump(q)) => p == *q,
            (Call::Pump(_), _) => false,
            (Call::Person(k), Source::People(who)) => who.contains(&k) || self.spread.contains(&k),
            (Call::Person(k), _) => self.spread.contains(&k),
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

/// After a night's trades, who catches it for tomorrow: everyone who traded
/// with a carrier, one in two (a coin per person from the night's seed).
/// Gifts don't spread it: kindness gets through.
pub fn spread(carriers: &[usize], trades: &[&Cycle], seed: u64) -> Vec<usize> {
    let coin = |k: usize| {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (k as u64 + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        s ^= s >> 31;
        s = s.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        s ^= s >> 29;
        s & 1 == 0
    };
    let mut out = vec![];
    for t in trades.iter().filter(|t| !t.is_gift()) {
        let people: Vec<usize> = t.legs.iter().map(|l| l.to).collect();
        if people.iter().any(|k| carriers.contains(k)) {
            out.extend(people.into_iter().filter(|k| !carriers.contains(k) && coin(*k)));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world;

    #[test]
    fn about_a_third_each_and_never_a_business() {
        let (mut clean, mut people, mut pump, mut tv) = (0, 0, 0, 0);
        for night in 1..=300 {
            let w = world::laundromat(night);
            let y = Yuck::roll(&w);
            match &y.source {
                Source::Clean => clean += 1,
                Source::People(who) => {
                    assert!(!who.is_empty() && who.len() <= 2);
                    assert!(who.iter().all(|&k| !w.npcs[k].business), "night {night}");
                    people += 1;
                }
                Source::Pump(p) => {
                    assert!(!w.pump(p).is_empty(), "night {night}: a pump touching nobody");
                    pump += 1;
                    tv += (*p == "tv") as usize;
                }
            }
            // The same night rolls the same yuck.
            assert_eq!(y, Yuck::roll(&world::laundromat(night)));
        }
        for k in [clean, people, pump] {
            assert!((70..=130).contains(&k), "clean {clean}, people {people}, pump {pump}");
        }
        assert!((20..=50).contains(&tv), "tv {tv} of {pump} pumps");
    }

    #[test]
    fn ghosts_are_killed_best_set_trades() {
        let mut seen = 0;
        for night in 1..=40 {
            let clean_w = world::laundromat(night);
            let y = Yuck::roll(&clean_w);
            let mut yucky_w = clean_w.clone();
            y.apply(&mut yucky_w);
            let clean = TradeComputer::new(clean_w, 5.0, 1.6);
            let yucky = TradeComputer::new(yucky_w, 5.0, 1.6);
            let g = Ghosts::find(&clean, &yucky);
            if y.source == Source::Clean {
                assert!(g.trades.is_empty() && g.cost.abs() < 1e-9, "night {night}: a clean night with ghosts");
                continue;
            }
            // Every ghost touches a carrier: the yuck is what killed it.
            let carriers = y.carriers(&yucky.world);
            for t in &g.trades {
                assert!(t.legs.iter().any(|l| carriers.contains(&l.to)), "night {night}: a ghost no carrier touched");
            }
            seen += g.trades.len();
        }
        assert!(seen > 0, "no ghosts in 40 nights");
    }

    /// Person or pump: only the real source is right, and blaming one of a
    /// pump's people is wrong. Curing and fixing take people off.
    #[test]
    fn the_call_and_the_cures() {
        let w = world::laundromat(1);
        let (upd, vape, cart) = (w.find_npc("Upddayett").unwrap(), w.find_npc("Vape").unwrap(), w.find_npc("Cart").unwrap());
        let mut people = Yuck { source: Source::People(vec![upd, vape]), ..Yuck::clean() };
        assert!(people.check(Call::Person(vape)));
        assert!(!people.check(Call::Person(cart)));
        assert!(!people.check(Call::Pump("hungry")));
        people.cured.push(vape);
        assert_eq!(people.carriers(&w), vec![upd]);

        let mut pump = Yuck { source: Source::Pump("hungry"), ..Yuck::clean() };
        let hungry = w.pump("hungry");
        assert!(hungry.len() >= 2, "night 1 has hungry people");
        assert!(pump.check(Call::Pump("hungry")));
        assert!(!pump.check(Call::Pump("cold")));
        assert!(!pump.check(Call::Person(hungry[0])), "blaming a hungry person for the hunger");
        assert_eq!(pump.carriers(&w), hungry);
        pump.pump_fixed = true;
        assert!(pump.carriers(&w).is_empty());
        // Someone who caught it last night stays yucky when the pump is fixed.
        pump.spread.push(hungry[0]);
        assert_eq!(pump.carriers(&w), vec![hungry[0]]);
        assert!(pump.check(Call::Person(hungry[0])));

        // The TV pumps only while it's on.
        let mut tv = Yuck { source: Source::Pump("tv"), ..Yuck::clean() };
        assert_eq!(tv.carriers(&w).len(), 3);
        tv.tv_on = false;
        assert!(tv.carriers(&w).is_empty());
    }

    #[test]
    fn it_spreads_along_trades_not_gifts() {
        let w = world::laundromat(1);
        let tc = TradeComputer::new(w, 5.0, 1.6);
        let trades: Vec<&Cycle> = tc.cycles.iter().filter(|c| !c.is_gift()).collect();
        let gifts: Vec<&Cycle> = tc.cycles.iter().filter(|c| c.is_gift()).collect();
        let carrier = trades[0].legs[0].to;
        let mut partners: Vec<usize> =
            trades.iter().filter(|t| t.legs.iter().any(|l| l.to == carrier)).flat_map(|t| t.legs.iter().map(|l| l.to)).filter(|&k| k != carrier).collect();
        partners.sort_unstable();
        partners.dedup();
        // Over many seeds about half the partners catch it, and only partners.
        let mut caught = 0;
        for seed in 0..200 {
            let s = spread(&[carrier], &trades, seed);
            assert!(s.iter().all(|k| partners.contains(k)));
            caught += s.len();
            assert!(spread(&[carrier], &gifts, seed).is_empty(), "a gift spread it");
        }
        let rate = caught as f64 / (200 * partners.len()) as f64;
        assert!((0.35..0.65).contains(&rate), "caught {rate:.2}");
    }
}
