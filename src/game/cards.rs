//! Full-screen cards: the "Press Start" title, the "powered by CortenForge"
//! splash, the opening reel of fake commercials (`ads`), and an ad between
//! nights now and then.
//! `UPD_CARDS=1` shoots each card to `shots/card_*.png` and exits.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::ads::{ADS, Stage};
use super::scene::BoardCam;
use super::sim::Laundromat;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Card {
    Title,
    Splash,
    Ad(usize),
}

#[derive(Resource)]
pub struct Cards {
    pub showing: Option<Card>,
    /// When the card went up (seconds since startup).
    since: f32,
    /// The night the last ad check ran on.
    night: u64,
    /// Ads break in between nights (off while screenshots are taken).
    ads: bool,
    /// Playing the opening reel: every ad back to back, before the game.
    reel: bool,
}

impl Cards {
    pub fn new(night: u64) -> Self {
        // Screenshot runs and the render bench skip the title.
        let shooting = super::shots::enabled() || shots_enabled() || std::env::var_os("UPD_BENCH").is_some();
        Cards { showing: if shooting && !shots_enabled() { None } else { Some(Card::Title) }, since: 0.0, night, ads: !shooting, reel: false }
    }
}

/// Run condition: no card is up, so the game's panels draw.
pub fn clear(cards: Res<Cards>) -> bool {
    cards.showing.is_none()
}

/// Seconds the splash stays up (any key or click skips it, or the ad on).
const SPLASH: f32 = 3.0;
/// A key or click this soon after a card goes up is the one that raised it.
const GRACE: f32 = 0.5;

const GOO: egui::Color32 = egui::Color32::from_rgb(150, 255, 90);
const GOO_DARK: egui::Color32 = egui::Color32::from_rgb(20, 70, 25);
/// Weathering steel: CortenForge's namesake.
const CORTEN: egui::Color32 = egui::Color32::from_rgb(190, 90, 40);
const CORTEN_DARK: egui::Color32 = egui::Color32::from_rgb(70, 28, 12);

/// Which ad breaks in when a night begins: every third night, in turn.
fn ad_for(night: u64) -> Option<usize> {
    night.is_multiple_of(3).then_some((night / 3) as usize % ADS.len())
}

/// The largest size up to `max` at which `text` fits in `width`.
pub fn fit(p: &egui::Painter, family: &egui::FontFamily, text: &str, width: f32, max: f32) -> f32 {
    let w = p.layout_no_wrap(text.to_string(), egui::FontId::new(100.0, family.clone()), egui::Color32::WHITE).size().x;
    (100.0 * width / w.max(1.0)).min(max)
}

/// Shout `text` centered at `at`, extruded down-right like a 2003 menu.
pub fn chunky(p: &egui::Painter, family: &egui::FontFamily, at: egui::Pos2, text: &str, size: f32, face: egui::Color32, side: egui::Color32) {
    let font = egui::FontId::new(size, family.clone());
    let depth = (size / 10.0).max(2.0) as i32;
    for d in (1..=depth).rev() {
        p.text(at + egui::vec2(d as f32 * 0.7, d as f32), egui::Align2::CENTER_CENTER, text, font.clone(), side);
    }
    // The default face is thin; without the chunky one, smear it bold.
    let smear: &[f32] = if *family == egui::FontFamily::Proportional { &[-1.5, -0.5, 0.5, 1.5] } else { &[0.0] };
    for &dx in smear {
        p.text(at + egui::vec2(dx * size / 40.0, 0.0), egui::Align2::CENTER_CENTER, text, font.clone(), face);
    }
}

/// Ferris the crab (Rust's mascot, public domain), waving a claw.
fn ferris(p: &egui::Painter, at: egui::Pos2, s: f32, t: f32) {
    let orange = egui::Color32::from_rgb(247, 76, 0);
    let dark = egui::Color32::from_rgb(160, 45, 0);
    let bob = (t * 4.0).sin() * 2.0;
    let at = at + egui::vec2(0.0, bob);
    let leg = egui::Stroke::new(s * 0.06, dark);
    for side in [-1.0f32, 1.0] {
        for k in 0..3 {
            let from = at + egui::vec2(side * s * (0.35 + 0.12 * k as f32), s * 0.1);
            p.line_segment([from, from + egui::vec2(side * s * 0.18, s * 0.32)], leg);
        }
    }
    p.add(egui::Shape::ellipse_filled(at, egui::vec2(s * 0.7, s * 0.36), orange));
    // Claws: the right one waves.
    for side in [-1.0f32, 1.0] {
        let wave = if side > 0.0 { (t * 6.0).sin() * 0.15 } else { 0.0 };
        let arm = at + egui::vec2(side * s * 0.62, -s * 0.15);
        let claw = arm + egui::vec2(side * s * 0.22, -s * (0.3 + wave));
        p.line_segment([arm, claw], egui::Stroke::new(s * 0.08, orange));
        p.circle_filled(claw, s * 0.16, orange);
        p.circle_filled(claw + egui::vec2(0.0, -s * 0.1), s * 0.07, dark);
    }
    for side in [-1.0f32, 1.0] {
        let eye = at + egui::vec2(side * s * 0.16, -s * 0.42);
        p.line_segment([eye + egui::vec2(0.0, s * 0.2), eye], egui::Stroke::new(s * 0.05, dark));
        p.circle_filled(eye, s * 0.1, egui::Color32::WHITE);
        p.circle_filled(eye + egui::vec2(0.0, s * 0.02), s * 0.05, egui::Color32::BLACK);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut contexts: EguiContexts,
    mut cards: ResMut<Cards>,
    lm: Res<Laundromat>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut font: Local<u8>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let now = time.elapsed_secs();

    // Load Arial Black for the chunky type when Windows has it. A new font
    // only takes effect next frame, so it's used from the frame after.
    let family = match *font {
        0 => {
            *font = 1;
            if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/ariblk.ttf") {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("chunky".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
                // Arial Black first, then egui's own faces for what it lacks (♪).
                let mut chain = vec!["chunky".to_string()];
                chain.extend(fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default());
                fonts.families.insert(egui::FontFamily::Name("chunky".into()), chain);
                ctx.set_fonts(fonts);
                *font = 2;
            }
            egui::FontFamily::Proportional
        }
        2 => {
            *font = 3;
            egui::FontFamily::Proportional
        }
        3 => egui::FontFamily::Name("chunky".into()),
        _ => egui::FontFamily::Proportional,
    };

    // A new night may bring a commercial break.
    if lm.night != cards.night {
        cards.night = lm.night;
        if cards.ads
            && cards.showing.is_none()
            && let Some(k) = ad_for(lm.night)
        {
            cards.showing = Some(Card::Ad(k));
            cards.since = now;
        }
    }

    let Some(card) = cards.showing else {
        return Ok(());
    };
    let t = now - cards.since;
    let pressed = keys.get_just_pressed().next().is_some() || mouse.just_pressed(MouseButton::Left);
    // Like the fake ads that open Tropic Thunder: after the splash, the reel
    // plays every ad before the game. Any key skips one ad; Esc skips the reel.
    let next = match card {
        Card::Title if pressed && t > GRACE => Some(Some(Card::Splash)),
        Card::Splash if t > SPLASH || (pressed && t > GRACE) => {
            cards.reel = cards.ads;
            Some(cards.reel.then_some(Card::Ad(0)))
        }
        Card::Ad(_) if keys.just_pressed(KeyCode::Escape) => Some(None),
        Card::Ad(k) if t > ADS[k].secs() || (pressed && t > GRACE) => Some((cards.reel && k + 1 < ADS.len()).then_some(Card::Ad(k + 1))),
        _ => None,
    };
    if let Some(next) = next {
        if next.is_none() {
            cards.reel = false;
        }
        cards.showing = next;
        cards.since = now;
        return Ok(());
    }

    let screen = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("cards")));
    let c = screen.center();
    let h = screen.height();
    let ui_font = |size: f32| egui::FontId::proportional(size);
    match card {
        Card::Title => {
            // The laundromat shows through, dimmed.
            p.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(185));
            chunky(&p, &family, c + egui::vec2(0.0, -h * 0.24), "UPDDAYETT'S", h * 0.07, egui::Color32::WHITE, egui::Color32::from_gray(60));
            chunky(&p, &family, c + egui::vec2(0.0, -h * 0.12), "SCHOOL OF BIDDNESS", h * 0.1, GOO, GOO_DARK);
            let pill = "LESSON 3: MONEY LAUNDERING (LEGALLY)";
            let size = fit(&p, &family, pill, screen.width() * 0.8, h * 0.032);
            let text_w = p.layout_no_wrap(pill.to_string(), egui::FontId::new(size, family.clone()), egui::Color32::WHITE).size().x;
            let lesson = egui::Rect::from_center_size(c + egui::vec2(0.0, h * 0.02), egui::vec2(text_w + size * 1.6, size * 2.2));
            p.rect_filled(lesson, 6.0, egui::Color32::from_rgb(255, 60, 160));
            chunky(&p, &family, lesson.center(), pill, size, egui::Color32::WHITE, egui::Color32::from_rgb(120, 0, 70));
            // PRESS START blinks.
            if (t * 1.6).fract() < 0.65 {
                chunky(&p, &family, c + egui::vec2(0.0, h * 0.2), "PRESS START", h * 0.05, egui::Color32::from_rgb(255, 220, 80), egui::Color32::from_rgb(90, 50, 0));
            }
            p.text(c + egui::vec2(0.0, h * 0.27), egui::Align2::CENTER_CENTER, "any key or click", ui_font(h * 0.02), egui::Color32::from_gray(170));
            p.text(
                screen.left_bottom() + egui::vec2(16.0, -14.0),
                egui::Align2::LEFT_BOTTOM,
                "RATED M: crude humor, mild thermodynamics",
                ui_font(h * 0.018),
                egui::Color32::from_gray(150),
            );
            p.text(
                screen.right_bottom() + egui::vec2(-16.0, -14.0),
                egui::Align2::RIGHT_BOTTOM,
                "streamed live from the Turk St library computer",
                ui_font(h * 0.018),
                egui::Color32::from_gray(150),
            );
        }
        Card::Splash => {
            // Fade in, hold, fade out.
            let a = (t / 0.4).min(1.0).min((SPLASH - t) / 0.4).clamp(0.0, 1.0);
            let fade = |col: egui::Color32| col.gamma_multiply(a);
            p.rect_filled(screen, 0.0, egui::Color32::from_rgb(12, 10, 9));
            p.text(c + egui::vec2(0.0, -h * 0.14), egui::Align2::CENTER_CENTER, "powered by", ui_font(h * 0.035), fade(egui::Color32::from_gray(190)));
            chunky(&p, &family, c + egui::vec2(0.0, -h * 0.04), "CortenForge", h * 0.11, fade(CORTEN), fade(CORTEN_DARK));
            p.text(
                c + egui::vec2(0.0, h * 0.06),
                egui::Align2::CENTER_CENTER,
                "every strip in the drum is a simulated particle, rattled by real thermal noise",
                ui_font(h * 0.022),
                fade(egui::Color32::from_gray(200)),
            );
            if a > 0.5 {
                ferris(&p, c + egui::vec2(0.0, h * 0.22), h * 0.09, t);
            }
            p.text(c + egui::vec2(0.0, h * 0.34), egui::Align2::CENTER_CENTER, "written in Rust", ui_font(h * 0.02), fade(egui::Color32::from_gray(150)));
        }
        Card::Ad(k) => {
            ADS[k].paint(&Stage { p: &p, family: family.clone(), screen }, t);
            let hint = if cards.reel { format!("{} of {}   any key: next   Esc: skip ads", k + 1, ADS.len()) } else { "any key: skip".to_string() };
            p.text(screen.right_top() + egui::vec2(-14.0, 12.0), egui::Align2::RIGHT_TOP, hint, ui_font(h * 0.018), egui::Color32::from_white_alpha(120));
        }
    }
    Ok(())
}

/// The board-cam inset is its own camera, drawn over egui: off while a card is up.
pub fn hide_inset(cards: Res<Cards>, mut cam: Single<&mut Camera, With<BoardCam>>) {
    let on = cards.showing.is_none();
    if cam.is_active != on {
        cam.is_active = on;
    }
}

pub fn shots_enabled() -> bool {
    std::env::var_os("UPD_CARDS").is_some()
}

/// `UPD_CARDS=1`: shoot the title, the splash and every beat of every ad
/// (`shots/card_ad<k>_<beat>.png`), then exit.
pub fn shots(mut commands: Commands, mut cards: ResMut<Cards>, time: Res<Time>, mut frame: Local<u32>, mut exit: MessageWriter<AppExit>) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    *frame += 1;
    let f = *frame;
    // Each shot: (frame to raise it, how far into the card to hold, card, name).
    let mut plan = vec![(1, 0.0, Card::Title, "title".to_string()), (40, 1.2, Card::Splash, "splash".into())];
    let mut at = 70;
    for (k, ad) in ADS.iter().enumerate() {
        let mut start = 0.0;
        for (b, beat) in ad.beats.iter().enumerate() {
            // Late in the beat, once everything has slammed in.
            plan.push((at, start + beat.secs * 0.75, Card::Ad(k), format!("ad{k}_{b}")));
            start += beat.secs;
            at += 30;
        }
    }
    // Hold the current card still, `into` seconds in.
    if let Some((at, into, card, name)) = plan.iter().rev().find(|(at, ..)| f >= *at) {
        cards.showing = Some(*card);
        cards.reel = matches!(card, Card::Ad(_));
        cards.since = time.elapsed_secs() - into;
        // Long enough for the chunky font to load on the first card.
        if f == at + 20 {
            std::fs::create_dir_all("shots").ok();
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/card_{name}.png")));
        }
    }
    if f > at + 30 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ad_every_third_night_in_turn() {
        let ads: Vec<_> = (1..=9).filter_map(ad_for).collect();
        assert_eq!(ads, vec![1, 2, 0]);
        assert!((1..100).filter_map(ad_for).all(|k| k < ADS.len()));
    }
}
