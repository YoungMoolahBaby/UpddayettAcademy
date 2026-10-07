//! egui overlay: the night banner, customer portrait cards, the spin-cycle
//! controls, the trade board and the rules.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use cortenforge::sim::thermostat::WellState;
use cortenforge_play::trade::{Cycle, Sabotage, World, qubo, salties};

use super::arrows::trade_color;
use super::scene::{ARROW_Y, LOOKS, MainCam, NpcSpots, board_cam_rect};
use super::sim::{Laundromat, Mode, PROGRAMS, TRACE_DT, TRACE_LEN};

fn c32(c: Color) -> egui::Color32 {
    let s = c.to_srgba();
    egui::Color32::from_rgba_unmultiplied((s.red * 255.0) as u8, (s.green * 255.0) as u8, (s.blue * 255.0) as u8, (s.alpha * 255.0) as u8)
}

const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 205, 60);
const GOO_GREEN: egui::Color32 = egui::Color32::from_rgb(90, 200, 30);
const ICY: egui::Color32 = egui::Color32::from_rgb(140, 200, 255);
const HUNGRY: egui::Color32 = egui::Color32::from_rgb(255, 150, 60);
/// Gifts and Karma: the same warm gold as the gift arrows (`arrows::GIFT`).
const GIFT_EGUI: egui::Color32 = egui::Color32::from_rgb(255, 140, 38);
/// The Salties: road-salt white with a cold blue cast.
const SALT: egui::Color32 = egui::Color32::from_rgb(200, 225, 240);
/// The counter (escrow): cardboard tan.
const COUNTER: egui::Color32 = egui::Color32::from_rgb(215, 180, 130);
/// PromiseTV's purple-and-gold.
const PROMISE: egui::Color32 = egui::Color32::from_rgb(215, 160, 255);
/// Why the counter: on hover of its rule and its result line.
const ESCROW_WHY: &str = "A 4-way swap only works if everyone delivers or nobody does. Hand things over one at a time and \
                          whoever already got theirs can walk off without giving. So the counter holds everything until the drum \
                          stops, then hands every trade over at once. That's also why loops stop at 4: each extra person is one \
                          more way the loop breaks. Kidney exchanges cap their loops for the same reason.";

/// Below this window size the panels go compact.
const COMPACT: egui::Vec2 = egui::vec2(1280.0, 760.0);

/// "costs the block 2 Goo" / "free"
fn cost_text(cost: f64) -> String {
    if cost < 0.5 { "free: already in the best set".into() } else { format!("costs the block {cost:.0} Goo") }
}

fn initials(name: &str) -> String {
    name.split([' ', '-']).filter_map(|w| w.chars().next()).take(2).collect()
}

/// "U -> VL -> SG -> U": a trade in portrait initials, for small windows.
fn initials_chain(cycle: &Cycle, w: &World) -> String {
    let mut s = initials(w.npcs[cycle.legs[0].from].name);
    for l in &cycle.legs {
        s.push_str(" -> ");
        s.push_str(&initials(w.npcs[l.to].name));
    }
    s
}

fn condition_color(c: &str) -> egui::Color32 {
    match c {
        "hungry" => HUNGRY,
        "cold" => ICY,
        "pigeons hungry" => egui::Color32::from_rgb(220, 190, 120),
        _ => egui::Color32::from_gray(185),
    }
}

/// A row of small labels centered under `at`; returns the y below them.
fn chips(painter: &egui::Painter, at: egui::Pos2, chips: &[(String, egui::Color32)]) -> f32 {
    if chips.is_empty() {
        return at.y;
    }
    let font = egui::FontId::proportional(12.0);
    let galleys: Vec<_> = chips.iter().map(|(t, c)| painter.layout_no_wrap(t.clone(), font.clone(), *c)).collect();
    let gap = 4.0;
    let total: f32 = galleys.iter().map(|g| g.size().x + 10.0).sum::<f32>() + gap * (galleys.len() - 1) as f32;
    let mut x = at.x - total / 2.0;
    let mut bottom = at.y;
    for (g, (_, c)) in galleys.into_iter().zip(chips) {
        let rect = egui::Rect::from_min_size(egui::pos2(x, at.y), g.size() + egui::vec2(10.0, 4.0));
        painter.rect(rect, 4.0, egui::Color32::from_black_alpha(200), egui::Stroke::new(1.0, c.gamma_multiply(0.6)), egui::StrokeKind::Inside);
        painter.galley(rect.min + egui::vec2(5.0, 2.0), g, *c);
        x = rect.max.x + gap;
        bottom = rect.max.y;
    }
    bottom
}

/// A labeled bar: `value` out of `best` (the night's best set).
fn meter(ui: &mut egui::Ui, value: f64, best: f64, fill: egui::Color32, text: String) -> egui::Response {
    let f = if best > 0.0 { (value / best).clamp(0.0, 1.0) as f32 } else { 0.0 };
    ui.add(egui::ProgressBar::new(f).fill(fill).text(egui::RichText::new(text).monospace().color(egui::Color32::WHITE)))
}

#[allow(clippy::too_many_arguments)]
pub fn panels(
    mut contexts: EguiContexts,
    mut lm: ResMut<Laundromat>,
    spots: Res<NpcSpots>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCam>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut hidden: Local<bool>,
    tv: Res<super::tv::Tv>,
    mut shop: ResMut<super::printer::PrintShop>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let (cam, cam_tf) = *camera;
    let screen = egui::vec2(window.width(), window.height());
    let small = screen.x < COMPACT.x || screen.y < COMPACT.y;
    let spinning = matches!(lm.mode, Mode::Cycle { .. });

    if let Ok(p) = cam.world_to_viewport(cam_tf, super::printer::bubble_anchor()) {
        super::printer::bubble(ctx, &shop, matches!(lm.mode, Mode::Cycle { .. }), egui::pos2(p.x, p.y));
    }

    // Portrait cards where the arrows start and end, with tonight's
    // conditions under the name.
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
        let tags: Vec<_> = lm.tc.world.conditions(k).into_iter().map(|c| (c.to_string(), condition_color(c))).collect();
        let below = chips(&painter, egui::pos2(label.x, rect.max.y + 3.0), &tags);
        // Whoever has a want gets a gold ring and a tag.
        if let Some((who, item)) = lm.tc.want
            && who == k
        {
            painter.circle_stroke(at, 21.0, egui::Stroke::new(2.5, GOLD));
            chips(&painter, egui::pos2(label.x, below.max(rect.max.y) + 3.0), &[(format!("wants: {}", lm.tc.world.items[item].name), GOLD)]);
        }
        // Upddayett heckles PromiseTV while an ad is on.
        if let Some(p) = &tv.promise
            && npc.name == "Upddayett"
        {
            let galley = painter.layout(format!("\"{}\"", p.heckle(&lm.tc.world)), egui::FontId::proportional(13.0), egui::Color32::WHITE, 230.0);
            // Up and to the left: the trade board covers the right.
            let size = galley.size() + egui::vec2(12.0, 8.0);
            let bubble = egui::Rect::from_min_size(at + egui::vec2(-26.0 - size.x, -34.0 - size.y), size);
            painter.rect(bubble, 6.0, egui::Color32::from_black_alpha(220), egui::Stroke::new(1.5, PROMISE), egui::StrokeKind::Outside);
            painter.line_segment([bubble.right_bottom() + egui::vec2(-8.0, 0.0), at + egui::vec2(-12.0, -14.0)], egui::Stroke::new(1.5, PROMISE));
            painter.galley(bubble.min + egui::vec2(6.0, 4.0), galley, egui::Color32::WHITE);
        }
    }

    // Caption above the board-cam inset.
    let (x, y, w, _) = board_cam_rect(&window);
    let cap = egui::Rect::from_min_size(egui::pos2(x, y - 22.0), egui::vec2(w, 20.0));
    painter.rect_filled(cap, 3.0, egui::Color32::from_black_alpha(200));
    painter.text(
        cap.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        if small { "BOARD CAM  -  green = trade on" } else { "BOARD CAM  -  slap bits (live CortenForge qpos)    green = trade on" },
        egui::FontId::monospace(12.0),
        egui::Color32::from_rgb(120, 255, 140),
    );

    // Tab hides the panels, to just watch the laundromat.
    if keys.just_pressed(KeyCode::Tab) && !ctx.egui_wants_keyboard_input() {
        *hidden = !*hidden;
    }
    if *hidden {
        egui::Area::new("tab_hint".into()).anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new("Tab: panels").small().color(egui::Color32::from_white_alpha(160)));
        });
        return Ok(());
    }

    // Tonight, top center.
    egui::Area::new("night_banner".into()).anchor(egui::Align2::CENTER_TOP, [0.0, 10.0]).show(ctx, |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from_black_alpha(215))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                let (cold, night) = (lm.tc.world.night.cold, lm.night);
                let hungry = lm.tc.world.night.hungry.iter().filter(|h| **h).count();
                let gifts = lm.tc.cycles.iter().filter(|c| c.is_gift()).count();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("NIGHT {night}")).strong().size(17.0).color(egui::Color32::WHITE));
                    let weather = if cold { egui::RichText::new("cold").color(ICY) } else { egui::RichText::new("mild").color(GOO_GREEN) };
                    ui.label(weather.strong());
                    ui.label(egui::RichText::new(format!("{hungry} hungry")).strong().color(if hungry > 0 { HUNGRY } else { egui::Color32::GRAY }));
                    let next = ui
                        .add_enabled(!spinning, egui::Button::new(egui::RichText::new("Next night >").strong()))
                        .on_hover_text("Close up. Tomorrow rolls new conditions, so everything is worth something new.")
                        .on_disabled_hover_text("Wait for the drum to stop.");
                    if next.clicked() {
                        lm.next_night();
                    }
                });
                let w = &lm.tc.world;
                for it in w.items.iter().filter(|it| it.gift) {
                    ui.label(egui::RichText::new(format!("{} gives away: {}", w.npcs[it.owner].name, it.name)).small().color(GIFT_EGUI));
                }
                // Only the dumb ones announce themselves.
                if let Some(brag) = lm.sabotage().and_then(|s| s.brag()) {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("SALTIES AROUND").strong().color(SALT));
                        ui.label(egui::RichText::new(brag).small().italics().color(SALT));
                    })
                    .response
                    .on_hover_text("Somebody's been hanging around the counter. Which of their brags is real physics? Check before you spin.");
                }
                if !small {
                    ui.label(egui::RichText::new(format!("{} trades + {gifts} gifts on the board", lm.tc.cycles.len() - gifts)).small());
                }
            });
    });

    let n = lm.n();
    let side = |big: f32, compact: f32| if small { compact } else { big };
    let board_cam_top = y - 22.0;
    // Panels scale with the window, so the washer stays in sight between them.
    let trades_w = (screen.x * 0.24).clamp(250.0, 400.0);
    let bragging = lm.sabotage().and_then(|s| s.brag()).is_some();

    egui::Window::new("SPIN CYCLE")
        .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
        .resizable(false)
        .default_width((screen.x * 0.19).clamp(240.0, 330.0))
        .default_height(board_cam_top - 24.0)
        .show(ctx, |ui| {
            // Scrolls rather than run under the board cam. (A fixed-size window
            // only offers last frame's height, so the room is set outright.)
            let room = board_cam_top - 12.0 - ui.cursor().min.y;
            egui::ScrollArea::vertical().max_height(room).min_scrolled_height(room).show(ui, |ui| {
                if !small {
                    ui.label(egui::RichText::new("Lesson 3: Money Laundering (Legally)").strong());
                }
                ui.label(egui::RichText::new(format!("DRUM  {:.2} kT", lm.temperature() * lm.physics.k_b_t)).monospace().size(side(22.0, 18.0)));
                match lm.mode {
                    Mode::Cycle { .. } => {
                        let p = lm.progress() as f32;
                        let label = match (lm.power_out(), lm.battery) {
                            (true, false) => "POWER CUT: the drum stopped",
                            (true, true) => "power cut: running on the battery",
                            _ if p < 0.98 && lm.row_spin.is_some() => "the row trades loads...",
                            _ if p < 0.98 => "spinning down...",
                            _ => "drum stopping",
                        };
                        ui.add(egui::ProgressBar::new(p).text(label));
                        if let Some(spin) = &lm.row_spin {
                            let ladder: Vec<String> = (0..=lm.row.len()).map(|i| format!("{:.1}", spin.setting(i) * lm.physics.k_b_t)).collect();
                            ui.small(format!(
                                "Row: ours {} kT, the back row warmer. Loads traded {}, {} carried down from the hottest.",
                                ladder.join(" / "),
                                spin.stats.accepts.iter().sum::<usize>(),
                                spin.stats.trips
                            ))
                            .on_hover_text("Parallel tempering: every unit, neighbors offer to swap loads, and the swap is taken by the Boltzmann odds of each load's energy at the other drum's heat. A load stuck in a bad set rides up the row, gets shaken loose, and comes back down. The i9 reads every washer and keeps the best set any of them shows.");
                        }
                    }
                    Mode::Done => {
                        ui.add(egui::ProgressBar::new(1.0).text("cycle done"));
                    }
                    Mode::Manual => {
                        ui.add(egui::ProgressBar::new(0.0).text("manual: you hold the dial"));
                    }
                }

                // Goo and Karma, against the night's best set.
                let (whose, bits) = if lm.latch.has_best() { ("i9 latched", lm.latch.best_bits) } else { ("strips now", lm.machine.bits()) };
                let (now, best) = (lm.tc.tally(bits), lm.tc.tally(lm.ground));
                let best_karma = if best.karma > 0.0 { best.karma } else { lm.tc.cycles.iter().map(|c| c.karma()).fold(0.0, f64::max) };
                ui.label(egui::RichText::new(format!("{whose} vs. the best set:")).small());
                meter(ui, now.goo, best.goo, GOO_GREEN.gamma_multiply(0.8), format!("GOO   {:>4.0} / {:.0}", now.goo, best.goo)).on_hover_text("Goo everyone gains tonight, trades and gifts alike");
                meter(ui, now.karma, best_karma, GIFT_EGUI.gamma_multiply(0.8), format!("KARMA {:>4.1} / {best_karma:.1}", now.karma))
                    .on_hover_text(format!(
                        "Karma is what tonight's gift does: relief {:.1} (needs met, 1x) + flourishing {:.1} (treats and tools, 1.5x once needs are covered)",
                        now.relief, now.flourishing
                    ));
                if lm.tc.want.is_some() && lm.latch.has_best() {
                    ui.small(format!("(without the want: {})", lm.tc.score_text(lm.tc.forward_best)));
                }

                ui.add_enabled_ui(!spinning, |ui| {
                    ui.label("Program (how slowly the drum cools):");
                    egui::Grid::new("programs").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                        for (k, p) in PROGRAMS.iter().enumerate() {
                            let detail = if p.row {
                                format!("4 x {:.0} units, i9 best {}", p.duration, p.i9_rate)
                            } else {
                                format!("{:>4.0} units, i9 best {}", p.duration, p.i9_rate)
                            };
                            if small {
                                ui.radio_value(&mut lm.program, k, p.name).on_hover_text(detail);
                            } else {
                                ui.radio_value(&mut lm.program, k, p.name);
                                ui.label(egui::RichText::new(detail).small().monospace());
                            }
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
                    ui.horizontal_wrapped(|ui| {
                        let npcs = &lm.tc.world.npcs;
                        let mut who = lm.picker_npc;
                        egui::ComboBox::from_id_salt("want_who").width(side(130.0, 110.0)).selected_text(npcs[who].name).show_ui(ui, |ui| {
                            for (k, npc) in npcs.iter().enumerate() {
                                ui.selectable_value(&mut who, k, npc.name);
                            }
                        });
                        let current = choice.filter(|&(n, _)| n == who).map(|(_, item)| lm.tc.world.items[item].name);
                        egui::ComboBox::from_id_salt("want_what")
                            .width(side(200.0, 110.0))
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
                    // PromiseTV's promise, checked against the picker's real price.
                    if let Some(p) = &tv.promise
                        && let Some(upd) = lm.tc.world.find_npc("Upddayett")
                    {
                        let supply = p.supply(&lm.tc.world);
                        let price = lm.want_menu[upd].iter().find(|(i, _)| *i == p.item).map(|&(_, c)| c);
                        let check = price.map_or("nobody's trading it tonight".to_string(), |c| format!("for Upddayett it {}", cost_text(c)));
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(egui::RichText::new("ON TV").small().strong().color(PROMISE));
                            ui.label(
                                egui::RichText::new(format!("{}: \"{}\" Market St has {supply}. Checked: {check}.", p.candidate, p.pitch))
                                    .small()
                                    .color(PROMISE),
                            );
                            if price.is_some()
                                && choice != Some((upd, p.item))
                                && ui.small_button("Want it").on_hover_text("Put it in the I WANT picker and see what it really takes.").clicked()
                            {
                                lm.picker_npc = upd;
                                choice = Some((upd, p.item));
                            }
                        });
                    }
                    if choice != lm.tc.want {
                        lm.set_want(choice);
                    }
                    if let Some((npc, item)) = lm.tc.want {
                        let cost = lm.want_menu[npc].iter().find(|(i, _)| *i == item).map_or(0.0, |&(_, c)| c);
                        ui.label(egui::RichText::new(format!("{}: {}", lm.want_text().unwrap_or_default(), cost_text(cost))).small().color(GOLD));
                    } else if !small {
                        ui.small("No want: the machine just finds the best trades for everyone.");
                    }

                    // Upddayett gives one of his things away: a gift strip of
                    // his own, routed by Karma like Amir's food.
                    ui.separator();
                    ui.label(egui::RichText::new("UPDDAYETT GIVES AWAY...").strong().color(GIFT_EGUI));
                    let mut give = lm.give;
                    let name = |item: usize| lm.tc.world.items[item].name;
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("give_what")
                            .width(side(200.0, 150.0))
                            .selected_text(give.map_or("nothing (keeps it all)", name))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut give, None, "nothing (keeps it all)");
                                for &(item, cost) in &lm.give_menu {
                                    let takers: Vec<&str> = (0..lm.tc.world.npcs.len())
                                        .filter(|&k| lm.tc.world.items[item].owner != k && lm.tc.world.value[k][item] > 0.0)
                                        .map(|k| lm.tc.world.npcs[k].name)
                                        .collect();
                                    ui.selectable_value(&mut give, Some(item), name(item))
                                        .on_hover_text(format!("costs him {cost:.0} Goo; wanted by {}", takers.join(", ")));
                                }
                            });
                    });
                    if give != lm.give {
                        lm.set_give(give);
                    }
                    if let Some(item) = lm.give {
                        let cost = lm.give_menu.iter().find(|(i, _)| *i == item).map_or(0.0, |&(_, c)| c);
                        let text = format!("It leaves the trades and costs him {cost:.0} Goo. The drum picks who gets it, by Karma.");
                        ui.label(egui::RichText::new(text).small().color(GIFT_EGUI));
                    } else if !small {
                        ui.small("Give something away and the drum sends it where it does the most good.");
                    }

                    // Upddayett prints a part: the i9 checks his draft first.
                    ui.separator();
                    super::printer::panel(ui, &mut shop, &lm, small);

                    // Red-team the machine before the Salties do: find the
                    // magnet, cover it, or bring the battery.
                    ui.separator();
                    ui.label(egui::RichText::new("DEFENSES").strong().color(SALT));
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("Idle check")
                            .on_hover_text("Stop the drum, let the strips settle, and read every Hall sensor against what the springs say. A stray field means a magnet.")
                            .clicked()
                        {
                            lm.idle_check();
                        }
                        if let Some(stray) = &lm.idle {
                            let (k, top) = stray.iter().enumerate().fold((0, 0.0f64), |b, (i, x)| if x.abs() > b.1 { (i, x.abs()) } else { b });
                            let x = top / lm.flat();
                            let text = if x >= 0.01 {
                                format!("strip {k} feels {x:.2}x the flattening field nobody installed")
                            } else {
                                format!("every strip sits where the springs say ({x:.2}x)")
                            };
                            ui.label(egui::RichText::new(text).small().color(if x >= 0.01 { SALT } else { egui::Color32::GRAY }));
                        }
                    });
                    // The spin check runs on every cycle: the i9 averages the
                    // same force balance while the drum shakes.
                    if lm.spin_check.samples() > 0 && !spinning {
                        let (text, color) = match lm.spin_alarm() {
                            Some((k, x)) => (format!("Spin check: strip {k} felt {x:.2}x while the drum spun"), SALT),
                            None => {
                                let quiet = lm.spin_check.strongest().map_or(0.0, |(_, x)| x.abs() / lm.flat());
                                (format!("Spin check: nothing unexplained while it spun ({quiet:.2}x)"), egui::Color32::GRAY)
                            }
                        };
                        ui.label(egui::RichText::new(text).small().color(color)).on_hover_text(format!(
                            "The idle check's force balance, averaged over the whole spin. The shaking averages out; a push that's only there \
                             while the drum spins doesn't. Over {:.1}x counts.",
                            salties::SPIN_ALARM
                        ));
                    }
                    let mut shield = lm.shield;
                    ui.horizontal(|ui| {
                        let mut on = shield.is_some();
                        ui.checkbox(&mut on, "Steel shield over strip").on_hover_text(format!(
                            "A plate from a dead hard drive. It passes {:.0}% of a magnet's field, but only if it covers the magnet (within {} strips).",
                            100.0 * salties::SHIELD,
                            salties::SHIELD_SPAN
                        ));
                        // Defaults to wherever a check last pointed.
                        let idle_top = lm.idle.as_ref().map(|s| s.iter().enumerate().fold((0, 0.0f64), |b, (i, x)| if x.abs() > b.1 { (i, x.abs()) } else { b }).0);
                        let mut at = shield.or(lm.spin_alarm().map(|a| a.0)).or(idle_top).unwrap_or(n / 2);
                        ui.add_enabled(on, egui::DragValue::new(&mut at).range(0..=n.saturating_sub(1)));
                        shield = on.then_some(at);
                    });
                    if shield != lm.shield {
                        lm.set_shield(shield);
                    }
                    let mut battery = lm.battery;
                    ui.checkbox(&mut battery, if lm.battery_cost < 0.5 { "Battery: Vape Lady's 18650s (free tonight)".to_string() } else { format!("Battery: Vape Lady's 18650s (costs the block {:.0} Goo)", lm.battery_cost) })
                        .on_hover_text("Finishes the cycle if the power goes. They're the cells Upddayett wants for his balance bot: while they run the drum, nobody trades them.");
                    if battery != lm.battery {
                        lm.set_battery(battery);
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
                let clash = lm.tc.evaluate(lm.machine.bits()).1;
                ui.label(format!("strips in a well: {in_wells}/{n}    sim t = {:.0}", lm.machine.time()));
                ui.label(format!("strips say: {}{}", lm.tc.score_text(lm.machine.bits()), if clash { "  (two trades fight over an item)" } else { "" }));
                if !small {
                    ui.small("Spin hot, cool slow. Too fast and the strips freeze before they agree.");
                    ui.small(format!("sim speed now: {:.0} time units per second", lm.speed()));
                    ui.small("Tab hides the panels.");
                }
            });
        });

    // The rules, one glance away (folded on small windows). Drawn before the
    // trade board so the board knows where it has to stop.
    let example = lm.tc.cycles.iter().find(|c| c.len() == 2).and_then(|c| Some((c.swap_text(&lm.tc.world)?, c.gains_text(&lm.tc.world))));
    let rules = egui::Window::new("HOW IT WORKS")
        .id("how_it_works".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
        .resizable(false)
        .default_open(!small)
        .default_width((screen.x * 0.26).clamp(300.0, 440.0))
        .show(ctx, |ui| {
            let rule = |ui: &mut egui::Ui, head: &str, color: egui::Color32, body: &str| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(egui::RichText::new(head).strong().color(color));
                    ui.label(body)
                })
                .inner
            };
            // The basics stay open; the rest fold, so the board keeps its room.
            let section = |ui: &mut egui::Ui, title: &str, open: bool, body: &mut dyn FnMut(&mut egui::Ui)| {
                egui::CollapsingHeader::new(egui::RichText::new(title).strong()).default_open(open).show(ui, |ui| body(ui));
            };
            section(ui, "The basics", true, &mut |ui| {
                rule(ui, "Goo", GOLD, "is how much someone personally values a thing. 1 Goo = a can of Mtn Goo to them.");
                rule(ui, "Trades", GOLD, "only happen if everyone in them gains Goo. Nobody loses.");
                if let Some((swap, gains)) = &example {
                    ui.label(egui::RichText::new(format!("   e.g. {swap}: {gains}")).small().italics());
                }
                rule(ui, "The drum", GOLD, "picks the trades that make the most Goo in total. No item moves twice.");
                rule(ui, "The counter", COUNTER, "holds every item during the spin, then hands each trade over whole, or not at all.")
                    .on_hover_text(ESCROW_WHY);
            });
            section(ui, "Tonight and Karma", false, &mut |ui| {
                rule(ui, "Tonight", GOLD, "sets the values: who's hungry, who's out in the cold. Food means more to someone hungry.");
                rule(
                    ui,
                    "Karma",
                    GIFT_EGUI,
                    "scores a gift by what it does. A need met (food when hungry, warmth on a cold night) counts 1x; \
                     a treat or a tool counts 1.5x, once that person's needs are covered. The drum sends the gift where it does the most good.",
                );
            });
            section(ui, "Wants and give-aways", false, &mut |ui| {
                rule(ui, "A want", GOLD, "gets delivered the cheapest way. Its price is what everyone else gives up.");
                rule(ui, "A give-away", GIFT_EGUI, "costs the giver its Goo and leaves the trades. It earns Karma wherever the drum sends it.");
            });
            // Opens itself on a night the dumb ones brag.
            egui::CollapsingHeader::new(egui::RichText::new("The Salties").strong().color(SALT))
                .id_salt(("salties", lm.night))
                .default_open(bragging)
                .show(ui, |ui| {
                    rule(
                        ui,
                        "Dumb Salties",
                        SALT,
                        "brag about big physics; only the honest kind works. A magnet really pushes the strips \
                         (find it with the idle check, cover it with the shield). A \"solar flare\" is them flipping the breaker (the battery \
                         finishes the cycle). The \"EMP\" does nothing.",
                    );
                    rule(
                        ui,
                        "Smart Salties",
                        SALT,
                        "never brag, so a quiet night isn't a safe one. They aim a coil that only runs while the drum spins (the idle check \
                         sees nothing; the spin check after a cycle does), or trip the breaker early with no flicker. Lose a spin, read the \
                         numbers, defend, spin again.",
                    );
                });
        });
    let rules_top = rules.map_or(screen.y, |r| r.response.rect.top());

    let mut clicked = None;
    egui::Window::new("TRADES")
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
        .resizable(false)
        .default_width(trades_w)
        .default_height(rules_top - 24.0)
        .show(ctx, |ui| {
            // The i9's call first, in its own scroll that takes most of the room
            // (at least half), leaving the board below a few rows.
            let room = rules_top - 24.0 - ui.cursor().min.y;
            if lm.mode == Mode::Done {
                let call = (room - 140.0).max(room * 0.55).max(80.0);
                egui::ScrollArea::vertical().id_salt("i9_call").max_height(call).show(ui, |ui| i9_call(ui, &lm, small));
                ui.separator();
            }
            let room = (rules_top - 24.0 - ui.cursor().min.y).max(80.0);
            egui::ScrollArea::vertical().id_salt("board").max_height(room).min_scrolled_height(room).show(ui, |ui| {
                egui::Grid::new("trades").num_columns(4).spacing([8.0, 3.0]).show(ui, |ui| {
                    for (c, cycle) in lm.tc.cycles.iter().enumerate() {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        if cycle.is_gift() {
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "♥", egui::FontId::proportional(14.0), GIFT_EGUI);
                        } else {
                            ui.painter().rect_filled(rect, 2.0, c32(trade_color(c, n)));
                        }
                        let state = match lm.well(c) {
                            WellState::Right => egui::RichText::new("ON ").color(egui::Color32::from_rgb(90, 255, 120)),
                            WellState::Left => egui::RichText::new("off").color(egui::Color32::GRAY),
                            WellState::Barrier => egui::RichText::new(" ~ ").color(egui::Color32::from_rgb(255, 170, 0)),
                        };
                        ui.label(state.monospace());
                        if cycle.is_gift() {
                            ui.label(egui::RichText::new(format!("{:>2.1} Karma", cycle.karma())).color(GIFT_EGUI));
                        } else {
                            ui.label(format!("{:>2.0} Goo", cycle.goo()));
                        }
                        let w = &lm.tc.world;
                        let mut text = egui::RichText::new(if small { initials_chain(cycle, w) } else { cycle.short(w) }).small();
                        if lm.locked(c) {
                            text = text.strong().color(egui::Color32::from_rgb(140, 200, 255));
                        }
                        ui.horizontal(|ui| {
                            if lm.scope == Some(c) {
                                text = text.underline();
                            }
                            let hover = format!("{}.\nEveryone gains: {}\nClick: watch its strip on the scope.", cycle.describe(w), cycle.gains_text(w));
                            if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(hover).clicked() {
                                clicked = Some(c);
                            }
                            if lm.carries_want(c) {
                                ui.label(egui::RichText::new("WANTED").small().strong().color(GOLD));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        });
    if clicked.is_some() {
        lm.scope = clicked;
    }
    // The scope sits bottom center, clear of the board cam and the rules.
    scope(ctx, &mut lm, 12.0);
    Ok(())
}

/// THE i9 CALLS IT: the trades it latched, the totals, and the AI's lines.
fn i9_call(ui: &mut egui::Ui, lm: &Laundromat, small: bool) {
    let n = lm.n();
    let best = lm.latch.best_bits;
    ui.label(egui::RichText::new("THE i9 CALLS IT").strong().size(16.0));
    for c in qubo::chosen(best, n) {
        let cy = &lm.tc.cycles[c];
        if cy.is_gift() {
            continue;
        }
        let w = &lm.tc.world;
        let (said, gains) = (cy.describe(w), cy.gains_text(w));
        if small {
            ui.label(egui::RichText::new(format!("- {}: +{:.0} Goo", initials_chain(cy, w), cy.goo())).small())
                .on_hover_text(format!("{said}. ({gains})"));
        } else {
            ui.label(egui::RichText::new(format!("- {said}. ({gains})")).small());
        }
    }
    let t = lm.tc.tally(best);
    ui.label(format!("In total: +{:.0} Goo, and nobody loses.", t.goo));
    for (line, karma) in lm.tc.gift_lines(best) {
        ui.label(egui::RichText::new(format!("♥ Gift: {line}. Karma +{karma:.1}")).color(GIFT_EGUI));
    }
    for line in lm.counter_lines() {
        ui.label(egui::RichText::new(line).small().color(COUNTER)).on_hover_text(ESCROW_WHY);
    }
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
    let ai = egui::Color32::from_rgb(255, 150, 220);
    let salty = lm.salty_lines();
    for (brag, roast) in &salty {
        if !brag.is_empty() {
            ui.label(egui::RichText::new(format!("Salties: {brag}")).italics().color(SALT));
        }
        ui.label(egui::RichText::new(format!("AI: \"{roast}\"")).italics().color(ai));
    }
    // A miss on a sabotaged night has its own explanation above.
    let line = if lm.tc.is_optimal(best) {
        Some("AI: \"It's not money laundering, Daddy. It's a Boltzmann machine.\"")
    } else if matches!(lm.sabotage(), None | Some(Sabotage::Emp)) {
        Some("AI: \"You spun it too fast. The strips froze before they could agree.\"")
    } else {
        None
    };
    if let Some(line) = line {
        ui.label(egui::RichText::new(line).italics().color(ai));
    }
}

/// THE SCOPE (Tentzhen's in-game nickname): one strip's qpos over sim time,
/// live from CortenForge, on green phosphor. At a hot drum the strip hops
/// between its wells (Kramers hops); as the drum cools the hops die out.
fn scope(ctx: &egui::Context, lm: &mut Laundromat, at_bottom: f32) {
    let Some(c) = lm.scope else {
        return;
    };
    let phosphor = egui::Color32::from_rgb(110, 255, 140);
    let mut open = true;
    egui::Window::new("THE SCOPE")
        .id("scope".into())
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -at_bottom])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            let w = &lm.tc.world;
            let cycle = &lm.tc.cycles[c];
            ui.label(egui::RichText::new(format!("strip {c}: {}", initials_chain(cycle, w))).monospace().color(phosphor))
                .on_hover_text(format!("{}.", cycle.describe(w)));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(380.0, 150.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, egui::Color32::from_rgb(4, 14, 7));
            let grid = egui::Stroke::new(1.0, egui::Color32::from_rgb(18, 55, 28));
            for k in 1..10 {
                let x = rect.left() + rect.width() * k as f32 / 10.0;
                p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], grid);
            }
            // qpos from -1.8 to 1.8, top is +.
            let y = |q: f32| rect.center().y - (q / 1.8).clamp(-1.0, 1.0) * rect.height() / 2.0;
            let levels = [(1.0, "ON"), (0.0, "barrier"), (-1.0, "off")];
            let level = egui::Color32::from_rgb(40, 110, 60);
            for (q, _) in levels {
                p.line_segment([egui::pos2(rect.left(), y(q)), egui::pos2(rect.right(), y(q))], egui::Stroke::new(1.0, level));
            }
            // Labels go on last, over the trace, at the left edge (the trace fills from the right).
            let labels = |p: &egui::Painter| {
                for (q, label) in levels {
                    let r = p.text(egui::pos2(rect.left() + 4.0, y(q)), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(10.0), level);
                    p.rect_filled(r.expand(2.0), 2.0, egui::Color32::from_black_alpha(200));
                    p.text(egui::pos2(rect.left() + 4.0, y(q)), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(10.0), phosphor);
                }
            };
            let Some(last) = lm.trace.back() else {
                labels(&p);
                return;
            };
            let window = (TRACE_LEN as f64 * TRACE_DT) as f32;
            let x = |t: f64| rect.right() - ((last.t - t) as f32 / window) * rect.width();
            // The drum's kT, faint amber along the bottom.
            let hottest = lm.trace.iter().map(|s| s.kt).fold(0.1, f32::max);
            let heat: Vec<_> = lm.trace.iter().map(|s| egui::pos2(x(s.t), rect.bottom() - 2.0 - s.kt / hottest * rect.height() * 0.3)).collect();
            p.add(egui::Shape::line(heat, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 170, 60, 110))));
            // The strip, with a glow under it.
            let pts: Vec<_> = lm.trace.iter().filter_map(|s| Some(egui::pos2(x(s.t), y(*s.x.get(c)?)))).collect();
            p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(4.0, phosphor.gamma_multiply(0.25))));
            p.add(egui::Shape::line(pts, egui::Stroke::new(1.3, phosphor)));
            labels(&p);
            // Count the hops on screen: well to well, past the barrier.
            let mut side = 0i8;
            let mut hops = 0;
            for s in &lm.trace {
                let q = s.x.get(c).copied().unwrap_or(0.0);
                let now = if q > 0.5 { 1 } else if q < -0.5 { -1 } else { side };
                if side != 0 && now != side {
                    hops += 1;
                }
                side = now;
            }
            ui.label(
                egui::RichText::new(format!("qpos, last {window:.0} sim units   hops: {hops}   drum {:.2} kT", last.kt))
                    .monospace()
                    .small()
                    .color(phosphor),
            )
            .on_hover_text(
                "Each hop is a Kramers hop: thermal noise kicks the strip over the barrier. Hot, it hops all the time; \
                 cooling slowly, it settles into the well the springs favor. Amber is the drum's temperature.",
            );
        });
    if !open {
        lm.scope = None;
    }
}
