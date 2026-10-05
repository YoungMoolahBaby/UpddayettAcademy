//! Laundromat customers, what they bring, and what they want.

/// One physical thing. Every item exists once and starts with one owner.
#[derive(Clone, Debug)]
pub struct Item {
    pub name: &'static str,
    pub owner: usize,
}

#[derive(Clone, Debug)]
pub struct Npc {
    pub name: &'static str,
}

/// `value[npc][item]` is what `npc` thinks `item` is worth (0 = don't want it).
#[derive(Clone, Debug, Default)]
pub struct World {
    pub npcs: Vec<Npc>,
    pub items: Vec<Item>,
    pub value: Vec<Vec<f64>>,
}

impl World {
    pub fn npc(&mut self, name: &'static str) -> usize {
        self.npcs.push(Npc { name });
        self.value.push(vec![0.0; self.items.len()]);
        self.npcs.len() - 1
    }

    /// Adds an item held by `owner`, who values it at `own_value`.
    pub fn has(&mut self, owner: usize, name: &'static str, own_value: f64) -> usize {
        self.items.push(Item { name, owner });
        for row in &mut self.value {
            row.push(0.0);
        }
        let id = self.items.len() - 1;
        self.value[owner][id] = own_value;
        id
    }

    pub fn wants(&mut self, npc: usize, item: usize, value: f64) {
        assert_ne!(self.items[item].owner, npc, "{} already has {}", self.npcs[npc].name, self.items[item].name);
        self.value[npc][item] = value;
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

/// Tuesday night at the Suds & Duds on Turk Street. Values are in
/// "Goo" (one can of Mtn Goo, the neon-green parody soda).
pub fn laundromat_tuesday() -> World {
    let mut w = World::default();
    let upd = w.npc("Upddayett");
    let cart = w.npc("Shopping-Cart Guy");
    let vape = w.npc("Vape Lady");
    let dave = w.npc("Bike Kitchen Dave");
    let pigeon = w.npc("Pigeon Lady");
    let ray = w.npc("Sound Guy Ray");
    let tamara = w.npc("Librarian Tamara");

    let goo = w.has(upd, "12-pack of Mtn Goo", 6.0);
    let phone = w.has(upd, "cracked Android phone", 4.0);
    let kale = w.has(upd, "bag of aeroponic kale", 3.0);

    let hub = w.has(cart, "350 W scooter hub motor", 5.0);
    let wheels = w.has(cart, "four shopping-cart casters", 3.0);

    let cells = w.has(vape, "six 18650 cells (ex-disposable vapes)", 4.0);
    let usbc = w.has(vape, "USB-C PD charger", 3.0);

    let iron = w.has(dave, "TS101 soldering iron", 6.0);
    let tube = w.has(dave, "patched 26\" inner tube", 1.0);
    let derailleur = w.has(dave, "Shimano derailleur", 4.0);

    let bag = w.has(pigeon, "zero-degree sleeping bag", 7.0);
    let seed = w.has(pigeon, "50 lb sack of birdseed", 3.0);

    let gpu = w.has(ray, "dead RTX 3090", 3.0);
    let vinyl = w.has(ray, "crate of 90s Seattle vinyl", 2.0);

    let card = w.has(tamara, "laminated library card", 2.0);
    let wifi = w.has(tamara, "Wi-Fi password (staff network)", 3.0);

    // Upddayett wants to build a balance bot.
    w.wants(upd, hub, 9.0);
    w.wants(upd, cells, 8.0);
    w.wants(upd, iron, 9.0);
    w.wants(upd, wifi, 5.0);

    w.wants(cart, goo, 8.0);
    w.wants(cart, bag, 9.0);
    w.wants(cart, tube, 4.0);

    w.wants(vape, phone, 7.0);
    w.wants(vape, goo, 7.0);
    w.wants(vape, gpu, 5.0);

    w.wants(dave, hub, 8.0);
    w.wants(dave, wheels, 5.0);
    w.wants(dave, cells, 6.0);

    w.wants(pigeon, kale, 6.0);
    w.wants(pigeon, tube, 2.0);
    w.wants(pigeon, usbc, 4.0);

    w.wants(ray, derailleur, 5.0);
    w.wants(ray, card, 4.0);
    w.wants(ray, usbc, 5.0);

    w.wants(tamara, seed, 5.0);
    w.wants(tamara, vinyl, 4.0);
    w.wants(tamara, kale, 5.0);

    w
}
