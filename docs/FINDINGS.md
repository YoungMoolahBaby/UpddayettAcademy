# CortenForge 0.9.0: end-user findings

Feedback gathered while using the published crates as an outside user would
(crates.io sources and docs only).

How to read it:
- **Dates** are when a finding was logged (local time, from git history).
  In the build sections the date and crate are on each heading, since
  everything under it shares them. Newest at the bottom of each section.
- **The friction log** is grouped by crate. Each entry starts with its type
  and date: **bug** (wrong results or a crash), **docs** (missing or
  misleading docs), **API** (works, but awkward or missing a piece),
  **perf**, **setup**, or **works** (worth keeping as is). Bugs and doc gaps
  are the upstream fix list.

## Probe results

Probes live in `examples/` and run with `cargo run --release --example <name>`.

### `probe_therm` (sim-thermostat, 2026-10-04): feasible, fast, validated

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

### `probe_soft` (sim-soft + sim-coupling, 2026-10-04): accurate, slow, narrow

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

## Building the game

### Step 1: trade computer (sim-thermostat, 2026-10-05): works

- 14-bit maximum-weight independent set (QUBO -> Ising -> coupled double
  wells): the i9 latch finds the exact optimum in 95% of 192 seeded runs, at
  0.38 s per anneal on one thread. Everything compiled first try from facade
  paths; `with_ctrl_temperature` made the anneal a one-liner per step.
- The Ising mapping holds only while couplings stay weak next to the
  barrier. With |J| x degree comparable to 8 dV (the well stiffness), strips
  deflect to |x| ~ 1.9, the `h` compensation over-shoots, and wrong states
  become metastable. That's expected soft-spin physics, but the crate doesn't
  warn about it (see friction log).

### Step 2: Bevy game (2026-10-05): CortenForge plays well with others

- `Model`, `Data` and the installed `PassiveStack` are `Send + Sync`, so
  the whole CortenForge machine dropped straight into a Bevy `Resource` with
  no wrapper, channel or `NonSend`. Stepping it inside a Bevy system (up to
  ~1,700 steps per frame at high sim speed) costs a few ms; reading
  `data.qpos` each frame drives 140 strip segments, LEDs and the arrows.
- No new CortenForge friction in Step 2: zero CF API changes were needed.
- Other libraries, for the record: Bevy 0.19 `RectLight` silently needs the
  `area_light_luts` feature (only a runtime WARN). `bevy_egui` attaches its
  context to the first camera spawned; with a second (inset) camera you
  must set `EguiGlobalSettings::auto_create_primary_context = false` and
  tag the main camera `PrimaryEguiContext`.

### Step 3 so far (sim-thermostat, 2026-10-05): lessons from a real workload

- The thermostat holds up as the problem grows: 17-20 strip boards with
  ~80 springs still anneal in under a second, and the physics behaves like
  physics. Longer gift chains (2-3 recipients) make a glassy landscape and
  the hit rate falls (81% to 54% / 42% on Normal), and slower cooling buys
  it back (86% / 65% on Delicates). That trade-off is the lesson the game
  teaches, which is a good sign for the crate.
- Coupled-board design rules learned the hard way: keep fields under the
  well-flattening tilt (stiffer springs or stronger biases made results
  worse every time), round values so near-ties become exact ties, and
  judge answers by value, not by bits.
- The exact-solver NaN bug (below) is the only CortenForge defect found in
  Step 3; everything else was problem design on our side.
- Composition pays off (3.6, the Salties' magnet): a second
  `ExternalField` in the same `PassiveStack` just sums with the first
  (stack.rs iterates the components, and each adds into `qfrc_passive`).
  That's the cleanest way to model a disturbance the controller doesn't
  know about: the i9 keeps scoring with the clean problem, and the idle
  check finds the hidden field to 1e-3 by force balance on strips at rest.
  Two small gaps: the docs never say that two components of the same type
  can share a stack (a line in stack.rs would settle it), and reading the
  force balance meant re-deriving the double well's force from its formula
  (double_well.rs:208), since a component can't report its force at a given
  `qpos` without a `Model` and `Data`.
- Extending the crate is easy (smart Salties): there's no field you can
  switch on and off, but `PassiveComponent` is a public two-method trait
  (component.rs), so a coil that pushes only while `ctrl[0] > 0` took ten
  lines, and `PassiveStackBuilder::with_arc` took a boxed one. Averaging the
  force balance over a whole Langevin run recovered a hidden 0.8x field
  to within 0.1x, with a clean noise floor of ~0.12x after 1000 reads: the
  thermostat's noise is as zero-mean as advertised. A short "write your own
  component" example in the crate docs would make this discoverable; we
  found it by reading component.rs.
- A power cut is just the anneal schedule dropping to zero early, and the
  crate's Langevin dynamics make it behave like a real quench, with no
  special support needed.

## Friction log

### `cortenforge` (facade) and docs overall

- **works** (2026-10-04): the facade is easy: one crate, and `load_model`
  -> `make_data` -> `step` is a clean MuJoCo-style path. Both probes
  compiled first try using facade paths only.
- **docs** (2026-10-04): top-level docs are thin: the `sim` README is 9
  lines, the prelude exports only the coupling driver, and no crate ships an
  `examples/` folder. Docs cite repo files that aren't shipped
  (`docs/keystone/*`, the book).

### `sim-thermostat`

`cargo run --release --example gaps_thermostat` re-checks every entry
marked *(probe: ...)* and prints OPEN or FIXED, so one run against 0.9.2
shows what landed. On 0.9.0: 26 open, 3 fine. Entries without a probe come
from reading the source (file:line) and can't be checked at run time
(docs, missing API).

#### Noise and seeding (the Langevin thermostat)

- **bug** (2026-10-05): the noise for joints 8-15 at step s is the noise
  for joints 0-7 at step s+1: the block counter is `(traj << 32) | step`,
  and the thermostat adds the group number to it (prf.rs:144-146,
  langevin.rs:208-221). Marginals and equipartition still pass, but every
  array over 8 joints gets lag-1 cross-correlations. That's every board in
  this game (17-20 strips); we haven't measured an effect on the anneal.
  No test uses more than 6 joints. *(probe: noise reused across groups of 8
  joints)*
- **bug** (2026-10-05): the docs promise any distinct u64 `traj_id` gives a
  disjoint stream, but `traj_id << 32` drops the high 32 bits, so 0 and
  `1 << 32` are the same stream (prf.rs:144-145, langevin.rs:125-126).
  Seeding `traj_id` with the exported `prf::splitmix64` hits this. The
  step counter also wraps after 2^32 calls. *(probe: traj_id high 32 bits
  ignored)*
- **bug** (2026-10-05): the noise is indexed by calls to `apply`, kept in
  the stack inside the `Model`, not by sim step. So `Data::reset`, a fresh
  `make_data()` and every extra `forward()` (for sensors or rendering)
  don't replay an episode, and there's no API to rewind the counter
  (langevin.rs:197-199). `OscillatingField` restarts on reset
  (oscillating_field.rs:48-53), so the two components disagree. *(probe:
  reset doesn't replay the noise)*
- **bug** (2026-10-05): a cloned `Model` shares the installed stack and its
  counter (the callback is an `Arc`), so clones draw from one stream and
  aren't independent or replayable. The same applies to
  `BatchSim::new(Arc<Model>, n)`: the "structurally immune to
  parallel-vs-sequential defect" note (langevin.rs:43-49) doesn't hold
  there. *(probe: cloned model shares the noise)*
- **bug** (2026-10-05): the noise variance `2 gamma kT / h` assumes one
  `apply` per step. RK4 calls forward 4 times per step with fresh draws,
  so the bath runs at about a quarter of kT, silently: <v^2> 0.259 instead
  of 1.0 (Euler: 0.996). Nothing says the thermostat needs Euler.
  *(probe: RK4 shrinks the noise)*
- **docs** (2026-10-05): the explicit damping `-gamma v` goes unstable past
  gamma dt / M of about 2 (gamma 3000 at dt 0.001: velocity 4.8e3 after 200
  steps). The docs mention only the O(h gamma / M) bias, with no stability
  limit (langevin.rs:115-118). *(probe: stiff damping diverges)*
- **works** (2026-10-05): the same seed replays bit for bit across threads,
  and a different `traj_id` (under 2^32) gives different noise. A negative
  ctrl temperature is treated as zero, not NaN. *(probe: seeds replay
  across threads; negative ctrl temperature)*

#### Building a stack (validation and indexing)

- **API** (2026-10-05): installing a second stack on a model silently
  replaces the first (stack.rs:131-146), so separately built pieces (or a
  user callback plus a stack) can't be combined. Install should refuse,
  or stacks should merge. *(probe: second stack replaces the first)*
- **bug** (2026-10-05): components index `qpos` by DOF number, which only
  works if every earlier joint has as many positions as velocities. Put a
  free body before the slide joints and a double well reads a quaternion
  entry: force 0.000 instead of -7.5 (double_well.rs:204-208; same in
  pairwise_coupling.rs:168-175 and ratchet.rs:144). Fix: map through
  `jnt_qposadr` / `jnt_dofadr`. *(probe: qpos indexing after a free joint)*
- **API** (2026-10-05): a component naming a joint the model doesn't have
  (a double well on joint 5 of 2, a spring to joint 7) is accepted and
  panics on the first step with a bare "Matrix index out of bounds".
  Install should check. *(probe: component joint index out of range)*
- **API** (2026-10-04, refined 2026-10-05): `ExternalField::new` doesn't
  check its length (external_field.rs:38,62): a field shorter than the
  model runs silently (the tail gets no field), and a longer one panics
  mid-step. *(probe: ExternalField length unchecked)*
- **API** (2026-10-05): `LangevinThermostat::new` takes a negative or NaN
  damping (positions go NaN) and a damping vector shorter than the model
  (the extra joints never feel the bath, silently) (langevin.rs:128-137).
  *(probe: thermostat parameters unchecked)*
- **bug** (2026-10-05): a thermostat with `with_ctrl_temperature` (or a
  ratchet) on a model with no actuators panics on the first step, and a
  NaN ctrl passes through `clamp` (langevin.rs:174-176, ratchet.rs:145).
  The zero-gain actuator requirement is only explained in an unshipped
  spec. *(probe: ctrl temperature without actuators)*
- **API** (2026-10-05): `PairwiseCoupling` accepts duplicate and reversed
  edges, `(0,1)` and `(1,0)`, and silently doubles J
  (pairwise_coupling.rs:74-85). *(probe: duplicate edges double J)*
- **API** (2026-10-04): no clamp API or gate/penalty library; couplings
  can't change after install.
- **API** (2026-10-04): an `ExternalField` above ~1.54 * delta_v / x0
  silently deletes the well.
- **API** (2026-10-05): `components()` is documented as supporting a
  per-component diagnostic report, but `PassiveComponent` has no
  `Diagnose` supertrait and no `as_any`, so
  `stack.components()[i].diagnostic_summary()` doesn't compile
  (stack.rs:199-205, diagnose.rs:8-9). The workaround (keep your own `Arc`
  and add it with `with_arc`) is undocumented.
- **API** (2026-10-05): dropping `StochasticGuard`s out of LIFO order turns
  the noise back on while an inner guard is still alive (stack.rs).
- **API** (2026-10-05): a component can't report its force at a given
  `qpos` without a `Model` and `Data`, so a force-balance check means
  re-deriving each force from its formula (double_well.rs:208). See Step 3.
- **docs** (2026-10-05): nothing says two components of the same type can
  share a `PassiveStack` (it works). See Step 3.
- **docs** (2026-10-05): writing your own `PassiveComponent` is easy but
  undocumented; a short example would make it discoverable. See Step 3.
- **API** (2026-10-05): `WellState` is documented as "hysteresis-based" but
  `from_position` is a stateless classifier, and a negative threshold
  makes the barrier vanish: `from_position(0.0, -0.1)` is `Right`
  (well_state.rs:3-36). `spin()` panics on `Barrier`, with no
  `Option`-returning version. *(probe: WellState threshold unchecked)*
- **API** (2026-10-05): `WellState::from_position` (well_state.rs:28) takes
  the threshold as a bare `f64` with no suggested default; 0.5 x x0 (from
  the crate's tests) worked.

#### Ising tools (exact solver, Gibbs sampler, distances)

- **bug** (2026-10-05): `ising::exact_distribution` overflows to NaN. It
  computes `exp(-E/kT)` for every state and normalizes by the sum
  (ising.rs:83-90) with no max-energy shift (log-sum-exp). On a 20-spin
  problem at low temperature, `-E/kT` passes ~709, `exp` returns infinity,
  and probabilities become `inf/inf = NaN`, silently (962 of 1M states in
  the probe). Fix: subtract the minimum energy before exponentiating.
  Workaround here: treat any non-finite probability as "solver
  unavailable" (`TradeComputer::exact_ground_state`). *(probe:
  exact_distribution overflows to NaN)*
- **bug** (2026-10-05): edge indices aren't checked against n. An edge to a
  spin that doesn't exist silently becomes a field: `(0,5)` on 2 spins
  leaves spin 0 up 12% of the time instead of 50%. Indices of 32 and up
  wrap the bit shift onto another spin; Gibbs and the learner panic
  instead (ising.rs:19-21,72; gibbs.rs:75-76). *(probe: Ising: edge out of
  range)*
- **bug** (2026-10-05): `GibbsSampler` always starts with every spin up and
  has no way to set or randomize the start, read the spins or reset. At low
  temperature a ferromagnetic chain never leaves the up basin: 4 spins,
  J 2, kT 0.3 gives P(all down) 0.000 against the exact 0.500, with no
  warning (gibbs.rs:40,83-86). The tests only run at kT 1. *(probe: Gibbs:
  always starts all up)*
- **bug** (2026-10-05): `GibbsSampler::sample(_, 0)` divides 0 by 0: every
  probability is NaN, though the docs promise they sum to 1
  (gibbs.rs:154-156). *(probe: Gibbs: zero samples)*
- **bug** (2026-10-05): `tv_distance` and `kl_divergence` pair entries by
  position, not by configuration, and don't check normalization. The same
  distribution listed in another order gives TV 0.8 (ising.rs:132-178).
  `kl_divergence` also returns infinity for q < 1e-300 even when the term
  is finite and tiny. *(probe: tv_distance ignores bitmasks)*
- **works** (2026-10-05): a self-loop edge `(i, i)` doesn't bias the Gibbs
  sampler (P(up) 0.479 vs exact 0.500); reading the code suggested it
  would, but it only slows mixing. *(probe: Gibbs: self-loop edge)*
- **API** (2026-10-04): `exact_distribution`/`GibbsSampler` cap at n <= 20
  (ising.rs:37), by panic. *(probe: exact solver caps at 20 spins)*
- **docs** (2026-10-04): the bit convention is documented only at
  ising.rs:17-34.
- **docs** (2026-10-05): `PairwiseCoupling` docs promise that a coupled
  bistable array's "equilibrium statistics match the Ising model"
  (pairwise_coupling.rs:8-10), validated only on a uniform-J 4-chain. They
  never state the condition: couplings and fields small compared with dV
  (positions stay near +-x0). In an antiferromagnetic QUBO with degree ~5,
  following the naive mapping gave 100% wrong answers. A one-line rule of
  thumb, or a helper that maps a QUBO/Ising problem to components and warns
  when |h| + sum|J| nears the well-flattening tilt, would have saved an hour
  of sweeps.
- **API** (2026-10-05): a "latch the lowest-energy state seen" helper would
  help: every annealing user needs one.

#### Boltzmann learner (`IsingLearner`)

- **bug** (2026-10-05): `n_steps - n_burn_in` is an unchecked subtraction.
  With burn-in longer than the run, a release build wraps to ~1.8e19
  measurement steps and `step()` never returns; debug panics
  (ising_learner.rs:219). *(probe: learner: burn-in longer than the run)*
- **bug** (2026-10-05): `n_trajectories = 0` is accepted; the measured
  statistics are 0/0 and J and h turn NaN (ising_learner.rs:301-319).
  *(probe: learner: zero trajectories)*
- **API** (2026-10-05): `new` doesn't check the target distribution's
  length (the fields are public); `step` then panics inside `kl_divergence`
  (ising_learner.rs:121-139). *(probe: learner: target unchecked)*
- **bug** (2026-10-05): a `LearningRecord` stores the parameters after the
  update but the KL from before it, though the docs say both are "at end
  of this iteration" (ising_learner.rs:86, 323-355). *(probe: learner:
  record mixes before and after)*
- **docs** (2026-10-05): the `Model` the learner needs is undocumented: n
  slide or hinge joints, no gravity or contacts, the timestep comes from
  the model, any existing `cb_passive` is replaced, and joints past n go
  unthermostatted. `therm_env::generate_mjcf` is the only way to build one
  and nothing points to it (ising_learner.rs:98-104, 198-200).
- **docs** (2026-10-05): the springs act on positions, so the effective
  Ising coupling is about J x0^2 (field h x0), but the reported KL uses the
  raw J and h; only x0 = 1 is tested (ising_learner.rs:323-330,
  tests/boltzmann_learning.rs:40).
- **bug** (2026-10-05): trajectory seeds are `seed_base + iteration * 1000 +
  traj`, so with more than 1000 trajectories, streams repeat across
  iterations (ising_learner.rs:302).
- **API** (2026-10-05): `train()` always prints to stderr, with no way to
  turn it off; `LearnerConfig` and `IsingTarget` have no `Clone`, `Debug`
  or `Default` (ising_learner.rs:366-377).

#### 1-D integrators and rates (`Baoab1D`, `ColoredDriveSim`, `DoubleWellPotential`)

- **API** (2026-10-05): neither constructor validates its inputs:
  `ColoredDriveSim` with tau 0 and `Baoab1D` with a negative gamma both
  give NaN on the first steps, silently (colored_drive.rs:69-71,
  baoab.rs:52-73). *(probe: 1-D integrators unchecked)*
- **API** (2026-10-05): `ColoredDriveSim` has no `position()` accessor
  (`Baoab1D` does), so you can't watch well occupancy, the point of a
  double-well sim (colored_drive.rs:115-127).
- **docs** (2026-10-05): the colored-noise SDE in the module docs
  (colored_drive.rs:11) gives variance sigma^2 tau / 2, but the code uses
  the exact OU update with stationary variance gamma kT / tau, and eta
  starts at 0 rather than a stationary draw.
- **bug** (2026-10-05): `depopulation_factor` loses precision at small
  energy loss and collapses to 0 instead of approaching delta (gamma
  1e-17: 0 vs 4.6e-17). `(-(-s).exp_m1()).ln()` fixes it
  (double_well.rs:173). The rate functions don't validate mass, gamma or
  kT (mass 0 gives inf or NaN). *(probe: depopulation factor underflows)*
- **docs** (2026-10-04): the `kramers_rate` counting convention is unclear
  vs `kramers_rate_turnover` (double_well.rs:180-192).

#### Docs overall

- **docs** (2026-10-04): docs cite internal roadmap labels ("Phase 3", "D1",
  "spec §3", "Ch 32 §4.6", "Route 2"; also "chassis Decision 5 + M4", "D4
  Layer-2 R6/R2", "R1 showed") and repo paths that aren't shipped
  (`sim/L0/tests/integration/batch_sim.rs`,
  `docs/thermo_computing/03_phases/d4_physical_pbit`). Every code example
  is `ignore` with an undefined `model` (lib.rs:40-63,
  double_well.rs:38-55).
- **docs** (2026-10-04): the tests' fixtures need sim-core's
  `test-fixtures` feature, which the facade doesn't forward;
  `therm_env::generate_mjcf` (therm_env builder.rs:22) is the workaround,
  found only by reading source.
- **docs** (2026-10-05): `OscillatingField`'s "dof indexes both qpos and
  qfrc_out" is wrong (it only writes `qfrc_out`), and `ExternalField` mixes
  indexing (`apply` by DOF, `field_energy` by qpos)
  (oscillating_field.rs:41-46, external_field.rs:52-66).
- **docs** (2026-10-05): `test_utils` is always compiled and exported, and
  `assert_within_n_sigma` panics as its API. Say whether it's meant for
  users (test_utils.rs:1-31, 221-243).

### `sim-soft`

- **perf** (2026-10-04): the SDF mesher keeps every grid vertex (18,696
  kept, 561 used; sdf_meshed_tet_mesh.rs:26-30).
- **bug** (2026-10-04): `replay_step` panics on a solver stall
  (newton.rs:723) instead of returning an error; `try_replay_step` is easy
  to miss.
- **API** (2026-10-04): `PenaltyRigidContact::with_params` is a "testing
  surface" and the default contact band is crate-private
  (penalty.rs:262-268).
- **API** (2026-10-04): `StaggeredCoupling::new` takes 11 positional
  arguments, a magic body index of 1, and hard-wires lambda = 4 mu
  (construct.rs:29-49).
- **docs** (2026-10-04): `Tensor` must come from `sim::ml_chassis`,
  unmentioned in sim-soft.
- **API** (2026-10-04): gradients are unavailable with friction, F-bar or
  Tet10. `sim_core::Data` isn't `Clone`, so gradient calls consume the
  scene.
- **bug** (2026-10-04): a stray `eprintln` ("faer LU fallback fired...")
  prints from the library.

### `sim-rl`

- **perf** (2026-10-04): the `Policy` / `DifferentiablePolicy` / `ValueFn`
  traits are a clean seam, but PPO calls per-sample `forward` /
  `log_prob_gradient` instead of the batch methods, which blocks GPU or
  SIMD backends from helping. SAC/TD3 batch their critic gradients.
- **docs** (2026-10-04): the `Algorithm` docs tell Bevy users to write their
  own training loops, but no published example shows how.

### Platform

- **setup** (2026-10-04): Windows 11 with Smart App Control enforcing:
  nothing builds (build scripts fail with os error 4551). Getting-started
  notes should say to turn it off. After that, the first release build of
  `cortenforge` took 2 min 18 s on a Ryzen 5 5600.
