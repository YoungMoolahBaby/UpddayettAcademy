# CortenForge 0.9.0: end-user findings

Feedback gathered while using the published crates as an outside user would
(crates.io sources and docs only). Newest at the bottom of each section.

## Probe results

Probes live in `examples/` and run with `cargo run --release --example <name>`.

### `probe_therm` (sim-thermostat): feasible, fast, validated

- Kramers check (delta_v 3, kT 1, M 1): measured switching rate 7-12% below the
  KGH prediction, within the crate's 25% tolerance. dt 0.01 gives the same rate
  as dt 0.001.
- Scaling: about 0.35 us per element per step, linear in N; up to 128 elements
  ran faster than real time on one thread. The general rigid-body step
  dominates, not the coupling.
- Invertible AND gate (Camsari couplings): clamping C=1 gives A=B=1 about 97%
  (exact 96.4%). Clamping C=0 spreads over the three valid inputs, with a lean
  toward 000.
- 2x2 multiplier, 12 elements: factors 9 at 95% and 6 at 69% (exact 97% and
  82%). It first finds a valid factorization of 6 after a median of about 0.3 s
  wall time (p90 1.2 s).
- Low temperature (kT 0.3) freezes into the random starting state, so an
  anneal is needed. At kT >= 1, magnetization matches exact Ising within ~0.04.

### `probe_soft` (sim-soft + sim-coupling): accurate, slow, narrow

- Every gradient matched central finite differences: trajectory material
  gradient ~1e-7 relative error, single-step and control gradients exactly. A
  gradient costs about 1.2-1.7x a forward run per parameter.
- Ball on a soft block settles correctly (2.875 N of support against 2.94 N of weight).
- Speed: ~2,200 tets cost 25-45 ms/step with contact; cost grows much faster
  than mesh size (7x tets ~ 40x time). A 2-10 s compute budget fits about 500
  tets and a few hundred steps.
- `StaggeredCoupling` supports only a cube block with its bottom face fixed.
- The platen scene copied from the crate's tests spikes to 1024 N of contact
  force and launches the platen after ~200 steps (tuned only for 20-40 steps).
- No pressure, active-stress or rest-length actuation; only fixed per-vertex
  loads and pinned vertices. No soft-soft contact (self-contact listed as
  future). Friction exists but has no gradients.

### Step 1: trade computer (sim-thermostat), works

- 14-bit maximum-weight independent set (QUBO -> Ising -> coupled double
  wells): the i9 latch finds the exact optimum in 95% of 192 seeded runs, at
  0.38 s per anneal on one thread. Everything compiled first try from facade
  paths; `with_ctrl_temperature` made the anneal a one-liner per step.
- The Ising mapping holds only while couplings stay weak next to the
  barrier. With |J| x degree comparable to 8 dV (the well stiffness), strips
  deflect to |x| ~ 1.9, the `h` compensation over-shoots, and wrong states
  become metastable. That's expected soft-spin physics, but the crate doesn't
  warn about it (see friction log).

## Friction log

- The facade is easy: one crate, and `load_model` -> `make_data` -> `step` is a
  clean MuJoCo-style path. Both probes compiled first try using facade paths only.
- Top-level docs are thin: the `sim` README is 9 lines, the prelude exports
  only the coupling driver, and no crate ships an `examples/` folder. Docs cite
  repo files that aren't shipped (`docs/keystone/*`, the book).
- `sim-thermostat`: docs cite internal roadmap labels ("Phase 3", "D1",
  "spec §3", "Ch 32 §4.6", "Route 2"). Every code example is `ignore` with an
  undefined `model` (lib.rs:40-63, double_well.rs:38-55). Its tests' fixtures
  need sim-core's `test-fixtures` feature, which the facade doesn't forward;
  `therm_env::generate_mjcf` (therm_env builder.rs:22) is the workaround, found
  only by reading source. The therm_env README is boilerplate.
- `sim-thermostat`: no clamp API or gate/penalty library; couplings can't
  change after install; an `ExternalField` above ~1.54 * delta_v / x0
  silently deletes the well; `ExternalField::new` doesn't check its length
  (external_field.rs:38,62); `exact_distribution`/`GibbsSampler` cap at
  n <= 20 (ising.rs:37); the bit convention is documented only at
  ising.rs:17-34; `kramers_rate` counting convention unclear vs
  `kramers_rate_turnover` (double_well.rs:180-192).
- `sim-soft`: the SDF mesher keeps every grid vertex (18,696 kept, 561 used;
  sdf_meshed_tet_mesh.rs:26-30). `replay_step` panics on a solver stall
  (newton.rs:723) instead of returning an error; `try_replay_step` is easy to
  miss. `PenaltyRigidContact::with_params` is a "testing surface" and the
  default contact band is crate-private (penalty.rs:262-268).
  `StaggeredCoupling::new` takes 11 positional arguments, a magic body index of
  1, and hard-wires lambda = 4 mu (construct.rs:29-49). `Tensor` must come from
  `sim::ml_chassis`, unmentioned in sim-soft. Gradients are unavailable with
  friction, F-bar or Tet10. `sim_core::Data` isn't `Clone`, so gradient calls
  consume the scene. A stray `eprintln` ("faer LU fallback fired...") prints
  from the library.
- RL: the `Policy` / `DifferentiablePolicy` / `ValueFn` traits are a clean
  seam, but PPO calls per-sample `forward` / `log_prob_gradient` instead of the
  batch methods, which blocks GPU or SIMD backends from helping. SAC/TD3 batch
  their critic gradients. The `Algorithm` docs tell Bevy users to write their
  own training loops, but no published example shows how.
- `sim-thermostat`: `PairwiseCoupling` docs promise that a coupled bistable
  array's "equilibrium statistics match the Ising model"
  (pairwise_coupling.rs:8-10), validated only on a uniform-J 4-chain. They
  never state the condition: couplings and fields small compared with dV
  (positions stay near +-x0). In an antiferromagnetic QUBO with degree ~5,
  following the naive mapping gave 100% wrong answers. A one-line rule of
  thumb, or a helper that maps a QUBO/Ising problem to components and warns
  when |h| + sum|J| nears the well-flattening tilt, would have saved an hour
  of sweeps. A "latch the lowest-energy state seen" helper would also help:
  every annealing user needs one.
- `WellState::from_position` (well_state.rs:28) takes the threshold as a bare `f64` with no
  suggested default; 0.5 x x0 (from the crate's tests) worked.
- Windows 11 with Smart App Control enforcing: nothing builds (build scripts
  fail with os error 4551). Getting-started notes should say to turn it off.
  After that, the first release build of `cortenforge` took 2 min 18 s on a
  Ryzen 5 5600.
