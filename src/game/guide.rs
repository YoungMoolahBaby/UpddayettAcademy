//! HOW TO PLAY: a picture book about the game, written with the user (PLAN
//! "Next arcs"). F1 or the banner's "How to play" opens it. Every picture is
//! drawn from tonight's real board, so the example on the page is one you
//! can find on the street.
//!
//! `UPD_GUIDE=all|<page>` shoots the pages (`shots/guide_<k>.png`) and exits.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use cortenforge_play::trade::{Cycle, World};

use super::scene::LOOKS;
use super::sim::Laundromat;
use super::ui::{COUNTER, GOLD, GOO_GREEN, c32};

#[derive(Resource, Default)]
pub struct Guide {
    pub open: bool,
    pub page: usize,
}

/// The chapters. Pages with no `draw` yet are the ones we write next.
struct Page {
    title: &'static str,
    draw: Option<fn(&mut egui::Ui, &World, &[Cycle])>,
}

const PAGES: &[Page] = &[
    Page { title: "The street", draw: Some(street) },
    Page { title: "Goo", draw: Some(goo) },
    Page { title: "A trade", draw: Some(trade) },
    Page { title: "Loops", draw: Some(loops) },
    Page { title: "Collisions", draw: Some(collisions) },
    Page { title: "The drum", draw: Some(drum) },
    Page { title: "Wash programs", draw: None },
    Page { title: "The counter", draw: None },
    Page { title: "Gifts and Karma", draw: None },
    Page { title: "Wants and give-aways", draw: None },
    Page { title: "The Salties", draw: None },
    Page { title: "Prints", draw: None },
    Page { title: "Yuck and ghosts", draw: None },
    Page { title: "The TV", draw: None },
    Page { title: "The tape deck", draw: None },
];

const RED: egui::Color32 = egui::Color32::from_rgb(255, 80, 70);
const INK: egui::Color32 = egui::Color32::from_gray(225);
const DIM: egui::Color32 = egui::Color32::from_gray(130);
const PICTURE_H: f32 = 250.0;

pub fn ui(mut contexts: EguiContexts, mut guide: ResMut<Guide>, lm: Res<Laundromat>, keys: Res<ButtonInput<KeyCode>>, window: Single<&Window, With<bevy::window::PrimaryWindow>>) -> Result {
    let ctx = contexts.ctx_mut()?;
    if keys.just_pressed(KeyCode::F1) {
        guide.open = !guide.open;
    }
    if !guide.open {
        return Ok(());
    }
    if keys.just_pressed(KeyCode::Escape) {
        guide.open = false;
    }
    if !ctx.egui_wants_keyboard_input() {
        if keys.just_pressed(KeyCode::ArrowRight) {
            guide.page = (guide.page + 1).min(PAGES.len() - 1);
        }
        if keys.just_pressed(KeyCode::ArrowLeft) {
            guide.page = guide.page.saturating_sub(1);
        }
    }
    let w = (window.width() - 40.0).min(860.0);
    let h = (window.height() - 60.0).min(560.0);
    let mut open = true;
    egui::Window::new("HOW TO PLAY")
        .id("how_to_play".into())
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Tooltip)
        .collapsible(false)
        .resizable(false)
        .fixed_size([w, h])
        .open(&mut open)
        .frame(egui::Frame::window(&ctx.global_style()).fill(egui::Color32::from_rgb(14, 16, 20)))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // Contents down the left.
                ui.vertical(|ui| {
                    ui.set_width(160.0);
                    for (k, p) in PAGES.iter().enumerate() {
                        let mut text = egui::RichText::new(format!("{}. {}", k + 1, p.title));
                        if p.draw.is_none() {
                            text = text.color(DIM).italics();
                        }
                        if ui.selectable_label(guide.page == k, text).clicked() {
                            guide.page = k;
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Grey: we write these next.").small().color(DIM));
                    ui.label(egui::RichText::new("F1 opens and closes. Arrows turn pages.").small().color(DIM));
                });
                ui.separator();
                ui.vertical(|ui| {
                    let page = &PAGES[guide.page];
                    ui.label(egui::RichText::new(page.title).size(22.0).strong().color(GOLD));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(h - 80.0).show(ui, |ui| match page.draw {
                        Some(draw) => draw(ui, &lm.tc.world, &lm.tc.cycles),
                        None => {
                            ui.label(egui::RichText::new("Not written yet. This is one we write together.").italics().color(DIM));
                        }
                    });
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                        ui.horizontal(|ui| {
                            let last = guide.page + 1 == PAGES.len();
                            if ui.add_enabled(!last, egui::Button::new("Next >")).clicked() {
                                guide.page += 1;
                            }
                            ui.label(egui::RichText::new(format!("{} / {}", guide.page + 1, PAGES.len())).color(DIM));
                            if ui.add_enabled(guide.page > 0, egui::Button::new("< Back")).clicked() {
                                guide.page -= 1;
                            }
                        });
                    });
                });
            });
        });
    if !open {
        guide.open = false;
    }
    Ok(())
}

// ---- drawing kit: the same look as the street (portrait rings, chips) ----

fn picture(ui: &mut egui::Ui) -> (egui::Painter, egui::Rect) {
    let size = egui::vec2(ui.available_width(), PICTURE_H);
    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
    painter.rect_filled(resp.rect, 8.0, egui::Color32::from_rgb(24, 27, 33));
    (painter, resp.rect)
}

fn initials(name: &str) -> String {
    name.split([' ', '-']).filter_map(|w| w.chars().next()).take(2).collect()
}

/// A portrait like the ones over the street: coat-colored ring, initials, name.
fn portrait(p: &egui::Painter, at: egui::Pos2, w: &World, k: usize) {
    let coat = c32(LOOKS[k % LOOKS.len()].0);
    p.circle(at, 20.0, egui::Color32::from_black_alpha(220), egui::Stroke::new(3.0, coat));
    p.text(at, egui::Align2::CENTER_CENTER, initials(w.npcs[k].name), egui::FontId::proportional(15.0), egui::Color32::WHITE);
    p.text(at + egui::vec2(0.0, 30.0), egui::Align2::CENTER_CENTER, w.npcs[k].name, egui::FontId::proportional(13.0), INK);
}

fn chip(p: &egui::Painter, center: egui::Pos2, text: &str, color: egui::Color32) {
    let g = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), egui::Color32::BLACK);
    let r = egui::Rect::from_center_size(center, g.size() + egui::vec2(10.0, 4.0));
    p.rect_filled(r, 4.0, color);
    p.galley(r.min + egui::vec2(5.0, 2.0), g, egui::Color32::BLACK);
}

/// A bowed arrow from `a` to `b` with `label` at its middle; `bow` > 0 bends
/// it to the left of the direction of travel.
fn arrow(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, bow: f32, color: egui::Color32, label: &str) {
    let d = (b - a).normalized();
    let (a, b) = (a + d * 26.0, b - d * 26.0);
    let mid = egui::pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0) + egui::vec2(d.y, -d.x) * bow;
    let pts: Vec<egui::Pos2> = (0..=20)
        .map(|i| {
            let t = i as f32 / 20.0;
            let u = 1.0 - t;
            egui::pos2(u * u * a.x + 2.0 * u * t * mid.x + t * t * b.x, u * u * a.y + 2.0 * u * t * mid.y + t * t * b.y)
        })
        .collect();
    let stroke = egui::Stroke::new(2.5, color);
    p.add(egui::Shape::line(pts.clone(), stroke));
    let tip = pts[20];
    let back = (pts[17] - tip).normalized() * 11.0;
    let side = egui::vec2(-back.y, back.x) * 0.5;
    p.add(egui::Shape::convex_polygon(vec![tip, tip + back + side, tip + back - side], color, egui::Stroke::NONE));
    if !label.is_empty() {
        let at = pts[10];
        let g = p.layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), INK);
        let r = egui::Rect::from_center_size(at, g.size() + egui::vec2(8.0, 4.0));
        p.rect_filled(r, 3.0, egui::Color32::from_black_alpha(230));
        p.galley(r.min + egui::vec2(4.0, 2.0), g, INK);
    }
}

fn text(ui: &mut egui::Ui, s: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(s).size(15.0).color(INK));
}

fn note(ui: &mut egui::Ui, s: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(s).small().italics().color(DIM));
}

/// Tonight's best 2-way swap (the clearest example), else any trade.
fn example(cycles: &[Cycle], len: impl Fn(usize) -> bool) -> Option<&Cycle> {
    cycles.iter().filter(|c| !c.is_gift() && len(c.len())).max_by(|a, b| a.goo().total_cmp(&b.goo()))
}

fn ring(rect: egui::Rect, n: usize, k: usize) -> egui::Pos2 {
    let r = egui::vec2(rect.width() * 0.36, rect.height() * 0.34);
    let a = std::f32::consts::TAU * k as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
    rect.center() + egui::vec2(a.cos() * r.x, a.sin() * r.y) - egui::vec2(0.0, 8.0)
}

// ---- the pages ----

fn street(ui: &mut egui::Ui, w: &World, _: &[Cycle]) {
    let (p, rect) = picture(ui);
    let n = w.npcs.len();
    for k in 0..n {
        portrait(&p, ring(rect, n, k), w, k);
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, "the laundromat", egui::FontId::proportional(14.0), DIM);
    text(
        ui,
        "This is the laundromat on Market St. Upddayett runs it. Every night his neighbors come in carrying stuff \
         they don't need much, and needing stuff somebody else has.",
    );
    text(
        ui,
        "Nobody has money. So the laundromat's big washer has been rigged into a trade computer: it figures out who \
         should swap what with whom, so that everybody goes home better off.",
    );
    text(ui, "You run that machine. These pages show you how, one idea at a time.");
    note(ui, "These are tonight's real neighbors. Every picture in this book comes from tonight's real board.");
}

fn goo(ui: &mut egui::Ui, w: &World, cycles: &[Cycle]) {
    let Some(c) = example(cycles, |n| n == 2) else {
        text(ui, "No 2-way swap on the board tonight. Try another night.");
        return;
    };
    let leg = c.legs[0];
    let (owner, to, item) = (leg.from, leg.to, leg.item);
    let (p, rect) = picture(ui);
    let (left, right) = (rect.left_center() + egui::vec2(rect.width() * 0.2, -10.0), rect.right_center() - egui::vec2(rect.width() * 0.2, 10.0));
    portrait(&p, left, w, owner);
    portrait(&p, right, w, to);
    let mid = rect.center() - egui::vec2(0.0, 40.0);
    p.text(mid, egui::Align2::CENTER_CENTER, w.items[item].name, egui::FontId::proportional(16.0), GOLD);
    let (mine, theirs) = (w.value[owner][item], w.value[to][item]);
    chip(&p, left + egui::vec2(0.0, 62.0), &format!("worth {mine:.0} Goo to {}", w.npcs[owner].name), GOO_GREEN);
    chip(&p, right + egui::vec2(0.0, 62.0), &format!("worth {theirs:.0} Goo to {}", w.npcs[to].name), GOO_GREEN);
    p.text(rect.center_bottom() - egui::vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, "same thing, different Goo", egui::FontId::proportional(14.0), INK);
    text(
        ui,
        &format!(
            "Goo is what a thing is worth to one person, tonight. 1 Goo is about a can of Mtn Goo to them. \
             The {} belongs to {}, and to its owner it's worth {mine:.0} Goo. To {} it's worth {theirs:.0}.",
            w.items[item].name, w.npcs[owner].name, w.npcs[to].name
        ),
    );
    text(ui, "That gap is where every trade comes from: the same thing is worth more in somebody else's hands.");
    text(ui, "Goo changes every night. Food is worth a lot more to someone who's hungry; a blanket is worth more on a cold night.");
    note(ui, "On the street, the colored tags under each name (hungry, cold, ...) are tonight's conditions. They set the Goo.");
}

fn trade(ui: &mut egui::Ui, w: &World, cycles: &[Cycle]) {
    let Some(c) = example(cycles, |n| n == 2) else {
        text(ui, "No 2-way swap on the board tonight. Try another night.");
        return;
    };
    let (a, b) = (c.legs[0], c.legs[1]);
    let (p, rect) = picture(ui);
    let (left, right) = (rect.left_center() + egui::vec2(rect.width() * 0.2, -10.0), rect.right_center() - egui::vec2(rect.width() * 0.2, 10.0));
    portrait(&p, left, w, a.from);
    portrait(&p, right, w, b.from);
    arrow(&p, left, right, 45.0, GOLD, w.items[a.item].name);
    arrow(&p, right, left, 45.0, GOLD, w.items[b.item].name);
    chip(&p, right + egui::vec2(0.0, 60.0), &format!("+{:.0} Goo", c.gains[0]), GOO_GREEN);
    chip(&p, left + egui::vec2(0.0, 60.0), &format!("+{:.0} Goo", c.gains[1]), GOO_GREEN);
    text(
        ui,
        &format!(
            "{} gives the {} to {}, and gets the {} back. Each of them ends up with something worth more to them \
             than what they gave up: {}.",
            w.npcs[a.from].name,
            w.items[a.item].name,
            w.npcs[b.from].name,
            w.items[b.item].name,
            c.gains_text(w)
        ),
    );
    text(ui, "The rule: a trade only happens if everyone in it comes out ahead. Nobody loses Goo. Ever.");
    text(ui, &format!("A trade is worth all the Goo everybody gains, added up. This one is worth {:.0} Goo.", c.goo()));
    note(ui, "On the street, a trade the machine is thinking about shows as colored arcs between the people in it.");
}

fn loops(ui: &mut egui::Ui, w: &World, cycles: &[Cycle]) {
    let Some(c) = example(cycles, |n| n >= 3) else {
        text(ui, "No loops on the board tonight. Try another night.");
        return;
    };
    let (p, rect) = picture(ui);
    let n = c.len();
    let at = |k: usize| ring(rect, n, k);
    for (k, leg) in c.legs.iter().enumerate() {
        portrait(&p, at(k), w, leg.from);
    }
    for (k, leg) in c.legs.iter().enumerate() {
        arrow(&p, at(k), at((k + 1) % n), -18.0, GOLD, w.items[leg.item].name);
    }
    text(
        ui,
        "Sometimes two people can't swap directly: you want what I have, but I don't want what you have. \
         So the trade goes around a loop instead. Each person hands one thing to the next and gets one from the one before.",
    );
    text(ui, &format!("Tonight's best loop: {}. Gains: {}.", c.short(w), c.gains_text(w)));
    text(ui, "Loops go up to 4 people. Longer ones break too easily (the counter page explains why).");
}

fn collisions(ui: &mut egui::Ui, w: &World, cycles: &[Cycle]) {
    // Two good trades that want the same thing.
    let pair = cycles.iter().enumerate().filter(|(_, c)| !c.is_gift()).find_map(|(i, a)| {
        cycles[i + 1..]
            .iter()
            .filter(|b| !b.is_gift() && a.conflicts(b))
            .max_by(|x, y| x.goo().total_cmp(&y.goo()))
            .map(|b| (a, b))
    });
    let Some((a, b)) = pair else {
        text(ui, "Nothing collides tonight. Lucky night.");
        return;
    };
    let shared = a.legs.iter().map(|l| l.item).find(|i| b.legs.iter().any(|l| l.item == *i));
    let (p, rect) = picture(ui);
    let row = |p: &egui::Painter, y: f32, c: &Cycle, label: &str| {
        let x0 = rect.left() + 20.0;
        p.text(egui::pos2(x0, y - 26.0), egui::Align2::LEFT_CENTER, format!("{label}: {} Goo", c.goo().round()), egui::FontId::proportional(13.0), DIM);
        let mut x = x0;
        for l in &c.legs {
            let hot = Some(l.item) == shared;
            let txt = format!("{} > {}", w.items[l.item].name, initials(w.npcs[l.to].name));
            let g = p.layout_no_wrap(txt, egui::FontId::proportional(13.0), egui::Color32::BLACK);
            let r = egui::Rect::from_min_size(egui::pos2(x, y - 10.0), g.size() + egui::vec2(12.0, 6.0));
            p.rect_filled(r, 4.0, if hot { RED } else { GOO_GREEN });
            p.galley(r.min + egui::vec2(6.0, 3.0), g, egui::Color32::BLACK);
            x = r.right() + 8.0;
        }
    };
    row(&p, rect.top() + 70.0, a, "trade A");
    row(&p, rect.top() + 170.0, b, "trade B");
    p.text(rect.center() + egui::vec2(0.0, -5.0), egui::Align2::CENTER_CENTER, "can't both happen", egui::FontId::proportional(15.0), RED);
    let item = shared.map(|i| w.items[i].name).unwrap_or("the same thing");
    text(
        ui,
        &format!("Both of these trades need the {item}. There's only one {item}, so at most one of them can happen."),
    );
    let trades = cycles.iter().filter(|c| !c.is_gift()).count();
    text(
        ui,
        &format!(
            "Tonight there are {trades} possible trades, and lots of them collide like this. The job is to pick the set of \
             trades that don't collide and add up to the most Goo."
        ),
    );
    text(
        ui,
        "That's a hard puzzle: every trade you add can knock out others, and the number of sets to check doubles with \
         each trade. Checking them all would take longer than the night. So we don't check them all. We let physics find one.",
    );
}

fn drum(ui: &mut egui::Ui, _: &World, cycles: &[Cycle]) {
    let (p, rect) = picture(ui);
    // Left: one strip's two wells, with a ball that is the strip.
    let well = egui::Rect::from_min_size(rect.min + egui::vec2(20.0, 30.0), egui::vec2(rect.width() * 0.42, 170.0));
    let curve: Vec<egui::Pos2> = (0..=60)
        .map(|i| {
            let x = -1.4 + 2.8 * i as f32 / 60.0;
            let v = (x * x - 1.0).powi(2) - 0.15 * x;
            egui::pos2(well.left() + (x + 1.4) / 2.8 * well.width(), well.top() + 133.0 - v * 100.0)
        })
        .collect();
    p.add(egui::Shape::line(curve, egui::Stroke::new(2.5, INK)));
    let y = |x: f32| well.top() + 133.0 - ((x * x - 1.0).powi(2) - 0.15 * x) * 100.0;
    let sx = |x: f32| well.left() + (x + 1.4) / 2.8 * well.width();
    p.circle_filled(egui::pos2(sx(1.0), y(1.0) - 9.0), 9.0, GOO_GREEN);
    p.circle_stroke(egui::pos2(sx(-1.0), y(-1.0) - 9.0), 9.0, egui::Stroke::new(1.5, DIM));
    p.text(egui::pos2(sx(-1.0), y(-1.0) + 18.0), egui::Align2::CENTER_CENTER, "off", egui::FontId::proportional(13.0), DIM);
    p.text(egui::pos2(sx(1.0), y(1.0) + 18.0), egui::Align2::CENTER_CENTER, "on", egui::FontId::proportional(13.0), GOO_GREEN);
    p.text(egui::pos2(sx(0.0), y(0.0) - 16.0), egui::Align2::CENTER_CENTER, "the hump", egui::FontId::proportional(12.0), DIM);
    p.text(well.center_bottom() + egui::vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, "one strip = one trade", egui::FontId::proportional(13.0), INK);
    // Right: a few strips, with pulls and pushes.
    let x0 = rect.left() + rect.width() * 0.55;
    let strips = 6;
    let gap = (rect.right() - 20.0 - x0) / strips as f32;
    let on = [true, false, true, false, false, true];
    let base = rect.top() + 170.0;
    for (k, on) in on.iter().enumerate() {
        let x = x0 + gap * (k as f32 + 0.5);
        let top = if *on { base - 110.0 } else { base - 40.0 };
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x - 8.0, top), egui::pos2(x + 8.0, base)), 3.0, if *on { GOO_GREEN } else { egui::Color32::from_gray(70) });
    }
    // A push between two colliding strips.
    let (xa, xb) = (x0 + gap * 0.5, x0 + gap * 1.5);
    p.line_segment([egui::pos2(xa + 10.0, base - 60.0), egui::pos2(xb - 10.0, base - 45.0)], egui::Stroke::new(2.0, RED));
    p.text(egui::pos2((xa + xb) / 2.0, base + 16.0), egui::Align2::CENTER_CENTER, "collide: push apart", egui::FontId::proportional(12.0), RED);
    p.text(egui::pos2(x0 + gap * 4.0, rect.top() + 30.0), egui::Align2::CENTER_CENTER, "green = trade on", egui::FontId::proportional(13.0), GOO_GREEN);
    let bits = cycles.len();
    text(
        ui,
        &format!(
            "Inside the drum, every possible trade is a little magnetic strip ({bits} tonight). Each strip can rest in one of \
             two dips: off, or on. In between is a hump."
        ),
    );
    text(
        ui,
        "Good trades tilt their strip toward on: the more Goo, the stronger the tilt. Trades that collide are tied \
         together so they push each other apart: if one goes on, the other gets shoved off.",
    );
    text(
        ui,
        "Then the drum spins. Spinning shakes the strips with heat, so they hop over their humps, try things, and \
         settle. As the drum slows down and cools, the strips stop hopping, and they freeze into a set of trades that \
         don't collide and make a lot of Goo.",
    );
    text(
        ui,
        "That's real physics, not a trick: CortenForge simulates every strip, the shaking and all, frame by frame. \
         The board cam (bottom left) shows the strips live.",
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("When it stops:").strong().color(COUNTER));
        ui.label("the green strips are tonight's trades. The counter hands them out.");
    });
}

/// `UPD_GUIDE=all|<page>`: open the book, shoot the pages, exit.
pub fn shots_enabled() -> bool {
    std::env::var_os("UPD_GUIDE").is_some()
}

pub fn shots(mut commands: Commands, mut guide: ResMut<Guide>, mut frame: Local<u32>, mut exit: MessageWriter<AppExit>) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let pages: Vec<usize> = match std::env::var("UPD_GUIDE").as_deref() {
        Ok("all") | Ok("1") | Err(_) => (0..PAGES.len()).filter(|k| PAGES[*k].draw.is_some()).collect(),
        Ok(s) => s.split(',').filter_map(|k| k.parse::<usize>().ok()).map(|k| k.clamp(1, PAGES.len()) - 1).collect(),
    };
    *frame += 1;
    // Let the scene come up behind the book, then a page every 20 frames.
    let f = *frame as usize;
    const START: usize = 90;
    if f < START {
        return;
    }
    let i = (f - START) / 20;
    let phase = (f - START) % 20;
    match pages.get(i) {
        Some(k) if phase == 0 => {
            guide.open = true;
            guide.page = *k;
        }
        Some(k) if phase == 10 => {
            std::fs::create_dir_all("shots").ok();
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/guide_{}.png", k + 1)));
        }
        Some(_) => {}
        None if phase == 15 => {
            exit.write(AppExit::Success);
        }
        None => {}
    }
}
