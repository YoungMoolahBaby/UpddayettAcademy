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
- **setup** (2026-10-05): the facade forwards no features: sim-core's
  `parallel` (rayon `BatchSim::step_all`, batch.rs:237-254) and
  ml-chassis's `parallel` can't be turned on through `cortenforge`, so
  `VecEnv` steps envs one at a time. `cargo tree -e features` shows
  sim-core built without it.

### `sim-core`

`cargo run --release --example gaps_sim_core` checks this crate and
sim-mjcf in one run. On 0.9.0: 27 open, 6 fine (both crates). Paths are
under sim-core's `src/` unless noted.

#### Callbacks

- **docs** (2026-10-05): `CbPassive` and `CbControl` (types/callbacks.rs:37-46)
  don't say how often they run. The passive callback runs once per `step` on
  Euler and the implicit integrators, 4 times on RK4 (once per stage,
  integrate/rk4.rs:129), and once per `forward()`. The control callback also
  runs on every plain `forward()` (forward/mod.rs:425). `mjd_transition_fd`
  steps a scratch copy 2(2nv+na+nu)+1 times (derivatives/fd.rs:87-250), so a
  noise source in the passive callback (the thermostat) draws during
  finite differences and makes the derivatives noisy. The counts match
  MuJoCo; the docs should say them. *(probe: passive callback calls per step)*
- **docs** (2026-10-05): disabling both springs and dampers returns from the
  passive stage before the user callback (forward/passive.rs:383-385), so a
  thermostat installed there goes silent: 0 calls in 10 steps. MuJoCo does
  the same; `CbPassive` should warn. *(probe: passive callback off with
  springs and dampers)*
- **bug** (2026-10-05): `step1` runs the control callback even with
  actuation disabled (forward/mod.rs:147); `forward` checks the flag
  (forward/mod.rs:425-427). *(probe: step1 runs control with actuation
  disabled)*
- **API** (2026-10-05): one slot per callback: `set_passive_callback`
  replaces the previous one (types/model.rs:1252-1257), with no way to
  chain. This is the root of the thermostat's "second stack replaces the
  first".

#### State and lifecycle

- **bug** (2026-10-05): `energy_initial` is documented as set at the first
  `forward()` with energy enabled (types/data.rs:437-441), but only
  `forward_skip` captures it (forward/mod.rs:388). After `forward` and 100
  steps it is still 0 while the total energy is 9.79, so "drift" is the
  total energy. *(probe: energy_initial never set)*
- **bug** (2026-10-05): `reset_to_keyframe` (types/data.rs:1229-1291) copies
  the keyframe without first doing what `reset` does: warnings, energy,
  plugin state and the force and constraint arrays stay. After a
  divergence, `divergence_detected()` is still true after
  `reset_to_keyframe` (false after `reset`). MuJoCo resets fully, then
  applies the keyframe. *(probe: reset_to_keyframe is a partial reset)*
- **API** (2026-10-05): nothing ties a `Data` to its `Model`. Stepping a
  1-dof Data with a 2-dof model panics ("Matrix index out of bounds",
  forward/check.rs:23), and the other way round panics in a copy. `step` could
  compare `qpos.len()` with `nq` and return an error. *(probe: Data stepped
  with another Model)*
- **API** (2026-10-05): a bad state (NaN, inf, or a value over 1e10 in qpos,
  qvel or qacc) makes `step` reset everything (time and ctrl to 0) and return
  `Ok(())` (forward/check.rs:22-110). A NaN ctrl zeroes all ctrl
  (forward/actuation.rs:488-496). The only signs are `divergence_detected()`
  and one `log::warn!` (types/warning.rs:63), which nobody sees without a
  logger. A trainer can't tell from the `Result`. *(probe: bad state: step
  resets and returns Ok)*
- **API** (2026-10-05): `Model` is all pub fields, but its derived caches
  don't follow edits. Hinge and slide damping reads `jnt_damping`
  (forward/passive.rs:918), so setting `dof_damping` (the MuJoCo field) does
  nothing. Setting `jnt_damping` after load leaves Euler's implicit damping
  at the load-time value (types/model_init.rs:967; integrate/mod.rs:83,93).
  From 0 to 5, qvel after 100 steps is 0.3660, against 0.3697 for a model
  loaded with 5, until you call `compute_implicit_params()`. Nothing says
  which fields need which recompute. *(probe: damping changed after load)*
- **bug** (2026-10-05): with `implicitspringdamper`, `forward()` writes qvel
  (forward/acceleration.rs:233, from forward/mod.rs:569 when no constraint
  is active). qvel goes 0, -0.1, -0.2 over two `forward()` calls on a
  spring, though `forward` is documented not to change the state. Any
  `forward()` after `step` (ml-chassis's `SimEnv::step` does one) advances
  qvel twice. *(probe: implicitspringdamper: forward() moves qvel)*
- **bug** (2026-10-05): `Data::clone` sets every `plugin_data` to `None`
  (types/data.rs:890-892), and finite-difference derivatives clone Data
  (derivatives/fd.rs:87,502). `make_data` can't fail: a plugin `init()`
  error panics (types/model_init.rs:897-900). RK4 never calls a plugin's
  `advance()`; only `Data::integrate` does (integrate/mod.rs:209-214). From
  the source; not run (no plugin here).
- **works** (2026-10-05): `Data` *is* `Clone` (types/data.rs:684), contrary to
  an earlier entry here: a clone taken mid-run steps on bit for bit. `Model`
  is `Clone` and shares its callbacks through `Arc`. *(probe: Data clone
  replays)*
- **works** (2026-10-05): `Model` and `Data` are `Send + Sync`; two Datas
  stepped on `std::thread::scope` threads over one `&Model` match a serial
  run. This works without the `parallel` feature. *(probe: threads share one
  Model)*
- **works** (2026-10-05): `reset` restores qpos0 and zeroes qvel, ctrl and
  time. ctrl is clamped to ctrlrange. Non-test code has no `unwrap()`,
  `todo!` or `eprintln!`. *(probe: reset restores the state)*

#### BatchSim

- **docs** (2026-10-05): `step_all` promises output "independent of thread
  count and scheduling order" (batch.rs:232-236). That's false with a
  stateful callback on the shared model, which is exactly what
  therm-env's `build_vec` installs (see sim-therm-env).
- **docs** (2026-10-05): `BatchSim::reset` says it doesn't zero
  `qfrc_applied`/`xfrc_applied` "(see Data::reset)" (batch.rs:289), but
  `Data::reset` does zero them (types/data.rs:1138-1141).
- **API** (2026-10-05): `model()` returns env 0's model even with per-env
  models (batch.rs:178-183); there's no `forward_all`, and `new_per_env`
  doesn't check that the models share a shape.

#### Docs

- **docs** (2026-10-05): lib.rs:8 calls `Model` "immutable after loading"
  (see the damping entry), and lib.rs:24 says one step is "forward() then
  integrate()". Doing that by hand skips the bad-state checks, sleep and the
  warmstart save.
- **docs** (2026-10-05): "no heap allocation during simulation"
  (types/data.rs:23, 553) isn't true. The constraint stage rebuilds the
  `efc_*` arrays and clones qM (constraint/mod.rs:314-319, 343, 410-411),
  and eulerdamp clones qLD (integrate/mod.rs:89, 102).
- **docs** (2026-10-05): the smaller ones:
  - `InvalidTimestep` displays "timestep is zero or negative"
    (types/enums.rs:846), but it is also returned for NaN and inf.
  - The `step2` doc says Euler is used "regardless of model.integrator"
    (forward/mod.rs:161-163); only RK4 falls back.
  - `batch.rs:61` mentions a warmstart `HashMap` that doesn't exist.
- **API** (2026-10-05): sim-types' `SimulationConfig` and `SolverConfig`
  (sim-types config.rs:14-40) aren't read by sim-core and can't be applied
  to a `Model`. Their default timestep is 1/240, against MJCF's 0.002.

### `sim-mjcf`

Checked by the same `gaps_sim_core` probe. No bad MJCF string panicked
except the joint-layout assert (from sim-core); the gaps are input that is
accepted or dropped without a word, and errors that don't say where. Paths
are under sim-mjcf's `src/`.

#### Hangs and crashes

- **bug** (2026-10-05): a NaN geom `mass` or `density`, or a NaN
  off-diagonal `fullinertia`, makes `load_model` hang forever.
  nalgebra's `symmetric_eigen()` has no iteration limit and never converges
  on NaN (builder/mass.rs:103, 233). The validator checks inertial mass and
  geom size but not geom mass or density. A NaN on the inertia diagonal
  loads with a NaN inertia instead. *(probe: NaN and inf load)*
- **bug** (2026-10-05): a chain of 150 nested bodies overflows the 1 MB
  Windows main-thread stack and kills the process (`0xc00000fd`); 100
  loads. Parsing and building recurse per body (parser/body.rs:85 and the
  builder's traversal). A rope or chain model hits this. *(probe: deep
  nesting)*
- **bug** (2026-10-05): a ball joint followed by a hinge on one body (valid
  MuJoCo) panics inside `load_model`: "ball joint 0 on body 1 must be the
  LAST joint on its body". `validate_joint_layout` asserts (sim-core
  types/model_init.rs:458-481) and runs from `make_data`, which the builder
  calls (builder/build.rs:40-41). *(probe: ball + hinge on one body)*
- **bug** (2026-10-05): `ctrlrange="1 -1"` loads, then the first `step`
  panics in `f64::clamp` ("min > max", sim-core forward/actuation.rs:510).
  The same applies to actrange (forward/actuation.rs:457). *(probe: ctrlrange lo > hi)*

#### Silently accepted or dropped

- **bug** (2026-10-05): typos load without a warning. Attributes are looked
  up by name and never checked against a list (parser/attrs.rs:32-39), and
  unknown elements are skipped (parser/mod.rs:163, parser/body.rs:53, 69, 151,
  193, parser/actuator.rs:22-34):
  - `<geom typ="box" size="1 1 1"/>` makes a sphere.
  - `<gemo/>` vanishes.
  - `<intvelocity>` gives nu 0, though `<default>` accepts it.
  - `<sensor><jointpositon/>` gives nsensor 0, which shifts every later
    sensordata index.
  - A `<joint type="free"/>` directly in the worldbody gives njnt 0.

  *(probe: typos load silently)*
- **bug** (2026-10-05): values that don't parse fall back to the default.
  Scalars go through `.parse().ok()` (parser/attrs.rs:42-49), keywords
  through `from_str` (parser/options.rs:79-132), and booleans compare against
  `"true"` (parser/attrs.rs:137):
  - `timestep="0.01s"` stays 0.002.
  - `damping="abc"` gives 0.
  - `mass=" 1"` (a space) is ignored and density gives 4.19.
  - `integrator="RK45"` runs Euler.
  - `<flag gravity="off"/>` keeps gravity on.
  - `limited="1"` means not limited.

  *(probe: bad values fall back to defaults)*
- **bug** (2026-10-05): `size` in `<default><geom>` isn't inherited:
  `MjcfGeomDefaults` has no size (types.rs:676), so a bare `<geom/>` under a
  0.05 default gets radius 0.1. *(probe: default geom size ignored)*
- **bug** (2026-10-05): a self-closing `<body .../>` is dropped (no `body` arm
  in the `Event::Empty` branches, parser/body.rs:56-70): a mocap target
  `<body name="t" mocap="true" pos="0 0 1"/>` gives nbody 1, nmocap 0.
  *(probe: self-closing <body/> dropped)*
- **bug** (2026-10-05): a second `<compiler>` or `<option>` replaces the
  first instead of merging (parser/mod.rs:85-90). `<compiler
  angle="radian"/><compiler autolimits="true"/>` reads `range="-1 1"` as
  degrees (±0.0175). Adding `<option><flag energy="enable"/></option>`
  after `<option timestep="0.001"/>` puts the timestep back to 0.002.
  *(probe: second <compiler>/<option> resets the first)*
- **bug** (2026-10-05): limits aren't checked (builder/joint.rs:104-111,
  builder/actuator.rs:94):
  - `limited="true"` with no range gets ±π, then the degree conversion makes
    it ±0.0548 rad.
  - `range="1 -1"` loads as (0.0175, -0.0175).
  - `ctrllimited="true"` with no ctrlrange gets (-1, 1).

  MuJoCo refuses all three. *(probe: limits and ranges unchecked)*
- **bug** (2026-10-05): sizes, masses and inertias that can't be right load
  anyway. Validation only checks finiteness, and only some fields
  (validation.rs:220-228, 380-403):
  - a geom with no size gets 0.1 (builder/geom.rs:373-383);
  - `size="-0.1"` gives body mass -4.19;
  - geom `mass="-1"` gives -1;
  - a moving body with no geom has mass 0 and steps to NaN with `Ok(())`;
  - `diaginertia="0 0 0"` does the same;
  - a fullinertia that isn't positive definite gets its eigenvalues
    `abs()`'d (3, 1, 1; builder/mass.rs:103-108);
  - `<inertial>` with no mass gets 1.

  *(probe: mass, size and inertia unchecked)*
- **bug** (2026-10-05): geometry attributes read wrong:
  - A plane's size becomes (0.1, 0.1, 0.1) (builder/geom.rs:883). A renderer
    sizing the ground from `geom_size` gets it wrong.
  - `fromto` on a box gives (0.1, 0.5, 0); MuJoCo gives (0.1, 0.2, 0.5)
    (builder/geom.rs:815-853).
  - A 5-value `fromto` is ignored and the capsule keeps half-length 0.1
    (parser/attrs.rs:148-162). A 4-value `pos` drops the 4th value.
  - `xyaxes` on a `<body>` is never parsed and gives the identity
    orientation (parser/body.rs:207-246).
  - `quat="0 0 0 0"` gives a NaN orientation (builder/orientation.rs:13-15).

  *(probe: geometry attributes misread)*
- **bug** (2026-10-05): NaN gets through where the validator doesn't look. A
  body `pos="nan 0 0"` loads as NaN, and a joint `axis="nan 0 0"` silently
  becomes the Z axis (`safe_normalize_axis`, parser/attrs.rs:12-15). An
  infinite gravity is refused. *(probe: NaN and inf load)*
- **bug** (2026-10-05): an undefined `class="nope"` loads (only `childclass`
  is checked, builder/frame.rs:37-58). MuJoCo's root class name `main`
  resolves to nothing (the root is stored as `""`), so `class="main"`
  loses the root defaults. Several top-level `<default>` blocks overwrite
  each other (defaults.rs:754-762, from the source). *(probe: undefined
  class ignored)*
- **bug** (2026-10-05): an explicit value that equals the built-in default
  is overwritten by a class default: `gear="1"` under a class with gear 50
  gives 50. The source has a `#todo` for it (defaults.rs:345-391); the same
  applies to kp=1 and sensor noise or cutoff 0. *(probe: explicit value
  overwritten by class default)*
- **bug** (2026-10-05): duplicate geom names load and the name map keeps the
  last (builder/geom.rs:89); the same applies to sites, tendons and sensors.
  Duplicate bodies, joints and actuators are refused. *(probe: duplicate geom
  names)*
- **bug** (2026-10-05): joints inside a `<frame>` are invisible to
  `validate()`, which walks the tree before `expand_frames`
  (builder/mod.rs:234 vs 246). An actuator on one fails with "reference to
  undefined joint: j". *(probe: joint inside <frame> undefined)*
- **bug** (2026-10-05): `load_model` refuses any string containing
  `<include`, even inside a comment (builder/mod.rs:367). *(probe: <include in
  a comment)*
- **API** (2026-10-05): a mesh with no `name` gets `""`, not its file name
  (parser/asset.rs:90), so `<geom mesh="base"/>` for `base.stl` fails, and two
  unnamed meshes clash. From the source; not run (needs a file).

#### Errors

- **API** (2026-10-05): build-stage errors come back as
  `MjcfError::Unsupported(String)` (builder/mod.rs:374, 411). Two bodies
  named "a" give `Unsupported("Model validation failed: duplicate body name:
  a")`, so the typed `DuplicateBody` and `UndefinedJoint` variants can't be
  matched. A file-read failure is `Unsupported` too, not `Io`. *(probe:
  errors flattened to Unsupported)*
- **API** (2026-10-05): errors give no line, element or attribute:
  `<geom size="0.1 x"/>` reports "XML parse error: invalid float: x".
  *(probe: errors have no location)*
- **works** (2026-10-05): unknown joint and geom types, short or
  non-numeric vectors, duplicate joints, undefined references, a NaN geom
  size, an inertial mass of 0 and a missing `<mujoco>` all return an error.
  `MjcfError` implements `Error` and `Display`. The compiler defaults match
  MuJoCo (angle degree, autolimits on). *(probe: bad input refused)*

#### Docs

- **docs** (2026-10-05): lib.rs:95 shows `<mesh vertex="..."/>` as embedded
  data, but it needs `face=` too (builder/mesh.rs:412-418): "embedded vertex
  data requires face data". MuJoCo builds the convex hull from vertices
  alone. lib.rs:60 lists textures and materials under `<asset>`, which are
  skipped (parser/asset.rs:39). *(probe: mesh with vertices only)*
- **docs** (2026-10-05): the smaller ones, from the source:
  - `condim` 2 or 5 warns and rounds (builder/geom.rs:272).
  - A later empty `<contact/>` wipes earlier pairs (parser/mod.rs:174-175).
  - The `.mjb` loader has no size limit on lengths (mjb.rs:276).

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

### `sim-therm-env`

`cargo run --release --example gaps_therm_env` does the same for this
crate. On 0.9.0: 10 open, 3 fine. All paths are in builder.rs unless noted.

#### Noise across envs

- **bug** (2026-10-05): `build_vec` installs one thermostat (traj_id 0, one
  counter) on the shared `Arc<Model>` (262-277, 321), so all envs draw from
  one stream in turn: env 0's path changes with the batch size (qpos 0.0026
  alone, 0.0037 beside a second env, same seed). An env can't be replayed
  on its own, and a reset doesn't restart its noise. The thermostat's
  `install_per_env` exists for this and isn't used. If sim-core's `parallel`
  feature were on, the order would also depend on thread scheduling (not
  tested: the facade can't turn it on, see the facade entry). *(probe:
  build_vec shares one noise stream)*
- **bug** (2026-10-05): `build()` and `build_vec(1)` walk different paths
  from the same seed. `SimEnv::step` calls `forward()` after stepping
  (ml-chassis env.rs:145), which runs the passive stack again, so the
  thermostat draws noise twice per step and throws one draw away. `VecEnv`
  draws once. Each reset's `forward()` takes a draw too. *(probe: build()
  and build_vec(1) differ)*
- **works** (2026-10-05): two envs built with the same seed replay bit for
  bit. *(probe: same seed replays)*

#### Builder validation

- **bug** (2026-10-05): validation only rejects NaN and infinity (283-297);
  the loader catches a zero or negative timestep. Everything else builds:
  - gamma -1 and k_b_t -1 give NaN qpos.
  - `ctrl_range(5, 1)` panics on the first step, in `f64::clamp` in
    ml-chassis space.rs:643.
  - `episode_steps(0)` gives one-step episodes.
  *(probe: physics parameters unchecked)*
- **bug** (2026-10-05): landscape components aren't checked against
  `n_particles` (330-332). A `DoubleWellPotential` on dof 5 of 1 builds,
  then panics on the first step ("Matrix index out of bounds"). A 2-long
  `ExternalField` on 3 particles runs silently, with no field on particle 2.
  `build` should return an Err. *(probe: landscape not checked against
  particles)*
- **API** (2026-10-05): `generate_mjcf` is public and returns a `String`,
  not a `Result`. It puts actuator `i` on joint `x{i}` (56), so more ctrls
  than particles gives XML that doesn't load ("reference to undefined
  joint: x1"), and the doc doesn't say `n_ctrl <= n_particles` (15-20).
  The loader does reject a NaN, 0 or negative timestep. *(probe:
  generate_mjcf: more ctrls than particles; generate_mjcf: bad timestep)*

#### Ctrl temperature

- **docs** (2026-10-05): the thermostat clamps the ctrl multiplier to
  [0, 10] (langevin.rs:175, env.rs:58), but `ctrl_range` takes any range
  and its doc doesn't say so (163-174): with `ctrl_range(0, 20)`, ctrl 20
  heats to 10 kT. *(probe: ctrl temperature clamped at 10x)*
- **docs** (2026-10-05): with `with_ctrl_temperature`, ctrl[0] is 0 after
  build and every reset, so the bath is cold (pure damping) until the first
  action. `k_b_t` reads like the starting temperature, and nothing says
  otherwise (131-136, 156-161). *(probe: ctrl temperature starts cold)*

#### Episodes and the VecEnv

- **bug** (2026-10-05): the default truncation tests accumulated float time,
  `d.time > max_time` (354-357), so `episode_steps(10)` runs 11 steps at
  h = 0.003, 0.005, 0.01 and 0.25, and 10 at 0.001 and 0.002, depending on
  how the sum rounds. Counting steps would be exact. *(probe: truncation off
  by one)*
- **API** (2026-10-05): `build_vec` wraps the user's `on_reset` as `|m, d,
  _idx| on_reset(m, d)` (273-275), so the env index `VecEnv` passes is
  dropped and per-env randomization can't tell envs apart. The hook type
  could take the index. *(probe: build_vec on_reset loses the env index)*
- **API** (2026-10-05): `build_vec` returns a bare `VecEnv` (262), losing
  `effective_temperature`, `n_particles` and `config_k_b_t`. There's also
  no handle to the stack, so `disable_stochastic` for a noise-free eval
  isn't reachable from either env. *(probe: build_vec loses the accessors)*

#### Docs

- **docs** (2026-10-05): several facts are left unstated (15-20, 72-76):
  - every particle has mass 1 and starts at qpos 0, which is the barrier
    top of a `DoubleWellPotential`, so `on_reset` is needed;
  - the integrator is Euler, which is right, since RK4 shrinks the bath
    (see sim-thermostat);
  - the observation is `[qpos.., qvel..]` as f32 (342-345).
  The crate docs have no example. *(probe: observation layout)*

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
  Tet10. `StaggeredCoupling` isn't `Clone`, so each gradient call needs a
  freshly built scene. (Corrected 2026-10-05: this used to blame
  `sim_core::Data`, which is `Clone`; see sim-core.)
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
