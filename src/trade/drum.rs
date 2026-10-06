//! The washer drum as a real tumbler (Step 5).
//!
//! The drum's inside is a `cf_design` Solid: a steel can with three lifter
//! paddles and an open front, closed by the door's glass bowl. Tonight's
//! trade items ride inside as free rigid bodies. `Mechanism::to_model` makes
//! the same Solids the sim-core colliders (SDF vs SDF, the drum concave), and
//! the game meshes them for rendering, so what you see is what collides.
//!
//! Units are cf-design's: mm, kg, s. The drum's axle is the y axis and the
//! door faces -y; gravity is -z. Settings come from `examples/probe_drum.rs`
//! (PLAN Step 5.0): a 10 mm SDF cell, 8 contacts per pair, and a 4 ms step with
//! softer contacts (solref 0.01).
//!
//! The model holds one body per item in the world, built once (~12 s, mostly
//! cf-design's 1 mm mass grid; FINDINGS cf-design). Items not in the drum
//! tonight park far outside it, where the broadphase never pairs them.

use cortenforge::cf_design::{JointDef, JointKind, Material, Mechanism, MechanismError, Part, Solid};
use cortenforge::sim::core::{Data, Model};
use nalgebra::{Point3, UnitQuaternion, Vector3};

/// Drum inner radius, inner half-depth and wall, mm.
pub const R: f64 = 250.0;
pub const HALF_DEPTH: f64 = 150.0;
pub const WALL: f64 = 10.0;
/// Radius of the drum's front opening (inside the lip), mm.
pub const OPENING: f64 = 200.0;
/// SDF grid cell, mm; it also sets the octree contact leaf (FINDINGS).
pub const CELL: f64 = 10.0;
/// Physics step, s. cf-design's mm default is 0.5 ms; 4 ms runs 2-3x
/// faster than 2 ms with about the same depth (probe_drum pool).
pub const DT: f64 = 0.004;
/// Contact time constant, s (cf-design's is 0.005; it must stay above 2x
/// the step).
pub const SOLREF: f64 = 0.01;
/// Contacts kept per SDF pair (sim-core's default is 50).
pub const MAX_CONTACTS: usize = 8;
/// Most items in the drum at once (each costs ~250 us a step).
pub const MAX_ITEMS: usize = 5;

/// The speed at which an item on the wall stops falling off: w^2 R = g.
pub fn critical() -> f64 {
    (9810.0 / R).sqrt()
}

/// Rotates a Solid built along z (cf-design's cylinder axis) onto the axle,
/// with +z going to the door (-y).
fn onto_axle(s: Solid) -> Solid {
    s.rotate(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::FRAC_PI_2))
}

/// The drum: a can with a lip around the door opening and three lifters.
pub fn drum_solid() -> Solid {
    let mut s = Solid::cylinder(R + WALL, HALF_DEPTH + WALL)
        .subtract(Solid::cylinder(R, HALF_DEPTH))
        .subtract(Solid::cylinder(OPENING, WALL).translate(Vector3::new(0.0, 0.0, HALF_DEPTH + WALL)));
    for k in 0..3 {
        let a = k as f64 * std::f64::consts::TAU / 3.0;
        // 50 mm tall radially, 24 mm thick, sunk 5 mm into the wall.
        let paddle = Solid::cuboid(Vector3::new(25.0, 12.0, HALF_DEPTH))
            .translate(Vector3::new(R - 20.0, 0.0, 0.0))
            .rotate(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), a));
        s = s.union(paddle);
    }
    onto_axle(s)
}

/// The door's glass bowl: it fills the opening and pokes 30 mm into the drum.
pub fn door_solid() -> Solid {
    onto_axle(Solid::cylinder(OPENING - 5.0, 25.0).translate(Vector3::new(0.0, 0.0, HALF_DEPTH + 5.0)))
}

/// How an item looks and weighs: its Solid (mm, centered), density (kg/m^3)
/// and color (sRGB).
pub struct Look {
    pub solid: Solid,
    pub density: f64,
    pub color: [f32; 3],
}

fn look(solid: Solid, density: f64, color: [f32; 3]) -> Look {
    Look { solid, density, color }
}

fn slab(x: f64, y: f64, z: f64) -> Solid {
    Solid::cuboid(Vector3::new(x, y, z))
}

/// A rounded box with outer half-extents (x, y, z).
fn pillow(x: f64, y: f64, z: f64, r: f64) -> Solid {
    slab(x - r, y - r, z - r).round(r)
}

/// Every item's shape, by its name in `world` (real sizes, squeezed to fit
/// a 500 mm drum where they don't). Anything unknown is a 80 mm box.
pub fn item_look(name: &str) -> Look {
    let has = |s: &str| name.contains(s);
    if has("Mtn Goo") {
        look(pillow(100.0, 66.0, 62.0, 6.0), 1350.0, [0.35, 0.95, 0.1])
    } else if has("phone") {
        look(pillow(36.0, 75.0, 5.0, 2.0), 2000.0, [0.12, 0.12, 0.14])
    } else if has("kale") {
        // A bag, not an ellipsoid: ellipsoid fields are only bounds and cost
        // the octree 3x (probe_drum pool).
        look(pillow(70.0, 50.0, 40.0, 30.0), 200.0, [0.15, 0.55, 0.2])
    } else if has("hub motor") {
        look(Solid::cylinder(70.0, 40.0), 3500.0, [0.2, 0.2, 0.22])
    } else if has("casters") {
        let wheel = || Solid::cylinder(38.0, 12.0);
        let pair = wheel().translate(Vector3::new(-40.0, 0.0, 0.0)).union(wheel().translate(Vector3::new(40.0, 0.0, 0.0)));
        look(pair.clone().translate(Vector3::new(0.0, 0.0, -13.0)).union(pair.translate(Vector3::new(0.0, 0.0, 13.0))), 1200.0, [0.55, 0.55, 0.6])
    } else if has("18650") {
        look(pillow(27.0, 33.0, 18.0, 8.0), 2200.0, [0.2, 0.4, 0.9])
    } else if has("charger") {
        look(pillow(30.0, 30.0, 15.0, 4.0), 1500.0, [0.95, 0.95, 0.95])
    } else if has("soldering iron") {
        look(Solid::capsule(9.0, 85.0), 1500.0, [0.85, 0.15, 0.1])
    } else if has("inner tube") {
        look(Solid::torus(80.0, 18.0), 600.0, [0.08, 0.08, 0.08])
    } else if has("derailleur") {
        look(pillow(35.0, 30.0, 15.0, 3.0), 4000.0, [0.75, 0.75, 0.8])
    } else if has("sleeping bag") {
        look(Solid::capsule(60.0, 40.0), 300.0, [0.95, 0.45, 0.1])
    } else if has("birdseed") {
        look(pillow(110.0, 75.0, 50.0, 12.0), 1500.0, [0.75, 0.62, 0.4])
    } else if has("RTX") {
        look(pillow(130.0, 58.0, 25.0, 3.0), 2000.0, [0.15, 0.15, 0.15])
    } else if has("vinyl") {
        look(pillow(90.0, 90.0, 70.0, 4.0), 1200.0, [0.6, 0.35, 0.2])
    } else if has("library card") {
        look(pillow(43.0, 27.0, 2.0, 1.0), 1400.0, [0.95, 0.85, 0.3])
    } else if has("Wi-Fi") {
        // The password, on a sticky note.
        look(slab(38.0, 38.0, 2.0), 900.0, [1.0, 0.95, 0.35])
    } else if has("adas polo") {
        look(pillow(110.0, 80.0, 25.0, 5.0), 900.0, [0.8, 0.8, 0.82])
    } else {
        look(pillow(40.0, 40.0, 40.0, 5.0), 1000.0, [0.6, 0.6, 0.6])
    }
}

/// Where item `k` waits when it isn't in the drum: far off to the side.
fn park(k: usize) -> Vector3<f64> {
    Vector3::new(5000.0 + 1000.0 * k as f64, 0.0, 0.0)
}

/// Items drop in here, one at a time, once the spot is clear.
const DROP_AT: Vector3<f64> = Vector3::new(0.0, 0.0, 40.0);
const DROP_CLEAR: f64 = 170.0;
const DROP_WAIT: f64 = 2.0;

/// Pinned items ride with the drum above this (x critical)...
const PIN_ABOVE: f64 = 1.5;
/// ...after this long there (s), and fall again below this.
const PIN_AFTER: f64 = 0.5;
const UNPIN_BELOW: f64 = 1.2;

struct Slot {
    qpos: usize,
    dof: usize,
    state: SlotState,
}

#[derive(Clone, Copy, PartialEq)]
enum SlotState {
    Parked,
    /// Waiting its turn to drop in.
    Queued,
    In,
}

/// The drum and every item, stepped by sim-core.
pub struct Tumbler {
    model: Model,
    data: Data,
    drum_qpos: usize,
    drum_dof: usize,
    /// One slot per world item (same index).
    slots: Vec<Slot>,
    queue: Vec<usize>,
    last_drop: f64,
    /// Time spent above the pinning speed.
    fast_for: f64,
    /// While pinned: each item's pose in the drum's frame.
    pinned: Option<Vec<(usize, Vector3<f64>, UnitQuaternion<f64>)>>,
    time: f64,
}

impl Tumbler {
    /// Builds the drum with one body per item (by name). Slow: ~10 s.
    pub fn new(items: &[&str]) -> Result<Self, MechanismError> {
        Self::with_cell(items, CELL)
    }

    /// [`Self::new`] with another SDF cell (mm), for probing.
    pub fn with_cell(items: &[&str], cell: f64) -> Result<Self, MechanismError> {
        let steel = Material::new("steel", 7800.0);
        let glass = Material::new("glass", 2500.0);
        let mut b = Mechanism::builder("tumbler")
            .part(Part::new("drum", drum_solid(), steel))
            .joint(JointDef::new("axle", "world", "drum", JointKind::Revolute, Point3::origin(), Vector3::y()))
            .part(Part::new("door", door_solid(), glass))
            .joint(JointDef::new("hinge", "world", "door", JointKind::Fixed, Point3::origin(), Vector3::y()));
        for (k, name) in items.iter().enumerate() {
            let l = item_look(name);
            let part = format!("item{k}");
            b = b.part(Part::new(&part, l.solid, Material::new(&part, l.density))).joint(JointDef::new(
                format!("{part}_free"),
                "world",
                &part,
                JointKind::Free,
                Point3::from(park(k)),
                Vector3::z(),
            ));
        }
        let mut model = b.build().to_model(cell, 8.0)?;
        model.timestep = DT;
        for s in &mut model.geom_solref {
            s[0] = SOLREF;
        }
        model.sdf_maxcontact = MAX_CONTACTS;
        let body = |name: &str| (0..model.nbody).find(|&i| model.body_name[i].as_deref() == Some(name)).expect("body");
        let drum = body("drum");
        // The drum and the door never touch (5 mm apart), but cf-design only
        // filters parent-child pairs, and the octree would search that whole
        // ring every step. Bits: shell = 2, items = 1; items hit everything.
        let door = body("door");
        for g in 0..model.ngeom {
            if model.geom_contype[g] == 0 {
                continue; // visual mesh
            }
            let shell = model.geom_body[g] == drum || model.geom_body[g] == door;
            (model.geom_contype[g], model.geom_conaffinity[g]) = if shell { (2, 1) } else { (1, 3) };
        }
        let slots = (0..items.len())
            .map(|k| {
                let b = body(&format!("item{k}"));
                let j = model.body_jnt_adr[b];
                Slot { qpos: model.jnt_qpos_adr[j], dof: model.jnt_dof_adr[j], state: SlotState::Parked }
            })
            .collect();
        let drum_j = model.body_jnt_adr[drum];
        let data = model.make_data();
        Ok(Self {
            drum_qpos: model.jnt_qpos_adr[drum_j],
            drum_dof: model.jnt_dof_adr[drum_j],
            model,
            data,
            slots,
            queue: vec![],
            last_drop: f64::NEG_INFINITY,
            fast_for: 0.0,
            pinned: None,
            time: 0.0,
        })
    }

    /// Puts these items in the drum (they drop in one at a time) and parks
    /// the rest. Items already in stay where they are.
    pub fn set_items(&mut self, items: &[usize]) {
        self.pinned = None;
        for k in 0..self.slots.len() {
            let want = items.contains(&k);
            match (self.slots[k].state, want) {
                (SlotState::Parked, true) => {
                    self.slots[k].state = SlotState::Queued;
                    self.queue.push(k);
                }
                (SlotState::Queued | SlotState::In, false) => {
                    self.slots[k].state = SlotState::Parked;
                    self.queue.retain(|&q| q != k);
                }
                _ => {}
            }
        }
        // Keep the order asked for.
        self.queue.sort_by_key(|q| items.iter().position(|i| i == q));
    }

    /// Items in the drum now.
    pub fn items_in(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.slots.len()).filter(|&k| self.slots[k].state == SlotState::In)
    }

    /// Drum angle about its axle (rad).
    pub fn drum_angle(&self) -> f64 {
        self.data.qpos[self.drum_qpos]
    }

    /// Item `k`'s pose (mm, drum frame = world frame), if it is in the drum.
    pub fn pose(&self, k: usize) -> Option<(Vector3<f64>, UnitQuaternion<f64>)> {
        let s = self.slots.get(k)?;
        (s.state == SlotState::In).then(|| self.qpose(k))
    }

    /// Items collide with each other (default) or only with the drum and door.
    /// For probing what the item-item pairs cost.
    pub fn items_collide(&mut self, on: bool) {
        for g in 0..self.model.ngeom {
            if self.model.geom_contype[g] == 1 {
                self.model.geom_conaffinity[g] = if on { 3 } else { 2 };
            }
        }
    }

    /// Another step (s) and contact time constant (s; keep it above 2x the
    /// step). For probing.
    pub fn set_step(&mut self, dt: f64, solref: f64) {
        self.model.timestep = dt;
        for s in &mut self.model.geom_solref {
            s[0] = solref;
        }
    }

    /// Deepest contact right now (mm).
    pub fn deepest(&self) -> f64 {
        self.data.contacts[..self.data.ncon].iter().map(|c| c.depth).fold(0.0, f64::max)
    }

    pub fn dt(&self) -> f64 {
        self.model.timestep
    }

    pub fn pinned(&self) -> bool {
        self.pinned.is_some()
    }

    fn qpose(&self, k: usize) -> (Vector3<f64>, UnitQuaternion<f64>) {
        let a = self.slots[k].qpos;
        let q = &self.data.qpos;
        let p = Vector3::new(q[a], q[a + 1], q[a + 2]);
        let r = UnitQuaternion::new_normalize(nalgebra::Quaternion::new(q[a + 3], q[a + 4], q[a + 5], q[a + 6]));
        (p, r)
    }

    fn set_qpose(&mut self, k: usize, p: Vector3<f64>, r: UnitQuaternion<f64>) {
        let a = self.slots[k].qpos;
        let q = &mut self.data.qpos;
        q[a] = p.x;
        q[a + 1] = p.y;
        q[a + 2] = p.z;
        q[a + 3] = r.w;
        q[a + 4] = r.i;
        q[a + 5] = r.j;
        q[a + 6] = r.k;
    }

    /// Sets item `k` moving rigidly with a drum turning at `omega`.
    fn ride(&mut self, k: usize, omega: f64) {
        let (p, r) = self.qpose(k);
        let w = Vector3::y() * omega;
        let v = w.cross(&p);
        // Free joints: linear velocity in world frame, angular in the body's.
        let wl = r.inverse() * w;
        let d = self.slots[k].dof;
        for i in 0..3 {
            self.data.qvel[d + i] = v[i];
            self.data.qvel[d + 3 + i] = wl[i];
        }
    }

    fn still(&mut self, k: usize) {
        let d = self.slots[k].dof;
        for i in 0..6 {
            self.data.qvel[d + i] = 0.0;
        }
    }

    /// One step (`DT` unless [`Self::set_step`] changed it) with the drum turning at `omega` (rad/s).
    pub fn step(&mut self, omega: f64) -> Result<(), cortenforge::sim::core::StepError> {
        let dt = self.model.timestep;
        self.time += dt;
        let fast = omega.abs() >= PIN_ABOVE * critical();
        self.fast_for = if fast { self.fast_for + dt } else { 0.0 };

        // Pinned: the items don't move in the drum's frame, so skip the physics.
        if self.pinned.is_some() && omega.abs() >= UNPIN_BELOW * critical() {
            let angle = self.drum_angle() + omega * dt;
            self.data.qpos[self.drum_qpos] = angle;
            let turn = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle);
            for (k, p, r) in self.pinned.clone().unwrap_or_default() {
                self.set_qpose(k, turn * p, turn * r);
            }
            return Ok(());
        }
        if let Some(pinned) = self.pinned.take() {
            for (k, _, _) in pinned {
                self.ride(k, omega);
            }
        }
        if self.fast_for >= PIN_AFTER && self.queue.is_empty() {
            let turn = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.drum_angle()).inverse();
            let held = self.items_in().map(|k| (k, self.qpose(k))).map(|(k, (p, r))| (k, turn * p, turn * r)).collect();
            self.pinned = Some(held);
        }

        // Drop the next item once the spot is clear (or it has waited long enough).
        if let Some(&k) = self.queue.first() {
            let clear = self.items_in().all(|i| (self.qpose(i).0 - DROP_AT).norm() > DROP_CLEAR);
            if clear || self.time - self.last_drop > DROP_WAIT {
                self.queue.remove(0);
                self.slots[k].state = SlotState::In;
                self.last_drop = self.time;
                let spin = UnitQuaternion::from_euler_angles(0.0, 0.0, 0.7 * k as f64);
                self.set_qpose(k, DROP_AT, spin);
                self.ride(k, omega);
            }
        }
        for k in 0..self.slots.len() {
            if self.slots[k].state != SlotState::In {
                self.set_qpose(k, park(k), UnitQuaternion::identity());
                self.still(k);
            }
        }
        self.data.qvel[self.drum_dof] = omega;
        self.data.step(&self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade::world;

    /// Tonight's items drop in, tumble, pin on the spin and fall again, and
    /// nothing leaves the drum.
    #[test]
    fn items_tumble_and_stay_in() {
        let w = world::laundromat(1);
        let names: Vec<&str> = w.items.iter().map(|i| i.name).collect();
        let mut t = Tumbler::new(&names).expect("build");
        let pick = [0, 1, 2, 3, 13];
        t.set_items(&pick);
        let inside = |t: &Tumbler| {
            t.items_in().all(|k| {
                let p = t.qpose(k).0;
                (p.x * p.x + p.z * p.z).sqrt() < R && p.y.abs() < HALF_DEPTH + 30.0
            })
        };
        let run = |t: &mut Tumbler, omega: f64, secs: f64| {
            for _ in 0..(secs / t.dt()) as usize {
                t.step(omega).expect("step");
                assert!(inside(t), "an item left the drum at t={}", t.time);
            }
        };
        run(&mut t, 0.6 * critical(), 4.0);
        assert_eq!(t.items_in().count(), pick.len(), "all dropped in");
        run(&mut t, 2.0 * critical(), 1.0);
        assert!(t.pinned(), "pinned on the spin");
        run(&mut t, 0.3 * critical(), 1.0);
        assert!(!t.pinned());
        // A new night swaps the load.
        t.set_items(&[4, 5]);
        run(&mut t, 0.6 * critical(), 1.5);
        assert_eq!(t.items_in().collect::<Vec<_>>(), vec![4, 5]);
    }
}
