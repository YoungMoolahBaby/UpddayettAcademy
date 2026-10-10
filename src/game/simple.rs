//! The simple screen, for a first-timer: the night banner, a short list of
//! things to do tonight, one big Start button, and after the wash a card
//! that says, in plain words, who got what they needed. Everything else
//! (programs, the dial, the strips, the board cam, the scope, the i9's
//! call, the ghosts) is under the hood: one button or H opens it.

use bevy::prelude::*;
use bevy_egui::egui;
use cortenforge_play::trade::world::Use;
use cortenforge_play::trade::{TradeComputer, qubo};

use super::sim::{Laundromat, Mode};
use super::ui::{GIFT_EGUI, GOLD, GOO_GREEN, HUNGRY, SALT, ai_lines, defense_controls, give_controls, want_controls};

/// Under the hood: the full machine room instead of the simple screen.
#[derive(Resource)]
pub struct Hood {
    pub open: bool,
}

impl Default for Hood {
    /// Closed, unless `UPD_HOOD=1` (or `UPD_SCOPE`, which needs it) asks for the machine room.
    fn default() -> Self {
        Hood { open: std::env::var_os("UPD_HOOD").is_some() || std::env::var_os("UPD_SCOPE").is_some() }
    }
}

/// Plain text for a need, in the color of its chip.
const NEED: egui::Color32 = HUNGRY;

/// An item's name without its aside: "mesh handheld (texts free, no bill)"
/// reads "mesh handheld".
fn plain(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

/// Who got what tonight, one line each, needs first: ("Ranchelle can text
/// now (mesh handheld)", true). `bits` is the set the drum settled on.
pub fn tonight_lines(tc: &TradeComputer, bits: u32) -> Vec<(String, bool)> {
    let w = &tc.world;
    let mut lines = vec![];
    for c in qubo::chosen(bits, tc.cycles.len()) {
        for l in &tc.cycles[c].legs {
            let (who, what) = (w.npcs[l.to].name, plain(w.items[l.item].name));
            let use_ = w.wants.iter().find(|x| x.npc == l.to && x.item == l.item).map(|x| x.use_);
            let need = w.why(l.to, l.item);
            let line = match (need, use_) {
                (Some("hungry"), _) => (format!("{who} ate tonight ({what})"), true),
                (Some("cold"), _) => (format!("{who} sleeps warm tonight ({what})"), true),
                (Some("pigeons hungry"), _) => (format!("{who}'s pigeons ate ({what})"), true),
                (Some("no phone"), _) => (format!("{who} can reach people now ({what})"), true),
                (_, Some(Use::Build)) => (format!("{who} got the {what} to build with"), false),
                _ => (format!("{who} got the {what}"), false),
            };
            lines.push(line);
        }
    }
    // Needs first; the order within each stays the board's.
    lines.sort_by_key(|(_, need)| !need);
    lines
}

/// Should "Protect the machine" show? On a night the Salties brag, or once
/// a spin gave them away (or the player already set a defense). The smart
/// ones never brag, so a quiet night only shows it after a spin.
fn salties_showing(lm: &Laundromat) -> bool {
    let bragging = lm.sabotage().and_then(|s| s.brag()).is_some();
    let caught = lm.mode == Mode::Done && !lm.salty_lines().is_empty();
    bragging || caught || lm.shield.is_some() || lm.battery || lm.idle.is_some()
}

/// The simple screen's cards. Returns true if the player opened the hood.
pub fn draw(
    ctx: &egui::Context,
    lm: &mut Laundromat,
    tv: &super::tv::Tv,
    shop: &mut super::printer::PrintShop,
    small: bool,
    screen: egui::Vec2,
) -> bool {
    let spinning = matches!(lm.mode, Mode::Cycle { .. });
    let mut open_hood = false;

    // Left: tonight's choices (or the wash, while it runs).
    egui::Window::new("TONIGHT")
        .id("simple_tonight".into())
        .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
        .resizable(false)
        .collapsible(false)
        .default_width((screen.x * 0.2).clamp(250.0, 330.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(screen.y - 140.0).show(ui, |ui| {
                if spinning {
                    let label = match (lm.power_out(), lm.battery) {
                        (true, false) => "The power's out: the drum stopped",
                        (true, true) => "Power cut: running on the battery",
                        _ => "Washing...",
                    };
                    ui.add(egui::ProgressBar::new(lm.progress() as f32).text(label));
                    ui.label(
                        egui::RichText::new("The drum shakes every trade loose, then cools slowly so the strips settle on the trades where everyone gains.")
                            .small(),
                    );
                    return;
                }
                ui.label(egui::RichText::new("THINGS TO DO TONIGHT").strong());
                ui.label(
                    egui::RichText::new("Pick any, or none, then start the wash. The drum finds the trades where everyone gains; nobody loses.")
                        .small(),
                );
                ui.add_space(4.0);
                let fold = |title: &str, color: egui::Color32| egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(color));
                fold("Give something away", GIFT_EGUI).id_salt("do_give").show(ui, |ui| give_controls(ui, lm, small));
                fold("Ask for something", GOLD).id_salt("do_want").show(ui, |ui| {
                    ui.label(egui::RichText::new("Pick someone and a thing they could get. The drum finds the cheapest way.").small());
                    want_controls(ui, lm, tv, small);
                });
                fold("Print a part", egui::Color32::from_rgb(255, 170, 90)).id_salt("do_print").show(ui, |ui| super::printer::panel(ui, shop, lm, small));
                if salties_showing(lm) {
                    let bragging = lm.sabotage().and_then(|s| s.brag()).is_some();
                    fold("Protect the machine", SALT).id_salt(("do_protect", lm.night)).default_open(bragging).show(ui, |ui| {
                        if let Some(brag) = lm.sabotage().and_then(|s| s.brag()) {
                            ui.label(egui::RichText::new(format!("The Salties are bragging: {brag}")).small().italics().color(SALT));
                        }
                        defense_controls(ui, lm, spinning);
                    });
                }
            });
        });

    // Bottom center: the one big button.
    if lm.mode == Mode::Manual {
        egui::Area::new("start_wash".into()).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0]).show(ctx, |ui| {
            let text = egui::RichText::new("▶  START THE WASH").strong().size(if small { 20.0 } else { 26.0 }).color(egui::Color32::BLACK);
            let button = egui::Button::new(text).fill(GOO_GREEN).corner_radius(10.0).min_size(egui::vec2(if small { 240.0 } else { 320.0 }, 54.0));
            if ui.add(button).on_hover_text("The drum spins hot, then cools slowly. When it stops, the i9 hands out the trades.").clicked() {
                lm.start_cycle();
            }
        });
    }

    // Right, after the wash: who got what, people first.
    if lm.mode == Mode::Done {
        egui::Window::new("WHAT HAPPENED TONIGHT")
            .id("simple_result".into())
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
            .resizable(false)
            .collapsible(false)
            .default_width((screen.x * 0.24).clamp(260.0, 380.0))
            .show(ctx, |ui| {
                let best = lm.latch.best_bits;
                let lines = tonight_lines(&lm.tc, best);
                // Every need met, and a few of the rest; the full list is under the hood.
                let shown = lines.iter().filter(|(_, need)| *need).count().max(5);
                for (text, need) in lines.iter().take(shown) {
                    let color = if *need { NEED } else { egui::Color32::from_gray(220) };
                    ui.label(egui::RichText::new(text).color(color));
                }
                if lines.len() > shown {
                    ui.label(egui::RichText::new(format!("...and {} more swaps.", lines.len() - shown)).small());
                }
                if lines.is_empty() {
                    ui.label("Nobody traded tonight.");
                }
                ui.add_space(4.0);
                let t = lm.tc.tally(best);
                let karma = if t.karma > 0.0 { format!("   Karma {:.0}", t.karma) } else { String::new() };
                ui.label(egui::RichText::new(format!("+{:.0} Goo, nobody lost.{karma}", t.goo)).strong());
                if let Some((npc, item)) = lm.tc.want {
                    let (who, what) = (lm.tc.world.npcs[npc].name, plain(lm.tc.world.items[item].name));
                    let line = if lm.tc.delivers_want(best) { format!("You asked: {who} got the {what}.") } else { format!("You asked, but nobody could hand {who} the {what}.") };
                    ui.label(egui::RichText::new(line).color(GOLD));
                }
                ai_lines(ui, lm);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("See every trade").on_hover_text("Under the hood: the i9's call, every strip, the machine.").clicked() {
                        open_hood = true;
                    }
                    if ui.button(egui::RichText::new("Next night >").strong()).clicked() {
                        lm.next_night();
                    }
                });
            });
    }
    open_hood
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortenforge_play::trade::world;

    /// Night 1's best set, in plain words: needs come first, and every
    /// item the set moves gets a line.
    #[test]
    fn tonight_reads_needs_first() {
        for night in 1..=10 {
            let tc = TradeComputer::new(world::laundromat(night), 5.0, 1.6);
            let best = tc.forward_best;
            let lines = tonight_lines(&tc, best);
            let legs: usize = qubo::chosen(best, tc.cycles.len()).into_iter().map(|c| tc.cycles[c].legs.len()).sum();
            assert_eq!(lines.len(), legs, "night {night}");
            let first_want = lines.iter().position(|(_, need)| !need).unwrap_or(lines.len());
            assert!(lines[first_want..].iter().all(|(_, need)| !need), "night {night}: {lines:?}");
            assert!(lines.iter().all(|(t, _)| !t.contains("((")), "night {night}: {lines:?}");
        }
    }

    #[test]
    fn the_mesh_handheld_reads_plainly() {
        let mut w = world::laundromat(1);
        let mesh = w.add_mesh();
        w.set_gift(mesh, true);
        let tc = TradeComputer::new(w, 5.0, 1.6);
        let lines = tonight_lines(&tc, tc.forward_best);
        assert!(lines.contains(&("Ranchelle can reach people now (mesh handheld)".to_string(), true)), "{lines:?}");
    }
}
