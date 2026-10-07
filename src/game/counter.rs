//! The folding counter is the escrow. When a cycle starts, every item on the
//! market flies onto it (a small crate in its owner's color); when the drum
//! stops, the counter hands each chosen trade over whole and sends the rest
//! home. The settlement itself is `trade::escrow`, which checks that nothing
//! is duplicated or lost.

use bevy::prelude::*;
use cortenforge_play::trade::escrow;

use super::scene::{LOOKS, NpcSpots};
use super::sim::{Laundromat, Mode};

/// Where the counter stands, and how it's turned (toward the main camera).
const COUNTER: Vec3 = Vec3::new(-1.95, 0.0, -0.15);
const COUNTER_YAW: f32 = 0.25;
const TOP_Y: f32 = 0.82;
const TOP: Vec2 = Vec2::new(1.2, 0.7);
const CRATE: f32 = 0.14;
/// Crates per row on the counter.
const ROW: usize = 6;
/// Where the battery's cells sit while they run the drum (beside the washer).
const BATTERY: Vec3 = Vec3::new(0.85, CRATE / 2.0, 0.55);
/// Seconds per flight, and the delay between one crate and the next.
const FLIGHT: f32 = 0.8;
const STAGGER: f32 = 0.03;

/// One item's crate, flying from `from` to `to` starting at `t0`.
#[derive(Component)]
pub struct Crate {
    item: usize,
    from: Vec3,
    to: Vec3,
    t0: f32,
}

fn counter_frame() -> Transform {
    Transform::from_translation(COUNTER).with_rotation(Quat::from_rotation_y(COUNTER_YAW))
}

/// The folding table, the cardboard sign, and a crate per item at its owner's feet.
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    lm: Res<Laundromat>,
    spots: Res<NpcSpots>,
) {
    let laminate = mats.add(StandardMaterial { base_color: Color::srgb(0.86, 0.82, 0.7), perceptual_roughness: 0.6, ..default() });
    let steel = mats.add(StandardMaterial { base_color: Color::srgb(0.25, 0.25, 0.27), metallic: 1.0, perceptual_roughness: 0.4, ..default() });
    let cardboard = mats.add(StandardMaterial { base_color: Color::srgb(0.62, 0.47, 0.3), perceptual_roughness: 0.95, ..default() });
    let leg = meshes.add(Cylinder::new(0.015, TOP_Y));
    commands.spawn((counter_frame(), Visibility::default())).with_children(|p| {
        p.spawn((Mesh3d(meshes.add(Cuboid::new(TOP.x, 0.04, TOP.y))), MeshMaterial3d(laminate), Transform::from_xyz(0.0, TOP_Y - 0.02, 0.0)));
        for (sx, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let at = Vec3::new(sx * (TOP.x / 2.0 - 0.06), TOP_Y / 2.0, sz * (TOP.y / 2.0 - 0.06));
            p.spawn((Mesh3d(leg.clone()), MeshMaterial3d(steel.clone()), Transform::from_translation(at)));
        }
        // A folded cardboard sign at the back edge; the word is drawn by `draw_sign`.
        p.spawn((
            Mesh3d(meshes.add(Cuboid::new(0.5, 0.16, 0.01))),
            MeshMaterial3d(cardboard),
            Transform::from_xyz(0.0, TOP_Y + 0.08, -TOP.y / 2.0 + 0.03).with_rotation(Quat::from_rotation_x(-0.25)),
        ));
    });

    let w = &lm.tc.world;
    let cube = meshes.add(Cuboid::new(CRATE, CRATE, CRATE));
    let paint: Vec<_> = LOOKS.iter().map(|(coat, _)| mats.add(StandardMaterial { base_color: *coat, perceptual_roughness: 0.85, ..default() })).collect();
    let owners: Vec<usize> = w.items.iter().map(|it| it.owner).collect();
    for (item, it) in w.items.iter().enumerate() {
        let at = home(&spots.0, &owners, item);
        commands.spawn((
            Crate { item, from: at, to: at, t0: 0.0 },
            Mesh3d(cube.clone()),
            MeshMaterial3d(paint[it.owner % paint.len()].clone()),
            Transform::from_translation(at),
        ));
    }
    // A spare crate for tonight's print (Upddayett's, and always the item
    // after the world's own), hidden until he prints one.
    let upd = w.find_npc("upddayett").unwrap_or(0);
    let at = home(&spots.0, &owners, owners.len() - 1);
    commands.spawn((
        Crate { item: w.items.len(), from: at, to: at, t0: 0.0 },
        Mesh3d(cube),
        MeshMaterial3d(paint[upd % paint.len()].clone()),
        Transform::from_translation(at),
        Visibility::Hidden,
    ));
}

/// Slot `item` beside whoever holds it in `holder`: a stack on the floor at
/// their left, a second stack behind it past four.
fn home(spots: &[Vec3], holder: &[usize], item: usize) -> Vec3 {
    let who = holder[item];
    let spot = spots[who];
    let forward = (-spot).normalize_or_zero();
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let j = holder[..item].iter().filter(|&&h| h == who).count();
    let (col, level) = (j / 4, j % 4);
    spot - right * (0.45 + 0.16 * col as f32) + forward * 0.1 + Vec3::Y * (CRATE / 2.0 + level as f32 * (CRATE + 0.005))
}

/// Slot `k` on the counter top.
fn on_counter(k: usize) -> Vec3 {
    let (col, row) = (k % ROW, k / ROW);
    let pitch = Vec2::new((TOP.x - 0.16) / (ROW - 1) as f32, 0.17);
    let local = Vec3::new(-(TOP.x - 0.16) / 2.0 + col as f32 * pitch.x, TOP_Y + CRATE / 2.0, -0.2 + row as f32 * pitch.y);
    counter_frame().transform_point(local)
}

/// Fly every crate to where it belongs right now: home before a cycle, on
/// the counter while the drum spins, and with its new holder (per the
/// counter's settlement) once it stops.
pub fn fly(time: Res<Time>, lm: Res<Laundromat>, spots: Res<NpcSpots>, mut crates: Query<(&mut Crate, &mut Transform, &mut Visibility)>) {
    let now = time.elapsed_secs();
    let w = &lm.tc.world;
    let owners: Vec<usize> = w.items.iter().map(|it| it.owner).collect();
    let holder = match (lm.mode, &lm.settled) {
        (Mode::Done, Some(s)) => s.owner.clone(),
        _ => owners.clone(),
    };
    // Counter slots go to escrowed items only, in item order.
    let mut slot = 0;
    let mut slots = vec![0; w.items.len()];
    for (i, s) in slots.iter_mut().enumerate() {
        if escrow::escrowed(w, i) {
            *s = slot;
            slot += 1;
        }
    }
    let mut n = 0;
    for (mut c, mut tf, mut vis) in &mut crates {
        let i = c.item;
        // The print's crate, on a night without a print.
        if i >= w.items.len() {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        n += 1;
        let target = if !escrow::escrowed(w, i) {
            BATTERY
        } else if matches!(lm.mode, Mode::Cycle { .. }) {
            on_counter(slots[i])
        } else {
            home(&spots.0, &holder, i)
        };
        if target.distance(c.to) > 1e-3 {
            c.from = tf.translation;
            c.to = target;
            c.t0 = now + STAGGER * i as f32;
        }
        let u = ((now - c.t0) / FLIGHT).clamp(0.0, 1.0);
        let e = u * u * (3.0 - 2.0 * u);
        let lift = (0.35 + 0.12 * c.from.distance(c.to)) * 4.0 * u * (1.0 - u);
        tf.translation = c.from.lerp(c.to, e) + Vec3::Y * lift;
        tf.rotation = Quat::from_rotation_y(std::f32::consts::TAU * e);
    }
    assert_eq!(n, w.items.len(), "one crate per item, no more, no less");
}

/// The cardboard sign's word.
pub fn draw_sign(mut g: Gizmos) {
    let f = counter_frame();
    let at = f.transform_point(Vec3::new(0.0, TOP_Y + 0.08, -TOP.y / 2.0 + 0.045));
    let rot = f.rotation * Quat::from_rotation_x(-0.25);
    g.text(Isometry3d::new(at, rot), "ESCROW", 0.07, Vec2::ZERO, Color::srgb(0.08, 0.06, 0.05));
}
