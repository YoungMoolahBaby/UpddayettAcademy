//! The fake commercials, cut like the ones that open Tropic Thunder: a
//! shameless celebrity pitch, hard cuts with a white flash, slam-zoom type,
//! light rays, starburst stickers, a jingle, and fine print read too fast.
//! Each ad is a run of beats; each beat paints itself from the seconds since
//! its cut. Parody products only, and the jokes are ours.

use bevy_egui::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use super::cards::{chunky, fit};

/// Where an ad paints: the painter, the chunky font, the screen.
pub struct Stage<'a> {
    pub p: &'a Painter,
    pub family: FontFamily,
    pub screen: Rect,
}

impl Stage<'_> {
    fn h(&self) -> f32 {
        self.screen.height()
    }
    /// A point `x`, `y` screen heights from the center.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.screen.center() + vec2(x, y) * self.h()
    }
    fn fill(&self, c: Color32) {
        self.p.rect_filled(self.screen, 0.0, c);
    }
}

/// One cut: how long it holds, and how it paints (`t` = seconds since the cut).
pub struct Beat {
    pub secs: f32,
    pub paint: fn(&Stage, f32),
}

/// How long every shot holds, against its written length. The impacts (slams,
/// flashes) stay quick; the holds stretch so every line can be read.
pub const PACE: f32 = 2.5;

impl Beat {
    /// Seconds this beat stays on screen.
    pub fn hold(&self) -> f32 {
        self.secs * PACE
    }
}

pub struct Ad {
    pub beats: &'static [Beat],
}

impl Ad {
    pub fn secs(&self) -> f32 {
        self.beats.iter().map(Beat::hold).sum()
    }

    /// Paint the ad `t` seconds in.
    pub fn paint(&self, st: &Stage, t: f32) {
        let mut start = 0.0;
        for beat in self.beats {
            if t < start + beat.hold() || std::ptr::eq(beat, self.beats.last().unwrap()) {
                let tb = t - start;
                (beat.paint)(st, tb);
                // Every cut lands with a white flash.
                flash(st, tb);
                return;
            }
            start += beat.hold();
        }
    }
}

/// The reel, in order: MTN GOO, SUPER INTELLIGENCE FOR DOGS, the two attack
/// ads back to back (each side on the other), CARTPASS, OKAYZA.
pub const ADS: [Ad; 6] = [
    Ad { beats: &GOO_BEATS },
    Ad { beats: &DOG_BEATS },
    Ad { beats: &ATTACK_PLINKO },
    Ad { beats: &ATTACK_GLORBMAN },
    Ad { beats: &CART_BEATS },
    Ad { beats: &OKAYZA_BEATS },
];

// ── The style kit ──

const GOO: Color32 = Color32::from_rgb(150, 255, 90);
const GOO_DARK: Color32 = Color32::from_rgb(20, 70, 25);
const DOG_BLUE: Color32 = Color32::from_rgb(120, 200, 255);
const DOG_DARK: Color32 = Color32::from_rgb(20, 40, 90);
const CART_GOLD: Color32 = Color32::from_rgb(255, 210, 60);
const CART_DARK: Color32 = Color32::from_rgb(110, 50, 10);
const RED: Color32 = Color32::from_rgb(235, 40, 40);

fn flash(st: &Stage, t: f32) {
    let a = (1.0 - t / 0.12).clamp(0.0, 1.0);
    if a > 0.0 {
        st.fill(Color32::from_white_alpha((a * 230.0) as u8));
    }
}

/// Type that slams in from huge and shakes on impact.
#[allow(clippy::too_many_arguments)]
fn slam(st: &Stage, t: f32, delay: f32, at: Pos2, text: &str, max: f32, face: Color32, side: Color32) {
    let t = t - delay;
    if t < 0.0 {
        return;
    }
    let u = (1.0 - t / 0.16).max(0.0);
    let scale = 1.0 + 2.2 * u * u;
    let shake = if t < 0.4 { (t * 95.0).sin() * max * 0.06 * (1.0 - t / 0.4) } else { 0.0 };
    let size = fit(st.p, &st.family, text, st.screen.width() * 0.9, max) * scale;
    chunky(st.p, &st.family, at + vec2(shake, -shake * 0.5), text, size, face, side);
}

/// Light rays wheeling out from `at`.
fn rays(st: &Stage, at: Pos2, color: Color32, t: f32) {
    let r = st.screen.width();
    for k in 0..16 {
        let a0 = k as f32 / 16.0 * std::f32::consts::TAU + t * 0.6;
        let a1 = a0 + std::f32::consts::TAU / 40.0;
        let pts = vec![at, at + vec2(a0.cos(), a0.sin()) * r, at + vec2(a1.cos(), a1.sin()) * r];
        st.p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }
}

/// A spiky sticker with a word on it, wobbling.
fn starburst(st: &Stage, at: Pos2, text: &str, fill: Color32, t: f32) {
    let r = st.h() * 0.09;
    let wob = (t * 7.0).sin() * 0.08;
    let pts: Vec<_> = (0..24)
        .map(|k| {
            let a = k as f32 / 24.0 * std::f32::consts::TAU + wob;
            let rr = if k % 2 == 0 { r } else { r * 0.72 };
            at + vec2(a.cos(), a.sin()) * rr
        })
        .collect();
    // A fan of wedges (a star isn't convex), then the outline.
    for k in 0..pts.len() {
        st.p.add(Shape::convex_polygon(vec![at, pts[k], pts[(k + 1) % pts.len()]], fill, Stroke::NONE));
    }
    st.p.add(Shape::closed_line(pts, Stroke::new(2.0, Color32::from_black_alpha(120))));
    let size = fit(st.p, &st.family, text, r * 1.25, r * 0.42);
    let galley = st.p.layout_no_wrap(text.to_string(), FontId::new(size, st.family.clone()), Color32::BLACK);
    let pos = at - galley.size() / 2.0;
    st.p.add(egui::epaint::TextShape::new(pos, galley, Color32::BLACK).with_angle_and_anchor(-0.25 + wob, Align2::CENTER_CENTER));
}

/// A rubber stamp, slapped on crooked.
fn stamp(st: &Stage, t: f32, at: Pos2, text: &str) {
    if t < 0.0 {
        return;
    }
    let u = (1.0 - t / 0.12).max(0.0);
    let size = st.h() * 0.05 * (1.0 + 1.5 * u);
    let galley = st.p.layout_no_wrap(text.to_string(), FontId::new(size, st.family.clone()), RED);
    let tilt = -0.12;
    let half = (galley.size() + vec2(size, size * 0.5)) / 2.0;
    let rot = egui::emath::Rot2::from_angle(tilt);
    let corners: Vec<_> = [vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)].iter().map(|c| at + rot * (*c * half)).collect();
    st.p.add(Shape::closed_line(corners, Stroke::new(size * 0.12, RED)));
    st.p.add(egui::epaint::TextShape::new(at - galley.size() / 2.0, galley, RED).with_angle_and_anchor(tilt, Align2::CENTER_CENTER));
}

/// The lower third: who's talking, and what they say.
fn caption(st: &Stage, who: &str, line: &str) {
    let h = st.h();
    let bar = Rect::from_min_max(pos2(st.screen.left(), st.screen.bottom() - h * 0.16), pos2(st.screen.right(), st.screen.bottom() - h * 0.06));
    st.p.rect_filled(bar, 0.0, Color32::from_black_alpha(200));
    let who = st.p.text(bar.left_center() + vec2(h * 0.04, 0.0), Align2::LEFT_CENTER, who, FontId::new(h * 0.03, st.family.clone()), Color32::from_rgb(255, 220, 80));
    st.p.text(pos2(who.right() + h * 0.02, bar.center().y), Align2::LEFT_CENTER, line, FontId::proportional(h * 0.036), Color32::WHITE);
}

/// Fine print, read by the fastest talker alive: a crawl across the bottom.
fn fine_print(st: &Stage, t: f32, text: &str) {
    let h = st.h();
    let galley = st.p.layout_no_wrap(text.to_string(), FontId::proportional(h * 0.02), Color32::from_gray(190));
    let w = galley.size().x;
    let x = st.screen.right() - (t * st.screen.width() * 0.35) % (w + st.screen.width());
    let y = st.screen.bottom() - h * 0.035;
    st.p.rect_filled(Rect::from_min_max(pos2(st.screen.left(), y - h * 0.018), pos2(st.screen.right(), st.screen.bottom())), 0.0, Color32::from_black_alpha(220));
    st.p.galley(pos2(x, y - galley.size().y / 2.0), galley, Color32::from_gray(190));
}

/// "♪ lyrics ♪", each word bouncing on the beat.
fn jingle(st: &Stage, t: f32, at: Pos2, words: &[&str], face: Color32, side: Color32) {
    let size = st.h() * 0.085;
    let font = FontId::new(size, st.family.clone());
    let widths: Vec<f32> = words.iter().map(|w| st.p.layout_no_wrap(w.to_string(), font.clone(), face).size().x + size * 0.35).collect();
    let mut x = at.x - widths.iter().sum::<f32>() / 2.0;
    for (i, (w, ww)) in words.iter().zip(&widths).enumerate() {
        let hop = ((t * 4.0 - i as f32 * 0.5).sin().max(0.0)) * size * 0.35;
        chunky(st.p, &st.family, pos2(x + ww / 2.0, at.y - hop), w, size, face, side);
        x += ww;
    }
    for (dx, k) in [(-0.55, 0.0), (0.55, 1.0)] {
        let bob = ((t * 5.0 + k).sin()) * size * 0.2;
        st.p.text(at + vec2(dx * st.h() * 1.2, bob - size * 0.6), Align2::CENTER_CENTER, "♪", FontId::proportional(size), face);
    }
}

// ── Props ──

fn can(st: &Stage, at: Pos2, s: f32, spin: f32) {
    let w = s * 0.5 * (0.35 + 0.65 * spin.cos().abs());
    let body = Rect::from_center_size(at, vec2(w, s));
    st.p.rect_filled(body, s * 0.06, Color32::from_rgb(60, 200, 50));
    st.p.rect_filled(Rect::from_center_size(at + vec2(0.0, s * 0.25), vec2(w, s * 0.08)), 0.0, GOO_DARK);
    st.p.add(Shape::ellipse_filled(body.center_top(), vec2(w / 2.0, s * 0.05), Color32::from_gray(200)));
    if spin.cos() > 0.3 {
        st.p.text(at + vec2(0.0, -s * 0.12), Align2::CENTER_CENTER, "MTN", FontId::new(s * 0.14, st.family.clone()), Color32::WHITE);
        st.p.text(at + vec2(0.0, s * 0.04), Align2::CENTER_CENTER, "GOO", FontId::new(s * 0.18, st.family.clone()), Color32::WHITE);
    }
}

/// DJ Hyperfocus, in silhouette under a spotlight: shades, chain, can held high.
fn dj(st: &Stage, at: Pos2, s: f32, t: f32) {
    let ink = Color32::from_rgb(25, 10, 35);
    let body = vec![at + vec2(-s * 0.32, s * 0.9), at + vec2(-s * 0.22, s * 0.1), at + vec2(s * 0.22, s * 0.1), at + vec2(s * 0.32, s * 0.9)];
    st.p.add(Shape::convex_polygon(body, ink, Stroke::NONE));
    let head = at + vec2(0.0, -s * 0.12);
    st.p.circle_filled(head, s * 0.2, ink);
    st.p.rect_filled(Rect::from_center_size(head + vec2(0.0, -s * 0.02), vec2(s * 0.34, s * 0.08)), s * 0.03, Color32::BLACK);
    st.p.line_segment([head + vec2(-s * 0.12, -s * 0.04), head + vec2(-s * 0.05, -s * 0.04)], Stroke::new(s * 0.015, Color32::WHITE));
    // The chain.
    let chain: Vec<_> = (0..=12).map(|k| {
        let a = std::f32::consts::PI * (0.15 + 0.7 * k as f32 / 12.0);
        at + vec2(a.cos() * s * 0.18, s * 0.12 + a.sin() * s * 0.2)
    }).collect();
    st.p.add(Shape::line(chain, Stroke::new(s * 0.03, CART_GOLD)));
    // The arm, pumping the can.
    let pump = (t * 6.0).sin() * s * 0.05;
    let hand = at + vec2(s * 0.5, -s * 0.45 + pump);
    st.p.line_segment([at + vec2(s * 0.2, s * 0.18), hand], Stroke::new(s * 0.1, ink));
    can(st, hand + vec2(0.0, -s * 0.12), s * 0.35, 0.0);
}

/// A dog in profile, facing right; the smart one wears glasses and a mortarboard.
fn dog(st: &Stage, at: Pos2, s: f32, smart: bool, t: f32) {
    let fur = Color32::from_rgb(205, 160, 95);
    let dark = Color32::from_rgb(120, 80, 40);
    for dx in [-0.32, -0.18, 0.2, 0.34] {
        st.p.rect_filled(Rect::from_min_size(at + vec2(dx * s, s * 0.12), vec2(s * 0.08, s * 0.3)), 2.0, dark);
    }
    st.p.add(Shape::ellipse_filled(at, vec2(s * 0.48, s * 0.24), fur));
    let wag = (t * 14.0).sin() * s * 0.08;
    st.p.line_segment([at + vec2(-s * 0.44, -s * 0.08), at + vec2(-s * 0.68, -s * 0.32 + wag)], Stroke::new(s * 0.06, fur));
    let head = at + vec2(s * 0.48, -s * 0.3);
    st.p.circle_filled(head, s * 0.21, fur);
    st.p.add(Shape::ellipse_filled(head + vec2(s * 0.2, s * 0.06), vec2(s * 0.14, s * 0.09), fur));
    st.p.circle_filled(head + vec2(s * 0.33, s * 0.03), s * 0.04, Color32::BLACK);
    st.p.add(Shape::ellipse_filled(head + vec2(-s * 0.1, s * 0.02), vec2(s * 0.07, s * 0.16), dark));
    let eye = head + vec2(s * 0.07, -s * 0.05);
    st.p.circle_filled(eye, s * 0.03, Color32::BLACK);
    if smart {
        st.p.circle_stroke(eye, s * 0.065, Stroke::new(s * 0.018, Color32::BLACK));
        st.p.line_segment([eye + vec2(-s * 0.065, 0.0), eye + vec2(-s * 0.16, -s * 0.02)], Stroke::new(s * 0.015, Color32::BLACK));
        let top = head + vec2(0.0, -s * 0.2);
        let board = vec![top + vec2(-s * 0.24, 0.0), top + vec2(0.0, -s * 0.08), top + vec2(s * 0.24, 0.0), top + vec2(0.0, s * 0.08)];
        st.p.add(Shape::convex_polygon(board, Color32::from_rgb(20, 20, 30), Stroke::NONE));
        st.p.line_segment([top, top + vec2(s * 0.2, s * 0.18)], Stroke::new(s * 0.02, CART_GOLD));
    }
}

fn poop(st: &Stage, at: Pos2, s: f32, t: f32) {
    let brown = Color32::from_rgb(110, 70, 30);
    for (k, (w, y)) in [(0.3, 0.0), (0.22, -0.13), (0.14, -0.24)].iter().enumerate() {
        st.p.add(Shape::ellipse_filled(at + vec2(0.0, y * s), vec2(w * s, s * 0.08), if k == 1 { Color32::from_rgb(125, 82, 38) } else { brown }));
    }
    for k in 0..3 {
        let x = (k as f32 - 1.0) * s * 0.18;
        let pts: Vec<_> = (0..10)
            .map(|i| {
                let y = -s * 0.35 - i as f32 * s * 0.05;
                at + vec2(x + ((i as f32 * 0.9 + t * 6.0 + k as f32).sin()) * s * 0.04, y)
            })
            .collect();
        st.p.add(Shape::line(pts, Stroke::new(s * 0.02, Color32::from_rgb(140, 200, 80))));
    }
}

fn toilet(st: &Stage, at: Pos2, s: f32, swirl: Option<f32>) {
    let white = Color32::from_gray(235);
    st.p.rect_filled(Rect::from_center_size(at + vec2(-s * 0.32, -s * 0.35), vec2(s * 0.22, s * 0.5)), 4.0, white);
    st.p.add(Shape::ellipse_filled(at + vec2(0.0, -s * 0.08), vec2(s * 0.38, s * 0.1), white));
    let bowl = vec![at + vec2(-s * 0.36, -s * 0.06), at + vec2(s * 0.36, -s * 0.06), at + vec2(s * 0.16, s * 0.35), at + vec2(-s * 0.16, s * 0.35)];
    st.p.add(Shape::convex_polygon(bowl, white, Stroke::new(1.5, Color32::from_gray(150))));
    st.p.rect_filled(Rect::from_center_size(at + vec2(-s * 0.32, -s * 0.52), vec2(s * 0.1, s * 0.03)), 2.0, Color32::from_gray(150));
    if let Some(t) = swirl {
        for k in 0..3 {
            let a = t * 10.0 + k as f32 * 2.1;
            st.p.circle_stroke(at + vec2(0.0, -s * 0.08), s * (0.08 + 0.08 * k as f32), Stroke::new(s * 0.02, Color32::from_rgb(90, 160, 255).gamma_multiply(0.5 + 0.5 * a.sin().abs())));
        }
    }
}

fn cart(st: &Stage, at: Pos2, s: f32, wheels_drop: f32) {
    let wire = Stroke::new(s * 0.02, Color32::from_gray(200));
    let (tl, tr, br, bl) = (at + vec2(-s * 0.5, -s * 0.3), at + vec2(s * 0.5, -s * 0.3), at + vec2(s * 0.38, s * 0.15), at + vec2(-s * 0.4, s * 0.15));
    for k in 0..=6 {
        let f = k as f32 / 6.0;
        st.p.line_segment([tl.lerp(tr, f), bl.lerp(br, f)], wire);
    }
    for k in 0..=3 {
        let f = k as f32 / 3.0;
        st.p.line_segment([tl.lerp(bl, f), tr.lerp(br, f)], wire);
    }
    st.p.line_segment([tl, tl + vec2(-s * 0.18, -s * 0.12)], Stroke::new(s * 0.04, RED));
    st.p.line_segment([bl, bl + vec2(0.0, s * 0.2)], wire);
    st.p.line_segment([br, br + vec2(0.0, s * 0.2)], wire);
    for x in [bl.x, br.x] {
        let fall = wheels_drop * wheels_drop * st.h() * 2.0;
        st.p.circle_filled(pos2(x, bl.y + s * 0.26 + fall), s * 0.06, Color32::from_gray(40));
    }
}

// ── MTN GOO: the celebrity energy-soda spot ──

const GOO_BEATS: [Beat; 5] = [
    Beat {
        secs: 2.6,
        paint: |st, t| {
            st.fill(Color32::from_rgb(10, 4, 16));
            rays(st, st.at(0.0, -0.6), Color32::from_rgba_unmultiplied(150, 255, 90, 18), t);
            dj(st, st.at(-0.05, -0.05), st.h() * 0.45, t);
            caption(st, "DJ HYPERFOCUS:", "Yo. Can't focus? Can't sleep? Can't stop?");
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(GOO_DARK);
            rays(st, st.at(0.0, 0.0), Color32::from_rgba_unmultiplied(150, 255, 90, 40), t * 3.0);
            can(st, st.at(0.0, 0.08), st.h() * 0.38, t * 9.0);
            slam(st, t, 0.0, st.at(0.0, -0.3), "MTN GOO", st.h() * 0.2, GOO, Color32::BLACK);
            if t > 0.5 {
                starburst(st, st.at(0.42, 0.12), "NEW!", Color32::from_rgb(255, 230, 0), t);
            }
        },
    },
    Beat {
        secs: 2.6,
        paint: |st, t| {
            let pulse = ((t * 8.0).sin() * 0.5 + 0.5) * 40.0;
            st.fill(Color32::from_rgb(20, 60 + pulse as u8, 25));
            jingle(st, t, st.at(0.0, -0.05), &["GET", "THE", "GOO", "IN", "YOU"], GOO, Color32::BLACK);
            caption(st, "CHOIR:", "Get the Goo in you!");
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            rays(st, st.at(0.0, 0.0), Color32::from_rgba_unmultiplied(150, 255, 90, 30), t * 2.0);
            slam(st, t, 0.0, st.at(0.0, -0.08), "NOW 40% GREENER", st.h() * 0.13, GOO, GOO_DARK);
            if t > 0.8 {
                st.p.text(st.at(0.0, 0.08), Align2::CENTER_CENTER, "(than what?)", FontId::proportional(st.h() * 0.035), Color32::from_gray(160));
            }
        },
    },
    Beat {
        secs: 3.0,
        paint: |st, t| {
            st.fill(Color32::from_rgb(8, 22, 10));
            rays(st, st.at(-0.4, 0.0), Color32::from_rgba_unmultiplied(150, 255, 90, 25), t);
            can(st, st.at(-0.4, 0.0), st.h() * 0.4, 0.0);
            slam(st, t, 0.0, st.at(0.2, -0.08), "MTN GOO", st.h() * 0.16, GOO, GOO_DARK);
            st.p.text(st.at(0.2, 0.06), Align2::CENTER_CENTER, "Hyperfocus in a can.", FontId::proportional(st.h() * 0.045), Color32::WHITE);
            fine_print(
                st,
                t,
                "Mtn Goo is not a food. Side effects may include hyperfocus, green teeth, typing very fast and starting four projects at 3 a.m. \
                 Do not operate a washing machine. On Turk St the empty can is worth more than the full one.",
            );
        },
    },
];

// ── SUPER INTELLIGENCE FOR DOGS: the infomercial ──

const DOG_BEATS: [Beat; 6] = [
    Beat {
        secs: 2.4,
        paint: |st, t| {
            // The problem, in sad infomercial gray.
            st.fill(Color32::from_rgb(70, 72, 78));
            st.p.rect_filled(Rect::from_min_max(st.at(-2.0, 0.22), st.at(2.0, 1.0)), 0.0, Color32::from_rgb(110, 100, 90));
            dog(st, st.at(-0.3, 0.1), st.h() * 0.4, false, t * 0.3);
            poop(st, st.at(0.3, 0.24), st.h() * 0.25, t);
            slam(st, t, 0.3, st.at(0.0, -0.3), "TIRED OF THIS?", st.h() * 0.12, Color32::WHITE, RED);
            caption(st, "ANNOUNCER:", "Tired of this?");
        },
    },
    Beat {
        secs: 1.4,
        paint: |st, t| {
            st.fill(DOG_DARK);
            rays(st, st.at(0.0, 0.0), Color32::from_rgba_unmultiplied(120, 200, 255, 35), t * 3.0);
            slam(st, t, 0.0, st.at(0.0, 0.0), "INTRODUCING", st.h() * 0.14, Color32::WHITE, DOG_DARK);
        },
    },
    Beat {
        secs: 2.6,
        paint: |st, t| {
            st.fill(Color32::from_rgb(10, 12, 30));
            rays(st, st.at(0.0, 0.15), Color32::from_rgba_unmultiplied(120, 200, 255, 28), t);
            slam(st, t, 0.0, st.at(0.0, -0.28), "SUPER INTELLIGENCE FOR DOGS", st.h() * 0.13, DOG_BLUE, DOG_DARK);
            dog(st, st.at(-0.05, 0.18), st.h() * 0.4, true, t);
            if t > 0.6 {
                starburst(st, st.at(0.48, 0.12), "GENIUS!", Color32::from_rgb(255, 230, 0), t);
            }
        },
    },
    Beat {
        secs: 2.6,
        paint: |st, t| {
            st.fill(Color32::from_rgb(200, 225, 240));
            toilet(st, st.at(0.15, 0.12), st.h() * 0.5, None);
            dog(st, st.at(0.0, -0.08), st.h() * 0.34, true, t);
            caption(st, "ANNOUNCER:", "So they will stop shitting on the floor.");
        },
    },
    Beat {
        secs: 2.2,
        paint: |st, t| {
            st.fill(Color32::from_rgb(200, 225, 240));
            toilet(st, st.at(0.0, 0.18), st.h() * 0.5, Some(t));
            slam(st, t, 0.0, st.at(0.0, -0.36), "*FLUSH*", st.h() * 0.1, Color32::from_rgb(60, 140, 255), Color32::WHITE);
            slam(st, t, 0.5, st.at(0.0, -0.2), "THEY USE THE TOILET NOW. THEY EVEN FLUSH.", st.h() * 0.06, DOG_DARK, Color32::WHITE);
        },
    },
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::from_rgb(10, 12, 30));
            slam(st, t, 0.0, st.at(0.0, -0.12), "SUPER INTELLIGENCE FOR DOGS", st.h() * 0.12, DOG_BLUE, DOG_DARK);
            st.p.text(st.at(0.0, 0.04), Align2::CENTER_CENTER, "So they will stop shitting on the floor.", FontId::proportional(st.h() * 0.045), Color32::WHITE);
            fine_print(st, t, "Intelligence is free now. Not available for cats (they declined).");
        },
    },
];

// ── CARTPASS: the subscription pitch ──

const CART_BEATS: [Beat; 5] = [
    Beat {
        secs: 2.4,
        paint: |st, t| {
            // Golden-hour lifestyle footage.
            st.fill(Color32::from_rgb(240, 150, 70));
            st.p.circle_filled(st.at(0.45, -0.1), st.h() * 0.18, Color32::from_rgb(255, 220, 120));
            st.p.rect_filled(Rect::from_min_max(st.at(-2.0, 0.2), st.at(2.0, 1.0)), 0.0, Color32::from_rgb(90, 60, 50));
            cart(st, st.at(-0.15 + t * 0.03, 0.02), st.h() * 0.45, 0.0);
            caption(st, "ANNOUNCER:", "You love your shopping cart.");
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(Color32::from_rgb(40, 40, 44));
            slam(st, t, 0.0, st.at(0.0, -0.06), "OWNING THINGS?", st.h() * 0.13, Color32::WHITE, Color32::BLACK);
            stamp(st, t - 0.6, st.at(0.0, 0.12), "SO 2003");
            caption(st, "ANNOUNCER:", "But owning things? Ugh.");
        },
    },
    Beat {
        secs: 2.4,
        paint: |st, t| {
            st.fill(Color32::from_rgb(30, 14, 6));
            rays(st, st.at(0.0, 0.1), Color32::from_rgba_unmultiplied(255, 210, 60, 30), t * 2.0);
            slam(st, t, 0.0, st.at(0.0, -0.28), "CARTPASS", st.h() * 0.2, CART_GOLD, CART_DARK);
            cart(st, st.at(0.0, 0.12), st.h() * 0.4, 0.0);
            if t > 0.5 {
                starburst(st, st.at(0.45, 0.1), "$9.99/MO", Color32::from_rgb(255, 80, 160), t);
            }
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(Color32::from_rgb(30, 14, 6));
            cart(st, st.at(0.0, 0.02), st.h() * 0.45, (t - 0.3).max(0.0));
            stamp(st, t - 0.6, st.at(0.0, -0.3), "WHEELS SOLD SEPARATELY");
        },
    },
    Beat {
        secs: 3.0,
        paint: |st, t| {
            st.fill(Color32::from_rgb(30, 14, 6));
            slam(st, t, 0.0, st.at(0.0, -0.12), "CARTPASS", st.h() * 0.18, CART_GOLD, CART_DARK);
            st.p.text(st.at(0.0, 0.04), Align2::CENTER_CENTER, "The shopping cart. Now a subscription.", FontId::proportional(st.h() * 0.045), Color32::WHITE);
            fine_print(
                st,
                t,
                "$9.99/mo. Wheels sold separately. Cancel anytime by mail, in person, at a location to be announced. \
                 The cart remains the property of CartPass Holdings. So do you.",
            );
        },
    },
];

// ── The attack ads: each side, the same template, the same length ──
//
// Both candidates (PromiseTV's) go after the other for something perfectly
// reasonable, made to sound sinister. Fictional people, no parties, and both
// cut from one template so neither side gets the nicer ad (DESIGN: skewer both
// equally, for behavior, not policy).

const ATTACK_RED: Color32 = Color32::from_rgb(170, 20, 20);

/// A grainy, red-tinted photo of the opponent, the kind attack ads find.
fn mugshot(st: &Stage, at: Pos2, s: f32, t: f32) {
    let frame = Rect::from_center_size(at, vec2(s * 0.8, s));
    st.p.rect_filled(frame, 0.0, Color32::from_gray(60));
    st.p.add(Shape::ellipse_filled(at + vec2(0.0, s * 0.42), vec2(s * 0.36, s * 0.22), Color32::from_gray(25)));
    st.p.circle_filled(at + vec2(0.0, -s * 0.05), s * 0.2, Color32::from_gray(35));
    // Grain, reshuffled twelve times a second.
    let seed = (t * 12.0) as u32;
    for k in 0..220u32 {
        let h = (k.wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)).wrapping_mul(2_246_822_519);
        let x = (h & 0xffff) as f32 / 65_535.0 - 0.5;
        let y = (h >> 16) as f32 / 65_535.0 - 0.5;
        st.p.circle_filled(at + vec2(x * s * 0.8, y * s), 1.2, Color32::from_white_alpha(70));
    }
    st.p.rect_filled(frame, 0.0, Color32::from_rgba_unmultiplied(200, 0, 0, 60));
    st.p.rect_stroke(frame, 0.0, Stroke::new(3.0, Color32::BLACK), egui::StrokeKind::Inside);
}

/// "FACT:" and a source nobody checked.
fn fact(st: &Stage, t: f32, line: &str, source: &str) {
    if t < 0.3 {
        return;
    }
    let tag = st.p.text(st.at(-0.75, 0.12), Align2::LEFT_CENTER, "FACT:", FontId::new(st.h() * 0.05, st.family.clone()), ATTACK_RED);
    st.p.text(pos2(tag.right() + st.h() * 0.02, tag.center().y), Align2::LEFT_CENTER, line, FontId::proportional(st.h() * 0.042), Color32::WHITE);
    st.p.text(st.at(-0.75, 0.19), Align2::LEFT_CENTER, source, FontId::proportional(st.h() * 0.022), Color32::from_gray(150));
}

fn road(st: &Stage) {
    let road = vec![st.at(-0.15, -0.2), st.at(0.15, -0.2), st.at(0.9, 0.5), st.at(-0.9, 0.5)];
    st.p.add(Shape::convex_polygon(road, Color32::from_gray(55), Stroke::NONE));
    for k in 0..5 {
        let f = k as f32 / 5.0;
        let y = -0.18 + f * 0.65;
        st.p.rect_filled(Rect::from_center_size(st.at(0.0, y), vec2(4.0 + f * 10.0, 10.0 + f * 30.0)), 1.0, Color32::from_rgb(220, 190, 40));
    }
    for (x, y, w) in [(-0.3, 0.3, 0.12), (0.25, 0.12, 0.08), (-0.08, 0.02, 0.05), (0.4, 0.38, 0.14)] {
        st.p.add(Shape::ellipse_filled(st.at(x, y), vec2(w, w * 0.35) * st.h(), Color32::from_gray(15)));
    }
}

fn paper_stack(st: &Stage, at: Pos2, s: f32) {
    for k in 0..24 {
        let y = s * 0.5 - k as f32 * s * 0.04;
        let dx = (k as f32 * 1.7).sin() * s * 0.03;
        st.p.rect_filled(Rect::from_center_size(at + vec2(dx, y), vec2(s * 0.7, s * 0.035)), 1.0, Color32::from_gray(if k % 2 == 0 { 235 } else { 210 }));
    }
}

/// The closing card: who paid, and the candidate approving it.
fn paid_for(st: &Stage, t: f32, by: &str, who: &str, line: &str) {
    st.fill(Color32::from_rgb(12, 12, 16));
    slam(st, t, 0.0, st.at(0.0, -0.1), by, st.h() * 0.07, Color32::WHITE, ATTACK_RED);
    caption(st, who, line);
}

const ATTACK_PLINKO: [Beat; 5] = [
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            mugshot(st, st.at(0.4, -0.08), st.h() * 0.55, t);
            slam(st, t, 0.0, st.at(-0.35, -0.12), "COUNCILWOMAN", st.h() * 0.06, Color32::WHITE, ATTACK_RED);
            slam(st, t, 0.2, st.at(-0.35, -0.02), "PLINKO", st.h() * 0.1, Color32::WHITE, ATTACK_RED);
            caption(st, "NARRATOR:", "Councilwoman Plinko says she'll fix the potholes on Turk St.");
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            slam(st, t, 0.0, st.at(0.0, -0.1), "FIX.", st.h() * 0.2, Color32::WHITE, ATTACK_RED);
            slam(st, t, 0.7, st.at(0.0, 0.1), "THEM.", st.h() * 0.2, Color32::WHITE, ATTACK_RED);
        },
    },
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::from_rgb(20, 18, 18));
            road(st);
            st.p.rect_filled(Rect::from_min_max(st.at(-2.0, 0.06), st.at(2.0, 0.24)), 0.0, Color32::from_black_alpha(200));
            fact(st, t, "These potholes have served Turk St for 40 years.", "(Turk St Gazette, probably)");
        },
    },
    Beat {
        secs: 2.4,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            mugshot(st, st.at(0.0, 0.05), st.h() * 0.6, t);
            slam(st, t, 0.0, st.at(0.0, -0.33), "PLINKO: TOO SMOOTH. TOO FAST. TOO FAR.", st.h() * 0.07, Color32::WHITE, ATTACK_RED);
        },
    },
    Beat {
        secs: 2.6,
        paint: |st, t| paid_for(st, t, "PAID FOR BY GLORBMAN FOR WHATEVER", "GLORBMAN:", "I'm Glorbman, and I approved this message."),
    },
];

const ATTACK_GLORBMAN: [Beat; 5] = [
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            mugshot(st, st.at(0.4, -0.08), st.h() * 0.55, t);
            slam(st, t, 0.0, st.at(-0.35, -0.12), "CANDIDATE", st.h() * 0.06, Color32::WHITE, ATTACK_RED);
            slam(st, t, 0.2, st.at(-0.35, -0.02), "GLORBMAN", st.h() * 0.1, Color32::WHITE, ATTACK_RED);
            caption(st, "NARRATOR:", "Glorbman read the bill before he voted on it.");
        },
    },
    Beat {
        secs: 2.0,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            slam(st, t, 0.0, st.at(0.0, -0.1), "ALL", st.h() * 0.2, Color32::WHITE, ATTACK_RED);
            slam(st, t, 0.7, st.at(0.0, 0.1), "400 PAGES.", st.h() * 0.15, Color32::WHITE, ATTACK_RED);
        },
    },
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::from_rgb(20, 18, 18));
            paper_stack(st, st.at(0.0, -0.12), st.h() * 0.6);
            st.p.rect_filled(Rect::from_min_max(st.at(-2.0, 0.06), st.at(2.0, 0.24)), 0.0, Color32::from_black_alpha(200));
            fact(st, t, "What was he looking for? He won't say.*", "*He said \"typos.\"");
        },
    },
    Beat {
        secs: 2.4,
        paint: |st, t| {
            st.fill(Color32::BLACK);
            mugshot(st, st.at(0.0, 0.05), st.h() * 0.6, t);
            slam(st, t, 0.0, st.at(0.0, -0.33), "GLORBMAN: TOO CAREFUL. TOO PREPARED. TOO LITERATE.", st.h() * 0.07, Color32::WHITE, ATTACK_RED);
        },
    },
    Beat {
        secs: 2.6,
        paint: |st, t| paid_for(st, t, "PAID FOR BY PLINKO CAN'T LOSE", "COUNCILWOMAN PLINKO:", "I'm Councilwoman Plinko, and I approved this message."),
    },
];

// ── OKAYZA: the pharma spot ──

const OKAYZA_TEAL: Color32 = Color32::from_rgb(60, 200, 190);
const OKAYZA_DARK: Color32 = Color32::from_rgb(10, 70, 80);

/// One of our capsule people (like the NPCs), smiling or not.
fn person(st: &Stage, at: Pos2, s: f32, coat: Color32, happy: bool) {
    st.p.rect_filled(Rect::from_center_size(at, vec2(s * 0.34, s * 0.7)), s * 0.17, coat);
    let head = at + vec2(0.0, -s * 0.5);
    st.p.circle_filled(head, s * 0.16, Color32::from_rgb(225, 185, 150));
    let mouth: Vec<_> = (0..=8)
        .map(|k| {
            let x = (k as f32 / 8.0 - 0.5) * s * 0.14;
            let curve = (1.0 - (x / (s * 0.07)).powi(2)) * s * 0.03;
            head + vec2(x, s * 0.06 + if happy { curve } else { -curve })
        })
        .collect();
    st.p.add(Shape::line(mouth, Stroke::new(s * 0.015, Color32::BLACK)));
    for dx in [-0.05, 0.05] {
        st.p.circle_filled(head + vec2(dx * s, -s * 0.02), s * 0.015, Color32::BLACK);
    }
}

/// A two-tone capsule, `spin` radians around.
fn capsule(st: &Stage, at: Pos2, s: f32, spin: f32) {
    let rot = egui::emath::Rot2::from_angle(spin);
    for (side, color) in [(-1.0f32, OKAYZA_TEAL), (1.0, Color32::WHITE)] {
        let pts: Vec<_> = (0..=16)
            .map(|k| {
                let a = k as f32 / 16.0 * std::f32::consts::PI;
                at + rot * vec2(side * (s * 0.35 + a.sin() * s * 0.18), -a.cos() * s * 0.18)
            })
            .chain([at + rot * vec2(0.0, s * 0.18), at + rot * vec2(0.0, -s * 0.18)])
            .collect();
        st.p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }
}

fn meadow(st: &Stage) {
    st.fill(Color32::from_rgb(130, 200, 250));
    st.p.circle_filled(st.at(0.6, -0.32), st.h() * 0.1, Color32::from_rgb(255, 235, 120));
    st.p.rect_filled(Rect::from_min_max(st.at(-2.0, 0.15), st.at(2.0, 1.0)), 0.0, Color32::from_rgb(90, 190, 90));
}

/// The side effects, read over the happy footage, one line at a time.
fn side_effects(st: &Stage, t: f32, lines: &[&str]) {
    let k = ((t / (0.8 * PACE)) as usize).min(lines.len() - 1);
    caption(st, "NARRATOR:", lines[k]);
}

const OKAYZA_BEATS: [Beat; 6] = [
    Beat {
        secs: 2.6,
        paint: |st, t| {
            st.fill(Color32::from_rgb(85, 90, 100));
            for k in 0..30 {
                let x = (k as f32 * 0.137).fract() * 2.0 - 1.0;
                let y = (k as f32 * 0.311 + t * 1.5).fract() * 1.2 - 0.6;
                st.p.line_segment([st.at(x, y), st.at(x - 0.01, y + 0.05)], Stroke::new(2.0, Color32::from_rgb(160, 170, 190)));
            }
            person(st, st.at(0.0, 0.05), st.h() * 0.5, Color32::from_gray(120), false);
            caption(st, "NARRATOR:", "Do you suffer from moderate-to-severe Being Fine?");
        },
    },
    Beat {
        secs: 1.8,
        paint: |st, t| {
            st.fill(Color32::from_rgb(40, 44, 52));
            slam(st, t, 0.0, st.at(0.0, -0.08), "MODERATE-TO-SEVERE", st.h() * 0.1, Color32::WHITE, Color32::BLACK);
            slam(st, t, 0.5, st.at(0.0, 0.06), "BEING FINE?", st.h() * 0.12, Color32::WHITE, Color32::BLACK);
        },
    },
    Beat {
        secs: 2.4,
        paint: |st, t| {
            st.fill(OKAYZA_DARK);
            rays(st, st.at(0.0, 0.1), Color32::from_rgba_unmultiplied(60, 200, 190, 30), t);
            slam(st, t, 0.0, st.at(0.0, -0.26), "OKAYZA", st.h() * 0.18, OKAYZA_TEAL, Color32::BLACK);
            st.p.text(st.at(0.0, -0.12), Align2::CENTER_CENTER, "(mehprozine)", FontId::proportional(st.h() * 0.035), Color32::from_gray(200));
            capsule(st, st.at(0.0, 0.1), st.h() * 0.4, t * 2.0);
            caption(st, "NARRATOR:", "Ask your doctor about Okayza.");
        },
    },
    Beat {
        secs: 4.0,
        paint: |st, t| {
            meadow(st);
            let hand = st.at(-0.05, -0.02);
            person(st, st.at(-0.05, 0.2), st.h() * 0.45, OKAYZA_TEAL, true);
            let kite = st.at(0.35 + (t * 1.3).sin() * 0.03, -0.32);
            let k = st.h() * 0.06;
            let diamond = vec![kite + vec2(0.0, -k), kite + vec2(k * 0.7, 0.0), kite + vec2(0.0, k * 1.3), kite + vec2(-k * 0.7, 0.0)];
            st.p.add(Shape::convex_polygon(diamond, Color32::from_rgb(255, 80, 120), Stroke::NONE));
            st.p.line_segment([hand, kite + vec2(0.0, k * 1.3)], Stroke::new(1.0, Color32::WHITE));
            side_effects(
                st,
                t,
                &[
                    "Side effects may include feeling great,",
                    "feeling nothing,",
                    "growing a second, smaller, more successful you,",
                    "uncontrollable pugcasting,",
                    "and sudden fluency in dolphin.",
                ],
            );
        },
    },
    Beat {
        secs: 3.2,
        paint: |st, t| {
            meadow(st);
            person(st, st.at(-0.2, 0.2), st.h() * 0.45, OKAYZA_TEAL, true);
            // The second, smaller, more successful you, in a tiny suit.
            person(st, st.at(0.15, 0.28), st.h() * 0.25, Color32::from_rgb(30, 30, 40), true);
            side_effects(
                st,
                t,
                &[
                    "Death may occur, but in a chill way.",
                    "Do not take Okayza if you are allergic to Okayza.",
                    "Tell your doctor if your doctor is a raccoon.",
                    "Okayza: it's fine.",
                ],
            );
        },
    },
    Beat {
        secs: 2.8,
        paint: |st, t| {
            st.fill(Color32::WHITE);
            capsule(st, st.at(-0.45, -0.02), st.h() * 0.3, -0.4);
            slam(st, t, 0.0, st.at(0.15, -0.08), "OKAYZA", st.h() * 0.16, OKAYZA_TEAL, OKAYZA_DARK);
            st.p.text(st.at(0.15, 0.06), Align2::CENTER_CENTER, "Because \"fine\" is a diagnosis.", FontId::proportional(st.h() * 0.045), Color32::from_gray(40));
            fine_print(
                st,
                t,
                "Okayza is not a drug. Okayza is not available. Results not typical. Results not anything. \
                 Ask your doctor, then ask a second doctor why the first one laughed.",
            );
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Neither side gets the longer or busier ad.
    #[test]
    fn both_sides_get_the_same_ad() {
        assert_eq!(ATTACK_PLINKO.len(), ATTACK_GLORBMAN.len());
        for (a, b) in ATTACK_PLINKO.iter().zip(&ATTACK_GLORBMAN) {
            assert_eq!(a.secs, b.secs);
        }
    }
}
