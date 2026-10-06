//! Laundromat customers, what they bring, what they want, and tonight's
//! conditions (who's hungry, who's cold), which set what everything is worth.

/// Why someone wants (or holds) a thing. The tag goes on the want, not the
/// item: Pigeon Lady wants kale to feed her pigeons, Tamara wants it to eat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Eat,
    WarmUp,
    FeedAnimals,
    /// A phone: how you reach services and help.
    Lifeline,
    /// Tools and parts for making things.
    Build,
    /// Treats.
    Enjoy,
}

/// What a want means tonight, for Karma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// Reduces suffering (relief).
    Need,
    /// Adds purpose (flourishing).
    Purpose,
    /// Adds pleasure (flourishing).
    Pleasure,
}

/// One physical thing. Every item exists once and starts with one owner.
#[derive(Clone, Debug)]
pub struct Item {
    pub name: &'static str,
    pub owner: usize,
    /// What the owner would use it for (sets how hard they hold on to it).
    pub owner_use: Use,
    /// The owner's value for it on a neutral night.
    pub base: f64,
    /// Given away tonight: it can start a gift chain, and its owner asks
    /// nothing for it.
    pub gift: bool,
}

#[derive(Clone, Debug)]
pub struct Npc {
    pub name: &'static str,
    pub sleeps_out: bool,
    pub has_animals: bool,
    pub has_phone: bool,
    /// A business (Amir's), not a person: it never goes hungry.
    pub business: bool,
}

/// `npc` wants `item` for `use_`, worth `base` Goo on a neutral night.
#[derive(Clone, Copy, Debug)]
pub struct Want {
    pub npc: usize,
    pub item: usize,
    pub base: f64,
    pub use_: Use,
}

/// Tonight's conditions.
#[derive(Clone, Debug, PartialEq)]
pub struct Night {
    pub seed: u64,
    pub cold: bool,
    pub hungry: Vec<bool>,
    pub animals_hungry: Vec<bool>,
}

impl Night {
    /// Nobody hungry, mild weather, animals fed.
    pub fn calm(n_npcs: usize) -> Self {
        Self { seed: 0, cold: false, hungry: vec![false; n_npcs], animals_hungry: vec![false; n_npcs] }
    }

    /// Roll a night. People sleeping out go hungry more often; only people
    /// with animals can have hungry animals. A business draws its
    /// hunger coin too (and ignores it), so every later coin stays put.
    pub fn roll(npcs: &[Npc], seed: u64) -> Self {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut coin = |p: f64| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64) < p
        };
        let cold = coin(0.5);
        let hungry = npcs.iter().map(|n| coin(if n.sleeps_out { 0.6 } else { 0.15 }) && !n.business).collect();
        let animals_hungry = npcs.iter().map(|n| n.has_animals && coin(0.6)).collect();
        Self { seed, cold, hungry, animals_hungry }
    }
}

/// `value[npc][item]` is what `npc` thinks `item` is worth tonight (0 = don't
/// want it). It's derived from the bases, the uses and the night.
#[derive(Clone, Debug)]
pub struct World {
    pub npcs: Vec<Npc>,
    pub items: Vec<Item>,
    pub wants: Vec<Want>,
    pub night: Night,
    pub value: Vec<Vec<f64>>,
}

/// Values are whole Goo: easier to read, and near-ties (36.6 vs 36.7) that
/// the drum can't tell apart become exact ties, which count as optimal.
/// Never rounds a real want down to zero.
fn whole_goo(v: f64) -> f64 {
    if v > 0.0 { v.round().max(1.0) } else { 0.0 }
}

/// How tonight scales a base value, and what the want means.
fn rule(use_: Use, npc: &Npc, i: usize, night: &Night) -> (f64, Tag) {
    match use_ {
        Use::Eat if night.hungry[i] => (1.4, Tag::Need),
        Use::Eat => (0.5, Tag::Pleasure),
        Use::WarmUp if night.cold && npc.sleeps_out => (1.3, Tag::Need),
        Use::WarmUp => (0.5, Tag::Pleasure),
        Use::FeedAnimals if night.animals_hungry[i] => (1.2, Tag::Need),
        Use::FeedAnimals => (0.6, Tag::Pleasure),
        Use::Lifeline if !npc.has_phone => (1.0, Tag::Need),
        Use::Lifeline => (0.5, Tag::Purpose),
        Use::Build => (1.0, Tag::Purpose),
        Use::Enjoy => (1.0, Tag::Pleasure),
    }
}

impl World {
    fn empty() -> Self {
        Self { npcs: vec![], items: vec![], wants: vec![], night: Night::calm(0), value: vec![] }
    }

    fn npc(&mut self, name: &'static str, sleeps_out: bool, has_animals: bool, has_phone: bool) -> usize {
        self.npcs.push(Npc { name, sleeps_out, has_animals, has_phone, business: false });
        self.npcs.len() - 1
    }

    /// Adds an item held by `owner`, worth `base` to them on a neutral night.
    fn has(&mut self, owner: usize, name: &'static str, base: f64, owner_use: Use) -> usize {
        self.items.push(Item { name, owner, owner_use, base, gift: false });
        self.items.len() - 1
    }

    fn wants(&mut self, npc: usize, item: usize, base: f64, use_: Use) {
        assert_ne!(self.items[item].owner, npc, "{} already has {}", self.npcs[npc].name, self.items[item].name);
        self.wants.push(Want { npc, item, base, use_ });
    }

    /// Switch to another night: recompute every value from the bases.
    pub fn set_night(&mut self, night: Night) {
        assert_eq!(night.hungry.len(), self.npcs.len(), "night rolled for a different cast");
        self.night = night;
        let mut value = vec![vec![0.0; self.items.len()]; self.npcs.len()];
        for (id, it) in self.items.iter().enumerate() {
            // What owners ask for consumables depends on tonight too: a
            // hungry person holds on to their food. Tools, treats and phones
            // keep their base value to their owner.
            let scale = match it.owner_use {
                Use::Eat | Use::WarmUp | Use::FeedAnimals => {
                    rule(it.owner_use, &self.npcs[it.owner], it.owner, &self.night).0
                }
                _ => 1.0,
            };
            value[it.owner][id] = if it.gift { 0.0 } else { whole_goo(it.base * scale) };
        }
        for w in &self.wants {
            let (scale, _) = rule(w.use_, &self.npcs[w.npc], w.npc, &self.night);
            value[w.npc][w.item] = whole_goo(w.base * scale);
        }
        self.value = value;
    }

    /// Roll and switch to night `seed`.
    pub fn roll_night(&mut self, seed: u64) {
        let night = Night::roll(&self.npcs, seed);
        self.set_night(night);
    }

    /// `npc`'s things that someone else wants tonight: what they could give
    /// away (so a gift strip can go out).
    pub fn giveable(&self, npc: usize) -> Vec<usize> {
        (0..self.items.len())
            .filter(|&i| {
                let it = &self.items[i];
                it.owner == npc && !it.gift && (0..self.npcs.len()).any(|k| k != npc && self.value[k][i] > 0.0)
            })
            .collect()
    }

    /// Give `item` away tonight, or take it back. A gift starts a gift chain,
    /// leaves the trades (nobody swaps something already promised), and its
    /// owner asks nothing for it.
    pub fn set_gift(&mut self, item: usize, gift: bool) {
        self.items[item].gift = gift;
        self.set_night(self.night.clone());
    }

    /// What `npc` wanting `item` means tonight, if they want it at all.
    pub fn tag(&self, npc: usize, item: usize) -> Option<Tag> {
        let w = self.wants.iter().find(|w| w.npc == npc && w.item == item)?;
        Some(rule(w.use_, &self.npcs[npc], npc, &self.night).1)
    }

    /// Why `npc` wants `item` tonight, in a word or two: the condition for a
    /// need ("hungry", "no phone"), else "purpose" or "a treat".
    pub fn why(&self, npc: usize, item: usize) -> Option<&'static str> {
        let w = self.wants.iter().find(|w| w.npc == npc && w.item == item)?;
        Some(match (rule(w.use_, &self.npcs[npc], npc, &self.night).1, w.use_) {
            (Tag::Need, Use::Eat) => "hungry",
            (Tag::Need, Use::WarmUp) => "cold",
            (Tag::Need, Use::FeedAnimals) => "pigeons hungry",
            (Tag::Need, Use::Lifeline) => "no phone",
            (Tag::Need, _) => "need",
            (Tag::Purpose, _) => "purpose",
            (Tag::Pleasure, _) => "a treat",
        })
    }

    /// Are `npc`'s basic needs covered tonight? (Not hungry, not cold
    /// outside, animals fed, has a phone.) Flourishing only counts extra then.
    pub fn needs_covered(&self, npc: usize) -> bool {
        let n = &self.npcs[npc];
        !self.night.hungry[npc]
            && !(self.night.cold && n.sleeps_out)
            && !self.night.animals_hungry[npc]
            && n.has_phone
    }

    /// Short words for tonight's conditions on `npc`: ["hungry", "cold"].
    pub fn conditions(&self, npc: usize) -> Vec<&'static str> {
        let n = &self.npcs[npc];
        let mut c = vec![];
        if self.night.hungry[npc] {
            c.push("hungry");
        }
        if self.night.cold && n.sleeps_out {
            c.push("cold");
        }
        if self.night.animals_hungry[npc] {
            c.push("pigeons hungry");
        }
        if !n.has_phone {
            c.push("no phone");
        }
        c
    }

    /// "cold night" / "mild night"
    pub fn weather(&self) -> &'static str {
        if self.night.cold { "cold night" } else { "mild night" }
    }

    pub fn find_npc(&self, name: &str) -> Option<usize> {
        let name = name.to_lowercase();
        self.npcs.iter().position(|n| n.name.to_lowercase().contains(&name))
    }

    pub fn find_item(&self, name: &str) -> Option<usize> {
        let name = name.to_lowercase();
        self.items.iter().position(|i| i.name.to_lowercase().contains(&name))
    }
}

/// The regulars at the Suds & Duds on Turk Street, on night `seed`. Values
/// are in Goo: what a can of Mtn Goo (the neon-green parody soda) is worth
/// to that person. Bases are for a neutral night; the night scales them.
pub fn laundromat(seed: u64) -> World {
    let mut w = World::empty();
    // sleeps out, has animals, has a phone (many unhoused people carry one,
    // e.g. through the federal Lifeline program; Vape Lady doesn't, so her
    // phone is a real need the drum can meet)
    let upd = w.npc("Upddayett", true, false, true);
    let cart = w.npc("Shopping-Cart Guy", true, false, true);
    let vape = w.npc("Vape Lady", true, false, false);
    let dave = w.npc("Bike Kitchen Dave", false, false, true);
    let pigeon = w.npc("Pigeon Lady", true, true, true);
    let ray = w.npc("Sound Guy Ray", false, false, true);
    let tamara = w.npc("Librarian Tamara", false, true, true);
    let amir = w.npc("Amir's Persian Kitchen", false, false, true);
    w.npcs[amir].business = true;

    let goo = w.has(upd, "12-pack of Mtn Goo", 6.0, Use::Enjoy);
    let phone = w.has(upd, "cracked Android phone", 4.0, Use::Lifeline);
    let kale = w.has(upd, "bag of aeroponic kale", 3.0, Use::Eat);

    let hub = w.has(cart, "350 W scooter hub motor", 5.0, Use::Build);
    let wheels = w.has(cart, "four shopping-cart casters", 3.0, Use::Build);

    let cells = w.has(vape, "six 18650 cells (ex-disposable vapes)", 4.0, Use::Build);
    let usbc = w.has(vape, "USB-C PD charger", 3.0, Use::Build);

    let iron = w.has(dave, "TS101 soldering iron", 6.0, Use::Build);
    let tube = w.has(dave, "patched 26\" inner tube", 1.0, Use::Build);
    let derailleur = w.has(dave, "Shimano derailleur", 4.0, Use::Build);

    let bag = w.has(pigeon, "zero-degree sleeping bag", 7.0, Use::WarmUp);
    let seed_sack = w.has(pigeon, "50 lb sack of birdseed", 3.0, Use::FeedAnimals);

    let gpu = w.has(ray, "dead RTX 3090", 3.0, Use::Build);
    let vinyl = w.has(ray, "crate of 90s Seattle vinyl", 2.0, Use::Enjoy);

    let card = w.has(tamara, "laminated library card", 2.0, Use::Build);
    let wifi = w.has(tamara, "Wi-Fi password (staff network)", 3.0, Use::Build);

    let polo = w.has(amir, "tray of day-old adas polo (lentil rice)", 0.0, Use::Eat);
    w.items[polo].gift = true;

    // Upddayett wants to build a balance bot.
    w.wants(upd, hub, 9.0, Use::Build);
    w.wants(upd, cells, 8.0, Use::Build);
    w.wants(upd, iron, 9.0, Use::Build);
    w.wants(upd, wifi, 5.0, Use::Build);

    w.wants(cart, goo, 8.0, Use::Enjoy);
    w.wants(cart, bag, 9.0, Use::WarmUp);
    w.wants(cart, tube, 4.0, Use::Build);

    w.wants(vape, phone, 7.0, Use::Lifeline);
    w.wants(vape, goo, 7.0, Use::Enjoy);
    w.wants(vape, gpu, 5.0, Use::Build); // strips them for scrap gold

    w.wants(dave, hub, 8.0, Use::Build);
    w.wants(dave, wheels, 5.0, Use::Build);
    w.wants(dave, cells, 6.0, Use::Build);

    w.wants(pigeon, kale, 6.0, Use::FeedAnimals);
    w.wants(pigeon, tube, 2.0, Use::Build);
    w.wants(pigeon, usbc, 4.0, Use::Build);

    w.wants(ray, derailleur, 5.0, Use::Build);
    w.wants(ray, card, 4.0, Use::Build);
    w.wants(ray, usbc, 5.0, Use::Build);

    w.wants(tamara, seed_sack, 5.0, Use::FeedAnimals); // the library courtyard pigeons
    w.wants(tamara, vinyl, 4.0, Use::Enjoy);
    w.wants(tamara, kale, 5.0, Use::Eat);

    // Tonight's surplus from Amir's. It's a gift: nobody has to give anything
    // back, and what it does for people counts as Karma.
    for (npc, base) in [(upd, 6.0), (cart, 6.0), (vape, 5.0), (pigeon, 5.0), (ray, 4.0), (tamara, 4.0)] {
        w.wants(npc, polo, base, Use::Eat);
    }

    w.roll_night(seed);
    w
}
