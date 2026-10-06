//! Step 7.0 feasibility probe: Upddayett prints things.
//!
//! cargo run --release --example probe_print -- [check|stl|hinge|all]
//!
//!   check   every catalog part, v1 (with a planted flaw) and v2 (fixed),
//!           through both checkers: cf-design's `Mechanism::validate` with a
//!           `PrintProfile`, and mesh-printability's `validate_for_printing`
//!           with `find_optimal_orientation`; timings
//!   stl     orient each v2 for the bed and write `prints/<part>.stl`
//!   hinge   the battery sled's print-in-place lid: `to_mjcf`, `load_model`,
//!           open it, let gravity close it on its hinge limit
//!
//! Env: `PART=sled|feeder|bracket|hook` runs one part.
//! Units are mm (cf-design's), densities kg/m^3.


use std::time::Instant;

use cortenforge::cf_design::{DesignWarning, JointDef, JointKind, Material, Mechanism, PrintProfile, Solid, templates};
use cortenforge::cf_design::{IndexedMesh, Part};
use cortenforge::mesh::io::save_stl;
use cortenforge::mesh::printability::{PrinterConfig, apply_orientation, find_optimal_orientation, place_on_build_plate, validate_for_printing};
use cortenforge::sim::mjcf::load_model;
use nalgebra::{Point3, Vector3};

/// PLA.
fn pla() -> Material {
    Material::new("PLA", 1240.0)
}

/// A typical FDM profile for cf-design: 0.3 mm clearance, 0.8 mm walls,
/// 1.5 mm holes (its own doc example).
fn fdm() -> PrintProfile {
    PrintProfile::new(0.3, 0.8, 1.5)
}

fn v3(x: f64, y: f64, z: f64) -> Vector3<f64> {
    Vector3::new(x, y, z)
}

/// The 6x18650 sled: a tray of six cell cradles (18.5 mm cells, 65 mm long,
/// side by side along x) with walls `wall` thick, and a lid that pins on at
/// the tray's back top edge. Designed the FDM way: flat bottoms, square
/// edges (a horizontal cylinder's underside is an overhang).
const CELL_HALF: f64 = 33.0;
const PITCH: f64 = 21.0;
const TRAY_H: f64 = 22.0;

fn sled_tray(wall: f64) -> Solid {
    let inner = v3(3.0 * PITCH, CELL_HALF + 0.5, TRAY_H / 2.0);
    let outer = inner + v3(wall, wall, 0.0);
    let mut s = Solid::cuboid(outer).translate(v3(0.0, 0.0, outer.z));
    // The pocket, open at the top; the floor is `wall` thick.
    s = s.subtract(Solid::cuboid(inner).translate(v3(0.0, 0.0, wall + inner.z)));
    // Dividers between the cells, 10 mm tall. (Round cradles cut from a
    // block leave knife-edged horns where the arc meets the block; both
    // checkers flagged them.)
    for k in 1..6 {
        let x = (k as f64 - 3.0) * PITCH;
        s = s.union(Solid::cuboid(v3(1.0, CELL_HALF, 5.0)).translate(v3(x, 0.0, wall + 5.0)));
    }
    s
}

/// The lid, in its own frame: the hinge axis is the x axis through the
/// origin, inside a square knuckle; the flat plate runs toward -y (closed:
/// lying over the tray). It prints flat.
fn sled_lid(wall: f64) -> Solid {
    let w = 3.0 * PITCH + wall;
    let d = CELL_HALF + 0.5 + wall;
    let knuckle = Solid::cuboid(v3(w * 0.9, wall, wall / 2.0));
    knuckle.union(Solid::cuboid(v3(w, d, wall / 2.0)).translate(v3(0.0, -d, 0.0)))
}

fn sled(wall: f64) -> Mechanism {
    // The hinge sits in the back wall's top edge (inside the tray, so the
    // anchor check passes), and the lid rests on top.
    let back = CELL_HALF + 0.5 + wall / 2.0;
    Mechanism::builder("battery_sled")
        .part(Part::new("tray", sled_tray(wall), pla()))
        .joint(JointDef::new("base", "world", "tray", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .part(Part::new("lid", sled_lid(wall), pla()))
        .joint(JointDef::new("hinge", "tray", "lid", JointKind::Revolute, Point3::new(0.0, back, TRAY_H - wall / 2.0), -Vector3::x()).with_range(-0.05, 2.0))
        .print_profile(fdm())
        .build()
}

/// Pigeon feeder: a dish on three posts under a roof. v1 is one piece with
/// a flat roof (a 90 deg overhang all the way round). v2 prints the roof as
/// its own part, a hollow cone standing on its rim (30 deg overhang inside),
/// that drops onto the posts.
fn feeder(two_part: bool) -> Mechanism {
    let mut base = Solid::cylinder(60.0, 6.0).subtract(Solid::cylinder(55.0, 6.0).translate(v3(0.0, 0.0, 3.0))).translate(v3(0.0, 0.0, 6.0));
    let post = |k: usize| {
        let a = k as f64 * std::f64::consts::TAU / 3.0;
        (50.0 * a.cos(), 50.0 * a.sin())
    };
    for k in 0..3 {
        let (x, y) = post(k);
        // Posts stand on the dish floor (z 0..3); the first cut floated 2 mm
        // over it, and both checkers flagged the post bottoms.
        base = base.union(Solid::cylinder(4.0, 43.0).translate(v3(x, y, 45.0)));
    }
    let b = Mechanism::builder("pigeon_feeder").print_profile(fdm());
    if !two_part {
        let roof = Solid::cylinder(70.0, 2.0).translate(v3(0.0, 0.0, 90.0));
        return b
            .part(Part::new("feeder", base.union(roof), pla()))
            .joint(JointDef::new("base", "world", "feeder", JointKind::Fixed, Point3::origin(), Vector3::z()))
            .build();
    }
    // 60 deg from horizontal: the inside overhangs 30 deg.
    let h = 70.0 * 3f64.sqrt();
    let (px, py) = post(0);
    // `Solid::cone` has its apex at the origin and its base at -h. Walls 3 mm
    // thick (the inner cone sits 6 mm lower), and the tip cut off 12 mm down,
    // leaving a vent: a hollow cone's tip is a knife edge.
    let shell = Solid::cone(70.0, h).subtract(Solid::cone((h - 6.0) / 3f64.sqrt(), h - 6.0).translate(v3(0.0, 0.0, -6.0)));
    // A 3 mm vertical lip at the rim: the cone meeting its base plane is a
    // 60 deg edge, and the thickness check reads that wedge as a thin wall.
    // Its inside (66) sits 0.5 mm inside the cone's (66.5): flush, the seam
    // meshed into slivers that read as overhangs.
    let lip = Solid::cylinder(70.0, 1.5).subtract(Solid::cylinder(66.0, 2.0)).translate(v3(0.0, 0.0, -h - 1.5));
    let shell = shell.union(lip);
    let roof = shell.intersect(Solid::cuboid(v3(80.0, 80.0, (h - 9.0) / 2.0)).translate(v3(0.0, 0.0, -12.0 - (h - 9.0) / 2.0))).translate(v3(-px, -py, 4.0 + h));
    b.part(Part::new("base", base, pla()))
        .joint(JointDef::new("stand", "world", "base", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .part(Part::new("roof", roof, pla()))
        .joint(JointDef::new("seat", "base", "roof", JointKind::Fixed, Point3::new(px, py, 87.0), Vector3::z()))
        .build()
}

/// The caster bracket: a 60 x 40 x 5 plate with four bolt holes. v1 drills
/// 0.6 mm holes. (Not `templates::bracket`: its rounding grows the plate to
/// 62 x 42 x 7, see FINDINGS.)
fn bracket(hole: f64) -> Mechanism {
    let mut s = Solid::cuboid(v3(30.0, 20.0, 2.5));
    for (x, y) in [(-22.0, -12.0), (22.0, -12.0), (-22.0, 12.0), (22.0, 12.0)] {
        s = s.subtract(Solid::cylinder(hole / 2.0, 5.0).translate(v3(x, y, 0.0)));
    }
    Mechanism::builder("caster_bracket")
        .part(Part::new("bracket", s, pla()))
        .joint(JointDef::new("base", "world", "bracket", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .print_profile(fdm())
        .build()
}

/// Headphone hook: a J profile extruded 10 mm, printed lying on its side
/// (every wall vertical). `t` is the arm's thickness; v1's is 0.6 mm.
fn hook(t: f64) -> Mechanism {
    let slab = |x0: f64, x1: f64, y0: f64, y1: f64| Solid::cuboid(v3((x1 - x0) / 2.0, (y1 - y0) / 2.0, 5.0)).translate(v3((x0 + x1) / 2.0, (y0 + y1) / 2.0, 5.0));
    let s = slab(0.0, 6.0, 0.0, 60.0).union(slab(0.0, 40.0, 0.0, t)).union(slab(34.0, 40.0, 0.0, 14.0));
    Mechanism::builder("headphone_hook")
        .part(Part::new("hook", s, pla()))
        .joint(JointDef::new("base", "world", "hook", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .print_profile(fdm())
        .build()
}

/// (name, v1 flaw, v1, v2)
fn catalog() -> Vec<(&'static str, &'static str, Mechanism, Mechanism)> {
    let want = std::env::var("PART").ok();
    let all: Vec<(&str, &str, Box<dyn Fn() -> (Mechanism, Mechanism)>)> = vec![
        ("sled", "0.5 mm walls", Box::new(|| (sled(0.5), sled(2.0)))),
        ("feeder", "flat roof", Box::new(|| (feeder(false), feeder(true)))),
        ("bracket", "0.6 mm bolt holes", Box::new(|| (bracket(0.6), bracket(4.2)))),
        ("hook", "0.6 mm arm", Box::new(|| (hook(0.6), hook(6.0)))),
    ];
    all.into_iter()
        .filter(|(n, ..)| want.as_deref().is_none_or(|w| w == *n))
        .map(|(n, flaw, f)| {
            let (a, b) = f();
            (n, flaw, a, b)
        })
        .collect()
}

fn warning(w: &DesignWarning) -> String {
    match w {
        DesignWarning::WallTooThin { part, thickness, min, .. } => format!("{part}: wall {thickness:.2} mm < {min}"),
        DesignWarning::HoleTooSmall { part, diameter, min, .. } => format!("{part}: hole {diameter:.2} mm < {min}"),
        DesignWarning::FeatureBelowResolution { part, feature_size, resolution } => {
            format!("{part}: feature {feature_size:.2} mm below resolution {resolution:.2}")
        }
        other => format!("{other:?}"),
    }
}

/// Mesh tolerance for printing: 0.1 mm (a quarter of the nozzle).
const TOL: f64 = 0.1;
fn tol() -> f64 {
    std::env::var("TOL").ok().and_then(|s| s.parse().ok()).unwrap_or(TOL)
}

/// The issues grouped by type and severity: how many regions, and the
/// worst one's description.
fn summarize(v: &cortenforge::mesh::printability::PrintValidation) -> Vec<String> {
    let mut groups: Vec<(String, usize, String)> = vec![];
    for i in &v.issues {
        let key = format!("{:?} {:?}", i.severity, i.issue_type);
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1 += 1,
            None => groups.push((key, 1, i.description.clone())),
        }
    }
    groups.sort_by(|a, b| b.0.starts_with("Critical").cmp(&a.0.starts_with("Critical")).then(a.0.cmp(&b.0)));
    groups.into_iter().map(|(k, n, d)| format!("{k} x{n} (e.g. {d})")).collect()
}

/// `REPAIR=1`: run the mesh crate's `repair_mesh` (defaults) on each kit
/// part before the printability checks.
fn maybe_repair(mesh: &mut IndexedMesh) -> Option<String> {
    use cortenforge::mesh::repair::{RepairParams, repair_mesh};
    if std::env::var("REPAIR").is_err() {
        return None;
    }
    let s = repair_mesh(mesh, &RepairParams::default());
    Some(format!("{s:?}"))
}

/// Checks one design: cf-design's validate on the whole mechanism, then each
/// part of its STL kit on its own (each part is its own print), as designed
/// and in the orientation `find_optimal_orientation` picks. Returns the
/// oriented kit parts, on the bed.
fn check_one(label: &str, m: &Mechanism) -> Vec<(String, IndexedMesh)> {
    let t = Instant::now();
    let warnings = m.validate();
    let t_validate = t.elapsed().as_secs_f64();
    println!(
        "  {label}: cf-design validate ({t_validate:.2} s): {}",
        if warnings.is_empty() { "clean".into() } else { warnings.iter().map(warning).collect::<Vec<_>>().join("; ") }
    );
    let t = Instant::now();
    let kit = m.to_stl_kit(tol());
    let t_mesh = t.elapsed().as_secs_f64();
    let cfg = PrinterConfig::fdm_default();
    let mut out = vec![];
    for (part, mut mesh) in kit {
        if let Some(r) = maybe_repair(&mut mesh) {
            println!("    part {part}: repair_mesh: {r}");
        }
        let t = Instant::now();
        let orient = find_optimal_orientation(&mesh, &cfg, 24);
        let t_orient = t.elapsed().as_secs_f64();
        let oriented = place_on_build_plate(&apply_orientation(&mesh, &orient));
        let t = Instant::now();
        let as_is = validate_for_printing(&place_on_build_plate(&mesh), &cfg);
        let best = validate_for_printing(&oriented, &cfg);
        let t_print = t.elapsed().as_secs_f64();
        println!("    part {part}: {} faces, volume {:.0} mm^3", mesh.faces.len(), volume(&mesh));
        for (what, v) in [("as designed", &as_is), ("oriented", &best)] {
            match v {
                Ok(v) => {
                    println!("      printability {what}: printable {}, support {:.0} mm^3", v.is_printable(), v.total_support_volume());
                    for line in summarize(v).iter().filter(|l| !l.starts_with("Info")) {
                        println!("        {line}");
                    }
                }
                Err(e) => println!("      printability {what}: error {e:?}"),
            }
        }
        let (axis, angle) = orient.rotation.axis_angle().map_or((Vector3::z(), 0.0), |(a, t)| (a.into_inner(), t));
        println!(
            "      orientation: {:.0} deg about [{:.2} {:.2} {:.2}], overhang area {:.0} mm^2  (kit mesh {t_mesh:.2} s, orient {t_orient:.2} s, checks {t_print:.2} s)",
            angle.to_degrees(),
            axis.x,
            axis.y,
            axis.z,
            orient.overhang_area
        );
        out.push((part, oriented));
    }
    out
}

fn check() {
    for (name, flaw, v1, v2) in catalog() {
        println!("\n== {name} (v1: {flaw})");
        check_one("v1", &v1);
        check_one("v2", &v2);
    }
}

fn stl() {
    std::fs::create_dir_all("prints").expect("prints dir");
    let cfg = PrinterConfig::fdm_default();
    for (name, _, _, v2) in catalog() {
        for (part, mesh) in v2.to_stl_kit(tol()) {
            // Flat as designed when that prints: `find_optimal_orientation`
            // scores overhang area alone, so it stands a 2 mm lid on its edge.
            let flat = place_on_build_plate(&mesh);
            let (on_bed, how) = if verdict(&flat).0 {
                (flat, "as designed")
            } else {
                (place_on_build_plate(&apply_orientation(&mesh, &find_optimal_orientation(&mesh, &cfg, 24))), "rotated")
            };
            let path = format!("prints/{name}_{part}.stl");
            save_stl(&on_bed, &path, true).expect("save stl");
            let bb = on_bed.vertices.iter().fold((v3(f64::MAX, f64::MAX, f64::MAX), v3(f64::MIN, f64::MIN, f64::MIN)), |(lo, hi), p| {
                (lo.inf(&p.coords), hi.sup(&p.coords))
            });
            let size = bb.1 - bb.0;
            println!("  wrote {path} ({how}): {:.1} x {:.1} x {:.1} mm, {} faces", size.x, size.y, size.z, on_bed.faces.len());
        }
    }
}

/// The lid opens to 1.2 rad; gravity should swing it shut onto its limit.
fn hinge() {
    let m = sled(2.0);
    let t = Instant::now();
    let xml = m.to_mjcf(1.0).expect("mjcf");
    println!("to_mjcf: {} KB in {:.2} s", xml.len() / 1024, t.elapsed().as_secs_f64());
    for line in xml.lines().filter(|l| l.contains("<option") || l.contains("<compiler") || l.contains("<joint") || l.contains("<body") || l.contains("<geom")).take(12) {
        let l = line.trim();
        println!("  {}", if l.len() > 160 { &l[..160] } else { l });
    }
    let t = Instant::now();
    let model = load_model(&xml).expect("load_model");
    println!("load_model: {:.2} s, nq {}, nbody {}, ngeom {}, timestep {}, gravity {:?}", t.elapsed().as_secs_f64(), model.nq, model.nbody, model.ngeom, model.timestep, model.gravity.as_slice());
    let mut data = model.make_data();
    let q = model.jnt_qpos_adr[model.njnt - 1];
    data.qpos[q] = 1.2;
    data.forward(&model).expect("forward");
    let dt = model.timestep;
    let mut t_sim = 0.0;
    let mut next = 0.0;
    while t_sim < 1.5 {
        data.step(&model).expect("step");
        t_sim += dt;
        if t_sim >= next {
            println!("  t {:.2} s: lid at {:+.3} rad, {:+.2} rad/s", t_sim, data.qpos[q], data.qvel[q]);
            next += 0.1;
        }
    }
    // The kit's clearance: each part shrinks by half of 0.3 mm.
    let kit = m.to_stl_kit(tol());
    for (name, mesh) in &kit {
        let raw = m.parts().iter().find(|p| p.name() == name).map(|p| p.solid().bounds());
        let lo = mesh.vertices.iter().fold(f64::MAX, |a, p| a.min(p.x));
        let hi = mesh.vertices.iter().fold(f64::MIN, |a, p| a.max(p.x));
        println!("  kit {name}: x span {:.2} mm (designed {:?})", hi - lo, raw.flatten().map(|b| (b.max.x - b.min.x) * 1.0));
    }
}

/// Signed volume (mm^3) by the divergence theorem: right for a closed,
/// consistently wound mesh, nonsense otherwise.
fn volume(m: &IndexedMesh) -> f64 {
    m.faces
        .iter()
        .map(|f| {
            let [a, b, c] = f.map(|i| m.vertices[i as usize].coords);
            a.dot(&b.cross(&c)) / 6.0
        })
        .sum()
}

/// Is the mesh sound? cf-design's meshers side by side on each v2 part:
/// the mesh crate's own validator, the printability crate's
/// self-intersection count, and the enclosed volume.
fn mesh_check() {
    use cortenforge::mesh::repair::validate_mesh;
    let t0 = tol();
    for (name, _, _, v2) in catalog() {
        println!("\n== {name} v2 (tolerance {t0} mm)");
        let part = &v2.parts()[0];
        let s = part.solid();
        let kit = v2.to_stl_kit(t0).into_iter().next().expect("kit").1;
        let t = Instant::now();
        let variants: Vec<(&str, IndexedMesh)> = vec![
            ("mesh", s.mesh(t0).geometry),
            ("mesh_adaptive", s.mesh_adaptive(t0).geometry),
            ("mesh_dc", s.mesh_dc(t0).geometry),
            ("to_stl_kit", kit),
        ];
        println!("  meshed 4 ways in {:.1} s", t.elapsed().as_secs_f64());
        for (how, m) in variants {
            let r = validate_mesh(&m);
            let selfx = validate_for_printing(&m, &PrinterConfig::fdm_default())
                .map(|v| v.self_intersecting.len())
                .unwrap_or(usize::MAX);
            println!(
                "  {how:<14} {:>7} faces, watertight {}, manifold {}, boundary edges {}, non-manifold {}, degenerate {}, self-intersecting regions {selfx}, volume {:.0} mm^3",
                m.faces.len(),
                r.is_watertight,
                r.is_manifold,
                r.boundary_edge_count,
                r.non_manifold_edge_count,
                r.degenerate_face_count,
                volume(&m)
            );
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "check".into());
    match mode.as_str() {
        "check" => check(),
        "stl" => stl(),
        "hinge" => hinge(),
        "mesh" => mesh_check(),
        "template" => template_check(),
        "walls" => walls(),
        "grid" => grid(),
        "verdict" => verdicts(),
        "selfx" => selfx(),
        "all" => {
            check();
            stl();
            hinge();
        }
        m => panic!("unknown mode {m}"),
    }
}

/// `templates::bracket` says its sizes are full extents; measure what it makes.
fn template_check() {
    let b = templates::bracket("b", 60.0, 40.0, 5.0, pla());
    let bb = b.solid().bounds().expect("bounds");
    let m = b.solid().mesh(0.25).geometry;
    println!(
        "templates::bracket(60, 40, 5): bounds {:.1} x {:.1} x {:.1} mm, volume {:.0} mm^3 (a 60 x 40 x 5 plate: 12000)",
        bb.max.x - bb.min.x,
        bb.max.y - bb.min.y,
        bb.max.z - bb.min.z,
        volume(&m)
    );
}

/// Plain 40 x 40 plates of known thickness through both wall checks:
/// cf-design (`PrintProfile` min wall 0.8) and printability (FDM, 1.0).
fn walls() {
    println!("thickness | cf-design validate | printability ThinWall");
    for t in [0.4, 0.6, 0.8, 1.0, 1.5, 2.0, 3.0, 4.0] {
        let plate = Solid::cuboid(v3(20.0, 20.0, t / 2.0)).translate(v3(0.0, 0.0, t / 2.0));
        let m = Mechanism::builder("plate")
            .part(Part::new("plate", plate.clone(), pla()))
            .joint(JointDef::new("base", "world", "plate", JointKind::Fixed, Point3::origin(), Vector3::z()))
            .print_profile(fdm())
            .build();
        let w = m.validate();
        let mesh = plate.mesh(0.1_f64.min(t / 4.0)).geometry;
        let thin = validate_for_printing(&mesh, &PrinterConfig::fdm_default())
            .map(|v| v.thin_walls.iter().map(|r| r.thickness).fold(f64::INFINITY, f64::min))
            .unwrap_or(f64::NAN);
        println!(
            "  {t:>4} mm | {} | {}",
            if w.is_empty() { "clean".into() } else { w.iter().map(warning).collect::<Vec<_>>().join("; ") },
            if thin.is_finite() { format!("min {thin:.2} mm") } else { "none".into() }
        );
    }
}

/// Does cf-design's wall verdict depend on where the part sits? The same
/// plates, shifted up by a fraction of the 0.8 mm sampling cell.
fn grid() {
    for t in [0.6, 1.0, 2.0] {
        let verdicts: Vec<String> = [0.0, 0.2, 0.4, 0.6]
            .iter()
            .map(|&dz| {
                let plate = Solid::cuboid(v3(20.0, 20.0, t / 2.0)).translate(v3(0.0, 0.0, t / 2.0 + dz));
                let m = Mechanism::builder("plate")
                    .part(Part::new("plate", plate, pla()))
                    .joint(JointDef::new("base", "world", "plate", JointKind::Fixed, Point3::origin(), Vector3::z()))
                    .print_profile(fdm())
                    .build();
                let w = m.validate();
                let thin = w.iter().find_map(|w| match w {
                    DesignWarning::WallTooThin { thickness, .. } => Some(*thickness),
                    _ => None,
                });
                format!("+{dz}: {}", thin.map_or("clean".into(), |x| format!("wall {x:.2}")))
            })
            .collect();
        println!("  {t} mm plate: {}", verdicts.join(", "));
    }
}

/// Self-intersections reported on plain primitives, per mesher.
fn selfx() {
    use cortenforge::mesh::repair::validate_mesh;
    let shapes: Vec<(&str, Solid)> = vec![
        ("sphere r10", Solid::sphere(10.0)),
        ("cube 20", Solid::cuboid(v3(10.0, 10.0, 10.0))),
        ("two boxes (union)", Solid::cuboid(v3(10.0, 10.0, 5.0)).union(Solid::cuboid(v3(3.0, 3.0, 10.0)))),
        ("box minus cylinder", Solid::cuboid(v3(10.0, 10.0, 5.0)).subtract(Solid::cylinder(3.0, 6.0))),
    ];
    for (name, s) in shapes {
        for (how, mut m) in [("mesh", s.mesh(0.5).geometry), ("mesh_adaptive", s.mesh_adaptive(0.5).geometry)] {
            let how = if maybe_repair(&mut m).is_some() { format!("{how}+repair") } else { how.to_string() };
            let r = validate_mesh(&m);
            let v = validate_for_printing(&m, &PrinterConfig::fdm_default()).expect("validate");
            let pairs = v.issues.iter().find(|i| format!("{:?}", i.issue_type) == "SelfIntersecting").map_or("none".to_string(), |i| i.description.clone());
            println!("  {name:<20} {how:<14} {:>6} faces, watertight {}, degenerate {}, self-intersecting: {pairs}", m.faces.len(), r.is_watertight, r.degenerate_face_count);
        }
    }
}

/// The verdict the game would use (the probe's answer to "can we trust the
/// checkers raw?": no). printability measures overhangs, bridges and walls
/// well, but flags cf-design's mesher slivers: faces of ~0.1 mm^2 and the
/// degenerate triangles that read as self-intersections. So: no repair (it
/// opens holes), drop issue regions under 1 mm^2, and drop self-
/// intersections on a mesh that is watertight and manifold.
fn verdict(mesh: &IndexedMesh) -> (bool, Vec<String>) {
    use cortenforge::mesh::repair::validate_mesh;
    let sound = {
        let r = validate_mesh(mesh);
        r.is_watertight && r.is_manifold
    };
    let area = |faces: &[u32]| -> f64 {
        faces
            .iter()
            .map(|&f| {
                let [a, b, c] = mesh.faces[f as usize].map(|i| mesh.vertices[i as usize].coords);
                (b - a).cross(&(c - a)).norm() / 2.0
            })
            .sum()
    };
    let Ok(v) = validate_for_printing(mesh, &PrinterConfig::fdm_default()) else { return (false, vec!["empty mesh".into()]) };
    let mut flaws = vec![];
    for i in &v.issues {
        if format!("{:?}", i.severity) != "Critical" {
            continue;
        }
        let kind = format!("{:?}", i.issue_type);
        if kind == "SelfIntersecting" && sound {
            continue;
        }
        if !i.affected_elements.is_empty() && area(&i.affected_elements) < 1.0 {
            continue;
        }
        let at = i.location.map_or(String::new(), |p| format!(" at ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z));
        flaws.push(format!("{kind}{at}: {}", i.description));
    }
    (flaws.is_empty(), flaws)
}

fn verdicts() {
    let cfg = PrinterConfig::fdm_default();
    for (name, flaw, v1, v2) in catalog() {
        println!("\n== {name} (v1: {flaw})");
        for (label, m) in [("v1", &v1), ("v2", &v2)] {
            for (part, mesh) in m.to_stl_kit(tol()) {
                let t = Instant::now();
                let orient = find_optimal_orientation(&mesh, &cfg, 24);
                let oriented = place_on_build_plate(&apply_orientation(&mesh, &orient));
                // Try the designer's orientation too, and keep the better.
                let flat = place_on_build_plate(&mesh);
                let (ok_o, flaws_o) = verdict(&oriented);
                let (ok_f, flaws_f) = verdict(&flat);
                if std::env::var("VERBOSE").is_ok() {
                    println!("    flat: {ok_f} {flaws_f:?}");
                }
                let (ok, flaws, which) = if ok_f || flaws_f.len() <= flaws_o.len() { (ok_f, flaws_f, "as designed") } else { (ok_o, flaws_o, "rotated") };
                println!("  {label} {part}: {} ({which}, {:.1} s)", if ok { "PRINTS" } else { "WON'T PRINT" }, t.elapsed().as_secs_f64());
                for f in flaws.iter().take(3) {
                    println!("      {}", if f.len() > 150 { &f[..150] } else { f });
                }
            }
        }
    }
}
