//! Gap probe for `sim-soft` and `sim-coupling` 0.9.x: one check per finding,
//! so the same run tells us what 0.9.2 fixed.
//!
//! cargo run --release --example gaps_sim_soft
//!
//! Each check prints OPEN (the gap is still there), FIXED (a known gap that
//! now behaves) or ok (never a gap; kept to catch regressions), with what
//! it saw. A panic inside a check is caught and reported, never fatal. Checks
//! that need the library's stderr run in a child process. The ids match the
//! entries in docs/FINDINGS.md.

use std::panic::{AssertUnwindSafe, catch_unwind};

use cortenforge::sim::coupling::StaggeredCoupling;
use cortenforge::sim::ml_chassis::{Tape, Tensor};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::soft::{
    Aabb3, BoundaryConditions, ConstantField, CpuNewtonSolver, CpuTet4NHSolver, DifferenceSdf,
    HandBuiltTetMesh, MaterialField, Mesh, MeshingHints, NeoHookean, NullContact,
    PenaltyRigidContactSolver, ResidualCorrections, RewardBreakdown, RewardWeights, Sdf,
    SdfMeshedTetMesh, SingleTetMesh, SoftScene, Solver, SolverConfig, SolverFailure, SphereSdf,
    Tet4, Tet10Mesh, Vec3, referenced_vertices,
};
use nalgebra::{Matrix3, Point3, Vector3};

/// A named check.
type Check = (&'static str, fn() -> Seen);

/// What a check found.
enum Seen {
    /// The gap is still there.
    Open(String),
    /// A known gap that now behaves as it should.
    Fixed(String),
    /// Never was a gap: kept so a regression shows up.
    Fine(String),
}

/// Runs `f`, turning a panic into its message.
fn run<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panic".into())
    })
}

/// First line of a panic message, cut short.
fn short(p: &str) -> String {
    p.lines().next().unwrap_or("").chars().take(110).collect()
}

/// A solver failure in one line (the `Debug` form dumps the whole state).
fn fail(e: &SolverFailure) -> String {
    match e {
        SolverFailure::ArmijoStall { last_iter, last_r_norm, .. } => format!("ArmijoStall (iter {last_iter}, |r| {last_r_norm:.2e})"),
        SolverFailure::NewtonIterCap { max_iter, last_r_norm, .. } => format!("NewtonIterCap (cap {max_iter}, |r| {last_r_norm:.2e})"),
        SolverFailure::DoublyFailedFactor { last_iter, .. } => format!("DoublyFailedFactor (iter {last_iter})"),
        SolverFailure::ValidityViolation { tet_id, message } => format!("ValidityViolation (tet {tet_id}: {})", short(message)),
        #[allow(unreachable_patterns)]
        _ => "another failure".into(),
    }
}

/// Runs this probe again with `args`, returning (stdout, stderr), or a note if
/// it died or ran past `secs`.
fn child(args: &[&str], secs: u64) -> Result<(String, String), String> {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().expect("exe");
    let mut c = Command::new(exe).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("child");
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = c.try_wait().expect("wait") {
            let out = std::io::read_to_string(c.stdout.take().expect("stdout")).unwrap_or_default();
            let err = std::io::read_to_string(c.stderr.take().expect("stderr")).unwrap_or_default();
            return if status.success() {
                Ok((out.trim().to_string(), err))
            } else {
                Err(format!("process died ({status}): {}", err.lines().next().unwrap_or("")))
            };
        }
        if start.elapsed().as_secs() >= secs {
            let _ = c.kill();
            return Err(format!("hangs (killed after {secs} s)"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

// ------------------------------------------------------------ one tet
//
// The crate's `one_tet_cube`: a 0.1 m tet, vertices 0-2 pinned, vertex 3
// pulled along +z by theta (newtons).

type TetSolver = CpuTet4NHSolver<SingleTetMesh>;

fn tet_cfg() -> SolverConfig {
    let mut cfg = SolverConfig::skeleton();
    cfg.dt = 0.01;
    cfg.max_newton_iter = 50;
    cfg
}

fn tet_with(field: &MaterialField, cfg: SolverConfig, bc: Option<BoundaryConditions>) -> (TetSolver, Vec<f64>) {
    let (_, scene_bc, initial) = SoftScene::one_tet_cube();
    let x = initial.x_prev.as_slice().to_vec();
    (CpuNewtonSolver::new(Tet4, SingleTetMesh::new(field), NullContact, cfg, bc.unwrap_or(scene_bc)), x)
}

fn tet(cfg: SolverConfig) -> (TetSolver, Vec<f64>) {
    tet_with(&MaterialField::uniform(1.0e5, 4.0e5), cfg, None)
}

fn tstep(s: &TetSolver, x: &[f64], v: &[f64], theta: &[f64], dt: f64) -> Result<(Vec<f64>, usize), SolverFailure> {
    s.try_replay_step(
        &Tensor::from_slice(x, &[x.len()]),
        &Tensor::from_slice(v, &[v.len()]),
        &Tensor::from_slice(theta, &[theta.len()]),
        dt,
    )
    .map(|st| (st.x_final, st.iter_count))
}

fn replay_panics_on_stall() -> Seen {
    let mut cfg = tet_cfg();
    cfg.max_newton_iter = 1;
    let (s, x) = tet(cfg);
    let v = vec![0.0; 12];
    let tried = match tstep(&s, &x, &v, &[20.0], 0.01) {
        Ok(_) => "try_replay_step converges".to_string(),
        Err(e) => format!("try_replay_step returns {}", fail(&e)),
    };
    match run(|| s.replay_step(&Tensor::from_slice(&x, &[12]), &Tensor::from_slice(&v, &[12]), &Tensor::from_slice(&[20.0], &[1]), 0.01)) {
        Err(p) => Seen::Open(format!("max_newton_iter 1: {tried}; replay_step panics: {}", short(&p))),
        Ok(_) => Seen::Fixed(format!("{tried}; replay_step no longer panics")),
    }
}

fn config_dt_ignored() -> Seen {
    let mut slow = tet_cfg();
    slow.dt = 1.0;
    let (a, x) = tet(slow);
    let (b, _) = tet(tet_cfg());
    let v = vec![0.0; 12];
    let za = tstep(&a, &x, &v, &[20.0], 0.01).map(|r| r.0[11]);
    let zb = tstep(&b, &x, &v, &[20.0], 0.01).map(|r| r.0[11]);
    match (za, zb) {
        (Ok(za), Ok(zb)) if za == zb => Seen::Open(format!(
            "cfg.dt 1.0 and 0.01 give the same step at the call's dt 0.01 (z {za:.6}); current_dt() still says {}",
            a.current_dt()
        )),
        (Ok(za), Ok(zb)) => Seen::Fixed(format!("cfg.dt now matters: z {za:.6} vs {zb:.6}")),
        _ => Seen::Open("a step failed".into()),
    }
}

fn newton_cap_off_by_one() -> Seen {
    let (s, x) = tet(tet_cfg());
    let v = vec![0.0; 12];
    let n = match tstep(&s, &x, &v, &[20.0], 0.01) {
        Ok((_, n)) => n,
        Err(e) => return Seen::Open(format!("the free solve failed: {}", fail(&e))),
    };
    let mut cfg = tet_cfg();
    cfg.max_newton_iter = n;
    let (capped, _) = tet(cfg);
    match tstep(&capped, &x, &v, &[20.0], 0.01) {
        Err(e) => Seen::Open(format!("converges in iter_count {n}, but max_newton_iter {n} fails: {}", fail(&e))),
        Ok(_) => Seen::Fixed(format!("max_newton_iter {n} is enough for a {n}-iteration solve")),
    }
}

fn zero_cap_and_zero_tol_at_rest() -> Seen {
    let v = vec![0.0; 12];
    let mut zero_cap = tet_cfg();
    zero_cap.max_newton_iter = 0;
    let (a, x) = tet(zero_cap);
    let mut zero_tol = tet_cfg();
    zero_tol.tol = 0.0;
    let (b, _) = tet(zero_tol);
    let show = |r: Result<(Vec<f64>, usize), SolverFailure>| match r {
        Ok((_, n)) => format!("Ok ({n} iterations)"),
        Err(e) => fail(&e),
    };
    let ra = tstep(&a, &x, &v, &[0.0], 0.01);
    let rb = tstep(&b, &x, &v, &[0.0], 0.01);
    let bad = ra.is_err() || rb.is_err();
    let text = format!("at rest with no load: max_newton_iter 0 gives {}; tol 0 gives {}", show(ra), show(rb));
    if bad { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn try_step_panics_on_bad_input() -> Seen {
    let (s, x) = tet(tet_cfg());
    let v = vec![0.0; 12];
    let mut out = Vec::new();
    let mut panics = 0;
    let cases: Vec<(&str, Box<dyn Fn() -> Result<(Vec<f64>, usize), SolverFailure>>)> = vec![
        ("dt 0", Box::new(|| tstep(&s, &x, &v, &[20.0], 0.0))),
        ("dt -0.01", Box::new(|| tstep(&s, &x, &v, &[20.0], -0.01))),
        ("x too short", Box::new(|| tstep(&s, &x[..9], &v, &[20.0], 0.01))),
        ("theta length 2", Box::new(|| tstep(&s, &x, &v, &[20.0, 1.0], 0.01))),
        ("dt inf", Box::new(|| tstep(&s, &x, &v, &[20.0], f64::INFINITY))),
    ];
    for (name, f) in cases {
        match run(f) {
            Err(p) => {
                panics += 1;
                out.push(format!("{name} panics ({})", short(&p)));
            }
            Ok(Err(e)) => out.push(format!("{name} -> {}", fail(&e))),
            Ok(Ok(_)) => out.push(format!("{name} -> Ok")),
        }
    }
    let text = format!("try_replay_step: {}", out.join("; "));
    if panics > 0 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn nan_state() -> Seen {
    let (s, x) = tet(tet_cfg());
    let mut v = vec![0.0; 12];
    v[11] = f64::NAN;
    let mut xn = x.clone();
    xn[11] = f64::NAN;
    let show = |r: Result<Result<(Vec<f64>, usize), SolverFailure>, String>| match r {
        Err(p) => format!("panics ({})", short(&p)),
        Ok(Err(e)) => fail(&e),
        Ok(Ok(_)) => "Ok".into(),
    };
    let rv = show(run(|| tstep(&s, &x, &v, &[20.0], 0.01)));
    let rx = show(run(|| tstep(&s, &xn, &vec![0.0; 12], &[20.0], 0.01)));
    let text = format!("NaN in v_prev: {rv}; NaN in x_prev: {rx}");
    if rv.starts_with("ArmijoStall") { Seen::Open(format!("{text} (a NaN input reads as a line-search stall)")) } else { Seen::Fixed(text) }
}

fn empty_theta_trap() -> Seen {
    let bc = BoundaryConditions { pinned_vertices: vec![0, 1, 2], roller_vertices: Vec::new(), loaded_vertices: Vec::new() };
    let (s, x) = tet_with(&MaterialField::uniform(1.0e5, 4.0e5), tet_cfg(), Some(bc));
    let v = vec![0.0; 12];
    let empty = Tensor::<f64>::zeros(&[]);
    let len = empty.as_slice().len();
    match run(|| s.try_replay_step(&Tensor::from_slice(&x, &[12]), &Tensor::from_slice(&v, &[12]), &empty, 0.01).map(|_| ())) {
        Err(p) => Seen::Open(format!("no loaded vertices: Tensor::zeros(&[]) has length {len}, and the step panics: {}", short(&p))),
        Ok(_) => Seen::Fixed(format!("Tensor::zeros(&[]) (length {len}) is accepted as an empty theta")),
    }
}

fn negative_density() -> Seen {
    let mut cfg = tet_cfg();
    cfg.gravity_z = -9.81;
    cfg.density = -1030.0;
    let (s, x) = tet(cfg);
    match run(|| tstep(&s, &x, &vec![0.0; 12], &[0.0], 0.01)) {
        Ok(Ok((xf, _))) => {
            let dz = xf[11] - x[11];
            let text = format!("density -1030 under gravity -9.81: accepted; the free vertex moves {dz:+.2e} m (up = negative mass)");
            if dz > 0.0 { Seen::Open(text) } else { Seen::Fixed(text) }
        }
        Ok(Err(e)) => Seen::Fixed(format!("refused: {}", fail(&e))),
        Err(p) => Seen::Fixed(format!("refused with a panic: {}", short(&p))),
    }
}

fn material_unchecked() -> Seen {
    let built = run(|| SingleTetMesh::new(&MaterialField::uniform(-1.0e5, 4.0e5)));
    let lame = run(|| NeoHookean::from_lame(-1.0e5, 4.0e5));
    let young = run(|| NeoHookean::from_young_poisson(1.0e6, 0.47)).err();
    let mut out = vec![
        format!("uniform(mu -1e5, lambda 4e5) {}", if built.is_ok() { "builds a mesh" } else { "is refused" }),
        format!("from_lame(-1e5, 4e5) {}", if lame.is_ok() { "builds" } else { "is refused" }),
    ];
    out.push(match young {
        Some(p) => format!("from_young_poisson(1e6, 0.47) panics ({}) though the fbar doc offers nu up to 0.49", short(&p)),
        None => "from_young_poisson(1e6, 0.47) builds".into(),
    });
    if built.is_ok() || lame.is_ok() { Seen::Open(out.join("; ")) } else { Seen::Fixed(out.join("; ")) }
}

fn stretch_gate() -> Seen {
    let (s, mut x) = tet(tet_cfg());
    x[11] = 0.21; // vertex 3 pulled to 2.1x its rest height
    match run(|| tstep(&s, &x, &vec![0.0; 12], &[0.0], 0.01)) {
        Ok(Err(e)) => Seen::Open(format!("start stretched 2.1x: {} (the 2x gate is fixed, no setter)", fail(&e))),
        Ok(Ok(_)) => Seen::Fixed("a 2.1x stretch steps".into()),
        Err(p) => Seen::Open(format!("start stretched 2.1x: panics ({})", short(&p))),
    }
}

fn fbar_gradient_panics() -> Seen {
    let mut cfg = tet_cfg();
    cfg.fbar = true;
    let (mut s, x) = tet(cfg);
    let v = vec![0.0; 12];
    let fwd = tstep(&s, &x, &v, &[20.0], 0.01).is_ok();
    let r = run(|| {
        let mut tape = Tape::new();
        let theta = tape.param_tensor(Tensor::from_slice(&[20.0], &[1]));
        s.try_step(&mut tape, &Tensor::from_slice(&x, &[12]), &Tensor::from_slice(&v, &[12]), theta, 0.01).map(|_| ())
    });
    let f = if fwd { "the forward step works" } else { "the forward step fails" };
    match r {
        Err(p) => Seen::Open(format!("fbar on: {f}; try_step (the taped step) panics instead of Err: {}", short(&p))),
        Ok(Err(e)) => Seen::Fixed(format!("fbar on: {f}; try_step returns Err: {}", fail(&e))),
        Ok(Ok(())) => Seen::Fixed(format!("fbar on: {f}; gradients work")),
    }
}

/// Child for `lu_fallback_prints`: a tet with negative shear modulus.
fn lu_child() {
    let (s, x) = tet_with(&MaterialField::uniform(-1.0e5, 4.0e5), tet_cfg(), None);
    match run(|| tstep(&s, &x, &vec![0.0; 12], &[20.0], 0.1)) {
        Ok(Ok(_)) => println!("steps Ok"),
        Ok(Err(e)) => println!("{}", fail(&e)),
        Err(p) => println!("panics ({})", short(&p)),
    }
}

fn lu_fallback_prints() -> Seen {
    match child(&["--lu"], 30) {
        Ok((out, err)) => {
            let lines: Vec<&str> = err.lines().filter(|l| l.contains("sim-soft:")).collect();
            match lines.first() {
                Some(l) => Seen::Open(format!(
                    "mu -1e5 tet ({out}): the library wrote {} line(s) to stderr, e.g. \"{}\"",
                    lines.len(),
                    l.chars().take(90).collect::<String>()
                )),
                None => Seen::Fixed(format!("mu -1e5 tet ({out}): nothing on stderr")),
            }
        }
        Err(e) => Seen::Open(format!("the child {e}")),
    }
}

// ------------------------------------------------------------ meshes

fn sphere_hints(r: f64, cell: f64) -> MeshingHints {
    let e = 1.5 * r;
    MeshingHints { bbox: Aabb3::new(Vec3::new(-e, -e, -e), Vec3::new(e, e, e)), cell_size: cell, material_field: None }
}

fn sphere_mesh(r: f64, cell: f64) -> SdfMeshedTetMesh {
    SdfMeshedTetMesh::from_sdf(&SphereSdf { radius: r }, &sphere_hints(r, cell)).expect("sphere meshes")
}

fn mesher_keeps_grid() -> Seen {
    let (mesh, _, _, _) = SoftScene::dropping_sphere(0.02, 3e-3, 0.035, MaterialField::uniform(2.0e5, 8.0e5)).expect("mesh");
    let used = referenced_vertices(&mesh).len();
    let all = mesh.n_vertices();
    let text = format!("dropping_sphere r 2 cm, cell 3 mm: {all} vertices kept, {used} used by tets ({:.0}% unused)", 100.0 * (all - used) as f64 / all as f64);
    if all > used { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn mesher_not_scale_invariant() -> Seen {
    let big = run(|| SdfMeshedTetMesh::from_sdf(&SphereSdf { radius: 0.02 }, &sphere_hints(0.02, 6e-3)).map(|m| m.n_tets()));
    let small = run(|| SdfMeshedTetMesh::from_sdf(&SphereSdf { radius: 2e-5 }, &sphere_hints(2e-5, 6e-6)).map(|m| m.n_tets()));
    let show = |r: &Result<Result<usize, _>, String>| match r {
        Ok(Ok(n)) => format!("{n} tets"),
        Ok(Err(e)) => format!("{e:?}"),
        Err(p) => format!("panics ({})", short(p)),
    };
    let text = format!("sphere r 2 cm / cell 6 mm: {}; the same shape 1000x smaller: {}", show(&big), show(&small));
    match (&big, &small) {
        (Ok(Ok(a)), Ok(Ok(b))) if a == b => Seen::Fixed(text),
        _ => Seen::Open(text),
    }
}

fn lattice_uncapped() -> Seen {
    // Both ask for terabytes up front, so they fail before touching memory.
    let mut out = Vec::new();
    let mut aborts = 0;
    for (r, cell) in [("0.1", "1e-4"), ("0.1", "1e-12")] {
        match child(&["--mesh", r, cell], 60) {
            Ok((o, _)) => out.push(format!("r {r} cell {cell}: {o}")),
            Err(e) => {
                aborts += 1;
                out.push(format!("r {r} cell {cell}: {e}"));
            }
        }
    }
    let text = out.join("; ");
    if aborts > 0 { Seen::Open(format!("{text} (an abort, not a MeshingError)")) } else { Seen::Fixed(text) }
}

fn inverted_bbox_panics() -> Seen {
    let mut hints = sphere_hints(0.02, 6e-3);
    std::mem::swap(&mut hints.bbox.min, &mut hints.bbox.max);
    match run(|| SdfMeshedTetMesh::from_sdf(&SphereSdf { radius: 0.02 }, &hints).map(|m| m.n_tets())) {
        Err(p) => Seen::Open(format!("bbox min > max panics ({}); Aabb3::new's doc promises MeshingError::EmptyMesh", short(&p))),
        Ok(r) => Seen::Fixed(format!("bbox min > max returns {:?}", r.map_err(|e| format!("{e:?}")))),
    }
}

fn projected_nodes_nan() -> Seen {
    let mesh = sphere_mesh(0.02, 6e-3);
    let v = referenced_vertices(&mesh)[0];
    let moved = mesh.with_projected_nodes(&[(v, Vec3::new(f64::NAN, 0.0, 0.0))], 0.5);
    let p = moved.positions()[v as usize];
    let text = format!("with_projected_nodes to a NaN target: vertex {v} is now ({:.3}, {:.3}, {:.3})", p.x, p.y, p.z);
    if p.iter().any(|c| !c.is_finite()) { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn projected_nodes_out_of_range() -> Seen {
    let mesh = sphere_mesh(0.02, 6e-3);
    match run(|| mesh.with_projected_nodes(&[(u32::MAX, Vec3::zeros())], 0.5).n_tets()) {
        Ok(_) => Seen::Open("a move on vertex u32::MAX is skipped silently; the doc says it panics".into()),
        Err(p) => Seen::Fixed(format!("vertex u32::MAX now refused: {}", short(&p))),
    }
}

/// An SDF that is NaN everywhere.
struct NanSdf;

impl Sdf for NanSdf {
    fn eval(&self, _p: Point3<f64>) -> f64 {
        f64::NAN
    }
    fn grad(&self, _p: Point3<f64>) -> Vector3<f64> {
        Vector3::zeros()
    }
}

fn difference_sdf_hessian() -> Seen {
    let p = Point3::new(0.1, 0.0, 0.0);
    let sphere: Matrix3<f64> = SphereSdf { radius: 0.1 }.hessian(p);
    let diff = DifferenceSdf::new(Box::new(SphereSdf { radius: 0.1 }), Box::new(SphereSdf { radius: 0.04 })).hessian(p);
    let text = format!("on the outer surface: sphere hessian norm {:.2}, the difference's {:.2}", sphere.norm(), diff.norm());
    if diff.norm() == 0.0 && sphere.norm() > 0.0 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn difference_sdf_hides_nan() -> Seen {
    let d = DifferenceSdf::new(Box::new(SphereSdf { radius: 0.1 }), Box::new(NanSdf));
    let v = d.eval(Point3::new(0.05, 0.0, 0.0));
    let text = format!("sphere minus an all-NaN SDF evaluates to {v}");
    if v.is_finite() { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn yeoh_on_hand_built() -> Seen {
    let field = MaterialField::from_yeoh_fields(
        Box::new(ConstantField::new(1.0e5)),
        Box::new(ConstantField::new(1.0e4)),
        Box::new(ConstantField::new(4.0e5)),
    );
    match run(|| HandBuiltTetMesh::uniform_block(2, 0.1, &field).n_tets()) {
        Err(p) => Seen::Open(format!("uniform_block with a Yeoh field panics: {}", short(&p))),
        Ok(n) => Seen::Fixed(format!("uniform_block with a Yeoh field builds {n} tets")),
    }
}

fn tet10_double_enrich() -> Seen {
    let block = HandBuiltTetMesh::uniform_block(2, 0.1, &MaterialField::uniform(1.0e5, 4.0e5));
    let once: Tet10Mesh = Tet10Mesh::from_tet4(&block);
    match run(|| Tet10Mesh::from_tet4(&once)) {
        Ok(twice) => {
            let text = format!(
                "from_tet4 of a Tet10 mesh is accepted in release: {} corners (the Tet10 mesh had {} corners, {} vertices)",
                twice.n_corners(),
                once.n_corners(),
                once.n_vertices()
            );
            if twice.n_corners() == once.n_vertices() { Seen::Open(text) } else { Seen::Fixed(text) }
        }
        Err(p) => Seen::Fixed(format!("re-enriching is refused: {}", short(&p))),
    }
}

fn reward_stubs() -> Seen {
    let stub = run(|| RewardBreakdown::default().apply_residuals(&ResidualCorrections).peak_bound);
    let w = RewardWeights { pressure_uniformity: 1.0, coverage: 1.0, peak_bound: 1.0, stiffness_bound: 1.0 };
    let diverged = RewardBreakdown { pressure_uniformity: f64::NAN, coverage: f64::NAN, peak_bound: f64::NAN, stiffness_bound: f64::NAN };
    let poor = RewardBreakdown { pressure_uniformity: -1.0, coverage: -1.0, peak_bound: -1.0, stiffness_bound: -1.0 };
    let (sd, sp) = (diverged.score_with(&w), poor.score_with(&w));
    let mut out = Vec::new();
    if let Err(p) = &stub {
        out.push(format!("apply_residuals panics ({})", short(p)));
    }
    if sd.is_finite() && sd > sp {
        out.push(format!("an all-NaN (diverged) breakdown scores {sd}, above a poor one at {sp}"));
    }
    if out.is_empty() { Seen::Fixed(format!("apply_residuals works; all-NaN scores {sd}")) } else { Seen::Open(out.join("; ")) }
}

// ------------------------------------------------------------ dropping sphere

/// Drops a sphere onto the penalty floor with friction `mu` (stick band
/// `eps_v`) and reports the first failure, if any.
fn friction_drop(mu: f64, eps_v: f64) -> String {
    let (mesh, bc, initial, contact) = SoftScene::dropping_sphere(0.02, 6e-3, 0.0215, MaterialField::uniform(2.0e5, 8.0e5)).expect("mesh");
    let mut cfg = SolverConfig::skeleton();
    cfg.dt = 1e-3;
    cfg.gravity_z = -9.81;
    cfg.max_newton_iter = 50;
    cfg.friction_mu = mu;
    cfg.friction_eps_v = eps_v;
    let s: PenaltyRigidContactSolver<SdfMeshedTetMesh> = CpuNewtonSolver::new(Tet4, mesh, contact, cfg, bc);
    let mut x = initial.x_prev.as_slice().to_vec();
    let mut v = initial.v_prev.as_slice().to_vec();
    let n = x.len();
    for k in 0..40 {
        let r = run(|| s.try_replay_step(&Tensor::from_slice(&x, &[n]), &Tensor::from_slice(&v, &[n]), &Tensor::from_slice(&[], &[0]), 1e-3).map(|st| st.x_final));
        match r {
            Ok(Ok(xf)) => {
                if xf.iter().any(|c| !c.is_finite()) {
                    return format!("NaN at step {k}");
                }
                v = xf.iter().zip(&x).map(|(a, b)| (a - b) / 1e-3).collect();
                x = xf;
            }
            Ok(Err(e)) => return format!("fails at step {k}: {}", fail(&e)),
            Err(p) => return format!("panics at step {k}: {}", short(&p)),
        }
    }
    "40 steps Ok".into()
}

fn friction_default_eps_v() -> Seen {
    let zero = friction_drop(0.5, 0.0);
    let set = friction_drop(0.5, 1e-3);
    let none = friction_drop(0.0, 0.0);
    let text = format!("sphere dropped onto the floor: friction 0.5 with the default eps_v 0 {zero}; with eps_v 1e-3 {set}; no friction {none}");
    if zero != "40 steps Ok" && none == "40 steps Ok" { Seen::Open(text) } else { Seen::Fixed(text) }
}

// ------------------------------------------------------------ sim-coupling
//
// The platen scene from probe_soft: a 0.2 kg box on a free joint over a
// soft cube (edge 0.1 m, bottom pinned). n_per_edge 2 keeps each step cheap.

fn platen_xml(pos: &str, option: &str) -> String {
    format!(
        r#"<mujoco><option gravity="0 0 -9.81" {option}/><worldbody>
  <body name="platen" pos="{pos}"><freejoint/><geom type="box" size="0.06 0.06 0.005" mass="0.2"/></body>
</worldbody></mujoco>"#
    )
}

const TS: &str = r#"timestep="0.001""#;

struct Couple<'a> {
    xml: &'a str,
    body: usize,
    clearance: f64,
    n: usize,
    mu: f64,
    dt: f64,
    damping: f64,
    forward: bool,
}

impl Couple<'_> {
    fn new(xml: &str) -> Couple<'_> {
        Couple { xml, body: 1, clearance: 0.005, n: 2, mu: 3.0e4, dt: 1.0e-3, damping: 60.0, forward: true }
    }
    fn build(&self) -> StaggeredCoupling {
        let model = load_model(self.xml).expect("mjcf");
        let mut data = model.make_data();
        if self.forward {
            data.forward(&model).expect("forward");
        }
        StaggeredCoupling::new(model, data, self.body, self.clearance, self.n, 0.1, self.mu, self.dt, 3.0e4, 1.0e-2, self.damping)
    }
}

fn odd_n_per_edge() -> Seen {
    let xml = platen_xml("0 0 0.108", TS);
    match run(|| Couple { n: 3, ..Couple::new(&xml) }.build().soft_positions().len()) {
        Err(p) => Seen::Open(format!("n_per_edge 3 panics with \"{}\" (new's # Panics doesn't say even sizes only)", short(&p))),
        Ok(n) => Seen::Fixed(format!("n_per_edge 3 builds ({} vertices)", n / 3)),
    }
}

fn dt_vs_model_timestep() -> Seen {
    let xml = platen_xml("0.05 0.05 0.5", ""); // no timestep: sim-mjcf's 0.002
    let mut c = Couple { damping: 0.0, ..Couple::new(&xml) }.build();
    for _ in 0..10 {
        c.step();
    }
    let vz = c.data().qvel[2];
    let text = format!("dt 1e-3 passed, MJCF timestep 0.002: after 10 free-fall steps vz = {vz:.4} (the dt passed gives -0.0981, the model's -0.1962)");
    if (vz + 0.1962).abs() < 1e-3 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn non_free_body_panics() -> Seen {
    let xml = r#"<mujoco><option gravity="0 0 -9.81" timestep="0.001"/><worldbody>
  <body name="arm" pos="0.05 0.05 0.108"><joint type="hinge" axis="0 1 0"/><geom type="box" size="0.06 0.06 0.005" mass="0.2"/></body>
</worldbody></mujoco>"#;
    match run(|| Couple::new(xml).build().step().rigid_z) {
        Err(p) => Seen::Open(format!("a body on one hinge (nv 1): new accepts it, step panics: {}", short(&p))),
        Ok(z) => Seen::Fixed(format!("a hinged body steps (z {z:.4})")),
    }
}

fn platen_not_first() -> Seen {
    let alone = platen_xml("0.05 0.05 0.5", TS);
    let behind = r#"<mujoco><option gravity="0 0 -9.81" timestep="0.001"/><worldbody>
  <body name="cart" pos="1 1 0"><joint type="slide" axis="1 0 0"/><geom type="sphere" size="0.01" mass="0.1"/></body>
  <body name="platen" pos="0.05 0.05 0.5"><freejoint/><geom type="box" size="0.06 0.06 0.005" mass="0.2"/></body>
</worldbody></mujoco>"#;
    let mut a = Couple::new(&alone).build();
    let mut b = Couple { xml: behind, body: 2, ..Couple::new(&alone) }.build();
    for _ in 0..50 {
        a.step();
        b.step();
    }
    let (va, vb) = (a.data().qvel[2], b.data().qvel[3]);
    let text = format!("rigid_damping 60, 50 steps of free fall: platen vz {va:.4} alone, {vb:.4} behind a one-joint body (damping read from qvel[2])");
    if (va - vb).abs() > 1e-6 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn body_out_of_range() -> Seen {
    let xml = platen_xml("0 0 0.108", TS);
    match run(|| Couple { body: 7, ..Couple::new(&xml) }.build()) {
        Err(p) => Seen::Fixed(format!("body 7 refused at new: {}", short(&p))),
        Ok(mut c) => match run(|| c.step().rigid_z) {
            Err(p) => Seen::Open(format!("body 7 of 2: new accepts it, step panics: {}", short(&p))),
            Ok(_) => Seen::Open("body 7 of 2 steps".into()),
        },
    }
}

fn rigid_z_lags() -> Seen {
    let xml = platen_xml("0.05 0.05 0.5", TS);
    let mut c = Couple { damping: 0.0, ..Couple::new(&xml) }.build();
    let mut s = c.step();
    for _ in 0..9 {
        s = c.step();
    }
    let (q, x) = (c.data().qpos[2], c.data().xpos[1].z);
    let text = format!("after 10 steps: rigid_z {:.6}, xpos.z {x:.6}, qpos z {q:.6}", s.rigid_z);
    if (q - s.rigid_z).abs() > 1e-9 { Seen::Open(format!("{text} (rigid_z is one step behind)")) } else { Seen::Fixed(text) }
}

fn last_control_gradient_zero() -> Seen {
    let xml = platen_xml("0.05 0.05 0.5", TS);
    let (z, g) = Couple::new(&xml).build().coupled_trajectory_control_gradient(&[0.0; 5]);
    let text = format!("5 controls, no contact: z_N {z:.5}, dz/du = [{}]", g.iter().map(|x| format!("{x:.2e}")).collect::<Vec<_>>().join(", "));
    if g[4] == 0.0 && g[0] != 0.0 { Seen::Open(format!("{text}; the last control can't move z_N")) } else { Seen::Fixed(text) }
}

fn force_sign_doc() -> Seen {
    let xml = platen_xml("0.05 0.05 0.108", TS);
    let s = Couple::new(&xml).build().step();
    let text = format!("platen resting in the contact band: force_on_soft.z = {:+.3} N", s.force_on_soft.z);
    if s.force_on_soft.z < 0.0 {
        Seen::Open(format!("{text}: it's the force on the soft body; the doc says the force it exerts"))
    } else {
        Seen::Fixed(text)
    }
}

/// What a platen run saw.
struct PlatenRun {
    peak: f64,
    peak_step: usize,
    z: f64,
    fz: f64,
    /// Contact plane height above the block's top at the end.
    gap: f64,
    ms_per_step: f64,
}

/// Runs the platen scene (`n_per_edge` 4, d_hat 1 cm) from height `z0` at
/// step `dt` for `secs` of sim time.
fn platen_run(z0: f64, dt: f64, secs: f64) -> PlatenRun {
    let xml = platen_xml(&format!("0.05 0.05 {z0}"), &format!(r#"timestep="{dt}""#));
    let mut c = Couple { n: 4, dt, ..Couple::new(&xml) }.build();
    let steps = (secs / dt).round() as usize;
    let t = std::time::Instant::now();
    let (mut peak, mut peak_step, mut last) = (0.0f64, 0, None);
    for k in 0..steps {
        let s = c.step();
        if s.force_on_soft.norm() > peak {
            peak = s.force_on_soft.norm();
            peak_step = k;
        }
        last = Some(s);
    }
    let s = last.expect("steps");
    let top = c.soft_positions().chunks(3).map(|p| p[2]).fold(f64::MIN, f64::max);
    PlatenRun {
        peak,
        peak_step,
        z: s.rigid_z,
        fz: s.force_on_soft.z,
        gap: s.rigid_z - 0.005 - top,
        ms_per_step: t.elapsed().as_secs_f64() * 1e3 / steps as f64,
    }
}

fn platen_starts_in_band() -> Seen {
    let r = platen_run(0.108, 1e-3, 0.3);
    let text = format!(
        "the crate's platen scene (contact plane 3 mm above the block, d_hat 1 cm): {:.0} N at step {} on a 1.96 N platen; after 0.3 s z {:.4} (started 0.108), fz {:+.2} N ({:.1} ms/step)",
        r.peak, r.peak_step, r.z, r.fz, r.ms_per_step
    );
    if r.peak > 10.0 * 1.96 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn platen_dropped_floats() -> Seen {
    let r = platen_run(0.125, 1e-3, 0.5);
    let text = format!(
        "platen dropped from 1 cm above the band, 0.5 s: peak {:.1} N (step {}), ends fz {:+.2} N with the contact plane {:.1} mm above the block",
        r.peak,
        r.peak_step,
        r.fz,
        r.gap * 1e3
    );
    if r.gap > 1e-3 || r.peak > 10.0 * 1.96 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn soft_block_weightless() -> Seen {
    let xml = platen_xml("0.05 0.05 0.5", TS);
    let mut c = Couple::new(&xml).build();
    let x0 = c.soft_positions().to_vec();
    for _ in 0..20 {
        c.step();
    }
    let moved = c.soft_positions().iter().zip(&x0).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    let text = format!("no contact, 20 steps: the soft block's largest move is {moved:.1e} m (gravity_z 0 in the coupling's SolverConfig, no setter)");
    if moved == 0.0 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn plane_is_infinite() -> Seen {
    let over = platen_xml("0.05 0.05 0.108", TS);
    let off = platen_xml("0.5 0.5 0.108", TS);
    let f_over = Couple::new(&over).build().step().force_on_soft.z;
    let f_off = Couple::new(&off).build().step().force_on_soft.z;
    let text = format!("platen over the block: fz {f_over:+.2} N; moved 0.45 m off to the side: fz {f_off:+.2} N");
    if f_off != 0.0 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn sphere_ignores_xy() -> Seen {
    let ball = |pos: &str| {
        format!(
            r#"<mujoco><option gravity="0 0 -9.81" timestep="0.001"/><worldbody>
  <body name="ball" pos="{pos}"><freejoint/><geom type="sphere" size="0.03" mass="0.3"/></body>
</worldbody></mujoco>"#
        )
    };
    let (over, off) = (ball("0.05 0.05 0.135"), ball("0.5 0.5 0.135"));
    let f = |xml: &str| Couple { clearance: 0.03, ..Couple::new(xml) }.build().with_sphere_collider(0.03).step().force_on_soft.z;
    let (a, b) = (f(&over), f(&off));
    let text = format!("ball over the block: fz {a:+.2} N; ball 0.45 m off to the side: fz {b:+.2} N");
    if b != 0.0 { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn kinematic_plane_crushes() -> Seen {
    let xml = platen_xml("0.05 0.05 0.5", TS);
    let mut c = Couple::new(&xml).build();
    match run(|| c.step_kinematic()) {
        Err(p) => Seen::Open(format!("step_kinematic with the default plane collider, platen 0.4 m clear: panics ({})", short(&p))),
        Ok(s) if s.force_on_soft.norm() > 1.0 => {
            Seen::Open(format!("step_kinematic with the default plane collider, platen 0.4 m clear: |f| {:.0} N (the plane sits at z 0)", s.force_on_soft.norm()))
        }
        Ok(s) => Seen::Fixed(format!("step_kinematic with the plane: |f| {:.2} N", s.force_on_soft.norm())),
    }
}

fn gradients_consume_scene() -> Seen {
    let xml = platen_xml("0.05 0.05 0.108", TS);
    let mut c = Couple::new(&xml).build();
    let (z1, g1) = c.coupled_trajectory_material_gradient(5, 0);
    let (z2, g2) = c.coupled_trajectory_material_gradient(5, 0);
    let text = format!("the same call twice on one coupling: z {z1:.6} then {z2:.6}, dz/dmu {g1:.3e} then {g2:.3e}");
    if z1 != z2 { Seen::Open(format!("{text} (each call advances the scene, and it isn't Clone)")) } else { Seen::Fixed(text) }
}

fn friction_gradient_panics() -> Seen {
    // Platen 0.5 mm above the band, undamped: contact within ~10 steps.
    let xml = platen_xml("0.05 0.05 0.1155", TS);
    let grad = |mu: f64, eps_v: f64| {
        let mut c = Couple { damping: 0.0, ..Couple::new(&xml) }.build();
        if mu > 0.0 {
            c = c.with_friction(mu, eps_v);
        }
        match run(|| c.coupled_trajectory_material_gradient(30, 0)) {
            Ok((_, g)) => format!("dz/dmu {g:.3e}"),
            Err(p) => format!("panics ({})", short(&p).chars().take(70).collect::<String>()),
        }
    };
    let (none, f1, f2) = (grad(0.0, 0.0), grad(0.5, 1e-3), grad(0.5, 1e-1));
    let text = format!("30 steps into contact: no friction {none}; with_friction(0.5, 1e-3) {f1}; (0.5, 0.1) {f2}");
    if f1.starts_with("panics") || f2.starts_with("panics") { Seen::Open(text) } else { Seen::Fixed(text) }
}

fn nan_mu_accepted() -> Seen {
    let xml = platen_xml("0.05 0.05 0.108", TS);
    match run(|| Couple { mu: f64::NAN, ..Couple::new(&xml) }.build()) {
        Err(p) => Seen::Fixed(format!("mu NaN refused at new: {}", short(&p))),
        Ok(mut c) => match run(|| c.step()) {
            Err(p) => Seen::Open(format!("mu NaN: new accepts it; step panics ({})", short(&p))),
            Ok(s) => Seen::Open(format!("mu NaN: new accepts it; step returns fz {} z {}", s.force_on_soft.z, s.rigid_z)),
        },
    }
}

fn new_skips_forward() -> Seen {
    let xml = platen_xml("0.05 0.05 0.108", TS);
    let with = Couple::new(&xml).build().step().force_on_soft.z;
    let without = run(|| Couple { forward: false, ..Couple::new(&xml) }.build().step().force_on_soft.z);
    match without {
        Ok(f) if (f - with).abs() > 1e-6 => Seen::Open(format!("first step fz {with:+.2} N after data.forward(), {f:+.2} N without (new doesn't run it)")),
        Ok(f) => Seen::Fixed(format!("first step fz {f:+.2} N either way")),
        Err(p) => Seen::Open(format!("first step fz {with:+.2} N after data.forward(); without it, step panics ({})", short(&p))),
    }
}

fn platen_settles_short_runs() -> Seen {
    // probe_soft's validated case: a ball resting on the block carries its weight.
    let xml = r#"<mujoco><option gravity="0 0 -9.81" timestep="0.001"/><worldbody>
  <body name="ball" pos="0.05 0.05 0.135"><freejoint/><geom type="sphere" size="0.03" mass="0.3"/></body>
</worldbody></mujoco>"#;
    let mut c = Couple { clearance: 0.03, n: 4, ..Couple::new(xml) }.build().with_sphere_collider(0.03);
    let mut s = c.step();
    for _ in 0..199 {
        s = c.step();
    }
    let w = 0.3 * 9.81;
    let text = format!("ball (weight {w:.3} N) after 200 steps: fz {:+.3} N", s.force_on_soft.z);
    if (s.force_on_soft.z.abs() - w).abs() < 0.2 * w { Seen::Fine(text) } else { Seen::Open(text) }
}

fn locked_version(krate: &str) -> &'static str {
    let lock = include_str!("../Cargo.lock");
    let at = lock.find(&format!("name = \"{krate}\"\n")).expect("crate in Cargo.lock");
    let rest = &lock[at..];
    let v = rest.split("version = \"").nth(1).and_then(|s| s.split('"').next()).expect("version");
    Box::leak(v.to_string().into_boxed_str())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--mesh") {
        let (r, cell): (f64, f64) = (args[2].parse().expect("radius"), args[3].parse().expect("cell"));
        let t = std::time::Instant::now();
        match SoftScene::dropping_sphere(r, cell, r + 0.01, MaterialField::uniform(2.0e5, 8.0e5)) {
            Ok((mesh, ..)) => println!(
                "{} tets, {} vertices ({} used) in {:.1} s",
                mesh.n_tets(),
                mesh.n_vertices(),
                referenced_vertices(&mesh).len(),
                t.elapsed().as_secs_f64()
            ),
            Err(e) => println!("{e:?}"),
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--lu") {
        std::panic::set_hook(Box::new(|_| {}));
        lu_child();
        return;
    }
    // Caught panics are reported by the checks; keep the default hook quiet.
    std::panic::set_hook(Box::new(|_| {}));
    let checks: Vec<Check> = vec![
        // sim-soft: solver
        ("replay_step panics on a stall", replay_panics_on_stall),
        ("SolverConfig.dt ignored", config_dt_ignored),
        ("max_newton_iter off by one", newton_cap_off_by_one),
        ("cap 0 / tol 0 fail at rest", zero_cap_and_zero_tol_at_rest),
        ("try_replay_step panics on bad input", try_step_panics_on_bad_input),
        ("NaN state", nan_state),
        ("empty theta: Tensor::zeros(&[]) has length 1", empty_theta_trap),
        ("negative density", negative_density),
        ("material parameters unchecked", material_unchecked),
        ("stretch gate fixed at 2x", stretch_gate),
        ("F-bar gradient panics", fbar_gradient_panics),
        ("library prints to stderr", lu_fallback_prints),
        ("friction with the default eps_v", friction_default_eps_v),
        // sim-soft: meshes, SDFs, readouts
        ("SDF mesher keeps every grid vertex", mesher_keeps_grid),
        ("mesher not scale-invariant", mesher_not_scale_invariant),
        ("lattice size uncapped", lattice_uncapped),
        ("inverted bbox panics", inverted_bbox_panics),
        ("with_projected_nodes: NaN target", projected_nodes_nan),
        ("with_projected_nodes: vertex out of range", projected_nodes_out_of_range),
        ("DifferenceSdf drops the hessian", difference_sdf_hessian),
        ("DifferenceSdf hides NaN", difference_sdf_hides_nan),
        ("Yeoh field on a hand-built mesh", yeoh_on_hand_built),
        ("Tet10 enriched twice", tet10_double_enrich),
        ("reward breakdown stubs and NaN", reward_stubs),
        // sim-coupling
        ("odd n_per_edge", odd_n_per_edge),
        ("dt vs the MJCF timestep", dt_vs_model_timestep),
        ("body without a free joint", non_free_body_panics),
        ("platen not the first joint", platen_not_first),
        ("body index out of range", body_out_of_range),
        ("rigid_z one step behind", rigid_z_lags),
        ("last control gets no gradient", last_control_gradient_zero),
        ("force_on_soft sign", force_sign_doc),
        ("platen starts inside the contact band", platen_starts_in_band),
        ("dropped platen floats", platen_dropped_floats),
        ("soft block weightless", soft_block_weightless),
        ("contact plane infinite", plane_is_infinite),
        ("sphere collider ignores x/y", sphere_ignores_xy),
        ("step_kinematic with the plane collider", kinematic_plane_crushes),
        ("trajectory gradients consume the scene", gradients_consume_scene),
        ("gradient with friction", friction_gradient_panics),
        ("mu NaN accepted", nan_mu_accepted),
        ("new doesn't run forward", new_skips_forward),
        ("ball settles on the block", platen_settles_short_runs),
    ];
    let (mut open, mut fixed, mut fine) = (0, 0, 0);
    for (name, check) in checks {
        let t = std::time::Instant::now();
        let (tag, text) = match run(check) {
            Ok(Seen::Open(t)) => {
                open += 1;
                ("OPEN ", t)
            }
            Ok(Seen::Fixed(t)) => {
                fixed += 1;
                ("FIXED", t)
            }
            Ok(Seen::Fine(t)) => {
                fine += 1;
                ("ok   ", t)
            }
            Err(p) => {
                open += 1;
                ("OPEN ", format!("the check itself panicked: {}", short(&p)))
            }
        };
        let secs = t.elapsed().as_secs_f64();
        let slow = if secs > 2.0 { format!(" [{secs:.0} s]") } else { String::new() };
        println!("{tag} {name}: {text}{slow}");
    }
    println!(
        "\n{open} open, {fixed} fixed, {fine} fine (cortenforge-sim-soft {}, cortenforge-sim-coupling {})",
        locked_version("cortenforge-sim-soft"),
        locked_version("cortenforge-sim-coupling")
    );
}
