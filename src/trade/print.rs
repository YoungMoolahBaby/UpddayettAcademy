//! Upddayett prints things (Step 7): parts designed in code with
//! `cf_design`, checked for an FDM printer with mesh-printability, written
//! out as STL, and put on tonight's board as his items.
//!
//! One Solid runs the whole way: the design, the print check, the STL, and
//! (in the game) the mesh and the drum's collider. Every part in the
//! [`CATALOG`] has a v1 with a real flaw that the check catches and a v2
//! that prints (the lesson: CortenForge catches a bad print before it wastes
//! the night).
//!
//! Neither checker is usable raw (`examples/probe_print.rs`, PLAN "7.0
//! result", FINDINGS cf-design and mesh). cf-design's `validate` misses walls
//! thinner than its 0.8 mm sampling cell, and printability fails nearly
//! every cf-design part on mesher slivers. So [`verdict`] uses printability
//! on the unrepaired mesh, minus regions under [`MIN_REGION`] and minus
//! self-intersections on a watertight, manifold mesh (zero-area triangles).

use std::path::{Path, PathBuf};
use std::time::Instant;

use cortenforge::cf_design::{DesignWarning, IndexedMesh, JointDef, JointKind, Material, Mechanism, Part, PrintProfile, Solid};
use cortenforge::mesh::io::save_stl;
use cortenforge::mesh::printability::{PrinterConfig, apply_orientation, find_optimal_orientation, place_on_build_plate, validate_for_printing};
use cortenforge::mesh::repair::validate_mesh;
use nalgebra::{Point3, Vector3};

use super::world::{Use, World};

/// Mesh tolerance for printing (mm). 0.1 runs for minutes on a 124 mm part
/// (FINDINGS cf-design); at 0.5 a part checks in 0.4-16 s.
pub const TOL: f64 = 0.5;

/// Issue regions smaller than this (mm^2) are mesher slivers, not flaws.
pub const MIN_REGION: f64 = 1.0;

/// One part Upddayett can print.
pub struct Print {
    /// Short name (`trade_cli print --part`).
    pub key: &'static str,
    /// What it's called on the board.
    pub item: &'static str,
    /// What it's worth to Upddayett, and what for.
    pub base: f64,
    pub owner_use: Use,
    /// Who wants it: (a piece of their name, Goo on a neutral night, why).
    pub wants: &'static [(&'static str, f64, Use)],
    /// The filament color (sRGB).
    pub color: [f32; 3],
    /// What v1 gets wrong, and what v2 changed.
    pub flaw: &'static str,
    pub fix: &'static str,
    design: fn(bool) -> Mechanism,
}

impl Print {
    /// The design: v1 (with its flaw) or v2 (`fixed`).
    pub fn design(&self, fixed: bool) -> Mechanism {
        (self.design)(fixed)
    }

    /// v2 assembled (each part at its joint's reference position), centered
    /// on the origin: what tumbles in the drum.
    pub fn solid(&self) -> Solid {
        let m = self.design(true);
        let origins = m.reference_origins().unwrap_or_default();
        let s = m
            .parts()
            .iter()
            .map(|p| p.solid().clone().translate(origins.get(p.name()).copied().unwrap_or_else(Vector3::zeros)))
            .reduce(Solid::union)
            .expect("a print has parts");
        match s.bounds() {
            Some(b) => {
                let c = (b.min.coords + b.max.coords) / 2.0;
                s.translate(-c)
            }
            None => s,
        }
    }
}

pub const CATALOG: [Print; 4] = [
    Print {
        key: "sled",
        item: "printed 6x18650 battery sled",
        base: 3.0,
        owner_use: Use::Build,
        wants: &[("Dave", 7.0, Use::Build), ("Vape Lady", 5.0, Use::Build)],
        color: [1.0, 0.45, 0.1],
        flaw: "0.5 mm walls: the nozzle lays 0.4 mm lines, so they come out as two-line slivers that crack",
        fix: "2 mm walls and floor, square dividers between the cells, and a flat lid that pins on",
        design: sled,
    },
    Print {
        key: "feeder",
        item: "printed pigeon feeder",
        base: 2.0,
        owner_use: Use::Build,
        wants: &[("Pigeon Lady", 6.0, Use::FeedAnimals), ("Tamara", 4.0, Use::FeedAnimals)],
        color: [0.1, 0.7, 0.62],
        flaw: "a flat roof in one piece: 140 mm of plastic over thin air, which the printer can't bridge",
        fix: "the roof is its own print: a steep cone standing on its rim, dropped onto the posts",
        design: feeder,
    },
    Print {
        key: "bracket",
        item: "printed shopping-cart caster bracket",
        base: 2.0,
        owner_use: Use::Build,
        wants: &[("Shopping-Cart", 6.0, Use::Build), ("Dave", 3.0, Use::Build)],
        color: [0.22, 0.22, 0.25],
        flaw: "drawn 300 mm long: the bed is 200 mm, and even corner to corner it's 283",
        fix: "60 x 40 x 5 mm with four 4.2 mm bolt holes",
        design: bracket,
    },
    Print {
        key: "hook",
        item: "printed headphone hook",
        base: 1.0,
        owner_use: Use::Build,
        wants: &[("Sound Guy Ray", 4.0, Use::Enjoy), ("Tamara", 2.0, Use::Enjoy)],
        color: [0.85, 0.15, 0.2],
        flaw: "a 0.6 mm arm: thinner than the 1 mm the printer can make, and it would snap under headphones anyway",
        fix: "a 6 mm arm, the J extruded 10 mm and printed lying on its side",
        design: hook,
    },
];

/// A catalog part by its key.
pub fn find(key: &str) -> Option<&'static Print> {
    CATALOG.iter().find(|p| p.key == key)
}

// ── The designs (mm). The FDM way: flat bottoms, square edges or extruded
// profiles, no tangent or flush CSG seams, no knife edges. ──

fn pla() -> Material {
    Material::new("PLA", 1240.0)
}

/// cf-design's profile: 0.3 mm clearance, 0.8 mm walls, 1.5 mm holes.
fn fdm() -> PrintProfile {
    PrintProfile::new(0.3, 0.8, 1.5)
}

fn v3(x: f64, y: f64, z: f64) -> Vector3<f64> {
    Vector3::new(x, y, z)
}

/// A one-part mechanism fixed to the world.
fn one_part(name: &str, solid: Solid) -> Mechanism {
    Mechanism::builder(name)
        .part(Part::new(name, solid, pla()))
        .joint(JointDef::new("base", "world", name, JointKind::Fixed, Point3::origin(), Vector3::z()))
        .print_profile(fdm())
        .build()
}

/// 18650 cells side by side along x (18.5 mm across, 65 mm long).
const CELL_HALF: f64 = 33.0;
const PITCH: f64 = 21.0;
const TRAY_H: f64 = 22.0;

fn sled_tray(wall: f64) -> Solid {
    let inner = v3(3.0 * PITCH, CELL_HALF + 0.5, TRAY_H / 2.0);
    let outer = inner + v3(wall, wall, 0.0);
    let mut s = Solid::cuboid(outer).translate(v3(0.0, 0.0, outer.z));
    // The pocket, open at the top; the floor is `wall` thick.
    s = s.subtract(Solid::cuboid(inner).translate(v3(0.0, 0.0, wall + inner.z)));
    // Dividers between the cells (round cradles cut from a block leave
    // knife-edged horns).
    for k in 1..6 {
        s = s.union(Solid::cuboid(v3(1.0, CELL_HALF, 5.0)).translate(v3((k as f64 - 3.0) * PITCH, 0.0, wall + 5.0)));
    }
    s
}

/// The lid, in its own frame: the hinge axis is the x axis through the
/// origin, inside a square knuckle; the plate runs toward -y and prints flat.
fn sled_lid(wall: f64) -> Solid {
    let w = 3.0 * PITCH + wall;
    let d = CELL_HALF + 0.5 + wall;
    Solid::cuboid(v3(w * 0.9, wall, wall / 2.0)).union(Solid::cuboid(v3(w, d, wall / 2.0)).translate(v3(0.0, -d, 0.0)))
}

fn sled(fixed: bool) -> Mechanism {
    let wall = if fixed { 2.0 } else { 0.5 };
    // The hinge is in the back wall's top edge; -x so that opening is +.
    let back = CELL_HALF + 0.5 + wall / 2.0;
    Mechanism::builder("battery_sled")
        .part(Part::new("tray", sled_tray(wall), pla()))
        .joint(JointDef::new("base", "world", "tray", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .part(Part::new("lid", sled_lid(wall), pla()))
        .joint(JointDef::new("hinge", "tray", "lid", JointKind::Revolute, Point3::new(0.0, back, TRAY_H - wall / 2.0), -Vector3::x()).with_range(-0.05, 2.0))
        .print_profile(fdm())
        .build()
}

/// A dish (floor 0..3 mm) on three posts; v1 has a flat roof on top, v2
/// prints the roof as its own part.
fn feeder(fixed: bool) -> Mechanism {
    let mut base = Solid::cylinder(60.0, 6.0).subtract(Solid::cylinder(55.0, 6.0).translate(v3(0.0, 0.0, 3.0))).translate(v3(0.0, 0.0, 6.0));
    let post = |k: usize| {
        let a = k as f64 * std::f64::consts::TAU / 3.0;
        (50.0 * a.cos(), 50.0 * a.sin())
    };
    for k in 0..3 {
        let (x, y) = post(k);
        // Down to z = 2, inside the floor (posts from z = 8 floated).
        base = base.union(Solid::cylinder(4.0, 43.0).translate(v3(x, y, 45.0)));
    }
    if !fixed {
        return one_part("feeder", base.union(Solid::cylinder(70.0, 2.0).translate(v3(0.0, 0.0, 90.0))));
    }
    // A cone 60 deg from horizontal (`Solid::cone`: apex at the origin, base
    // at -h), walls 3 mm (the inner cone 6 mm lower), the tip cut off 12 mm
    // down as a vent, and a 3 mm vertical lip so the rim isn't a 60 deg
    // wedge. The lip's inside (66) sits inside the cone's (66.5): a flush
    // seam meshed into slivers.
    let h = 70.0 * 3f64.sqrt();
    let shell = Solid::cone(70.0, h)
        .subtract(Solid::cone((h - 6.0) / 3f64.sqrt(), h - 6.0).translate(v3(0.0, 0.0, -6.0)))
        .union(Solid::cylinder(70.0, 1.5).subtract(Solid::cylinder(66.0, 2.0)).translate(v3(0.0, 0.0, -h - 1.5)));
    let (px, py) = post(0);
    let roof = shell.intersect(Solid::cuboid(v3(80.0, 80.0, (h - 9.0) / 2.0)).translate(v3(0.0, 0.0, -12.0 - (h - 9.0) / 2.0))).translate(v3(-px, -py, 4.0 + h));
    Mechanism::builder("pigeon_feeder")
        .part(Part::new("base", base, pla()))
        .joint(JointDef::new("stand", "world", "base", JointKind::Fixed, Point3::origin(), Vector3::z()))
        .part(Part::new("roof", roof, pla()))
        .joint(JointDef::new("seat", "base", "roof", JointKind::Fixed, Point3::new(px, py, 87.0), Vector3::z()))
        .print_profile(fdm())
        .build()
}

/// A plate with four bolt holes. (Not `templates::bracket`: its rounding
/// grows a 60 x 40 x 5 plate to 62 x 42 x 7.)
fn bracket(fixed: bool) -> Mechanism {
    let half_len = if fixed { 30.0 } else { 150.0 };
    let mut s = Solid::cuboid(v3(half_len, 20.0, 2.5));
    let x = half_len - 8.0;
    for (x, y) in [(-x, -12.0), (x, -12.0), (-x, 12.0), (x, 12.0)] {
        s = s.subtract(Solid::cylinder(2.1, 5.0).translate(v3(x, y, 0.0)));
    }
    one_part("bracket", s)
}

/// A J profile extruded 10 mm, printed lying on its side (every wall
/// vertical).
fn hook(fixed: bool) -> Mechanism {
    let t = if fixed { 6.0 } else { 0.6 };
    let slab = |x0: f64, x1: f64, y0: f64, y1: f64| Solid::cuboid(v3((x1 - x0) / 2.0, (y1 - y0) / 2.0, 5.0)).translate(v3((x0 + x1) / 2.0, (y0 + y1) / 2.0, 5.0));
    one_part("hook", slab(0.0, 6.0, 0.0, 60.0).union(slab(0.0, 40.0, 0.0, t)).union(slab(34.0, 40.0, 0.0, 14.0)))
}

// ── The check ──

/// What one printed part does on the bed.
pub struct PartReport {
    pub part: String,
    pub prints: bool,
    /// Why not, in the checker's words (with its numbers), and in shop
    /// words (one line per kind of flaw).
    pub flaws: Vec<String>,
    pub short: Vec<String>,
    /// As placed on the bed (mm), and whether that's the designer's
    /// orientation or the one `find_optimal_orientation` picked.
    pub size: Vector3<f64>,
    pub rotated: bool,
    pub volume: f64,
    /// The part on the bed, for the STL and the game.
    pub mesh: IndexedMesh,
}

/// A whole design's check.
pub struct PrintReport {
    pub parts: Vec<PartReport>,
    /// cf-design's own `validate` warnings. Not part of the verdict: its
    /// wall check misses walls under its sampling cell (FINDINGS).
    pub design_warnings: Vec<String>,
    pub secs: f64,
}

impl PrintReport {
    pub fn prints(&self) -> bool {
        self.parts.iter().all(|p| p.prints)
    }

    /// Every part's flaws, labeled by part when there are several.
    pub fn flaws(&self) -> Vec<String> {
        self.labeled(|p| &p.flaws)
    }

    /// The same in shop words.
    pub fn short(&self) -> Vec<String> {
        self.labeled(|p| &p.short)
    }

    fn labeled(&self, f: impl Fn(&PartReport) -> &Vec<String>) -> Vec<String> {
        let label = self.parts.len() > 1;
        self.parts.iter().flat_map(|p| f(p).iter().map(move |s| if label { format!("{}: {s}", p.part) } else { s.clone() })).collect()
    }

    /// Plastic in every part (cm^3).
    pub fn plastic_cm3(&self) -> f64 {
        self.parts.iter().map(|p| p.volume).sum::<f64>() / 1000.0
    }

    /// A rough print time (hours) on a cheap FDM printer: ~40% of the solid
    /// (walls plus 20% infill) at ~15 cm^3 an hour. printability's own
    /// estimate is never filled in (FINDINGS mesh).
    pub fn hours(&self) -> f64 {
        self.plastic_cm3() * 0.4 / 15.0
    }
}

/// Checks a design: each part of its STL kit (each part is its own print),
/// as designed first, then in the orientation the printability crate picks.
pub fn check(m: &Mechanism, tol: f64) -> PrintReport {
    let t = Instant::now();
    let design_warnings = m.validate().iter().map(describe).collect();
    let cfg = PrinterConfig::fdm_default();
    let parts = m
        .to_stl_kit(tol)
        .into_iter()
        .map(|(part, mesh)| {
            let flat = place_on_build_plate(&mesh);
            let mut v = verdict(&flat);
            let mut on_bed = flat;
            let mut rotated = false;
            if !v.prints {
                // `find_optimal_orientation` scores overhang alone (it stands
                // a 2 mm lid on its edge), so it's only the fallback.
                let turned = place_on_build_plate(&apply_orientation(&mesh, &find_optimal_orientation(&mesh, &cfg, 24)));
                let t = verdict(&turned);
                if t.prints {
                    (v, on_bed, rotated) = (t, turned, true);
                }
            }
            let (lo, hi) = bounds(&on_bed);
            let size = hi - lo;
            let volume = volume(&on_bed);
            // A part thinner than the mesh tolerance meshes to (almost)
            // nothing, and nothing has no flaws: the sled v1's 0.5 mm lid.
            if size.min() < 2.0 * tol || volume < 1.0 {
                v.prints = false;
                v.flaws.push(format!("thin wall: {:.1} mm at its thinnest, below what the {tol} mm mesh (and the 0.4 mm nozzle) can make", size.min().max(0.0)));
                v.short.push(format!("{:.1} mm thin: less than one 0.4 mm line of plastic", size.min().max(0.0)));
            }
            PartReport { part, prints: v.prints, flaws: v.flaws, short: v.short, size, rotated, volume, mesh: on_bed }
        })
        .collect();
    PrintReport { parts, design_warnings, secs: t.elapsed().as_secs_f64() }
}

/// Will this mesh (on the bed) print? printability's critical issues, minus
/// the mesher's slivers and zero-area triangles. `flaws` are in the
/// checker's words; `short` says the same in shop words, one per kind.
pub fn verdict(mesh: &IndexedMesh) -> Verdict {
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
    let Ok(v) = validate_for_printing(mesh, &PrinterConfig::fdm_default()) else {
        return Verdict { prints: false, flaws: vec!["nothing to print (empty mesh)".into()], short: vec!["nothing to print".into()] };
    };
    let mut flaws = vec![];
    let mut kinds: Vec<String> = vec![];
    for i in &v.issues {
        if format!("{:?}", i.severity) != "Critical" {
            continue;
        }
        let kind = format!("{:?}", i.issue_type);
        if kind == "SelfIntersecting" && sound {
            continue;
        }
        if !i.affected_elements.is_empty() && area(&i.affected_elements) < MIN_REGION {
            continue;
        }
        flaws.push(format!("{}: {}", plain(&kind), i.description));
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let cfg = &v.config;
    let short = kinds
        .iter()
        .map(|k| match k.as_str() {
            "ExcessiveOverhang" => {
                let big = v.overhangs.iter().filter(|r| r.area >= MIN_REGION);
                let (angle, total) = big.fold((0.0f64, 0.0), |(a, t), r| (a.max(r.angle), t + r.area));
                format!("{total:.0} mm^2 hangs over air at up to {angle:.0} deg (the printer manages {:.0})", cfg.max_overhang_angle)
            }
            "LongBridge" => {
                let span = v.long_bridges.iter().map(|r| r.span).fold(0.0, f64::max);
                format!("a {span:.0} mm bridge with nothing under it ({:.0} mm at most)", cfg.max_bridge_span)
            }
            "ThinWall" => {
                let t = v.thin_walls.iter().filter(|r| r.area >= MIN_REGION).map(|r| r.thickness).fold(f64::INFINITY, f64::min);
                format!("walls down to {t:.2} mm thick ({:.1} mm at least)", cfg.min_wall_thickness)
            }
            "ExceedsBuildVolume" => format!("too big for the {:.0} x {:.0} mm bed", cfg.build_volume.0, cfg.build_volume.1),
            other => format!("{}", plain(other)),
        })
        .collect();
    Verdict { prints: flaws.is_empty(), flaws, short }
}

/// [`verdict`]'s answer.
pub struct Verdict {
    pub prints: bool,
    pub flaws: Vec<String>,
    pub short: Vec<String>,
}

/// printability's issue types in shop words.
fn plain(kind: &str) -> &str {
    match kind {
        "ExcessiveOverhang" => "overhang",
        "LongBridge" => "bridge",
        "ThinWall" => "thin wall",
        "SmallFeature" => "too small",
        "ExceedsBuildVolume" => "too big for the bed",
        "NotWatertight" | "NonManifold" | "SelfIntersecting" => "not a solid",
        "TrappedVolume" => "trapped pocket",
        other => other,
    }
}

fn describe(w: &DesignWarning) -> String {
    match w {
        DesignWarning::WallTooThin { part, thickness, min, .. } => format!("{part}: wall {thickness:.2} mm < {min}"),
        DesignWarning::HoleTooSmall { part, diameter, min, .. } => format!("{part}: hole {diameter:.2} mm < {min}"),
        other => format!("{other:?}"),
    }
}

fn bounds(m: &IndexedMesh) -> (Vector3<f64>, Vector3<f64>) {
    m.vertices.iter().fold((Vector3::repeat(f64::MAX), Vector3::repeat(f64::MIN)), |(lo, hi), p| (lo.inf(&p.coords), hi.sup(&p.coords)))
}

/// Enclosed volume (mm^3) of a closed, consistently wound mesh.
fn volume(m: &IndexedMesh) -> f64 {
    m.faces
        .iter()
        .map(|f| {
            let [a, b, c] = f.map(|i| m.vertices[i as usize].coords);
            a.dot(&b.cross(&c)) / 6.0
        })
        .sum()
}

/// Writes each part that prints to `dir/<key>_<part>.stl` (binary).
pub fn write_stls(report: &PrintReport, key: &str, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut out = vec![];
    for p in report.parts.iter().filter(|p| p.prints) {
        let path = dir.join(format!("{key}_{}.stl", p.part));
        save_stl(&p.mesh, &path, true).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        out.push(path);
    }
    Ok(out)
}

impl World {
    /// Upddayett printed `p` tonight: it's his item now, and the people in
    /// its `wants` want it. Returns the item.
    pub fn add_print(&mut self, p: &Print) -> usize {
        let upd = self.find_npc("upddayett").expect("Upddayett runs the place");
        let item = self.add_item(upd, p.item, p.base, p.owner_use);
        for &(who, base, use_) in p.wants {
            let npc = self.find_npc(who).unwrap_or_else(|| panic!("nobody called {who}"));
            self.add_want(npc, item, base, use_);
        }
        self.set_night(self.night.clone());
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::{TradeComputer, world};

    /// The fast parts: v1 fails for its flaw, v2 prints. (The sled and the
    /// feeder take ~25 s; `trade_cli print` runs them.)
    #[test]
    fn v1_fails_and_v2_prints() {
        for key in ["bracket", "hook"] {
            let p = find(key).unwrap();
            let v1 = check(&p.design(false), TOL);
            let v2 = check(&p.design(true), TOL);
            assert!(!v1.prints(), "{key} v1 should fail");
            assert!(v2.prints(), "{key} v2 should print: {:?}", v2.flaws());
        }
        let too_big = check(&find("bracket").unwrap().design(false), TOL).flaws();
        assert!(too_big.iter().any(|f| f.starts_with("too big")), "{too_big:?}");
    }

    /// A print joins the board as Upddayett's item, and someone wants it.
    #[test]
    fn a_print_joins_the_board() {
        let mut w = world::laundromat(1);
        let n = w.items.len();
        let item = w.add_print(find("hook").unwrap());
        assert_eq!(item, n);
        let ray = w.find_npc("Ray").unwrap();
        assert!(w.value[ray][item] > 0.0);
        let tc = TradeComputer::new(w, 5.0, 1.6);
        assert!(tc.cycles.iter().any(|c| c.legs.iter().any(|l| l.item == item)), "no trade moves the hook");
    }
}
