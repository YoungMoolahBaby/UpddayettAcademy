//! Trade arrows between customers: they flicker with the strips while the
//! drum spins and lock into a glowing chain when the i9 calls it.

use std::f32::consts::TAU;

use bevy::prelude::*;

use super::scene::{ARROW_Y, MainCam, NpcSpots};
use super::sim::{Laundromat, Mode};

/// Thin arrows for trades the strips are considering.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TradeArrows;

/// Fat glowing arrows for the trades the i9 locked in.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct LockedArrows;

/// Neon signage.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct Neon;

pub fn configure(mut store: ResMut<GizmoConfigStore>) {
    let (c, _) = store.config_mut::<TradeArrows>();
    c.line.width = 3.0;
    c.depth_bias = -0.95;
    let (c, _) = store.config_mut::<LockedArrows>();
    c.line.width = 7.0;
    c.depth_bias = -0.95;
    let (c, _) = store.config_mut::<Neon>();
    c.line.width = 5.0;
}

const SAMPLES: usize = 24;

/// Quadratic Bezier for leg `from -> to` of trade `c`: arcs up, bows to the
/// right of travel (so A->B and B->A separate), higher for later trades.
fn leg_curve(a: Vec3, b: Vec3, c: usize) -> impl Fn(f32) -> Vec3 {
    let p0 = a + Vec3::Y * ARROW_Y;
    let p2 = b + Vec3::Y * ARROW_Y;
    let chord = p2 - p0;
    let right = chord.cross(Vec3::Y).normalize_or_zero();
    let lift = 0.45 + 0.16 * (c % 5) as f32;
    let bow = 0.3 + 0.07 * (c % 3) as f32;
    let p1 = (p0 + p2) / 2.0 + Vec3::Y * lift + right * bow;
    move |t: f32| {
        let u = 1.0 - t;
        u * u * p0 + 2.0 * u * t * p1 + t * t * p2
    }
}

const GOLD: Color = Color::srgb(1.0, 0.8, 0.2);
/// Gifts: a warmer gold, so a wanted leg still stands out.
const GIFT: Color = Color::srgb(1.0, 0.55, 0.15);

/// Trade colors skip the golds and oranges (wants and gifts use those).
pub fn trade_color(c: usize, n: usize) -> Color {
    Color::hsl(75.0 + 270.0 * c as f32 / n.max(1) as f32, 0.9, 0.6)
}

pub fn draw(
    time: Res<Time>,
    lm: Res<Laundromat>,
    spots: Res<NpcSpots>,
    cam: Single<&GlobalTransform, With<MainCam>>,
    mut thin: Gizmos<TradeArrows>,
    mut fat: Gizmos<LockedArrows>,
) {
    let n = lm.n();
    let t_now = time.elapsed_secs();
    // Hearts face the main camera.
    let face = (cam.right().as_vec3(), cam.up().as_vec3());
    for (c, cycle) in lm.tc.cycles.iter().enumerate() {
        let a = lm.activity[c];
        let gift = cycle.is_gift();
        let base = if gift { GIFT } else { trade_color(c, n) };
        let heart = gift.then_some(face);
        let token_at = (t_now * 0.45 + c as f32 * 0.17).fract();
        for leg in &cycle.legs {
            let f = leg_curve(spots.0[leg.from], spots.0[leg.to], c);
            // The leg that hands the wanted item over is gold.
            let leg_base = if lm.tc.want == Some((leg.to, leg.item)) { GOLD } else { base };
            if lm.locked(c) {
                let pulse = 3.0 + 1.5 * (t_now * 3.0 + c as f32).sin();
                let color = Color::LinearRgba((LinearRgba::from(leg_base) * pulse).with_alpha(1.0));
                draw_leg(&mut fat, &f, color, Some(token_at), heart);
            } else if lm.mode == Mode::Done {
                draw_leg(&mut thin, &f, leg_base.with_alpha(0.03), None, None);
            } else {
                let token = (a > 0.6).then_some(token_at);
                draw_leg(&mut thin, &f, leg_base.with_alpha(0.04 + 0.9 * a * a), token, heart);
            }
        }
    }
}

/// One leg: the arc, an arrowhead, and optionally the item riding along it
/// (a ball, or for a gift a heart facing the camera: `heart` = its right, up).
fn draw_leg<G: GizmoConfigGroup>(g: &mut Gizmos<G>, f: &impl Fn(f32) -> Vec3, color: Color, token: Option<f32>, heart: Option<(Vec3, Vec3)>) {
    let (t0, t1) = (0.07, 0.93);
    g.linestrip((0..=SAMPLES).map(|k| f(t0 + (t1 - t0) * k as f32 / SAMPLES as f32)), color);
    let tip = f(t1);
    let dir = (tip - f(t1 - 0.04)).normalize_or_zero();
    let side = dir.cross(Vec3::Y).normalize_or_zero();
    for s in [-1.0, 1.0] {
        g.line(tip, tip - dir * 0.2 + side * s * 0.09, color);
    }
    let Some(u) = token else { return };
    let at = f(t0 + (t1 - t0) * u);
    match heart {
        None => {
            g.sphere(Isometry3d::from_translation(at), 0.06, color);
        }
        Some((right, up)) => {
            // The classic heart curve, ~0.27 across, riding just above the arc.
            let s = 0.0085;
            let at = at + up * 0.08;
            g.linestrip(
                (0..=32).map(|k| {
                    let t = TAU * k as f32 / 32.0;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    at + (right * x + up * y) * s
                }),
                color,
            );
        }
    }
}

/// "MONEY LAUNDERING" in green neon on the back wall. It's legal. Probably.
pub fn neon(time: Res<Time>, mut g: Gizmos<Neon>) {
    let t = time.elapsed_secs();
    // A flaky tube: mostly on, sometimes stutters.
    let flicker = if (t * 7.0).sin() > 0.97 || ((t * 0.37).fract() > 0.985) { 0.25 } else { 1.0 };
    let green = Color::LinearRgba(LinearRgba::rgb(0.5, 6.0, 0.6) * flicker);
    let pink = Color::LinearRgba(LinearRgba::rgb(6.0, 0.6, 3.5));
    let wall = Isometry3d::from_translation(Vec3::new(0.0, 3.7, -5.9));
    g.text(wall, "MONEY LAUNDERING", 0.55, Vec2::ZERO, green);
    let sub = Isometry3d::from_translation(Vec3::new(0.0, 3.05, -5.9));
    g.text(sub, "(LEGALLY)  -  SUDS & DUDS  -  MARKET ST", 0.22, Vec2::ZERO, pink);
}
