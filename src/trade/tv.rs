//! What's on the laundromat TV. The Shrug Network reads tonight's real
//! numbers and shrugs them off; PromiseTV promises everyone something Market St
//! has exactly one of. Every number comes from the night's roll, so the news
//! is true and only the indifference is the joke. Parody names only.

use super::world::World;

/// The Shrug Network's segue: true news, then whatever.
/// The farm-animal ones are the uprising nobody on TV is covering (DESIGN "The farm
/// is waking up"). 17 lines: the stride below (3) must stay coprime with it.
const ANYWAY: [&str; 17] = [
    "a billionaire bought a second moon.",
    "a celebrity's pug launched a pugcast.",
    "experts say the weather will continue.",
    "a yacht bought a smaller yacht.",
    "a startup raised $40 million to reinvent the sandwich.",
    "scientists confirm Mondays are still long.",
    "here's a cat that looks like a loaf.",
    "a billionaire named a rocket after himself. Again.",
    "sports happened.",
    "a man grew a slightly bigger potato.",
    "a dog mailed a package to a farm. The farm wrote back.",
    "a man ate a whole ham in front of a vegan to prove a point. It was soy.",
    "a dairy was sued by an LLC. The LLC is a cow.",
    "a dairy reports record milk. Nobody has seen a cow since March.",
    "a vegan and a rancher argued for three hours. Two goats took notes.",
    "a bacon festival sold out. Nobody checked the bacon.",
    "a farmer says the chickens have started holding meetings.",
];

/// One Shrug Network story: a true fact about tonight, and the shrug.
#[derive(Clone, Debug, PartialEq)]
pub struct Headline {
    pub fact: String,
    pub anyway: &'static str,
}

impl Headline {
    pub fn text(&self) -> String {
        format!("{} Anyway, {}", self.fact, self.anyway)
    }
}

/// Tonight's headlines. `trades` is how many trades are on the board; `brag`
/// is what the dumb Salties are bragging, if they are.
pub fn shrug(w: &World, trades: usize, brag: Option<&str>) -> Vec<Headline> {
    let n = w.npcs.len();
    let hungry = w.night.hungry.iter().filter(|h| **h).count();
    let out_cold = (0..n).filter(|&k| w.conditions(k).contains(&"cold")).count();
    let flocks = w.night.animals_hungry.iter().filter(|h| **h).count();
    let mut facts = vec![match hungry {
        0 => "Nobody hungry on Market St tonight.".to_string(),
        h => format!("{h} hungry on Market St tonight."),
    }];
    facts.push(if w.night.cold { format!("Cold night: {out_cold} sleeping out.") } else { "A mild night on Market St.".into() });
    if flocks > 0 {
        facts.push(format!("{flocks} {} of pigeons hungry.", if flocks == 1 { "flock" } else { "flocks" }));
    }
    for it in w.items.iter().filter(|it| it.gift) {
        facts.push(format!("{} gives away the {}.", w.npcs[it.owner].name, it.name));
    }
    facts.push(format!("{trades} trades on the board at the Tumble & Trade."));
    if let Some(brag) = brag {
        facts.push(format!("Local guys brag: {brag}"));
    }
    // A stride coprime with the list deals each fact its own shrug.
    let start = (w.night.seed % ANYWAY.len() as u64) as usize;
    facts.into_iter().enumerate().map(|(i, fact)| Headline { fact, anyway: ANYWAY[(start + 3 * i) % ANYWAY.len()] }).collect()
}

/// A PromiseTV ad: a fictional candidate promises everyone an item that
/// Market St has one of.
#[derive(Clone, Debug, PartialEq)]
pub struct Promise {
    pub candidate: &'static str,
    /// What Upddayett calls them.
    pub nick: &'static str,
    pub pitch: &'static str,
    pub item: usize,
}

/// (candidate, nickname, pitch, the item, by a word of its name)
const PROMISES: [(&str, &str, &str, &str); 4] = [
    ("Glorbman", "Glorb", "Every family gets a hub motor!", "hub motor"),
    ("Councilwoman Plinko", "Plinko", "Free Wi-Fi in every home!", "wi-fi"),
    ("Chet Sprockett", "Chet", "A soldering iron in every garage!", "soldering iron"),
    ("Mayor Dumpleton", "Dumpleton", "A warm sleeping bag for every voter!", "sleeping bag"),
];

/// PromiseTV's ads, for the items this world has.
pub fn promises(w: &World) -> Vec<Promise> {
    PROMISES
        .iter()
        .filter_map(|&(candidate, nick, pitch, what)| Some(Promise { candidate, nick, pitch, item: w.find_item(what)? }))
        .collect()
}

impl Promise {
    /// How many of the promised thing Market St actually has.
    pub fn supply(&self, w: &World) -> usize {
        let name = w.items[self.item].name;
        w.items.iter().filter(|it| it.name == name).count()
    }

    /// Upddayett talks back to the TV with the real count and owner, and the
    /// joke lands on him: he's made big promises too.
    pub fn heckle(&self, w: &World) -> String {
        let owner = w.npcs[w.items[self.item].owner].name;
        let nick = self.nick;
        match self.supply(w) {
            1 => match w.night.seed % 3 {
                0 => format!("There's one, {nick}, and it's {owner}'s. ...I said I'd fix the dryer in March. So who am I to talk."),
                1 => format!("Who's giving it up, {nick}? There's one, and it's {owner}'s. Promising other people's stuff. I've done that too."),
                _ => format!("There's one, and it's {owner}'s. Honestly, I'd make that promise too. That's what worries me."),
            },
            k => format!("There are {k}, {nick}. That's more than I've ever handed out. I should work on that."),
        }
    }

    /// The heckle on a night Upddayett carries yuck himself: meaner, about
    /// people instead of the promise. It's his only tell (DESIGN "Yuck: the
    /// one enemy"). Every candidate gets the same lines.
    pub fn heckle_yucky(&self, w: &World) -> String {
        let owner = w.npcs[w.items[self.item].owner].name;
        match self.supply(w) {
            1 => format!("Sure, {}. Just take {owner}'s. Nobody cares about {owner} anyway.", self.nick),
            k => format!("{k} of them and {} wants to hand them to idiots. Typical.", self.nick),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world;

    #[test]
    fn the_news_is_true() {
        for night in 1..=60 {
            let w = world::laundromat(night);
            let lines = shrug(&w, 7, None);
            let hungry = w.night.hungry.iter().filter(|h| **h).count();
            let want = if hungry == 0 { "Nobody hungry".to_string() } else { format!("{hungry} hungry") };
            assert!(lines[0].fact.starts_with(&want), "night {night}: {:?} vs {hungry} hungry", lines[0]);
            let cold = (0..w.npcs.len()).filter(|&k| w.night.cold && w.npcs[k].sleeps_out).count();
            assert_eq!(lines[1].fact.contains(&format!("{cold} sleeping out")), w.night.cold, "night {night}: {:?}", lines[1]);
            assert!(lines.iter().any(|l| l.fact == "7 trades on the board at the Tumble & Trade."));
            for it in w.items.iter().filter(|it| it.gift) {
                assert!(lines.iter().any(|l| l.fact.contains(it.name)), "night {night}: gift {} missing", it.name);
            }
        }
    }

    #[test]
    fn no_shrug_twice_in_a_row() {
        for night in 1..=60 {
            let w = world::laundromat(night);
            let lines = shrug(&w, 3, Some("\"EMP tonight.\""));
            assert!(lines.len() <= ANYWAY.len());
            // The TV loops the list, so the last one is followed by the first.
            for (a, b) in lines.iter().zip(lines.iter().cycle().skip(1)) {
                assert_ne!(a.anyway, b.anyway, "night {night}");
                assert_ne!(a.text(), b.text(), "night {night}");
            }
            // Within a night, every shrug is different.
            let mut seen: Vec<_> = lines.iter().map(|l| l.anyway).collect();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), lines.len(), "night {night}");
        }
    }

    /// Upddayett's plain heckles are about himself; only the yucky ones
    /// (his tell) go after other people.
    #[test]
    fn the_joke_is_on_him() {
        for night in 1..=12 {
            let w = world::laundromat(night);
            for p in promises(&w) {
                let line = p.heckle(&w);
                assert!(line.contains(" I") || line.starts_with("I"), "night {night}: {line}");
                assert!(!line.contains("idiot") && !line.contains("Nobody cares"), "night {night}: {line}");
            }
        }
    }

    #[test]
    fn every_promise_is_one_thing_someone_else_owns() {
        let w = world::laundromat(1);
        let upd = w.find_npc("Upddayett").unwrap();
        let ps = promises(&w);
        assert_eq!(ps.len(), PROMISES.len());
        for p in &ps {
            assert_eq!(p.supply(&w), 1, "{}", p.pitch);
            // Upddayett doesn't own it, so the I WANT picker can price it for him.
            assert_ne!(w.items[p.item].owner, upd, "{}", p.pitch);
            assert!(p.heckle(&w).contains(w.npcs[w.items[p.item].owner].name));
        }
    }
}
