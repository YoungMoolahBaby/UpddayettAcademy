//! End-user probe of cortenforge 0.9.0 soft-body stack.
//! Usage: cargo run --release --example probe_soft -- [drop|couple|grad|all]

use cortenforge::sim::coupling::StaggeredCoupling;
use cortenforge::sim::ml_chassis::Tensor;
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::soft::{
    CpuNewtonSolver, CpuTet4NHSolver, LmConfig, MaterialField, Mesh, NullContact,
    PenaltyRigidContactSolver, SceneInitial, SdfMeshedTetMesh, SoftScene, Solver, SolverConfig,
    Tet4, VertexId, referenced_vertices,
};
use std::time::Instant;

// ---------------------------------------------------------------- part 1
/// Drop an SDF-meshed NeoHookean "jelly" sphere onto the default rigid floor.
fn drop_jelly(radius: f64, cell: f64, mu: f64, n_steps: usize, lm: bool, null_contact: bool) {
    let t0 = Instant::now();
    let (mesh, bc, initial, contact) =
        SoftScene::dropping_sphere(radius, cell, radius + 0.012, MaterialField::uniform(mu, 4.0 * mu))
            .expect("mesh");
    let t_mesh = t0.elapsed().as_secs_f64();
    let n_tets = mesh.n_tets();
    let n_verts = mesh.n_vertices();
    let refd: Vec<VertexId> = referenced_vertices(&mesh);
    let n_ref = refd.len();
    let n_dof = 3 * n_verts;

    let mut cfg = SolverConfig::skeleton();
    cfg.dt = 1e-3;
    cfg.gravity_z = -9.81;
    cfg.max_newton_iter = 50;
    if lm {
        cfg.lm_regularization = Some(LmConfig::fork_b());
    }
    let dt = cfg.dt;
    let t1 = Instant::now();
    type StepFn = Box<dyn Fn(&[f64], &[f64]) -> Result<(Vec<f64>, usize), String>>;
    let step_fn: StepFn = if null_contact {
        let solver: CpuTet4NHSolver<SdfMeshedTetMesh> =
            CpuNewtonSolver::new(Tet4, mesh, NullContact, cfg, bc);
        Box::new(move |x: &[f64], v: &[f64]| {
            solver
                .try_replay_step(
                    &Tensor::from_slice(x, &[n_dof]),
                    &Tensor::from_slice(v, &[n_dof]),
                    &Tensor::from_slice(&[], &[0]),
                    dt,
                )
                .map(|st| (st.x_final, st.iter_count))
                .map_err(|e| format!("{e:?}").chars().take(160).collect())
        })
    } else {
        let solver: PenaltyRigidContactSolver<SdfMeshedTetMesh> =
            CpuNewtonSolver::new(Tet4, mesh, contact, cfg, bc);
        Box::new(move |x: &[f64], v: &[f64]| {
            solver
                .try_replay_step(
                    &Tensor::from_slice(x, &[n_dof]),
                    &Tensor::from_slice(v, &[n_dof]),
                    &Tensor::from_slice(&[], &[0]),
                    dt,
                )
                .map(|st| (st.x_final, st.iter_count))
                .map_err(|e| format!("{e:?}").chars().take(160).collect())
        })
    };
    let t_build = t1.elapsed().as_secs_f64();

    let SceneInitial { x_prev, v_prev } = initial;
    let mut x: Vec<f64> = x_prev.as_slice().to_vec();
    let mut v: Vec<f64> = v_prev.as_slice().to_vec();
    let z0 = min_z(&x, &refd);
    let mut iters = 0usize;
    let mut max_it = 0usize;
    let mut zmin_hist = f64::INFINITY;
    let mut done = 0usize;
    let mut fail = String::new();
    let t2 = Instant::now();
    for k in 0..n_steps {
        match step_fn(&x, &v) {
            Ok((xf, it)) => {
                iters += it;
                max_it = max_it.max(it);
                v = xf.iter().zip(&x).map(|(a, b)| (a - b) / dt).collect();
                x = xf;
                zmin_hist = zmin_hist.min(min_z(&x, &refd));
                done += 1;
            }
            Err(e) => {
                fail = format!(" FAILED at step {k}: {e}");
                break;
            }
        }
    }
    let wall = t2.elapsed().as_secs_f64();
    let n_steps = done.max(1);
    println!(
        "DROP{} lm={lm} r={radius} cell={cell} mu={mu:.0e}: tets={n_tets} verts={n_verts} (ref {n_ref}) | mesh {:.1} ms, solver build {:.1} ms | {done} steps in {:.3} s = {:.2} ms/step = {:.0} steps/s | newton avg {:.1} max {max_it} | minz {z0:.4} -> lowest {zmin_hist:.5} final {:.5}{fail}",
        if null_contact { "(no-contact freefall)" } else { "" },
        t_mesh * 1e3,
        t_build * 1e3,
        wall,
        wall / n_steps as f64 * 1e3,
        n_steps as f64 / wall,
        iters as f64 / n_steps as f64,
        min_z(&x, &refd),
    );
}

fn min_z(x: &[f64], refd: &[VertexId]) -> f64 {
    refd.iter()
        .map(|&v| x[3 * v as usize + 2])
        .fold(f64::INFINITY, f64::min)
}

// ---------------------------------------------------------------- part 2
const PLATEN_MJCF: &str = r#"<mujoco>
  <option gravity="0 0 -9.81" timestep="0.001"/>
  <worldbody>
    <body name="platen" pos="0 0 0.108">
      <freejoint/>
      <geom type="box" size="0.06 0.06 0.005" mass="0.2"/>
    </body>
  </worldbody>
</mujoco>"#;

const BALL_MJCF: &str = r#"<mujoco>
  <option gravity="0 0 -9.81" timestep="0.001"/>
  <worldbody>
    <body name="ball" pos="0 0 0.135">
      <freejoint/>
      <geom type="sphere" size="0.03" mass="0.3"/>
    </body>
  </worldbody>
</mujoco>"#;

fn build(mjcf: &str, n_per_edge: usize, mu: f64, sphere: Option<f64>) -> StaggeredCoupling {
    let model = load_model(mjcf).expect("mjcf");
    let mut data = model.make_data();
    data.forward(&model).expect("forward");
    let clearance = if sphere.is_some() { 0.03 } else { 0.005 };
    let c = StaggeredCoupling::new(
        model, data, 1, clearance, n_per_edge, 0.1, mu, 1.0e-3, 3.0e4, 1.0e-2, 60.0,
    );
    match sphere {
        Some(r) => c.with_sphere_collider(r),
        None => c,
    }
}

fn couple(label: &str, mjcf: &str, n_per_edge: usize, sphere: Option<f64>, n_steps: usize) {
    let t0 = Instant::now();
    let mut c = build(mjcf, n_per_edge, 3.0e4, sphere);
    let t_build = t0.elapsed().as_secs_f64();
    let nv = c.soft_positions().len() / 3;
    let t1 = Instant::now();
    let mut last = None;
    let mut fmax = 0.0f64;
    for _ in 0..n_steps {
        let s = c.step();
        fmax = fmax.max(s.force_on_soft.norm());
        last = Some(s);
    }
    let wall = t1.elapsed().as_secs_f64();
    let s = last.unwrap();
    println!(
        "COUPLE {label} n_per_edge={n_per_edge} (verts={nv}, tets~{}): build {:.1} ms | {n_steps} steps {:.3} s = {:.2} ms/step = {:.0} steps/s | final z={:.5} fz={:.3} N (max |f| {:.3} N, weight {:.3} N) peakP={:.0} Pa",
        6 * n_per_edge.pow(3),
        t_build * 1e3,
        wall,
        wall / n_steps as f64 * 1e3,
        n_steps as f64 / wall,
        s.rigid_z,
        s.force_on_soft.z,
        fmax,
        if label == "ball" { 0.3 * 9.81 } else { 0.2 * 9.81 },
        s.peak_pressure,
    );
}

// ---------------------------------------------------------------- part 3
fn final_z(mu: f64, n_per_edge: usize, n: usize) -> f64 {
    let mut c = build(PLATEN_MJCF, n_per_edge, mu, None);
    let mut z = c.data().xpos[1].z;
    for _ in 0..n {
        z = c.step().rigid_z;
    }
    z
}

fn grad(n_per_edge: usize, n: usize) {
    let mu0 = 3.0e4;
    let t0 = Instant::now();
    let (z_n, g_mu) = build(PLATEN_MJCF, n_per_edge, mu0, None).coupled_trajectory_material_gradient(n, 0);
    let t_mu = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let (_, g_lam) = build(PLATEN_MJCF, n_per_edge, mu0, None).coupled_trajectory_material_gradient(n, 1);
    let t_lam = t1.elapsed().as_secs_f64();
    // block ties lambda = 4 mu, so total d/dmu = d/dmu + 4 d/dlambda
    let g = g_mu + 4.0 * g_lam;

    let t2 = Instant::now();
    let z_fwd = final_z(mu0, n_per_edge, n);
    let t_fwd = t2.elapsed().as_secs_f64();
    let mut fd_line = String::new();
    for rel in [1e-2, 1e-3] {
        let eps = mu0 * rel;
        let zp = final_z(mu0 + eps, n_per_edge, n);
        let zm = final_z(mu0 - eps, n_per_edge, n);
        let fd = (zp - zm) / (2.0 * eps);
        fd_line += &format!(
            " | FD(rel {rel:e}) = {fd:.6e} (z+ {zp:.7} z- {zm:.7}; rel err {:.2e}, sign {})",
            (g - fd).abs() / fd.abs().max(1e-30),
            if fd.signum() == g.signum() { "OK" } else { "MISMATCH" }
        );
    }
    println!(
        "GRAD n_per_edge={n_per_edge} n_steps={n}: z_N={z_n:.7} (fwd-only {z_fwd:.7}) dz/dmu_partial={g_mu:.6e} dz/dlam={g_lam:.6e} dz/dmu_total={g:.6e} | grad time mu {:.3} s + lam {:.3} s (fwd-only rollout {:.3} s){fd_line}",
        t_mu, t_lam, t_fwd
    );

    // Single-step material gradient (non-mutating probe)
    let c = build(PLATEN_MJCF, n_per_edge, mu0, None);
    let h = c.data().xpos[1].z - 0.005;
    let t3 = Instant::now();
    let (vz, gs) = c.coupled_step_material_gradient(h, 0);
    let t_s = t3.elapsed().as_secs_f64();
    let eps = mu0 * 1e-3;
    let fd = (c.coupled_step_material_vz(h, 0, mu0 + eps) - c.coupled_step_material_vz(h, 0, mu0 - eps)) / (2.0 * eps);
    println!(
        "GRAD1 single-step n_per_edge={n_per_edge}: vz'={vz:.6e} dvz'/dmu(partial)={gs:.6e} FD={fd:.6e} ({:.3} ms)",
        t_s * 1e3
    );

    // Open-loop control gradient: dz_N/du_k
    let controls = vec![0.0; n];
    let t4 = Instant::now();
    let (zc, gu) = build(PLATEN_MJCF, n_per_edge, mu0, None).coupled_trajectory_control_gradient(&controls);
    let t_c = t4.elapsed().as_secs_f64();
    let k = 0usize;
    let du = 0.05;
    let mut up = controls.clone();
    up[k] += du;
    let mut um = controls.clone();
    um[k] -= du;
    let fdu = (build(PLATEN_MJCF, n_per_edge, mu0, None).coupled_trajectory_control_z(&up)
        - build(PLATEN_MJCF, n_per_edge, mu0, None).coupled_trajectory_control_z(&um))
        / (2.0 * du);
    println!(
        "GRADU control n_per_edge={n_per_edge} n={n}: z_N={zc:.7} dz/du_0={:.6e} FD={fdu:.6e} dz/du_last={:.6e} ({:.3} s)",
        gu[k],
        gu[n - 1],
        t_c
    );
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    if which == "drop" || which == "all" {
        // radius 2 cm jelly, ~gelatin-ish shear modulus, then the crate's tested silicone value
        let arg2 = std::env::args().nth(2).unwrap_or_default();
        if arg2 == "fine" {
            drop_jelly(0.02, 2e-3, 2.0e5, 100, true, false);
        } else {
            drop_jelly(0.02, 6e-3, 2.0e5, 200, false, true);
            for &cell in &[6e-3, 4e-3, 3e-3] {
                drop_jelly(0.02, cell, 2.0e5, 200, false, false);
            }
            drop_jelly(0.02, 6e-3, 2.0e5, 200, true, false);
            drop_jelly(0.02, 6e-3, 2.0e4, 200, false, false);
            drop_jelly(0.02, 6e-3, 2.0e4, 200, true, false);
        }
    }
    if which == "couple" || which == "all" {
        for &n in &[4usize, 6, 8, 10] {
            couple("platen", PLATEN_MJCF, n, None, 200);
        }
        for &n in &[4usize, 8] {
            couple("ball", BALL_MJCF, n, Some(0.03), 200);
        }
    }
    if which == "grad" || which == "all" {
        grad(4, 20);
        grad(8, 20);
        grad(4, 100);
    }
}
