//! Upddayett's printer (Step 7.3): pick a part from his catalog, let the i9
//! check his first draft (it won't print, and the check says why), fix it,
//! check again, and print it. The part grows on the printer's bed, layer by
//! layer, then joins tonight's board as his item (`Laundromat::add_print`)
//! and tumbles in the drum as the same Solid.
//!
//! A check takes 0.3-30 s (`print::check`, PLAN 7.1), so it runs on a
//! worker thread, which also builds the display meshes.

use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_egui::egui;
use cortenforge::cf_design::{IndexedMesh, Solid};
use cortenforge_play::trade::print::{self, CATALOG, PrintReport};
use nalgebra::Vector3;

use super::sim::{Laundromat, Mode};

/// Scene metres per print millimetre (the scene is drawn ~1.7x real size).
const SCALE: f32 = 1.7 / 1000.0;
/// Where the printer stands: on a stool in front of our washer, between the
/// customers (low enough not to hide the porthole from the camera).
const PRINTER: Vec3 = Vec3::new(0.15, 0.0, 2.2);
const STOOL_H: f32 = 0.55;
/// The bed's top, above the stool.
const BED_Y: f32 = 0.09;
/// A print plays out over this many real seconds (at 1x watch speed).
pub const PRINT_SECS: f32 = 10.0;

pub const PLA_ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 140, 40);

/// Where the shop is with tonight's print.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Idle,
    /// The i9 is checking draft 1 (`fixed` false) or draft 2.
    Checking { fixed: bool },
    /// Draft 1 won't print ([`PrintShop::v1`] says why).
    Rejected,
    /// Draft 2 prints; waiting for the go.
    Approved,
    /// Printing: `t` real seconds in.
    Printing { t: f32 },
    Done,
}

/// One checked draft, as the panel shows it.
pub struct Draft {
    pub report: PrintReport,
    /// The kit's parts on the bed, ready to render.
    meshes: Vec<(Mesh, Vec3)>,
}

#[derive(Resource)]
pub struct PrintShop {
    /// The catalog part picked in the panel.
    pub pick: usize,
    pub stage: Stage,
    pub v1: Option<Draft>,
    pub v2: Option<Draft>,
    /// The STLs written when it printed.
    pub stl: Vec<String>,
    job: Option<Arc<Mutex<Option<Draft>>>>,
    night: u64,
}

impl PrintShop {
    /// Ask the i9 to check a draft of the picked part.
    pub fn check(&mut self, fixed: bool) {
        let slot = Arc::new(Mutex::new(None));
        let out = slot.clone();
        let k = self.pick;
        std::thread::Builder::new()
            .name("print check".into())
            .spawn(move || {
                let report = print::check(&CATALOG[k].design(fixed), print::TOL);
                let meshes = layout(&report);
                if let Ok(mut s) = out.lock() {
                    *s = Some(Draft { report, meshes });
                }
            })
            .expect("print check thread");
        self.job = Some(slot);
        self.stage = Stage::Checking { fixed };
    }

    /// Pick part `k` and check his first draft (shot mode starts here).
    pub fn start(&mut self, k: usize) {
        self.reset(k);
        self.check(false);
    }

    /// Start over with part `k` (a new pick or a new night).
    fn reset(&mut self, k: usize) {
        self.pick = k;
        self.stage = Stage::Idle;
        self.v1 = None;
        self.v2 = None;
        self.stl.clear();
        self.job = None;
    }
}

/// The printer prop's moving bits.
#[derive(Component)]
pub struct Nozzle;
#[derive(Component)]
pub struct Gantry;
/// A printed part on (or beside) the bed: kit part `.0`.
#[derive(Component)]
pub struct PrintedPart(usize);

/// The printer: a stool, a frame, a bed and a gantry with its nozzle, all
/// cf-design CSG in metres.
pub fn setup(mut commands: Commands, lm: Res<Laundromat>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(PrintShop { pick: 0, stage: Stage::Idle, v1: None, v2: None, stl: vec![], job: None, night: lm.night });
    let v = |x: f32, y: f32, z: f32| Vector3::new(x as f64, y as f64, z as f64);
    let bx = |x: f32, y: f32, z: f32, at: Vector3<f64>| Solid::cuboid(v(x / 2.0, y / 2.0, z / 2.0)).translate(at);
    let mesh = |s: Solid| super::drum::to_mesh(&s.mesh(0.004));
    let paint = |mats: &mut Assets<StandardMaterial>, c: Color, metal: f32| mats.add(StandardMaterial { base_color: c, metallic: metal, perceptual_roughness: 0.45, ..default() });
    // A shop stool: a round seat on four legs.
    let mut stool = Solid::cylinder(0.24, 0.02).translate(v(0.0, 0.0, STOOL_H - 0.02));
    for (x, z) in [(0.15, 0.15), (-0.15, 0.15), (0.15, -0.15), (-0.15, -0.15)] {
        stool = stool.union(bx(0.03, 0.03, STOOL_H - 0.04, v(x, z, (STOOL_H - 0.04) / 2.0)));
    }
    // The frame (z up in the Solid; the entity turns it y up): a base, two
    // towers, and a top rail.
    let w = 0.42;
    let frame = bx(w, w, 0.06, v(0.0, 0.0, 0.03))
        .union(bx(0.035, 0.035, 0.5, v(-w / 2.0 + 0.02, -0.12, 0.28)))
        .union(bx(0.035, 0.035, 0.5, v(w / 2.0 - 0.02, -0.12, 0.28)))
        .union(bx(w, 0.035, 0.035, v(0.0, -0.12, 0.52)));
    let bed = bx(0.36, 0.36, 0.012, v(0.0, 0.0, BED_Y - 0.006));
    let gantry = bx(w - 0.04, 0.025, 0.025, v(0.0, 0.0, 0.0));
    let nozzle = bx(0.045, 0.05, 0.05, v(0.0, 0.0, 0.025)).union(Solid::cone(0.012, 0.02).translate(v(0.0, 0.0, 0.0)));
    let frame_mat = paint(&mut mats, Color::srgb(0.12, 0.12, 0.13), 0.3);
    let bed_mat = paint(&mut mats, Color::srgb(0.75, 0.75, 0.7), 0.8);
    let head_mat = paint(&mut mats, Color::srgb(0.85, 0.25, 0.1), 0.2);
    let wood = paint(&mut mats, Color::srgb(0.45, 0.32, 0.2), 0.0);
    let up = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    commands.spawn((Transform::from_translation(PRINTER).with_rotation(Quat::from_rotation_y(-0.5)), Visibility::default())).with_children(|p| {
        p.spawn((Mesh3d(meshes.add(mesh(stool))), MeshMaterial3d(wood), Transform::from_rotation(up)));
        p.spawn((Transform::from_xyz(0.0, STOOL_H, 0.0), Visibility::default())).with_children(|p| {
            p.spawn((Mesh3d(meshes.add(mesh(frame))), MeshMaterial3d(frame_mat.clone()), Transform::from_rotation(up)));
            p.spawn((Mesh3d(meshes.add(mesh(bed))), MeshMaterial3d(bed_mat), Transform::from_rotation(up)));
            p.spawn((Gantry, Mesh3d(meshes.add(mesh(gantry))), MeshMaterial3d(frame_mat), Transform::from_xyz(0.0, 0.3, 0.0).with_rotation(up)));
            p.spawn((Nozzle, Mesh3d(meshes.add(mesh(nozzle))), MeshMaterial3d(head_mat), Transform::from_xyz(0.0, 0.3, 0.0).with_rotation(up)));
        });
    });
}

/// Each kit part as a render mesh (z up, mm), and where it sits: on the bed
/// while it prints, then to the side once done. Runs on the worker.
fn layout(report: &PrintReport) -> Vec<(Mesh, Vec3)> {
    report
        .parts
        .iter()
        .map(|p| {
            let (lo, hi) = bounds(&p.mesh);
            // Centered on the bed in x and y, standing on z = 0.
            let c = (lo + hi) / 2.0;
            (render_mesh(&p.mesh), Vec3::new(-c.x as f32, -c.y as f32, -lo.z as f32))
        })
        .collect()
}

fn bounds(m: &IndexedMesh) -> (Vector3<f64>, Vector3<f64>) {
    m.vertices.iter().fold((Vector3::repeat(f64::MAX), Vector3::repeat(f64::MIN)), |(lo, hi), p| (lo.inf(&p.coords), hi.sup(&p.coords)))
}

/// Flat-shaded: prints have hard edges.
fn render_mesh(m: &IndexedMesh) -> Mesh {
    let pos: Vec<[f32; 3]> = m.vertices.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
    let idx: Vec<u32> = m.faces.iter().flatten().copied().collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_indices(Indices::U32(idx));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

/// Polls the check, runs the print, and grows the part on the bed.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    time: Res<Time>,
    mut lm: ResMut<Laundromat>,
    mut shop: ResMut<PrintShop>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut heads: ParamSet<(Query<&mut Transform, With<Nozzle>>, Query<&mut Transform, With<Gantry>>)>,
    mut parts: Query<(Entity, &PrintedPart, &mut Transform, &mut Visibility), (Without<Nozzle>, Without<Gantry>)>,
) {
    // A new night: the shop starts over (one print a night).
    if lm.night != shop.night {
        shop.night = lm.night;
        let k = shop.pick;
        shop.reset(k);
        for (e, ..) in &parts {
            commands.entity(e).despawn();
        }
    }
    // A finished check.
    let finished = shop.job.as_ref().and_then(|j| j.lock().ok().and_then(|mut s| s.take()));
    if let (Some(draft), Stage::Checking { fixed }) = (finished, shop.stage) {
        shop.job = None;
        let prints = draft.report.prints();
        info!(
            "print check: {} draft {} {} in {:.1} s{}",
            CATALOG[shop.pick].item,
            if fixed { 2 } else { 1 },
            if prints { "prints" } else { "won't print" },
            draft.report.secs,
            if prints { String::new() } else { format!(": {}", draft.report.short().join("; ")) }
        );
        if fixed {
            shop.v2 = Some(draft);
            shop.stage = if prints { Stage::Approved } else { Stage::Rejected };
        } else {
            shop.v1 = Some(draft);
            shop.stage = if prints { Stage::Approved } else { Stage::Rejected };
        }
    }
    let shop = &mut *shop;
    let Stage::Printing { t } = shop.stage else { return };
    let Some(v2) = shop.v2.as_mut() else { return };
    // The parts appear on the bed when printing starts.
    if parts.is_empty() {
        let [r, g, b] = CATALOG[shop.pick].color;
        let mat = mats.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.55, ..default() });
        for (k, (mesh, offset)) in v2.meshes.drain(..).enumerate() {
            let m = meshes.add(mesh);
            commands.spawn((
                PrintedPart(k),
                Transform::from_translation(PRINTER + Vec3::new(0.0, STOOL_H + BED_Y, 0.0)).with_scale(Vec3::new(1.0, 0.0, 1.0)),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(m),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(SCALE)) * Transform::from_translation(offset),
                ));
            });
        }
        return;
    }
    // Parts print one after another; each takes its share of the time by height.
    let report = &v2.report;
    let heights: Vec<f32> = report.parts.iter().map(|p| p.size.z as f32).collect();
    let total: f32 = heights.iter().sum::<f32>().max(1.0);
    let t = t + time.delta_secs() * lm.watch as f32;
    let mut left = t / PRINT_SECS * total;
    let mut head = (0.3, 0.0);
    for (_, part, mut tf, mut vis) in &mut parts {
        let h = heights.get(part.0).copied().unwrap_or(1.0);
        let done = (left / h).clamp(0.0, 1.0);
        let printing = left > 0.0 && done < 1.0;
        let finished_before = left >= h;
        left -= h;
        if done <= 0.0 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        tf.scale.y = done.max(0.001);
        // Finished parts step off the bed to the stool's edge, in a row.
        let base = PRINTER + Vec3::new(0.0, STOOL_H + BED_Y, 0.0);
        tf.translation = if finished_before && part.0 + 1 < heights.len() { base + Vec3::new(-0.42 - 0.25 * part.0 as f32, -BED_Y - STOOL_H, 0.25) } else { base };
        if printing {
            head = (BED_Y + h * done * SCALE + 0.03, (t * 7.0).sin() * 0.12);
        }
    }
    for mut tf in &mut heads.p0() {
        tf.translation = Vec3::new(head.1, head.0, (t * 2.3).sin() * 0.1);
    }
    for mut tf in &mut heads.p1() {
        tf.translation.y = head.0 + 0.04;
    }
    if t >= PRINT_SECS {
        if matches!(lm.mode, Mode::Cycle { .. }) {
            // The board can't change mid-spin: it joins when the drum stops.
            shop.stage = Stage::Printing { t: PRINT_SECS };
            return;
        }
        let key = CATALOG[shop.pick].key;
        shop.stl = match print::write_stls(&v2.report, key, std::path::Path::new("prints")) {
            Ok(paths) => paths.iter().map(|p| p.display().to_string()).collect(),
            Err(e) => vec![format!("(STL not written: {e})")],
        };
        shop.stage = Stage::Done;
        let k = shop.pick;
        lm.add_print(k);
    } else {
        shop.stage = Stage::Printing { t };
    }
}

/// The PRINT section of the left panel.
pub fn panel(ui: &mut egui::Ui, shop: &mut PrintShop, lm: &Laundromat, small: bool) {
    let spinning = matches!(lm.mode, Mode::Cycle { .. });
    ui.label(egui::RichText::new("UPDDAYETT PRINTS...").strong().color(PLA_ORANGE));
    let busy = matches!(shop.stage, Stage::Checking { .. } | Stage::Printing { .. });
    let printed = lm.printed;
    let mut pick = shop.pick;
    ui.add_enabled_ui(!busy && printed.is_none(), |ui| {
        egui::ComboBox::from_id_salt("print_what").width(if small { 150.0 } else { 220.0 }).selected_text(CATALOG[pick].item).show_ui(ui, |ui| {
            for (k, p) in CATALOG.iter().enumerate() {
                let who: Vec<String> = p.wants.iter().map(|(n, g, _)| format!("{n} ({g:.0} Goo)")).collect();
                ui.selectable_value(&mut pick, k, p.item).on_hover_text(format!("wanted by {}", who.join(", ")));
            }
        });
    });
    if pick != shop.pick {
        shop.reset(pick);
    }
    let p = &CATALOG[shop.pick];
    let flaw_lines = |ui: &mut egui::Ui, d: &Draft| {
        for line in d.report.short().iter().take(3) {
            ui.label(egui::RichText::new(format!("- {line}")).small().color(egui::Color32::from_rgb(255, 120, 100)));
        }
    };
    match shop.stage {
        Stage::Idle => {
            if ui
                .add_enabled(printed.is_none(), egui::Button::new("Check his draft"))
                .on_hover_text("The i9 meshes Upddayett's design (cf-design) and checks it for the printer (mesh-printability): overhangs, bridges, thin walls, the bed size.")
                .clicked()
            {
                shop.check(false);
            }
            if let Some(k) = printed {
                ui.small(format!("One print a night: the {} is already on the board.", CATALOG[k].item));
            } else if !small {
                ui.small("He designs it in code; CortenForge says if it prints.");
            }
        }
        Stage::Checking { fixed } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.small(format!("The i9 slices draft {}...", if fixed { 2 } else { 1 }));
            });
        }
        Stage::Rejected => {
            let (n, d) = if shop.v2.is_some() { (2, shop.v2.as_ref()) } else { (1, shop.v1.as_ref()) };
            ui.label(egui::RichText::new(format!("Draft {n} WON'T PRINT:")).strong().color(egui::Color32::from_rgb(255, 120, 100)));
            if let Some(d) = d {
                flaw_lines(ui, d);
            }
            if n == 1 {
                ui.label(egui::RichText::new(format!("Why: {}.", p.flaw)).small());
                if ui.button("Fix it and check again").on_hover_text(format!("Draft 2: {}.", p.fix)).clicked() {
                    shop.check(true);
                }
            }
        }
        Stage::Approved => {
            if let Some(d) = &shop.v2 {
                ui.label(egui::RichText::new("Draft 2 PRINTS.").strong().color(egui::Color32::from_rgb(120, 230, 120)));
                ui.label(egui::RichText::new(format!("The fix: {}.", p.fix)).small());
                ui.small(format!(
                    "{} part{}, {:.0} cm3 of PLA, about {:.1} h on a real printer.",
                    d.report.parts.len(),
                    if d.report.parts.len() == 1 { "" } else { "s" },
                    d.report.plastic_cm3(),
                    d.report.hours()
                ));
                if ui.add_enabled(!spinning, egui::Button::new("Print it")).on_hover_text("It joins tonight's board as his item.").clicked() {
                    shop.stage = Stage::Printing { t: 0.0 };
                }
            }
        }
        Stage::Printing { t } => {
            ui.add(egui::ProgressBar::new((t / PRINT_SECS).min(1.0)).text("printing, layer by layer"));
            if t >= PRINT_SECS && spinning {
                ui.small("Done printing: it joins the board when the drum stops.");
            }
        }
        Stage::Done => {
            let who: Vec<String> = p.wants.iter().map(|(n, g, _)| format!("{n} ({g:.0} Goo)")).collect();
            ui.label(egui::RichText::new(format!("Printed: the {}.", p.item)).color(PLA_ORANGE));
            ui.small(format!("Wanted by {}. It's on tonight's board.", who.join(", ")));
            if !small && !shop.stl.is_empty() {
                ui.small(format!("STL: {}", shop.stl.join(", ")));
            }
        }
    }
}

/// The printer's little screen: a bubble above it with the draft's verdict,
/// its flaws, or the print's progress (where the player is looking).
pub fn bubble(ctx: &egui::Context, shop: &PrintShop, spinning: bool, at: egui::Pos2) {
    let p = &CATALOG[shop.pick];
    let red = egui::Color32::from_rgb(255, 120, 100);
    let lines: Vec<(String, egui::Color32)> = match shop.stage {
        Stage::Idle => return,
        Stage::Checking { fixed } => vec![(format!("slicing draft {}...", if fixed { 2 } else { 1 }), egui::Color32::LIGHT_GRAY)],
        Stage::Rejected => {
            let d = shop.v2.as_ref().or(shop.v1.as_ref());
            let mut l = vec![(format!("DRAFT {} WON'T PRINT", if shop.v2.is_some() { 2 } else { 1 }), red)];
            l.extend(d.into_iter().flat_map(|d| d.report.short()).take(3).map(|s| (format!("- {s}"), egui::Color32::WHITE)));
            l
        }
        Stage::Approved => vec![("DRAFT 2 PRINTS".into(), egui::Color32::from_rgb(120, 230, 120)), (format!("{}", p.fix), egui::Color32::WHITE)],
        Stage::Printing { t } => vec![(format!("printing {}: {:.0}%", p.item.trim_start_matches("printed "), 100.0 * (t / PRINT_SECS).min(1.0)), PLA_ORANGE)],
        Stage::Done if spinning => return,
        Stage::Done => vec![(format!("done: {}", p.item.trim_start_matches("printed ")), PLA_ORANGE)],
    };
    egui::Area::new("printer_bubble".into()).fixed_pos(at).pivot(egui::Align2::CENTER_TOP).interactable(false).show(ctx, |ui| {
        egui::Frame::new().fill(egui::Color32::from_black_alpha(200)).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
            ui.set_max_width(320.0);
            for (text, color) in lines {
                ui.label(egui::RichText::new(text).size(12.5).color(color));
            }
        });
    });
}

/// Where the bubble hangs: on the floor in front of the printer (above it,
/// it covered the porthole).
pub fn bubble_anchor() -> Vec3 {
    PRINTER + Vec3::new(0.0, 0.0, 0.45)
}
