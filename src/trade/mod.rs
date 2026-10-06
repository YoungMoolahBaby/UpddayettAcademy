//! Lesson 3, "Money Laundering (Legally)": a laundromat trade computer.
//!
//! Customers bring haves and wants ([`world`]). Every closed giving loop in
//! which everybody comes out ahead is a candidate trade ([`cycles`]). Picking
//! the best set of trades that never moves an item twice is a QUBO
//! ([`qubo`]), which becomes the springs and biases of a board of bistable
//! strips that CortenForge's thermostat shakes and cools ([`machine`]).

pub mod cycles;
pub mod machine;
pub mod qubo;
pub mod salties;
pub mod world;

pub use cycles::{Cycle, Kind, Leg};
pub use machine::{Anneal, Error, Machine, Physics};
pub use qubo::{Ising, Qubo, TradeProblem};
pub use salties::{Magnet, Sabotage};
pub use world::World;

use cortenforge::sim::thermostat::ising::exact_distribution;

/// The longest loop the machine considers.
pub const MAX_LOOP: usize = 4;

/// The crate's exact solver enumerates all states and stops at 20 bits.
pub const MAX_EXACT_BITS: usize = 20;

/// Least extra want bias beyond the bare minimum, as a fraction of the
/// smallest cycle value: the energy gap the anneal gets to find the wanted
/// chain.
pub const WANT_MARGIN: f64 = 1.0;

/// How hard a want clamps its strip: the idle field on the wanted strip, as
/// a multiple of the field that flattens a well. Above 1 the strip is pinned
/// (a thumb clamp). Measured on the costly wants of nights 1-7: delivery
/// rises with the pull and levels off around 1.3-1.9x (margins 4-8), so the
/// want pulls at 1.5x unless the minimum needs more.
pub const WANT_CLAMP: f64 = 1.5;

/// What the i9 saw during one spin cycle.
#[derive(Clone, Copy, Debug)]
pub struct Latch {
    /// Where the strips came to rest.
    pub final_bits: u32,
    /// Lowest-energy configuration read while every strip sat in a well.
    pub best_bits: u32,
    pub best_energy: f64,
    pub best_time: f64,
}

impl Default for Latch {
    fn default() -> Self {
        Self::new()
    }
}

impl Latch {
    /// Nothing latched yet.
    pub fn new() -> Self {
        Self { final_bits: 0, best_bits: 0, best_energy: f64::INFINITY, best_time: 0.0 }
    }

    /// Whether anything has been latched.
    pub fn has_best(&self) -> bool {
        self.best_energy.is_finite()
    }

    /// One Hall-sensor read: latch the strips if they all sit in a well and
    /// beat the best so far.
    pub fn observe(&mut self, m: &Machine, q: &Qubo) {
        if !m.all_in_wells() {
            return;
        }
        let bits = m.bits();
        let e = q.energy(bits);
        if e < self.best_energy - 1e-12 {
            self.best_bits = bits;
            self.best_energy = e;
            self.best_time = m.time();
        }
    }
}

/// Everything needed to run the machine on one night's laundry.
#[derive(Clone)]
pub struct TradeComputer {
    pub world: World,
    pub cycles: Vec<Cycle>,
    pub problem: TradeProblem,
    pub ising: Ising,
    /// QUBO units -> kT: how hard the springs pull compared to the shaking.
    pub beta: f64,
    /// Conflict penalty as a multiple of the largest normalized cycle value.
    pub penalty: f64,
    /// Least want bias beyond the bare minimum, as a fraction of the smallest
    /// cycle value (default [`WANT_MARGIN`]). Takes effect on the next `want`.
    pub want_margin: f64,
    /// Idle field on the wanted strip, as a multiple of the well-flattening
    /// field (default [`WANT_CLAMP`]); 0 turns the clamp off and leaves just
    /// the margin. Takes effect on the next `want`.
    pub want_clamp: f64,
    /// Barrier height the clamp is sized for (the machine's `delta_v`).
    pub delta_v: f64,
    /// Backward mode target, if any.
    pub want: Option<(usize, usize)>,
    /// Best trade set with no want (forward mode), for pricing a want.
    pub forward_best: u32,
    /// Candidate trades left out because the board holds [`MAX_BITS`].
    pub dropped: usize,
    /// `masks[i]`: the trades that conflict with trade i.
    masks: Vec<u32>,
    /// Best trade set for the current problem (want included), cached.
    ground: u32,
    bias: Option<Vec<f64>>,
}

/// Most strips on the board. The crate's exact solver also stops here.
pub const MAX_BITS: usize = MAX_EXACT_BITS;

/// Strips kept free for gift chains even on a night full of trades.
pub const MIN_GIFT_SLOTS: usize = 4;

/// Up to `slots` gift chains: first the best chain for each gift (so every
/// donor gets a strip), then the best for each other first recipient (so the
/// drum really chooses *who* gets the gift, which is the routing), then the
/// rest by Karma. `gifts` comes sorted by Karma.
fn pick_gifts(gifts: Vec<Cycle>, slots: usize) -> Vec<Cycle> {
    let (mut items, mut firsts) = (vec![], vec![]);
    let mut ranked: Vec<(u8, Cycle)> = gifts
        .into_iter()
        .map(|g| {
            let first = (g.legs[0].item, g.legs[0].to);
            let rank = if !items.contains(&first.0) {
                0
            } else if !firsts.contains(&first) {
                1
            } else {
                2
            };
            items.push(first.0);
            firsts.push(first);
            (rank, g)
        })
        .collect();
    // Stable: each rank stays in Karma order.
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().take(slots).map(|(_, g)| g).collect()
}

/// What a set of trades and gifts does, for display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tally {
    /// Goo everyone gains (trades and gift recipients alike).
    pub goo: f64,
    /// Karma from gifts: relief + flourishing.
    pub karma: f64,
    pub relief: f64,
    pub flourishing: f64,
}

impl TradeComputer {
    pub fn new(world: World, beta: f64, penalty: f64) -> Self {
        Self::with_board(world, beta, penalty, MAX_BITS, cycles::MAX_CHAIN)
    }

    /// Like [`TradeComputer::new`], with at most `max_gifts` gift chains of
    /// at most `max_chain` recipients.
    pub fn with_board(world: World, beta: f64, penalty: f64, max_gifts: usize, max_chain: usize) -> Self {
        // Trades take the board first (best first), keeping at least
        // MIN_GIFT_SLOTS strips for gifts; gifts fill the rest.
        let all_gifts = cycles::enumerate_gifts(&world, max_chain);
        let mut trades = cycles::enumerate(&world, MAX_LOOP);
        let total = all_gifts.len() + trades.len();
        trades.truncate(MAX_BITS - MIN_GIFT_SLOTS.min(all_gifts.len()).min(max_gifts));
        let mut gifts = pick_gifts(all_gifts, (MAX_BITS - trades.len()).min(max_gifts));
        let dropped = total - gifts.len() - trades.len();
        let mut cycles = trades;
        cycles.append(&mut gifts);
        cycles.sort_by(|a, b| b.value().partial_cmp(&a.value()).unwrap());
        let masks = qubo::conflict_masks(&cycles);
        let values: Vec<f64> = cycles.iter().map(Cycle::value).collect();
        let forward_best = qubo::best_valid_set(&values, &masks).0;
        let problem = qubo::build(&cycles, None, penalty * qubo::MAX_VALUE);
        let ising = problem.qubo.to_ising(beta);
        Self {
            world,
            cycles,
            problem,
            ising,
            beta,
            penalty,
            want_margin: WANT_MARGIN,
            want_clamp: WANT_CLAMP,
            delta_v: Physics::default().delta_v,
            want: None,
            forward_best,
            dropped,
            masks,
            ground: forward_best,
            bias: None,
        }
    }

    /// The want bias: at least the smallest bias whose best set delivers the
    /// want plus a margin, and otherwise enough to clamp the wanted strips
    /// at `want_clamp` times the flattening field.
    ///
    /// The minimum is exact and cheap: every valid set delivering the want
    /// contains exactly one delivering trade (they all move the same item),
    /// so the bias lifts each of those sets by the same amount, and the
    /// minimum is (best set without the want) - (best set with it), or 0.
    ///
    /// The minimum alone (plus 1x margin) delivered costly wants only
    /// 58-85% of the time: a 6-Goo want leaves a gap of one small trade
    /// between the wanted chain and the free best set, many strip flips
    /// away. Pinning the wanted strip removes the choice (the strips around
    /// it settle the rest), so costly wants deliver ~100%. The idle field
    /// on strip i (every other strip off) is beta * (value_i + bias) *
    /// scale / 2 in QUBO units, so the clamp bias is closed-form.
    fn want_bias(&self, npc: usize, item: usize) -> Vec<f64> {
        let hits: Vec<bool> = self.cycles.iter().map(|c| c.delivers(npc, item)).collect();
        let (mut with, mut without) = (f64::NEG_INFINITY, 0.0f64);
        qubo::for_each_valid_set(&self.masks, |s| {
            let on = qubo::chosen(s, self.cycles.len());
            let v: f64 = on.iter().map(|&i| self.cycles[i].value()).sum();
            if on.iter().any(|&i| hits[i]) {
                with = with.max(v);
            } else {
                without = without.max(v);
            }
        });
        let need = (without - with).max(0.0);
        let values = || self.cycles.iter().map(Cycle::value);
        let vmin = values().fold(f64::INFINITY, f64::min);
        let floor = need + self.want_margin * vmin;
        // Clamp sized for the weakest delivering strip, so every one is pinned.
        let scale = qubo::MAX_VALUE / values().fold(0.0, f64::max);
        let weakest = values().zip(&hits).filter(|(_, h)| **h).map(|(v, _)| v).fold(f64::INFINITY, f64::min);
        let target = self.want_clamp * machine::max_safe_field(self.delta_v);
        let clamp = 2.0 * target / (self.beta * scale) - weakest;
        let b = floor.max(clamp);
        hits.iter().map(|&h| if h { b } else { 0.0 }).collect()
    }

    /// Forward mode again: no want.
    pub fn clear_want(&mut self) {
        self.bias = None;
        self.want = None;
        self.rebuild();
    }

    /// Items some candidate trade delivers to `npc`, in item order.
    pub fn deliverable(&self, npc: usize) -> Vec<usize> {
        (0..self.world.items.len()).filter(|&item| self.cycles.iter().any(|c| c.delivers(npc, item))).collect()
    }

    /// What granting the want cost everyone, in world value units: the
    /// forward-mode best minus the value of `bits`.
    pub fn want_cost(&self, bits: u32) -> f64 {
        self.evaluate(self.forward_best).0 - self.evaluate(bits).0
    }

    fn rebuild(&mut self) {
        self.problem = qubo::build(&self.cycles, self.bias.as_deref(), self.penalty * qubo::MAX_VALUE);
        self.ising = self.problem.qubo.to_ising(self.beta);
        let weights: Vec<f64> = match &self.bias {
            Some(b) => self.cycles.iter().zip(b).map(|(c, b)| c.value() + b).collect(),
            None => self.cycles.iter().map(Cycle::value).collect(),
        };
        self.ground = qubo::best_valid_set(&weights, &self.masks).0;
    }

    /// Backward mode: make `npc` end up with `item`. Returns `false` (and
    /// changes nothing) if no candidate trade delivers it.
    pub fn want(&mut self, npc: usize, item: usize) -> bool {
        if !self.cycles.iter().any(|c| c.delivers(npc, item)) {
            return false;
        }
        self.bias = Some(self.want_bias(npc, item));
        self.want = Some((npc, item));
        self.rebuild();
        true
    }

    pub fn machine(&self, physics: Physics, seed: u64) -> Result<Machine, Error> {
        Machine::new(&self.ising, physics, seed)
    }

    /// The board with a magnet under the counter. The i9 doesn't know about
    /// it: it still latches and scores by the clean night's QUBO, so
    /// whatever the magnet costs shows up as a loss.
    pub fn tampered_machine(&self, physics: Physics, seed: u64, magnet: &Magnet) -> Result<Machine, Error> {
        Machine::with_stray_field(&self.ising, physics, seed, &magnet.field(self.ising.n, physics.delta_v))
    }

    /// Runs a spin cycle while the i9 watches: every `sample` time units it
    /// reads the Hall sensors and, if every strip is in a well, latches the
    /// configuration if it beats the best so far. `tick` fires every `every`
    /// time units for display.
    pub fn spin(
        &self,
        m: &mut Machine,
        anneal: &Anneal,
        sample: f64,
        every: f64,
        mut tick: impl FnMut(&Machine, &Latch),
    ) -> Result<Latch, Error> {
        let dt = m.physics.dt;
        let steps = (anneal.total_time() / dt).round() as usize;
        let sample_steps = ((sample / dt).round() as usize).max(1);
        let tick_steps = if every.is_finite() { ((every / dt).round() as usize).max(1) } else { usize::MAX };
        let t0 = m.time();
        let mut latch = Latch::new();
        for k in 1..=steps {
            m.set_temperature(anneal.temperature(m.time() - t0));
            m.step()?;
            if k % sample_steps == 0 || k == steps {
                latch.observe(m, &self.problem.qubo);
            }
            if k % tick_steps == 0 {
                tick(m, &latch);
            }
        }
        latch.final_bits = m.bits();
        Ok(latch)
    }

    /// Best trade set for the current problem (exact, cached on rebuild).
    pub fn ground_state(&self) -> u32 {
        self.ground
    }

    /// Brute-force ground state of the QUBO itself (slow at 20 bits; for
    /// cross-checks).
    pub fn qubo_ground_state(&self) -> u32 {
        self.problem.qubo.brute_force().0
    }

    /// Ground state according to the crate's exact Boltzmann solver, with
    /// its probability at temperature `k_b_t`. `None` above 20 bits, or when
    /// the crate's solver overflows: it exponentiates -E/kT without a
    /// max-energy shift, so big boards at low temperature give NaN.
    pub fn exact_ground_state(&self, k_b_t: f64) -> Option<(u32, f64)> {
        if self.ising.n > MAX_EXACT_BITS {
            return None;
        }
        let i = &self.ising;
        let dist = exact_distribution(i.n, &i.edges, &i.j, &i.h, k_b_t);
        if dist.iter().any(|(_, p)| !p.is_finite()) {
            return None;
        }
        dist.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The drum's score for a set (Goo from trades + Karma from gifts), and
    /// whether any two of them try to move the same item.
    pub fn evaluate(&self, bits: u32) -> (f64, bool) {
        let on = qubo::chosen(bits, self.cycles.len());
        let value = on.iter().map(|&i| self.cycles[i].value()).sum();
        let clash = on.iter().enumerate().any(|(a, &i)| on[a + 1..].iter().any(|&k| self.cycles[i].conflicts(&self.cycles[k])));
        (value, clash)
    }

    /// "43 Goo + 8 Karma" (Karma left out when there is none).
    pub fn score_text(&self, bits: u32) -> String {
        let t = self.tally(bits);
        if t.karma > 0.0 { format!("{:.0} Goo + {:.1} Karma", t.goo, t.karma) } else { format!("{:.0} Goo", t.goo) }
    }

    /// Who the chosen gifts reached, what it did for them, and the Karma
    /// each hand-off earned:
    /// [("Shopping-Cart Guy (hungry) gets the tray of adas polo", 8.0), ...].
    pub fn gift_lines(&self, bits: u32) -> Vec<(String, f64)> {
        let w = &self.world;
        qubo::chosen(bits, self.cycles.len())
            .into_iter()
            .map(|i| &self.cycles[i])
            .filter(|c| c.is_gift())
            .flat_map(|c| c.legs.iter().zip(&c.gains))
            .map(|(l, gain)| {
                let why = w.why(l.to, l.item).unwrap_or("a treat");
                let karma = cycles::karma_weight(w, l.to, l.item).1 * gain;
                (format!("{} ({why}) gets the {}", w.npcs[l.to].name, w.items[l.item].name), karma)
            })
            .collect()
    }

    /// Goo and Karma a set of trades and gifts produces, for display.
    pub fn tally(&self, bits: u32) -> Tally {
        let mut t = Tally::default();
        for i in qubo::chosen(bits, self.cycles.len()) {
            let c = &self.cycles[i];
            t.goo += c.goo();
            t.relief += c.relief;
            t.flourishing += c.flourishing;
        }
        t.karma = t.relief + t.flourishing;
        t
    }

    /// Is `bits` a best answer? Judged by value, not by matching the
    /// ground state bit for bit: several trade sets can tie (e.g. when
    /// Tamara wants the kale, ~70% of runs find an equally good set that
    /// isn't the brute-force one). Must also be clash-free and deliver the want.
    pub fn is_optimal(&self, bits: u32) -> bool {
        let (v, clash) = self.evaluate(bits);
        let best = self.evaluate(self.ground_state()).0;
        !clash && self.delivers_want(bits) && v >= best - 1e-9
    }

    pub fn delivers_want(&self, bits: u32) -> bool {
        match self.want {
            None => true,
            Some((npc, item)) => qubo::chosen(bits, self.cycles.len()).iter().any(|&i| self.cycles[i].delivers(npc, item)),
        }
    }

    /// Largest net field on any strip while every other strip is off (the
    /// static `h` alone overstates it: the springs cancel most of it), and
    /// the field that would flatten a well.
    pub fn field_headroom(&self, physics: &Physics) -> (f64, f64) {
        let fmax = self.idle_fields().iter().fold(0.0f64, |m, x| m.max(x.abs()));
        (fmax, machine::max_safe_field(physics.delta_v))
    }

    /// Net field on each strip while every other strip is off.
    pub fn idle_fields(&self) -> Vec<f64> {
        let mut f = self.ising.h.clone();
        for (&(a, b), &j) in self.ising.edges.iter().zip(&self.ising.j) {
            f[a] -= j;
            f[b] -= j;
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nights to sweep in tests.
    const NIGHTS: std::ops::Range<u64> = 1..25;

    fn tc(night: u64) -> TradeComputer {
        TradeComputer::new(world::laundromat(night), 5.0, 1.6)
    }

    #[test]
    fn exact_solver_matches_brute_force_every_night() {
        for night in NIGHTS {
            let tc = tc(night);
            assert!(tc.cycles.len() <= MAX_BITS);
            let fast = tc.evaluate(tc.ground_state());
            let slow = tc.evaluate(tc.qubo_ground_state());
            assert!(!fast.1 && !slow.1, "night {night}: a best set clashes");
            assert!((fast.0 - slow.0).abs() < 1e-9, "night {night}: {} vs brute force {}", fast.0, slow.0);
        }
    }

    #[test]
    fn every_deliverable_want_is_delivered_without_clashes() {
        let mut checked = 0;
        for night in NIGHTS {
            let mut tc = tc(night);
            for npc in 0..tc.world.npcs.len() {
                for item in tc.deliverable(npc) {
                    assert!(tc.want(npc, item));
                    let best = tc.ground_state();
                    let (who, what) = (tc.world.npcs[npc].name, tc.world.items[item].name);
                    assert!(tc.delivers_want(best), "night {night}: {who} wants the {what}: best set doesn't deliver");
                    assert!(!tc.evaluate(best).1, "night {night}: {who} wants the {what}: best set clashes");
                    assert!(tc.want_cost(best) >= -1e-9, "a want can't beat the forward optimum");
                    // The cached best set really is the QUBO's ground state
                    // (brute force is ~1M states, so once per night).
                    if checked % 7 == 0 {
                        let q = &tc.problem.qubo;
                        assert!((q.energy(best) - q.energy(tc.qubo_ground_state())).abs() < 1e-9, "night {night}: {who}/{what}");
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked >= 100, "only {checked} deliverable wants across the test nights");
    }

    #[test]
    fn a_want_clamps_its_strips_without_changing_the_answer() {
        let flat = machine::max_safe_field(Physics::default().delta_v);
        for night in 1..8 {
            let mut tc = tc(night);
            let mut loose = tc.clone();
            loose.want_clamp = 0.0;
            for npc in 0..tc.world.npcs.len() {
                for item in tc.deliverable(npc) {
                    tc.want(npc, item);
                    loose.want(npc, item);
                    let f = tc.idle_fields();
                    let hits: Vec<usize> = (0..tc.cycles.len()).filter(|&i| tc.cycles[i].delivers(npc, item)).collect();
                    // Every wanted strip is pinned, the weakest at exactly the target.
                    let weakest = hits.iter().map(|&i| f[i]).fold(f64::INFINITY, f64::min);
                    assert!(weakest >= WANT_CLAMP * flat - 1e-9, "night {night}: wanted strip field {weakest:.2}");
                    // Unwanted strips are untouched.
                    for i in (0..tc.cycles.len()).filter(|i| !hits.contains(i)) {
                        assert!((f[i] - loose.idle_fields()[i]).abs() < 1e-9);
                    }
                    // Pulling harder picks an equally good set.
                    let (a, b) = (tc.evaluate(tc.ground_state()).0, loose.evaluate(loose.ground_state()).0);
                    assert!((a - b).abs() < 1e-9, "night {night}: clamped best {a} vs unclamped {b}");
                    assert!(tc.delivers_want(tc.ground_state()) && !tc.evaluate(tc.ground_state()).1);
                }
            }
        }
    }

    #[test]
    fn undeliverable_wants_are_refused_and_clear_restores_forward_mode() {
        let mut tc = tc(1);
        let upd = tc.world.find_npc("upddayett").unwrap();
        let owned = tc.world.find_item("Mtn Goo").unwrap();
        assert!(!tc.deliverable(upd).contains(&owned));
        assert!(!tc.want(upd, owned));
        assert!(tc.want.is_none());

        let (npc, item) = (0..tc.world.npcs.len()).find_map(|n| tc.deliverable(n).first().map(|&i| (n, i))).unwrap();
        assert!(tc.want(npc, item));
        tc.clear_want();
        assert!(tc.want.is_none());
        assert_eq!(tc.ground_state(), tc.forward_best);
    }

    #[test]
    fn rewiring_keeps_the_strips_where_they_were() {
        let mut tc = tc(1);
        let physics = Physics::default();
        let mut old = tc.machine(physics, 7).unwrap();
        old.set_temperature(1.5);
        for _ in 0..500 {
            old.step().unwrap();
        }
        let (npc, item) = (0..tc.world.npcs.len()).find_map(|n| tc.deliverable(n).first().map(|&i| (n, i))).unwrap();
        assert!(tc.want(npc, item));
        let mut new = tc.machine(physics, 8).unwrap();
        new.take_state_from(&old).unwrap();
        assert_eq!(new.positions(), old.positions());
        assert_eq!(new.velocities(), old.velocities());
        assert_eq!(new.time(), old.time());
        assert_eq!(new.temperature(), old.temperature());
    }

    #[test]
    fn nights_change_values_and_tags_by_the_rules() {
        use world::{Night, Tag};
        let mut w = world::laundromat(1);
        let tamara = w.find_npc("tamara").unwrap();
        let cart = w.find_npc("shopping-cart").unwrap();
        let vape = w.find_npc("vape").unwrap();
        let ray = w.find_npc("ray").unwrap();
        let kale = w.find_item("kale").unwrap();
        let bag = w.find_item("sleeping bag").unwrap();
        let phone = w.find_item("phone").unwrap();
        let card = w.find_item("library card").unwrap();

        let calm = Night::calm(w.npcs.len());
        let mut hungry_cold = calm.clone();
        hungry_cold.hungry[tamara] = true;
        hungry_cold.cold = true;

        w.set_night(calm.clone());
        let (kale_fed, bag_mild) = (w.value[tamara][kale], w.value[cart][bag]);
        assert_eq!(w.tag(tamara, kale), Some(Tag::Pleasure));
        assert_eq!(w.tag(cart, bag), Some(Tag::Pleasure));
        assert!(w.needs_covered(tamara));

        w.set_night(hungry_cold);
        assert!(w.value[tamara][kale] > 2.0 * kale_fed, "hunger raises what food is worth");
        assert!(w.value[cart][bag] > 2.0 * bag_mild, "a cold night raises what warmth is worth to someone outside");
        assert_eq!(w.tag(tamara, kale), Some(Tag::Need));
        assert_eq!(w.tag(cart, bag), Some(Tag::Need));
        assert!(!w.needs_covered(tamara));

        // No phone: a phone is a need (and stays one on any night).
        assert_eq!(w.tag(vape, phone), Some(Tag::Need));
        assert!(!w.needs_covered(vape));
        // Tools and parts are purpose, whatever the night.
        assert_eq!(w.tag(ray, card), Some(Tag::Purpose));
        assert_eq!(w.tag(ray, kale), None);
    }

    #[test]
    fn nights_differ_and_replay() {
        let a = world::laundromat(3);
        let b = world::laundromat(3);
        assert_eq!(a.night, b.night, "same seed, same night");
        let distinct = (1..20u64).map(|s| format!("{:?}", world::laundromat(s).night.hungry)).collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() > 5, "nights should vary");
    }

    #[test]
    fn gifts_come_from_the_donor_and_score_karma() {
        for night in NIGHTS {
            let tc = tc(night);
            let w = &tc.world;
            for c in tc.cycles.iter().filter(|c| c.is_gift()) {
                let first = c.legs[0];
                assert!(w.items[first.item].gift, "a gift chain starts with a donated item");
                assert_eq!(w.items[first.item].owner, first.from);
                assert!(c.gains.iter().all(|&g| g > 0.0));
                assert!((c.karma() - (c.relief + c.flourishing)).abs() < 1e-12);
                assert_eq!(c.value(), c.karma(), "the drum weighs gifts by Karma");
                // Direct gifts: relief iff the recipient is hungry tonight.
                assert_eq!(c.relief > 0.0, w.night.hungry[first.to], "night {night}: {}", c.short(w));
            }
            assert!(tc.cycles.iter().filter(|c| !c.is_gift()).all(|c| c.relief == 0.0 && c.flourishing == 0.0));
        }
    }

    #[test]
    fn karma_puts_needs_first() {
        use world::Night;
        let mut w = world::laundromat(1);
        let (cart, ray) = (w.find_npc("shopping-cart").unwrap(), w.find_npc("ray").unwrap());
        let polo = w.find_item("adas polo").unwrap();
        let mut night = Night::calm(w.npcs.len());
        night.hungry[cart] = true;
        w.set_night(night);
        let gifts = cycles::enumerate_gifts(&w, 1);
        let to = |npc: usize| gifts.iter().find(|c| c.legs[0].to == npc).unwrap();
        // Fed and covered: pleasure counts 1.5x, but food is worth less to him.
        assert!(w.needs_covered(ray));
        assert_eq!(to(ray).flourishing, cycles::FLOURISH * w.value[ray][polo]);
        // Hungry: relief, 1x, and it outweighs the treat.
        assert_eq!(to(cart).relief, w.value[cart][polo]);
        assert!(to(cart).karma() > to(ray).karma(), "feeding the hungry beats a treat for the fed");
    }

    #[test]
    fn the_drum_routes_food_to_someone_hungry() {
        for night in 1..60 {
            let tc = tc(night);
            let w = &tc.world;
            let best = tc.forward_best;
            let gift = qubo::chosen(best, tc.cycles.len()).into_iter().map(|i| &tc.cycles[i]).find(|c| c.is_gift());
            let hungry_wants = (0..w.npcs.len()).any(|k| w.night.hungry[k] && w.items.iter().enumerate().any(|(i, it)| it.gift && w.value[k][i] > 0.0));
            if hungry_wants {
                let g = gift.expect("someone hungry wants the food, so it goes out");
                assert!(w.night.hungry[g.legs[0].to], "night {night}: food went to {} while someone hungry wanted it", w.npcs[g.legs[0].to].name);
            }
        }
    }

    #[test]
    fn a_restaurant_never_goes_hungry_and_the_other_coins_stay_put() {
        let npcs = world::laundromat(1).npcs;
        let amir = npcs.iter().position(|n| n.business).expect("Amir's is a business");
        let mut as_person = npcs.clone();
        as_person[amir].business = false;
        let mut was_hungry = 0;
        for seed in 0..200 {
            let night = world::Night::roll(&npcs, seed);
            let mut old = world::Night::roll(&as_person, seed);
            assert!(!night.hungry[amir], "night {seed}: the restaurant is hungry");
            was_hungry += old.hungry[amir] as usize;
            // Only the restaurant's own flag differs from the old roll.
            old.hungry[amir] = false;
            assert_eq!(night, old, "night {seed}");
        }
        assert!(was_hungry > 0, "the old roll did make it hungry");
    }

    #[test]
    fn upddayett_gives_his_things_away() {
        for night in NIGHTS {
            let w = world::laundromat(night);
            let upd = w.find_npc("upddayett").unwrap();
            let polo = w.find_item("adas polo").unwrap();
            let mine = w.giveable(upd);
            assert_eq!(mine.len(), 3, "night {night}: goo, phone and kale all have takers");
            for &item in &mine {
                let mut given = w.clone();
                given.set_gift(item, true);
                assert_eq!(given.value[upd][item], 0.0, "he asks nothing for it");
                for (k, row) in given.value.iter().enumerate() {
                    for (i, v) in row.iter().enumerate() {
                        if (k, i) != (upd, item) {
                            assert_eq!(*v, w.value[k][i], "giving changes nothing else");
                        }
                    }
                }
                let tc = TradeComputer::new(given.clone(), 5.0, 1.6);
                let moves = |c: &Cycle| c.legs.iter().any(|l| l.item == item);
                assert!(tc.cycles.iter().filter(|c| moves(c)).all(|c| c.is_gift() && c.legs[0].from == upd), "a promised gift leaves the trades");
                assert!(tc.cycles.iter().any(|c| c.is_gift() && c.legs[0].item == polo), "Amir's still gets a strip");
                let sent = qubo::chosen(tc.forward_best, tc.cycles.len()).into_iter().any(|i| moves(&tc.cycles[i]));
                assert!(sent, "night {night}: nothing else wants the strip, so the gift goes out");
                if given.items[item].name.contains("phone") {
                    let (line, karma) = tc.gift_lines(tc.forward_best).into_iter().find(|(l, _)| l.contains("phone")).unwrap();
                    assert_eq!(line, "Vape Lady (no phone) gets the cracked Android phone", "the reason is the phone, not her hunger");
                    assert_eq!(karma, given.value[given.find_npc("vape lady").unwrap()][item], "a need met counts 1x");
                }
                given.set_gift(item, false);
                assert_eq!(given.value, w.value, "taking it back restores the night");
            }
        }
    }

    #[test]
    fn the_salties_draw_last_so_old_nights_stay_put() {
        // The roll as it was before the Salties (3.3c), frozen here.
        fn old_roll(npcs: &[world::Npc], seed: u64) -> (bool, Vec<bool>, Vec<bool>) {
            let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut coin = |p: f64| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                ((s >> 11) as f64 / (1u64 << 53) as f64) < p
            };
            let cold = coin(0.5);
            let hungry = npcs.iter().map(|n| coin(if n.sleeps_out { 0.6 } else { 0.15 }) && !n.business).collect();
            let animals = npcs.iter().map(|n| n.has_animals && coin(0.6)).collect();
            (cold, hungry, animals)
        }
        let npcs = world::laundromat(1).npcs;
        let (mut magnets, mut cuts, mut emps) = (0, 0, 0);
        for seed in 0..500 {
            let night = world::Night::roll(&npcs, seed);
            assert_eq!((night.cold, night.hungry.clone(), night.animals_hungry.clone()), old_roll(&npcs, seed), "night {seed}");
            match night.sabotage {
                Some(Sabotage::Magnet(m)) => {
                    magnets += 1;
                    assert!((0.2..=1.6).contains(&m.strength.abs()) && (-0.5..=19.5).contains(&m.pos));
                }
                Some(Sabotage::PowerCut(at)) => {
                    cuts += 1;
                    assert!((0.15..=0.85).contains(&at));
                }
                Some(Sabotage::Emp) => emps += 1,
                None => {}
            }
        }
        let salty = (magnets + cuts + emps) as f64 / 500.0;
        assert!((salty - salties::SALTY_NIGHTS).abs() < 0.06, "Salties on {salty:.2} of nights");
        assert!(magnets > cuts && cuts > emps && emps > 0, "{magnets} magnets, {cuts} cuts, {emps} EMPs");
    }

    #[test]
    fn a_magnets_field_falls_off_with_distance() {
        let flat = machine::max_safe_field(5.0);
        let m = Magnet { pos: 6.0, strength: -0.8 };
        let f = m.field(20, 5.0);
        assert!((f[6] + 0.8 * flat).abs() < 1e-12, "full strength right above it");
        assert!(f.iter().all(|&x| x < 0.0), "one direction everywhere: pushing off");
        for d in 1..6 {
            assert!(f[6 + d].abs() < f[6 + d - 1].abs() && (f[6 + d] - f[6 - d]).abs() < 1e-12, "falls off evenly");
        }
        // Far away (r >> DEPTH) it's a plain 1/r^3.
        let r = 13.0;
        let far = 0.8 * flat * (salties::DEPTH / r).powi(3);
        assert!((f[19].abs() / far - 1.0).abs() < 0.03, "1/r^3 far away: {} vs {far}", f[19]);
        let s = m.shielded().field(20, 5.0);
        assert!(f.iter().zip(&s).all(|(a, b)| (b / a - salties::SHIELD).abs() < 1e-12), "the shield passes a fixed fraction");
    }

    #[test]
    fn the_idle_check_reads_the_magnet_on_top_of_the_want() {
        let physics = Physics::default();
        for night in [1, 4, 7] {
            let mut tc = tc(night);
            // A want's clamp is the biggest field the i9 installs itself.
            let upd = tc.world.find_npc("upddayett").unwrap();
            let item = tc.deliverable(upd)[0];
            assert!(tc.want(upd, item));
            let mut clean = tc.machine(physics, night).unwrap();
            clean.rest(30.0).unwrap();
            let quiet = clean.stray_field(&tc.ising).iter().fold(0.0f64, |m, x| m.max(x.abs()));
            assert!(quiet < 1e-3, "night {night}: a clean board reads {quiet} of stray field");

            let magnet = Magnet { pos: 3.0, strength: 0.3 };
            let mut m = tc.tampered_machine(physics, night, &magnet).unwrap();
            m.rest(30.0).unwrap();
            let read = m.stray_field(&tc.ising);
            for (i, (r, f)) in read.iter().zip(magnet.field(tc.ising.n, physics.delta_v)).enumerate() {
                assert!((r - f).abs() < 1e-3, "night {night} strip {i}: the i9 reads {r}, the magnet pushes {f}");
            }
        }
    }

    #[test]
    fn a_strong_magnet_pins_the_strips_it_overpowers() {
        use cortenforge::sim::thermostat::WellState;
        let physics = Physics::default();
        let flat = machine::max_safe_field(physics.delta_v);
        let mut pinned = 0;
        for night in [1, 2, 5] {
            let tc = tc(night);
            let n = tc.ising.n;
            // The strongest push toward "on" the strip's own board could give
            // it, with every neighbor anywhere in its wells (|x| <= 1.1).
            let mut most = tc.ising.h.clone();
            for (&(a, b), &j) in tc.ising.edges.iter().zip(&tc.ising.j) {
                most[a] += 1.1 * j.abs();
                most[b] += 1.1 * j.abs();
            }
            let magnet = Magnet { pos: n as f64 / 2.0, strength: -1.6 };
            let field = magnet.field(n, physics.delta_v);
            for seed in 0..4 {
                let mut m = tc.tampered_machine(physics, seed, &magnet).unwrap();
                m.rest(30.0).unwrap();
                for i in (0..n).filter(|&i| most[i] + field[i] < -flat) {
                    assert_eq!(m.well(i), WellState::Left, "night {night} seed {seed}: strip {i} should be pinned off");
                    pinned += 1;
                }
            }
        }
        assert!(pinned >= 4, "only {pinned} strips were overpowered: the test checks nothing");
    }

    #[test]
    fn the_shield_works_only_over_the_magnet() {
        let m = Magnet { pos: 7.3, strength: 1.0 };
        assert_eq!(m.through(None), m);
        assert_eq!(m.through(Some(6.0)), m.shielded(), "within the plate's span");
        assert_eq!(m.through(Some(12.0)), m, "a plate somewhere else does nothing");
        assert_eq!(m.strip(20), 7);
        assert_eq!(Magnet { pos: 19.4, strength: 1.0 }.strip(12), 11, "past the end of a short board");
    }

    #[test]
    fn the_battery_takes_its_cells_off_the_market() {
        for night in NIGHTS {
            let mut w = world::laundromat(night);
            let cells = w.find_item(salties::BATTERY_CELLS).unwrap();
            let open = TradeComputer::new(w.clone(), 5.0, 1.6);
            w.set_held(cells, true);
            let held = TradeComputer::new(w.clone(), 5.0, 1.6);
            assert!(held.cycles.iter().all(|c| c.legs.iter().all(|l| l.item != cells)), "night {night}: a held item moved");
            let cost = open.evaluate(open.forward_best).0 - held.evaluate(held.forward_best).0;
            assert!(cost >= -1e-9, "night {night}: holding something can't help");
            w.set_held(cells, false);
            assert_eq!(w.value, open.world.value, "putting it back restores the night");
        }
    }

    #[test]
    fn a_power_cut_stops_the_drum_early() {
        let a = Anneal { cut: Some(0.4), ..Anneal::default() };
        assert_eq!(a.stop_time(), 400.0);
        assert_eq!(a.total_time(), 420.0);
        assert!(a.temperature(399.0) > a.cold, "still shaking hot when the power goes");
        assert_eq!(a.temperature(400.0), 0.0);
        assert_eq!(Anneal::default().total_time(), 1020.0, "no cut, the full cycle");
    }
}
