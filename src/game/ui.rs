//! egui overlay: customer portrait cards, the spin-cycle controls, and the
//! trade board.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::qubo;

use super::arrows::trade_color;
use super::scene::{ARROW_Y, LOOKS, MainCam, NpcSpots, board_cam_rect};
use super::sim::{Laundromat, Mode, PROGRAMS};

fn c32(c: Color) -> egui::Color32 {
    let s = c.to_srgba();
    egui::Color32::from_rgba_unmultiplied((s.red * 255.0) as u8, (s.green * 255.0) as u8, (s.blue * 255.0) as u8, (s.alpha * 255.0) as u8)
}

const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 205, 60);

/// "costs the block 2 Goo" / "free"
fn cost_text(cost: f64) -> String {
    if cost < 0.5 { "free: already in the best set".into() } else { format!("costs the block {cost:.0} Goo") }
}

fn initials(name: &str) -> String {
    name.split([' ', '-']).filter_map(|w| w.chars().next()).take(2).collect()
}

pub fn panels(
    mut contexts: EguiContexts,
    mut lm: ResMut<Laundromat>,
    spots: Res<NpcSpots>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCam>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let (cam, cam_tf) = *camera;

    // Portrait cards where the arrows start and end.
    let painter = ctx.layer_painter(egui::LayerId::background());
    for (k, npc) in lm.tc.world.npcs.iter().enumerate() {
        let Ok(p) = cam.world_to_viewport(cam_tf, spots.0[k] + Vec3::Y * ARROW_Y) else {
            continue;
        };
        let at = egui::pos2(p.x, p.y);
        let coat = c32(LOOKS[k % LOOKS.len()].0);
        painter.circle(at, 17.0, egui::Color32::from_black_alpha(200), egui::Stroke::new(2.5, coat));
        painter.text(at, egui::Align2::CENTER_CENTER, initials(npc.name), egui::FontId::proportional(14.0), egui::Color32::WHITE);
        let label = at + egui::vec2(0.0, 26.0);
        let galley = painter.layout_no_wrap(npc.name.to_string(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
        let rect = egui::Rect::from_center_size(label, galley.size() + egui::vec2(10.0, 4.0));
        painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(170));
        painter.galley(rect.min + egui::vec2(5.0, 2.0), galley, egui::Color32::WHITE);
        // Whoever has a want gets a gold ring and a tag.
        if let Some((who, item)) = lm.tc.want
            && who == k
        {
            painter.circle_stroke(at, 21.0, egui::Stroke::new(2.5, GOLD));
            let tag = format!("wants: {}", lm.tc.world.items[item].name);
            let galley = painter.layout_no_wrap(tag, egui::FontId::proportional(12.0), GOLD);
            let rect = egui::Rect::from_center_size(label + egui::vec2(0.0, 19.0), galley.size() + egui::vec2(10.0, 4.0));
            painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(200));
            painter.galley(rect.min + egui::vec2(5.0, 2.0), galley, GOLD);
        }
    }

    // Caption above the board-cam inset.
    let (x, y, w, _) = board_cam_rect(&window);
    let cap = egui::Rect::from_min_size(egui::pos2(x, y - 22.0), egui::vec2(w, 20.0));
    painter.rect_filled(cap, 3.0, egui::Color32::from_black_alpha(200));
    painter.text(
        cap.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "BOARD CAM  -  slap bits (live CortenForge qpos)    green = trade on",
        egui::FontId::monospace(12.0),
        egui::Color32::from_rgb(120, 255, 140),
    );

    let n = lm.n();
    let ground_value = lm.tc.evaluate(lm.ground).0;

    egui::Window::new("SPIN CYCLE")
        .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
        .resizable(false)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new("Lesson 3: Money Laundering (Legally)").strong());
            ui.label(egui::RichText::new(format!("DRUM  {:.2} kT", lm.temperature() * lm.physics.k_b_t)).monospace().size(22.0));
            match lm.mode {
                Mode::Cycle { .. } => {
                    let p = lm.progress() as f32;
                    let label = if p < 0.98 { "spinning down..." } else { "drum stopping" };
                    ui.add(egui::ProgressBar::new(p).text(label));
                }
                Mode::Done => {
                    ui.add(egui::ProgressBar::new(1.0).text("cycle done"));
                }
                Mode::Manual => {
                    ui.add(egui::ProgressBar::new(0.0).text("manual: you hold the dial"));
                }
            }
            let spinning = matches!(lm.mode, Mode::Cycle { .. });
            ui.add_enabled_ui(!spinning, |ui| {
                ui.label("Program (how slowly the drum cools):");
                egui::Grid::new("programs").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                    for (k, p) in PROGRAMS.iter().enumerate() {
                        ui.radio_value(&mut lm.program, k, p.name);
                        ui.label(egui::RichText::new(format!("{:>4.0} units, i9 best {}", p.duration, p.i9_rate)).small().monospace());
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    let run = format!("Run {}", PROGRAMS[lm.program].name);
                    if ui.button(egui::RichText::new(run).strong()).clicked() {
                        lm.start_cycle();
                    }
                    if ui.button("New load").on_hover_text("Fresh random strips, drum on the manual dial").clicked() {
                        lm.new_load();
                    }
                });
                let mut dial = lm.dial;
                if ui.add(egui::Slider::new(&mut dial, 0.0..=6.0).text("manual dial (kT)")).changed() {
                    lm.dial = dial;
                    lm.mode = Mode::Manual;
                }

                // Backward mode: pick a customer and something they could get.
                ui.separator();
                ui.label(egui::RichText::new("I WANT...").strong());
                let mut choice = lm.tc.want;
                ui.horizontal(|ui| {
                    let npcs = &lm.tc.world.npcs;
                    let mut who = lm.picker_npc;
                    egui::ComboBox::from_id_salt("want_who").width(130.0).selected_text(npcs[who].name).show_ui(ui, |ui| {
                        for (k, npc) in npcs.iter().enumerate() {
                            ui.selectable_value(&mut who, k, npc.name);
                        }
                    });
                    let current = choice.filter(|&(n, _)| n == who).map(|(_, item)| lm.tc.world.items[item].name);
                    egui::ComboBox::from_id_salt("want_what")
                        .width(200.0)
                        .selected_text(current.unwrap_or("pick an item..."))
                        .show_ui(ui, |ui| {
                            for (item, it) in lm.tc.world.items.iter().enumerate() {
                                if it.owner == who {
                                    continue;
                                }
                                match lm.want_menu[who].iter().find(|(i, _)| *i == item) {
                                    Some(&(_, cost)) => {
                                        let picked = choice == Some((who, item));
                                        if ui.selectable_label(picked, it.name).on_hover_text(cost_text(cost)).clicked() {
                                            choice = Some((who, item));
                                        }
                                    }
                                    None => {
                                        ui.add_enabled(false, egui::Button::selectable(false, it.name))
                                            .on_disabled_hover_text("nobody's trading that tonight");
                                    }
                                }
                            }
                        });
                    lm.picker_npc = who;
                    if choice.is_some() && ui.button("Clear").clicked() {
                        choice = None;
                    }
                });
                if choice != lm.tc.want {
                    lm.set_want(choice);
                }
                if let Some((npc, item)) = lm.tc.want {
                    let cost = lm.want_menu[npc].iter().find(|(i, _)| *i == item).map_or(0.0, |&(_, c)| c);
                    ui.label(egui::RichText::new(format!("{}: {}", lm.want_text().unwrap_or_default(), cost_text(cost))).small().color(GOLD));
                } else {
                    ui.small("No want: the machine just finds the best trades for everyone.");
                }
            });
            let mut watch = lm.watch;
            if ui
                .add(egui::Slider::new(&mut watch, 0.25..=8.0).logarithmic(true).text("watch speed x"))
                .on_hover_text("How fast you watch. Doesn't change the physics; the program does.")
                .changed()
            {
                lm.watch = watch;
            }
            ui.separator();
            let in_wells = (0..n).filter(|&i| lm.well(i).is_in_well()).count();
            let (rest_v, clash) = lm.tc.evaluate(lm.machine.bits());
            ui.label(format!("strips in a well: {in_wells}/{n}    sim t = {:.0}", lm.machine.time()));
            ui.label(format!("strips say: {rest_v:.0} Goo{}", if clash { "  (two trades fight over an item)" } else { "" }));
            if lm.latch.has_best() {
                let v = lm.tc.evaluate(lm.latch.best_bits).0;
                ui.label(format!("i9 latched: {v:.0} Goo   (best possible: {ground_value:.0})"));
                if lm.tc.want.is_some() {
                    ui.small(format!("(without the want: {:.0})", lm.tc.evaluate(lm.tc.forward_best).0));
                }
            } else {
                ui.label("i9 latched: nothing yet (it reads during a cycle)");
            }
            ui.small("Spin hot, cool slow. Too fast and the strips freeze before they agree.");
            ui.small(format!("sim speed now: {:.0} time units per second", lm.speed()));
        });

    egui::Window::new("TRADES")
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            egui::Grid::new("trades").num_columns(4).spacing([8.0, 3.0]).show(ui, |ui| {
                for (c, cycle) in lm.tc.cycles.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, c32(trade_color(c, n)));
                    let state = match lm.well(c) {
                        WellState::Right => egui::RichText::new("ON ").color(egui::Color32::from_rgb(90, 255, 120)),
                        WellState::Left => egui::RichText::new("off").color(egui::Color32::GRAY),
                        WellState::Barrier => egui::RichText::new(" ~ ").color(egui::Color32::from_rgb(255, 170, 0)),
                    };
                    ui.label(state.monospace());
                    ui.label(format!("{:>2.0} Goo", cycle.value()));
                    let mut text = egui::RichText::new(cycle.short(&lm.tc.world)).small();
                    if lm.locked(c) {
                        text = text.strong().color(egui::Color32::from_rgb(140, 200, 255));
                    }
                    ui.horizontal(|ui| {
                        ui.label(text);
                        if lm.carries_want(c) {
                            ui.label(egui::RichText::new("WANTED").small().strong().color(GOLD));
                        }
                    });
                    ui.end_row();
                }
            });
            if lm.mode == Mode::Done {
                ui.separator();
                let best = lm.latch.best_bits;
                let (v, _) = lm.tc.evaluate(best);
                ui.label(egui::RichText::new("THE i9 CALLS IT").strong().size(16.0));
                for c in qubo::chosen(best, n) {
                    ui.label(egui::RichText::new(format!("- {}.", lm.tc.cycles[c].describe(&lm.tc.world))).small());
                }
                ui.label(format!("Everybody ends up {v:.0} Goo better off."));
                if let Some((npc, item)) = lm.tc.want {
                    let (who, what) = (lm.tc.world.npcs[npc].name, lm.tc.world.items[item].name);
                    let line = if lm.tc.delivers_want(best) {
                        let cost = lm.tc.want_cost(best);
                        let price = if cost < 0.5 { "Cost the block nothing.".to_string() } else { format!("Cost the block {cost:.0} Goo.") };
                        format!("Got {who} the {what}. {price}")
                    } else {
                        format!("Nobody handed {who} the {what}.")
                    };
                    ui.label(egui::RichText::new(line).strong().color(GOLD));
                }
                let line = if lm.tc.is_optimal(best) {
                    "AI: \"It's not money laundering, Daddy. It's a Boltzmann machine.\""
                } else {
                    "AI: \"You spun it too fast. The strips froze before they could agree.\""
                };
                ui.label(egui::RichText::new(line).italics().color(egui::Color32::from_rgb(255, 150, 220)));
            }
        });
    Ok(())
}
