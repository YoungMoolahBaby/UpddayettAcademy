//! Suds & Duds on Turk Street: the room, the washer, the slap-bit board on
//! top of it, the i9, and the customers standing around.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use cortenforge::sim::thermostat::WellState;

use super::sim::{Laundromat, Mode};

// ── Layout ─────────────────────────────────────────────────────────────────

const WASHER: Vec3 = Vec3::new(1.3, 1.6, 1.3);
/// Top of the washer's PCB, where the strip clamps stand.
const BOARD_Y: f32 = 1.64;
/// Clamp rails hold the strip ends this high above the board.
const RAIL_H: f32 = 0.2;
const STRIP_LEN: f32 = 0.62;
const STRIP_PITCH: f32 = 0.085;
/// Arch height per unit of `qpos` (a strip in a well sits near |x| = 1).
const ARCH: f32 = 0.1;
const SEGMENTS: usize = 10;
const NPC_RING: f32 = 3.3;
/// Where trade arrows leave and arrive, above each customer's head.
pub const ARROW_Y: f32 = 2.15;

/// Root of everything bolted to the washer, so it all shakes together.
#[derive(Component)]
pub struct WasherRoot;

#[derive(Component)]
pub struct Drum;

#[derive(Component)]
pub struct StripSegment {
    bit: usize,
    k: usize,
}

#[derive(Component)]
pub struct HallLed(usize);

#[derive(Component)]
pub struct I9Led(usize);

/// Floor positions of the customers, in world order.
#[derive(Resource)]
pub struct NpcSpots(pub Vec<Vec3>);

#[derive(Resource)]
pub struct LedMaterials {
    right: Handle<StandardMaterial>,
    left: Handle<StandardMaterial>,
    barrier: Handle<StandardMaterial>,
    i9_on: Handle<StandardMaterial>,
    i9_off: Handle<StandardMaterial>,
}

/// Look per customer: hoodie/coat color, head-wear color.
pub const LOOKS: [(Color, Color); 7] = [
    (Color::srgb(0.35, 0.95, 0.05), Color::srgb(0.1, 0.1, 0.12)), // Upddayett: Mtn Goo green hoodie, black beanie
    (Color::srgb(0.55, 0.33, 0.15), Color::srgb(0.75, 0.2, 0.15)), // Shopping-Cart Guy
    (Color::srgb(0.55, 0.25, 0.75), Color::srgb(0.95, 0.5, 0.8)),  // Vape Lady
    (Color::srgb(0.15, 0.4, 0.85), Color::srgb(0.95, 0.85, 0.1)),  // Bike Kitchen Dave (helmet)
    (Color::srgb(0.45, 0.47, 0.5), Color::srgb(0.6, 0.15, 0.25)),  // Pigeon Lady
    (Color::srgb(0.08, 0.08, 0.1), Color::srgb(0.85, 0.85, 0.9)),  // Ex-Crypto Bro (vest, AirPods-white cap)
    (Color::srgb(0.5, 0.1, 0.15), Color::srgb(0.35, 0.22, 0.12)),  // Librarian Tamara (cardigan, bun)
];

pub fn npc_spots(n: usize) -> Vec<Vec3> {
    (0..n)
        .map(|k| {
            // Upddayett front and center-right; everyone else around the ring.
            let a = 0.35 + TAU * k as f32 / n as f32;
            Vec3::new(NPC_RING * a.sin(), 0.0, NPC_RING * a.cos())
        })
        .collect()
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    lm: Res<Laundromat>,
    mut egui_settings: ResMut<bevy_egui::EguiGlobalSettings>,
) {
    // Main camera: HDR + bloom so the neon, LEDs and locked trades glow. egui
    // goes on this one explicitly, not on whichever camera spawns first.
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::srgb(0.02, 0.02, 0.03)), ..default() },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Transform::from_xyz(0.0, 5.4, 8.6).looking_at(Vec3::new(0.0, 1.35, 0.0), Vec3::Y),
        MainCam,
        bevy_egui::PrimaryEguiContext,
    ));
    // Board cam: a close-up of the slap bits, drawn as an inset.
    commands.spawn((
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::Custom(Color::srgb(0.02, 0.02, 0.03)), ..default() },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Transform::from_xyz(0.55, 2.75, 0.95).looking_at(Vec3::new(0.0, BOARD_Y + 0.1, -0.02), Vec3::Y),
        BoardCam,
    ));

    // ── Room ──
    let tile = meshes.add(Cuboid::new(1.0, 0.02, 1.0));
    let tile_a = mat(&mut mats, Color::srgb(0.78, 0.76, 0.68), 0.35, 0.0);
    let tile_b = mat(&mut mats, Color::srgb(0.18, 0.2, 0.22), 0.35, 0.0);
    for x in -8..8 {
        for z in -6..8 {
            let m = if (x + z) % 2 == 0 { tile_a.clone() } else { tile_b.clone() };
            commands.spawn((Mesh3d(tile.clone()), MeshMaterial3d(m), Transform::from_xyz(x as f32 + 0.5, -0.01, z as f32 + 0.5)));
        }
    }
    let wall = mat(&mut mats, Color::srgb(0.55, 0.62, 0.55), 0.9, 0.0);
    let wall_mesh = meshes.add(Cuboid::new(16.0, 5.0, 0.1));
    commands.spawn((Mesh3d(wall_mesh.clone()), MeshMaterial3d(wall.clone()), Transform::from_xyz(0.0, 2.5, -6.0)));
    let side = meshes.add(Cuboid::new(0.1, 5.0, 14.0));
    for x in [-8.0, 8.0] {
        commands.spawn((Mesh3d(side.clone()), MeshMaterial3d(wall.clone()), Transform::from_xyz(x, 2.5, 1.0)));
    }

    // A row of other machines along the back wall (all "Out of Order").
    let enamel = mat(&mut mats, Color::srgb(0.92, 0.92, 0.9), 0.25, 0.0);
    let chrome = mat(&mut mats, Color::srgb(0.8, 0.8, 0.85), 0.15, 1.0);
    let glass = mats.add(StandardMaterial {
        base_color: Color::srgba(0.1, 0.15, 0.2, 0.55),
        perceptual_roughness: 0.05,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let body = meshes.add(Cuboid::from_size(WASHER));
    let ring = meshes.add(Torus::new(0.36, 0.46));
    let pane = meshes.add(Cylinder::new(0.37, 0.02));
    for k in 0..7 {
        let x = -5.4 + k as f32 * 1.8;
        let base = Vec3::new(x, WASHER.y / 2.0, -5.2);
        commands.spawn((Mesh3d(body.clone()), MeshMaterial3d(enamel.clone()), Transform::from_translation(base)));
        let door = base + Vec3::new(0.0, -0.1, WASHER.z / 2.0 + 0.01);
        commands.spawn((Mesh3d(ring.clone()), MeshMaterial3d(chrome.clone()), Transform::from_translation(door).with_rotation(Quat::from_rotation_x(FRAC_PI_2))));
        commands.spawn((Mesh3d(pane.clone()), MeshMaterial3d(glass.clone()), Transform::from_translation(door).with_rotation(Quat::from_rotation_x(FRAC_PI_2))));
    }

    // Fluorescent tubes with rect lights under them.
    let tube = meshes.add(Cuboid::new(0.12, 0.06, 2.4));
    let tube_glow = glow(&mut mats, LinearRgba::rgb(6.0, 7.0, 6.5));
    for x in [-3.5, 0.0, 3.5] {
        commands.spawn((Mesh3d(tube.clone()), MeshMaterial3d(tube_glow.clone()), Transform::from_xyz(x, 4.6, 0.5)));
        commands.spawn((
            RectLight { color: Color::srgb(0.85, 1.0, 0.9), intensity: 90_000.0, width: 0.3, height: 2.4, range: 20.0 },
            Transform::from_xyz(x, 4.55, 0.5).looking_at(Vec3::new(x, 0.0, 0.5), Vec3::Z),
        ));
    }
    commands.spawn((
        PointLight { intensity: 900_000.0, range: 20.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(1.5, 4.2, 2.5),
    ));
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.8, 0.75), brightness: 120.0, ..default() });

    // ── The washer ──
    let root = commands
        .spawn((WasherRoot, Transform::default(), Visibility::default()))
        .id();
    let paint = mat(&mut mats, Color::srgb(0.95, 0.95, 0.92), 0.2, 0.0);
    let pcb = mat(&mut mats, Color::srgb(0.05, 0.3, 0.12), 0.5, 0.0);
    let steel = mat(&mut mats, Color::srgb(0.72, 0.74, 0.78), 0.25, 1.0);
    let dark = mat(&mut mats, Color::srgb(0.1, 0.1, 0.11), 0.6, 0.3);
    let door_at = Vec3::new(0.0, 0.7, WASHER.z / 2.0 + 0.01);

    commands.entity(root).with_children(|p| {
        p.spawn((Mesh3d(body.clone()), MeshMaterial3d(paint), Transform::from_xyz(0.0, WASHER.y / 2.0, 0.0)));
        p.spawn((Mesh3d(ring.clone()), MeshMaterial3d(chrome.clone()), Transform::from_translation(door_at).with_rotation(Quat::from_rotation_x(FRAC_PI_2))));
        p.spawn((Mesh3d(pane.clone()), MeshMaterial3d(glass.clone()), Transform::from_translation(door_at + Vec3::Z * 0.01).with_rotation(Quat::from_rotation_x(FRAC_PI_2))));

        // Laundry tumbling behind the glass.
        let sock = meshes.add(Cuboid::new(0.16, 0.1, 0.06));
        let colors = [Color::srgb(0.9, 0.2, 0.2), Color::srgb(0.2, 0.5, 0.95), Color::srgb(0.95, 0.85, 0.2), Color::srgb(0.35, 0.95, 0.1), Color::srgb(0.9, 0.9, 0.9)];
        p.spawn((Drum, Transform::from_translation(door_at - Vec3::Z * 0.12), Visibility::default()))
            .with_children(|d| {
                for (k, c) in colors.iter().enumerate() {
                    let a = TAU * k as f32 / colors.len() as f32;
                    d.spawn((
                        Mesh3d(sock.clone()),
                        MeshMaterial3d(mats.add(StandardMaterial { base_color: *c, perceptual_roughness: 0.9, ..default() })),
                        Transform::from_xyz(0.22 * a.cos(), 0.22 * a.sin(), 0.0).with_rotation(Quat::from_rotation_z(a)),
                    ));
                }
            });

        // Slap-bit board: PCB, two clamp rails, a clamp screw per strip end.
        p.spawn((Mesh3d(meshes.add(Cuboid::new(1.25, 0.04, 1.05))), MeshMaterial3d(pcb.clone()), Transform::from_xyz(0.0, BOARD_Y - 0.02, 0.0)));
        let rail = meshes.add(Cuboid::new(1.25, RAIL_H, 0.05));
        let screw = meshes.add(Cylinder::new(0.014, 0.03));
        for z in [-STRIP_LEN / 2.0, STRIP_LEN / 2.0] {
            p.spawn((Mesh3d(rail.clone()), MeshMaterial3d(dark.clone()), Transform::from_xyz(0.0, BOARD_Y + RAIL_H / 2.0, z)));
            for bit in 0..lm.n() {
                p.spawn((Mesh3d(screw.clone()), MeshMaterial3d(steel.clone()), Transform::from_xyz(strip_x(bit, lm.n()), BOARD_Y + RAIL_H + 0.015, z)));
            }
        }
        let seg = meshes.add(Cuboid::new(0.05, 0.006, STRIP_LEN / SEGMENTS as f32 * 1.08));
        let puck = meshes.add(Cylinder::new(0.02, 0.02));
        let led = meshes.add(Sphere::new(0.016));
        for bit in 0..lm.n() {
            for k in 0..SEGMENTS {
                p.spawn((Mesh3d(seg.clone()), MeshMaterial3d(steel.clone()), StripSegment { bit, k }, Transform::default()));
            }
            // The magnet rides the middle of the strip, painted with glow paint so the
            // state reads from across the room; the Hall LED sits at the front edge.
            p.spawn((Mesh3d(puck.clone()), MeshMaterial3d(dark.clone()), StripSegment { bit, k: usize::MAX }, HallLed(bit), Transform::default()));
            p.spawn((Mesh3d(led.clone()), MeshMaterial3d(dark.clone()), HallLed(bit), Transform::from_xyz(strip_x(bit, lm.n()), BOARD_Y + 0.02, 0.46)));
        }

        // The i9, zip-tied to the front panel above the door, with one LED per trade.
        p.spawn((Mesh3d(meshes.add(Cuboid::new(0.68, 0.36, 0.02))), MeshMaterial3d(pcb.clone()), Transform::from_xyz(0.0, 1.32, WASHER.z / 2.0 + 0.012)));
        p.spawn((Mesh3d(meshes.add(Cuboid::new(0.12, 0.12, 0.012))), MeshMaterial3d(dark.clone()), Transform::from_xyz(-0.22, 1.36, WASHER.z / 2.0 + 0.026)));
        let i9_led = meshes.add(Cuboid::new(0.026, 0.026, 0.012));
        for bit in 0..lm.n() {
            let x = -0.12 + (bit % 7) as f32 * 0.05;
            let y = 1.4 - (bit / 7) as f32 * 0.07;
            p.spawn((Mesh3d(i9_led.clone()), MeshMaterial3d(dark.clone()), I9Led(bit), Transform::from_xyz(x + 0.12, y, WASHER.z / 2.0 + 0.026)));
        }
    });

    commands.insert_resource(LedMaterials {
        right: glow(&mut mats, LinearRgba::rgb(0.2, 6.0, 0.4)),
        left: glow(&mut mats, LinearRgba::rgb(0.25, 0.02, 0.02)),
        barrier: glow(&mut mats, LinearRgba::rgb(5.0, 2.5, 0.0)),
        i9_on: glow(&mut mats, LinearRgba::rgb(1.5, 3.0, 9.0)),
        i9_off: glow(&mut mats, LinearRgba::rgb(0.02, 0.03, 0.06)),
    });

    // ── Customers ──
    let spots = npc_spots(lm.tc.world.npcs.len());
    let torso = meshes.add(Capsule3d::new(0.26, 0.8));
    let head = meshes.add(Sphere::new(0.19));
    let hat = meshes.add(Sphere::new(0.2));
    let skin = mat(&mut mats, Color::srgb(0.78, 0.6, 0.47), 0.7, 0.0);
    for (k, spot) in spots.iter().enumerate() {
        let (coat, cap) = LOOKS[k % LOOKS.len()];
        let face = Transform::from_translation(*spot).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
        let coat = mat(&mut mats, coat, 0.8, 0.0);
        let cap = mat(&mut mats, cap, 0.6, 0.0);
        commands.spawn((face, Visibility::default())).with_children(|p| {
            p.spawn((Mesh3d(torso.clone()), MeshMaterial3d(coat), Transform::from_xyz(0.0, 0.67, 0.0)));
            p.spawn((Mesh3d(head.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 1.55, 0.0)));
            p.spawn((
                Mesh3d(hat.clone()),
                MeshMaterial3d(cap),
                Transform::from_xyz(0.0, 1.63, 0.02).with_scale(Vec3::new(1.05, 0.6, 1.05)),
            ));
            match k {
                0 => {
                    // A can of Mtn Goo, obviously.
                    let goo = glow(&mut mats, LinearRgba::rgb(0.6, 3.0, 0.1));
                    p.spawn((Mesh3d(meshes.add(Cylinder::new(0.045, 0.14))), MeshMaterial3d(goo), Transform::from_xyz(0.32, 0.95, -0.12)));
                }
                1 => {
                    // The cart.
                    let wire = mats.add(StandardMaterial { base_color: Color::srgba(0.75, 0.75, 0.8, 0.35), metallic: 1.0, alpha_mode: AlphaMode::Blend, ..default() });
                    p.spawn((Mesh3d(meshes.add(Cuboid::new(0.6, 0.5, 0.8))), MeshMaterial3d(wire), Transform::from_xyz(0.6, 0.6, -0.3)));
                }
                4 => {
                    // A pigeon on her shoulder.
                    let feathers = mat(&mut mats, Color::srgb(0.5, 0.52, 0.6), 0.9, 0.0);
                    p.spawn((Mesh3d(meshes.add(Sphere::new(0.09))), MeshMaterial3d(feathers.clone()), Transform::from_xyz(0.24, 1.38, 0.0).with_scale(Vec3::new(0.8, 0.8, 1.3))));
                    p.spawn((Mesh3d(meshes.add(Sphere::new(0.05))), MeshMaterial3d(feathers), Transform::from_xyz(0.24, 1.47, -0.09)));
                }
                _ => {}
            }
        });
    }
    commands.insert_resource(NpcSpots(spots));
}

fn strip_x(bit: usize, n: usize) -> f32 {
    (bit as f32 - (n as f32 - 1.0) / 2.0) * STRIP_PITCH
}

/// The washer rattles harder (and the drum spins faster) the hotter it runs.
pub fn shake_washer(
    time: Res<Time>,
    lm: Res<Laundromat>,
    mut root: Single<&mut Transform, (With<WasherRoot>, Without<Drum>)>,
    mut drum: Single<&mut Transform, (With<Drum>, Without<WasherRoot>)>,
    mut spin: Local<f32>,
) {
    let t = time.elapsed_secs();
    let temp = lm.temperature() as f32;
    let amp = 0.006 * temp.sqrt();
    root.translation = Vec3::new(
        amp * (t * 61.0).sin() * (t * 7.3).cos(),
        0.5 * amp * (t * 83.0).sin(),
        amp * (t * 47.0 + 1.0).sin() * (t * 5.1).sin(),
    );
    root.rotation = Quat::from_rotation_y(amp * 0.6 * (t * 53.0).sin());
    let rate = if lm.mode == Mode::Done { 0.0 } else { 2.5 * temp.min(6.0) };
    *spin += rate * time.delta_secs();
    drum.rotation = Quat::from_rotation_z(-*spin);
}

/// Bend every strip into the arch its CortenForge particle says it has.
pub fn bend_strips(lm: Res<Laundromat>, mut segs: Query<(&StripSegment, &mut Transform)>) {
    let n = lm.n();
    let pos = lm.machine.positions();
    for (s, mut tf) in &mut segs {
        let x = (pos[s.bit] as f32).clamp(-1.7, 1.7);
        let amp = ARCH * x;
        let px = strip_x(s.bit, n);
        let base = BOARD_Y + RAIL_H;
        if s.k == usize::MAX {
            tf.translation = Vec3::new(px, base + amp + 0.012 * x.signum(), 0.0);
            continue;
        }
        let u = (s.k as f32 + 0.5) / SEGMENTS as f32;
        let z = -STRIP_LEN / 2.0 + u * STRIP_LEN;
        let y = base + amp * (PI * u).sin();
        let slope = amp * PI / STRIP_LEN * (PI * u).cos();
        tf.translation = Vec3::new(px, y, z);
        tf.rotation = Quat::from_rotation_x(-slope.atan());
    }
}

pub fn update_leds(
    lm: Res<Laundromat>,
    leds: Res<LedMaterials>,
    mut hall: Query<(&HallLed, &mut MeshMaterial3d<StandardMaterial>), Without<I9Led>>,
    mut i9: Query<(&I9Led, &mut MeshMaterial3d<StandardMaterial>), Without<HallLed>>,
) {
    for (HallLed(bit), mut m) in &mut hall {
        let want = match lm.well(*bit) {
            WellState::Right => &leds.right,
            WellState::Left => &leds.left,
            WellState::Barrier => &leds.barrier,
        };
        if m.0 != *want {
            m.0 = want.clone();
        }
    }
    for (I9Led(bit), mut m) in &mut i9 {
        let on = lm.latch.has_best() && (lm.latch.best_bits >> bit) & 1 == 1;
        let want = if on { &leds.i9_on } else { &leds.i9_off };
        if m.0 != *want {
            m.0 = want.clone();
        }
    }
}

fn mat(mats: &mut Assets<StandardMaterial>, c: Color, rough: f32, metal: f32) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, metallic: metal, ..default() })
}

fn glow(mats: &mut Assets<StandardMaterial>, c: LinearRgba) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial { base_color: Color::BLACK, emissive: c, ..default() })
}

#[derive(Component)]
pub struct MainCam;

#[derive(Component)]
pub struct BoardCam;

/// Inset rectangle for the board cam, in logical pixels: (left, top, width, height).
pub fn board_cam_rect(window: &Window) -> (f32, f32, f32, f32) {
    let w = (window.width() * 0.3).round();
    let h = (w * 9.0 / 16.0).round();
    (12.0, window.height() - h - 12.0, w, h)
}

/// Keep the board cam pinned to the bottom-left corner as the window resizes.
pub fn place_board_cam(window: Single<&Window, With<bevy::window::PrimaryWindow>>, mut cam: Single<&mut Camera, With<BoardCam>>) {
    let s = window.scale_factor();
    let (x, y, w, h) = board_cam_rect(&window);
    let pos = UVec2::new((x * s) as u32, (y * s) as u32);
    let size = UVec2::new((w * s).max(1.0) as u32, (h * s).max(1.0) as u32);
    if cam.viewport.as_ref().map(|v| (v.physical_position, v.physical_size)) != Some((pos, size)) {
        cam.viewport = Some(bevy::camera::Viewport { physical_position: pos, physical_size: size, ..default() });
    }
}
