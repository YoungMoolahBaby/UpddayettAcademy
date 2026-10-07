//! The drum tumbler behind the porthole (Step 5): tonight's trade items
//! tumble in a `cf_design` drum, stepped by sim-core on a worker thread
//! (`cortenforge_play::trade::drum`). The meshes come from the same Solids
//! the physics collides, and the drum's speed follows the wash program.

use std::f32::consts::FRAC_PI_2;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use cortenforge::cf_design::{AttributedMesh, Solid};
use cortenforge_play::trade::drum::{self, Tumbler};
use cortenforge_play::trade::print;
use nalgebra::Vector3;

use super::scene::WasherRoot;
use super::sim::{Laundromat, Mode};

/// Scene metres per drum millimetre: the washer is drawn ~1.7x real size.
const SCALE: f32 = 1.7 / 1000.0;
/// Drum center in the washer's frame: the lip sits just behind the door.
const CENTER: Vec3 = Vec3::new(0.0, 0.7, 0.64 - (drum::HALF_DEPTH + drum::WALL) as f32 * SCALE);
/// How fast the motor changes speed (rad/s^2).
const RAMP: f64 = 8.0;
/// The final spin: how long it holds the pinning speed once the cycle ends.
const FINAL_SPIN: f32 = 4.0;

/// Shared between the game and the worker.
#[derive(Default)]
struct Shared {
    ready: bool,
    failed: Option<String>,
    /// What the game asks for: drum speed (rad/s) and which items are in.
    target: f64,
    items: Vec<usize>,
    items_gen: u64,
    /// What the worker last stepped to.
    angle: f64,
    omega: f64,
    pinned: bool,
    poses: Vec<Option<(Vec3, Quat)>>,
}

#[derive(Resource)]
pub struct DrumView {
    shared: Arc<Mutex<Shared>>,
    /// Each body's item name (the world's items, then the print catalog).
    names: Vec<&'static str>,
    items: Vec<usize>,
    done_at: Option<f32>,
}

impl DrumView {
    /// The model is built (it takes ~12 s at startup).
    pub fn ready(&self) -> bool {
        self.shared.lock().map(|s| s.ready || s.failed.is_some()).unwrap_or(true)
    }
}

/// The spinning part of the drum (its mesh), under the drum frame.
#[derive(Component)]
pub struct Spinner;

/// Item `k` of the world, under the drum frame.
#[derive(Component)]
pub struct ItemBody(usize);

/// Starts the worker (it builds the model off the main thread) and spawns
/// the drum and every item's mesh behind the porthole.
pub fn setup(
    mut commands: Commands,
    lm: Res<Laundromat>,
    root: Single<Entity, With<WasherRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    // One body per world item, plus one per print in Upddayett's catalog
    // (built now: the model takes ~12 s, so a print can't wait for one).
    let names: Vec<&'static str> = lm.tc.world.items.iter().map(|i| i.name).chain(print::CATALOG.iter().map(|p| p.item)).collect();
    let bodies = names.clone();
    let shared = Arc::new(Mutex::new(Shared { poses: vec![None; names.len()], ..default() }));
    let worker = shared.clone();
    std::thread::Builder::new()
        .name("drum".into())
        .spawn(move || run(&bodies, &worker))
        .expect("drum thread");

    let steel = mats.add(StandardMaterial {
        base_color: Color::srgb(0.75, 0.77, 0.8),
        metallic: 1.0,
        perceptual_roughness: 0.3,
        ..default()
    });
    // The drum's axle is the physics y axis and its door faces -y; in the
    // scene the door faces +z, so the frame turns y onto -z.
    let frame = Transform::from_translation(CENTER)
        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
        .with_scale(Vec3::splat(SCALE));
    let t = Instant::now();
    let drum_mesh = meshes.add(to_mesh(&drum::drum_solid().mesh(4.0)));
    let items: Vec<_> = names
        .iter()
        .map(|&name| {
            let look = drum::item_look(name);
            let mesh = meshes.add(to_mesh(&look.solid.mesh(tolerance(&look.solid))));
            let [r, g, b] = look.color;
            let mat = mats.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() });
            (mesh, mat)
        })
        .collect();
    info!("drum: meshed the drum and {} items in {:.2} s", items.len(), t.elapsed().as_secs_f32());
    commands.entity(*root).with_children(|p| {
        p.spawn((frame, Visibility::default())).with_children(|f| {
            f.spawn((Spinner, Mesh3d(drum_mesh), MeshMaterial3d(steel), Transform::default()));
            for (k, (mesh, mat)) in items.into_iter().enumerate() {
                f.spawn((ItemBody(k), Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden));
            }
        });
        // A lamp in the drum, like the real ones, so the porthole isn't a cave.
        p.spawn((
            PointLight { intensity: 60_000.0, range: 1.5, radius: 0.05, ..default() },
            Transform::from_translation(CENTER + Vec3::new(0.0, 0.3, 0.25)),
        ));
    });
    commands.insert_resource(DrumView { shared, names, items: vec![], done_at: None });
}

/// Mesh tolerance for an item: a quarter of its thinnest side, 0.7-4 mm.
fn tolerance(s: &Solid) -> f64 {
    s.bounds().map_or(3.0, |b| {
        let e = b.size();
        (e.x.min(e.y).min(e.z) / 4.0).clamp(0.7, 4.0)
    })
}

pub(super) fn to_mesh(m: &AttributedMesh) -> Mesh {
    let pos: Vec<[f32; 3]> = m.geometry.vertices.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
    let idx: Vec<u32> = m.geometry.faces.iter().flatten().copied().collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_indices(Indices::U32(idx));
    match &m.normals {
        Some(n) => mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, n.iter().map(|v| [v.x as f32, v.y as f32, v.z as f32]).collect::<Vec<_>>()),
        None => mesh.compute_smooth_normals(),
    }
    mesh
}

/// The washer's cabinet in scene metres: a box (`size`, standing on the
/// floor) with a round cavity behind the door at `door` for the drum.
pub fn cabinet_mesh(size: Vec3, door: Vec3) -> Mesh {
    let h = size.as_dvec3() / 2.0;
    let cavity = ((drum::R + drum::WALL) as f64 * SCALE as f64) + 0.012;
    let deep = 0.3;
    let s = Solid::cuboid(Vector3::new(h.x, h.y, h.z))
        .subtract(Solid::cylinder(cavity, deep).translate(Vector3::new(door.x as f64, door.y as f64 - h.y, h.z - deep + 0.02)))
        .translate(Vector3::new(0.0, h.y, 0.0));
    to_mesh(&s.mesh_adaptive(0.01))
}

/// The worker: builds the model, then steps it in real time.
fn run(names: &[&str], shared: &Mutex<Shared>) {
    let t = Instant::now();
    let mut tb = match Tumbler::new(names) {
        Ok(tb) => tb,
        Err(e) => {
            error!("drum: no tumbler ({e})");
            if let Ok(mut s) = shared.lock() {
                s.failed = Some(e.to_string());
            }
            return;
        }
    };
    info!("drum: built the tumbler ({} item bodies) in {:.1} s", names.len(), t.elapsed().as_secs_f64());
    let mut generation = u64::MAX;
    let mut omega = 0.0f64;
    let mut clock = Instant::now();
    let mut owed = 0.0f64;
    if let Ok(mut s) = shared.lock() {
        s.ready = true;
    }
    loop {
        let (target, items) = {
            let Ok(s) = shared.lock() else { return };
            let items = (s.items_gen != generation).then(|| (s.items_gen, s.items.clone()));
            (s.target, items)
        };
        if let Some((g, items)) = items {
            tb.set_items(&items);
            generation = g;
        }
        let now = Instant::now();
        // Falling behind slows the drum down rather than piling up work.
        owed = (owed + (now - clock).as_secs_f64()).min(0.05);
        clock = now;
        while owed >= drum::DT {
            omega += (target - omega).clamp(-RAMP * drum::DT, RAMP * drum::DT);
            if let Err(e) = tb.step(omega) {
                error!("drum: step failed ({e:?}); stopping the tumbler");
                return;
            }
            owed -= drum::DT;
        }
        if let Ok(mut s) = shared.lock() {
            s.angle = tb.drum_angle();
            s.omega = omega;
            s.pinned = tb.pinned();
            for k in 0..s.poses.len() {
                s.poses[k] = tb.pose(k).map(|(p, r)| {
                    (Vec3::new(p.x as f32, p.y as f32, p.z as f32), Quat::from_xyzw(r.i as f32, r.j as f32, r.k as f32, r.w as f32))
                });
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// Tonight's drum load: the items in the most trades on the board (not the
/// answer: that would give it away before the spin), as drum bodies. A
/// fresh print always goes in: Upddayett wants to see it tumble.
fn load(lm: &Laundromat, names: &[&str]) -> Vec<usize> {
    let w = &lm.tc.world;
    let mut count = vec![0usize; w.items.len()];
    for c in lm.tc.cycles.iter().take(lm.n()) {
        for leg in &c.legs {
            count[leg.item] += 1;
        }
    }
    let fresh = |i: usize| print::CATALOG.iter().any(|p| p.item == w.items[i].name);
    let mut items: Vec<usize> = (0..count.len()).filter(|&i| (count[i] > 0 || fresh(i)) && !w.items[i].held).collect();
    items.sort_by_key(|&i| (!fresh(i), std::cmp::Reverse(count[i]), i));
    items.truncate(drum::MAX_ITEMS);
    items.into_iter().filter_map(|i| names.iter().position(|&n| n == w.items[i].name)).collect()
}

/// The drum's speed for the wash program right now: a tumble from ~0.25x
/// critical (cold) to 0.8x (hot), stopped when the power's off, and a final
/// spin to 2x once the i9 has its answer.
fn target(lm: &Laundromat, view: &mut DrumView, now: f32) -> f64 {
    let crit = drum::critical();
    if lm.mode == Mode::Done {
        let at = *view.done_at.get_or_insert(now);
        return if now - at < FINAL_SPIN { 2.0 * crit } else { 0.0 };
    }
    view.done_at = None;
    let temp = lm.temperature();
    if temp < 0.02 {
        return 0.0;
    }
    (0.2 + 0.15 * temp).min(0.8) * crit
}

pub fn update(
    time: Res<Time>,
    lm: Res<Laundromat>,
    mut view: ResMut<DrumView>,
    mut spinner: Single<&mut Transform, (With<Spinner>, Without<ItemBody>)>,
    mut bodies: Query<(&ItemBody, &mut Transform, &mut Visibility), Without<Spinner>>,
) {
    let items = load(&lm, &view.names);
    let target = target(&lm, &mut view, time.elapsed_secs());
    let changed = items != view.items;
    if changed {
        view.items = items.clone();
    }
    let Ok(mut s) = view.shared.lock() else { return };
    s.target = target;
    if changed {
        s.items = items;
        s.items_gen = s.items_gen.wrapping_add(1);
    }
    spinner.rotation = Quat::from_rotation_y(s.angle as f32);
    for (b, mut tf, mut vis) in &mut bodies {
        match s.poses.get(b.0).copied().flatten() {
            Some((p, r)) => {
                tf.translation = p;
                tf.rotation = r;
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}
