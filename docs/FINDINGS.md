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
- The platen scene copied from the crate's tests reaches 1024 N of contact
  force and launches the platen (tuned only for 20-40 steps). (2026-10-06:
  the force comes at step 0, because the scene starts inside the 1 cm contact
  band; see sim-coupling.)
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
  sim-core built without it. (2026-10-06: sim-soft's `phase-timing`
  isn't forwarded either, so `sim::soft::profile` always reads zero.)

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
- **API** (2026-10-06, Step 4): the observation is fixed at `[qpos..,
  qvel..]` (341-345), with no hook to add `time()` or `ctrl` or drop the
  velocities. A policy can't tell how far through the episode it is, and
  its input size changes with the particle count, so one policy can't run
  on boards of 17 and 20 strips. The builder would carry the board itself
  (`with` takes the `PairwiseCoupling` and `ExternalField` fine), but the
  smart wash had to skip it: our board model plus `VecEnv::builder` with
  its own `ObservationSpace` (time, then qpos) took about 40 lines
  (src/trade/smart.rs `wash_env`).

#### Docs

- **docs** (2026-10-05): several facts are left unstated (15-20, 72-76):
  - every particle has mass 1 and starts at qpos 0, which is the barrier
    top of a `DoubleWellPotential`, so `on_reset` is needed;
  - the integrator is Euler, which is right, since RK4 shrinks the bath
    (see sim-thermostat);
  - the observation is `[qpos.., qvel..]` as f32 (342-345).
  The crate docs have no example. *(probe: observation layout)*

### `sim-soft`

`cargo run --release --example gaps_sim_soft` checks this crate and
sim-coupling in one run (10 s). On 0.9.0: 42 open, 1 fine (both crates).
Paths are under sim-soft's `src/` unless noted.

#### Solver: failures and validation

- **bug** (2026-10-04): `replay_step` panics on a solver stall instead of
  returning an error (newton.rs:710-745: ArmijoStall 723, NewtonIterCap
  731, DoublyFailedFactor 740, ValidityViolation 743); `try_replay_step`
  is easy to miss. Still true 2026-10-06: with `max_newton_iter` 1,
  `try_replay_step` returns `NewtonIterCap` and `replay_step` panics.
  *(probe: replay_step panics on a stall)*
- **API** (2026-10-06): the `try_` methods still panic on bad input instead
  of returning `Err`. Each of these panics: dt 0 or negative, an `x_prev`
  of the wrong length, and a theta of the wrong length
  (newton.rs:956-968, solver/backward_euler/assembly.rs:177-215). dt = inf
  gets through and comes back as `ArmijoStall` with a NaN residual.
  *(probe: try_replay_step panics on bad input)*
- **bug** (2026-10-06): `max_newton_iter` is one short. A solve that
  reports `iter_count` 3 fails with `NewtonIterCap` when the cap is 3,
  because convergence is tested only before each update
  (newton.rs:1028-1080). *(probe: max_newton_iter off by one)*
- **API** (2026-10-06): a solve can't succeed with `max_newton_iter` 0 or
  with `tol` 0, even at rest with no load. Both return `NewtonIterCap`
  with a residual of exactly 0 (newton.rs:1026, 1043, 1076). Neither value
  is refused up front. *(probe: cap 0 / tol 0 fail at rest)*
- **API** (2026-10-06): `SolverConfig.dt` is ignored. The step uses the `dt`
  argument, and `cfg.dt` 1.0 and 0.01 give the same step at dt 0.01, yet
  `current_dt()` still reports 1.0 (trait_impl.rs:97-99).
  *(probe: SolverConfig.dt ignored)*
- **API** (2026-10-06): NaN in `v_prev` comes back as
  `ArmijoStall { last_iter: 0, last_r_norm: NaN }`, which reads as a
  line-search failure, and the message blames a non-SPD tangent. NaN in
  `x_prev` is caught properly as a `ValidityViolation`. *(probe: NaN state)*
- **API** (2026-10-06): `SolverFailure` is only `Debug` (solver/mod.rs:106),
  so `?` into `Box<dyn Error>` or anyhow doesn't compile, and `{e:?}` dumps
  the whole `x_partial`. `MeshingError` is the same
  (sdf_meshed_tet_mesh.rs:91-92); the crate's other error types
  implement `Display`. From the source; not run.
- **docs** (2026-10-06): stale docs. solver/mod.rs:66-72 still says
  non-convergence panics; material/mod.rs:96-104 says the bounds panic, but
  on the `try_` path they return `Err`; neo_hookean.rs:14-16 describes an
  older gate. From the source; not run.

#### Materials and config values

- **API** (2026-10-06): material values aren't checked.
  `MaterialField::uniform(-1e5, 4e5)` and `NeoHookean::from_lame(-1e5, 4e5)`
  both build. `ValidityDomain.poisson_range` is never read, although
  newton.rs:145 says it's checked at construction. The negative-mu tet then
  "steps Ok" by way of the LU fallback. *(probe: material parameters
  unchecked)*
- **docs** (2026-10-06): `from_young_poisson(1e6, 0.47)` panics ("requires
  nu < 0.45", neo_hookean.rs:70-74), while the `fbar` doc offers nu up to
  0.49 (config.rs:298-305). The only way to get a high nu is `from_lame`.
  *(probe: material parameters unchecked)*
- **API** (2026-10-06): a negative `density` is accepted. With density -1030
  under gravity the free vertex moves up, as negative mass would
  (construct.rs:598, 614). Density is also one global value: a
  `SiliconeMaterial.density` or per-layer density goes unused.
  *(probe: negative density)*
- **API** (2026-10-06): NeoHookean refuses any stretch beyond 2x, and that
  limit can't be changed. A tet that starts 2.1x stretched gets
  `ValidityViolation: max_stretch_deviation = 1.100 exceeds bound 1.000`
  (neo_hookean.rs:104-106), and there's no setter. For jelly-like game
  props that is a hard cap. *(probe: stretch gate fixed at 2x)*
- **API** (2026-10-06): `MaterialField::from_yeoh_fields_with_bounds` silently
  drops `min_principal_stretch` (material_field.rs:253-271, 424-427).
  From the source; not run.
- **docs** (2026-10-06): `SiliconeMaterial` and `fit_yeoh_uniaxial` fix
  lambda = 4 mu, which is nu = 0.40 (silicone_table.rs:205-209,
  uniaxial.rs:177). Real silicone is about 0.49. The tie isn't mentioned
  where you'd choose a material. From the source; not run.
- **API** (2026-10-06): gravity is z-only (`gravity_z`, config.rs:260-268).
  From the source; not run.

#### Contact and friction

- **bug** (2026-10-06): friction with the default `friction_eps_v` of 0
  fails. A sphere dropped on the penalty floor with `friction_mu` 0.5 and
  nothing else hits `ArmijoStall` at step 9 (first contact). With
  eps_v 1e-3, or with no friction, it runs 40 steps. With eps_v 0 the
  friction Hessian is 2/0 at zero slip (assembly.rs:348, friction.rs:127-129).
  The field doc quotes an "IPC default ~1e-3 L_bbox" (config.rs:295-297),
  but the default is 0. *(probe: friction with the default eps_v)*
- **API** (2026-10-04): `PenaltyRigidContact::with_params` is a "testing
  surface", and the default kappa and d_hat are crate-private
  (penalty.rs:78, 85, 262-269). Still true 2026-10-06. `with_params`
  doesn't validate either: a negative kappa makes the contact attractive.
  `IpcRigidContact::with_params` does validate (ipc.rs:96-108).
  From the source; not run.
- **docs** (2026-10-06): IPC docs disagree. ipc.rs:41 says a converged solve
  stays at d > 0; barrier.rs:76-78 says it can report `min_sd <= 0`. CCD
  is a stub (`ccd_toi` returns infinity, ipc.rs:488-491). From the source;
  not run.

#### Gradients

- **API** (2026-10-04): no gradients with friction or F-bar
  (factor.rs:467, 496, 899, 918). Still true 2026-10-06:
  - with `fbar` on, the forward step works and `try_step` panics instead
    of returning `Err` *(probe: F-bar gradient panics)*;
  - with friction, the panic says "friction-exact gradient requested
    without x_prev", but the caller can't pass one: `step`/`try_step`
    always pass `None` (trait_impl.rs:44, 80) *(probe: gradient with
    friction, under sim-coupling)*.

  Frictionless Tet10 gradients now work (factor.rs:475-483), but the
  `CpuTet10NHSolver` doc still says they're guarded (lib.rs:113-122).
  `StaggeredCoupling` isn't `Clone`, so each gradient call needs a freshly
  built scene (see sim-coupling). (Corrected 2026-10-05: this used to blame
  `sim_core::Data`, which is `Clone`; see sim-core.)
- **API** (2026-10-06): `CpuDifferentiable` is exported at the crate root
  (lib.rs:49), but all four methods are `unimplemented!()`
  (differentiable/newton_vjp.rs:95-123). From the source; not run.
- **bug** (2026-10-06): `equilibrium_pose_sensitivity`'s doc tells you to pass
  `x_prev = None` for the frictionless path (sensitivities.rs:508-514).
  With friction on, that path panics at factor.rs:496. From the source; not
  run.
- **bug** (2026-10-06): some contracts are checked only by `debug_assert`,
  so release builds miss them:
  - a custom `ContactModel` whose Hessian couples vertices of different
    tets writes into the wrong sparse slot (assembly.rs:111-123);
  - the sensitivity functions check slice lengths only in debug
    (sensitivities.rs:128, 174, 523, 594, 684-687).

  From the source; not run.

#### Tensor and theta

- **docs** (2026-10-04): `Tensor` must come from `sim::ml_chassis`
  (solver/mod.rs:17), and sim-soft doesn't say so. Still true 2026-10-06.
  There is also a trap: `Tensor::zeros(&[])` has length 1, so a scene with
  no loaded vertices panics with "theta has length 1 but BC has no loaded
  vertices" (assembly.rs:177-181). `Tensor::from_slice(&[], &[0])` works.
  *(probe: empty theta: Tensor::zeros(&[]) has length 1)*
- **docs** (2026-10-06): the docs call theta a traction, but with `AxisZ` it is
  one nodal force applied to every loaded vertex, so the total load is
  n_loaded x theta (assembly.rs:192-201). `NewtonStep` returns no velocity,
  and the docs don't say to compute `(x_final - x_prev) / dt`.
  From the source; not run.

#### Meshing and SDFs

- **perf** (2026-10-04): the SDF mesher keeps every grid vertex
  (sdf_meshed_tet_mesh.rs:26-31, 187). Still true 2026-10-06:
  `dropping_sphere` at r 2 cm, cell 3 mm keeps 39,732 vertices and uses
  3,155 (92% unused). The unused vertices also go into `x_prev`,
  `boundary_surface` and `Tet10Mesh::from_tet4`. *(probe: SDF mesher keeps
  every grid vertex)*
- **bug** (2026-10-06): meshing breaks at small scales. Sub-tets under an
  absolute 1e-15 m^3 are dropped (stuffing.rs:142, 571). A sphere that
  meshes to 2,208 tets at r 2 cm gives `EmptyMesh` at r 20 um with the same
  cell ratio. *(probe: mesher not scale-invariant)*
- **bug** (2026-10-06): the lattice has no size cap. `BccLattice::new`
  reserves `2 nx ny nz` positions and `12 cubes` tets before it samples
  anything (lattice.rs:281-310), and sdf_meshed_tet_mesh.rs:145 wrongly
  says the lattice caps itself. Measured with `dropping_sphere`:

  | Sphere | Cell | Result | Time | Peak memory |
  |---|---|---|---|---|
  | r 2 cm | 1 mm | 410k tets | 0.4 s | 0.13 GB |
  | r 5 cm | 1 mm | 6.3M tets | 8.5 s | 1.8 GB |
  | r 5 cm | 0.5 mm | 50M tets, 57% of vertices unused | 82 s | 13.5 GB commit |

  At r 10 cm, cell 0.1 mm the process aborts ("memory allocation of
  391536777456 bytes failed", exit 0xc0000409). A cell of 1e-12 saturates
  the `as i32` extents, the `+ 1` wraps in release (lattice.rs:257-278),
  and the process aborts asking for 2.47 TB. Neither returns a
  `MeshingError`. *(probe: lattice size uncapped; the timings come from
  `gaps_sim_soft --mesh R CELL`)*
- **docs** (2026-10-06): a box with min > max panics ("bbox.min must be
  componentwise <= bbox.max", lattice.rs:251-255), while `Aabb3::new`'s
  doc promises `MeshingError::EmptyMesh` (sdf_bridge/mod.rs:69-72).
  *(probe: inverted bbox panics)*
- **bug** (2026-10-06): `SdfMeshedTetMesh::with_projected_nodes` writes NaN
  into the mesh for a non-finite target (sdf_meshed_tet_mesh.rs:490). The
  Tet10 version guards this (tet10_mesh.rs:442, 664). A vertex out of
  range is skipped silently (466), but the doc says it panics (400-403).
  *(probe: with_projected_nodes: NaN target; ...: vertex out of range)*
- **bug** (2026-10-06): `DifferenceSdf` doesn't forward `hessian`, so it
  returns the trait's zero default (difference.rs:104-120). On the outer
  surface the sphere's hessian norm is 14.14 and the difference's is 0.
  Its `eval` uses `f64::max`, which drops NaN: a sphere minus an all-NaN
  SDF evaluates to -0.05, which slips past `NonFiniteSdfValue`.
  *(probe: DifferenceSdf drops the hessian; DifferenceSdf hides NaN)*
- **bug** (2026-10-06): `HandBuiltTetMesh::uniform_block` with a Yeoh field
  panics ("MaterialField::sample expects NH variant",
  material_field.rs:352), but the comments there call that path
  unreachable, and no constructor documents it. *(probe: Yeoh field on a
  hand-built mesh)*
- **bug** (2026-10-06): `Tet10Mesh::from_tet4` on a mesh that is already
  Tet10 is guarded only by `debug_assert!` (tet10_mesh.rs:127). In release
  it turns the 125 vertices into 125 "corners". *(probe: Tet10 enriched
  twice)*

#### Scenes, readouts, lowering

- **API** (2026-10-06): reward stubs. `RewardBreakdown::apply_residuals` is
  `unimplemented!()` (reward_breakdown.rs:73-74), as are `BasicObservable`'s
  three field methods (observable/basic.rs:91-99). `score_with` drops NaN
  terms (reward_breakdown.rs:47-62), so a diverged all-NaN breakdown scores
  0, ahead of a poor real design at -4. *(probe: reward breakdown stubs and
  NaN)*
- **docs** (2026-10-06): `peak_bound` is documented as "Peak Cauchy stress ...
  (unitless)" (reward_breakdown.rs:27), but `BasicObservable` fills it
  with a sum of positions in metres (observable/basic.rs:132-139). The
  scene module says every constructor returns a 3-tuple (scene.rs:3-4);
  only `one_tet_cube` does. `SceneInitial` isn't `Clone` (scene.rs:955).
  From the source; not run.
- **API** (2026-10-06): the lowering and obstacle pipeline leads nowhere for a
  facade user. `Lowered.model: ExplicitModel`, `LoweringError::Model(ModelError)`
  (lowering/model.rs:14, 55), `BakedObstacle.grid` and `.fine`
  (obstacle.rs:48, 52), and `SampledPath.poses` and `::obstacle`
  (lowering/path.rs:705, 713) are all sim-soft-explicit types. Neither
  sim-soft nor the facade re-exports that crate, so you can't name them or
  run the explicit solver. `lower()` also rejects per-element densities as
  `MaterialsMeet` (lowering/model.rs:158-159). From the source; not run.
- **setup** (2026-10-06): `profile::snapshot()` always reads zero through the
  facade. Its `phase-timing` feature (sim-soft Cargo.toml:72) isn't
  forwarded; see the facade entry. From the source; not run.

#### Prints

- **bug** (2026-10-04): the library prints to stderr. Still true 2026-10-06:
  a negative-mu tet writes 4 lines like "sim-soft: faer LU fallback fired at
  factor_and_solve_free ...", and a NaN mu prints one in the middle of the
  probe's output. They come from factor.rs:417 and 727 (LU fallback) and
  from the LM retry logs (337, 354, 360, 378, 808, 821, 827, 838).
  *(probe: library prints to stderr)*

### `sim-coupling`

Checked by the same probe, `gaps_sim_soft`. Paths are under sim-coupling's
`src/`. The scene throughout: a 0.2 kg platen (or 0.3 kg ball) on a free
joint over a soft cube with edge 0.1 m and its bottom pinned.

#### Construction

- **API** (2026-10-04): `StaggeredCoupling::new` takes 11 positional
  arguments (construct.rs:29-41) and hard-wires lambda = 4 mu
  (construct.rs:42). It only builds a cube block with its bottom face
  pinned (44-48). Still true 2026-10-06. The only check is the "no z=0
  base" assert (48):
  - n_per_edge 3 panics with "nz must be >= 2 and even (bilayer interface
    ...)" from sim-soft's `cantilever_bilayer_beam`, and `new`'s
    `# Panics` doesn't mention even sizes *(probe: odd n_per_edge)*;
  - body 7 of 2 is accepted, and `step` panics with an index out of bounds
    *(probe: body index out of range)*;
  - mu NaN is accepted; `step` then panics in the Armijo line search
    *(probe: mu NaN accepted)*.
- **bug** (2026-10-06): `dt` drives only the soft side. The doc says it's
  "used for both" (construct.rs:16-17), but the rigid body steps at the
  MJCF timestep. With dt 1e-3 and no `<option timestep>` (0.002), 10
  free-fall steps give vz -0.1962, the model's rate, not -0.0981. The
  gradients carry the soft dt (freebody.rs:86 and the control and policy
  paths), so a mismatch also makes them wrong. *(probe: dt vs the MJCF
  timestep)*
- **API** (2026-10-06): `new` doesn't run forward kinematics. Skip
  `data.forward()` and the first step sees the plane at -clearance, inside
  the whole block: fz is -29,883 N against -898 N after `forward()`.
  *(probe: new doesn't run forward)*
- **API** (2026-10-06): the soft block is weightless. The coupling's
  `SolverConfig::skeleton()` has gravity_z 0, and there's no setter for it,
  density, tol or the Newton cap. In 20 steps without contact, no block
  vertex moves. *(probe: soft block weightless)*

#### The rigid body

- **bug** (2026-10-06): `qvel[2]` is hard-coded as the body's vertical
  velocity (step.rs:88, also freebody.rs, control.rs, policy_grad.rs,
  single_step.rs:173):
  - a body on one hinge (nv 1) is accepted by `new`, and `step` panics with
    "Matrix index out of bounds" *(probe: body without a free joint)*;
  - put one slide-joint body ahead of the platen and the damping reads the
    platen's y velocity instead: after 50 steps the platen falls at
    -0.4905 m/s instead of settling at -0.0327 *(probe: platen not the
    first joint)*.
- **bug** (2026-10-06): `rigid_z` and `data().xpos` are one step behind. Both
  read `xpos` after `data.step`, which doesn't redo forward kinematics, so
  after 10 steps rigid_z is 0.499559 while qpos z is 0.499460 (step.rs:113),
  and the next contact is posed from the stale value (construct.rs:225).
  The same lag means the last control can never move the result:
  `coupled_trajectory_control_gradient` returns dz/du = [1.27e-5,
  1.09e-5, 8.50e-6, 5.00e-6, 0] for 5 controls. The docs blame sim-core
  for integrating with the starting velocity (freebody.rs:38, 59), but
  qpos has already moved; the stale value is `xpos`. *(probe: rigid_z one
  step behind; last control gets no gradient)*
- **API** (2026-10-06): there's no `data_mut`, and `step` overwrites
  `xfrc_applied[body]` (step.rs:103), so the only way to push on the body
  is through the control or policy rollouts. `step` panics on a soft-solver
  failure, but its `# Panics` mentions only the rigid side (step.rs:18-21),
  and there's no `try_step`. From the source; not run.

#### Contact

- **docs** (2026-10-06): `CoupledStep.force_on_soft` is documented as "the
  force the soft body exerts" (types.rs:12), but it's the force on the
  soft body: -898 N (down) under a resting platen. *(probe: force_on_soft
  sign)*
- **API** (2026-10-06): the penalty contact pushes from up to d_hat away.
  The platen scene from the crate's tests uses d_hat 1 cm and starts the
  contact plane 3 mm above the block, inside that band. The first step
  gives 1,025 N on a 1.96 N platen and launches it: after 0.3 s z is
  0.1166 (it started at 0.108) and there's no contact. That explains the
  2026-10-04 "spike after ~200 steps". Dropped from above the band, the
  platen settles on its weight (peak 3.9 N), but it rests with the plane a
  full 10.0 mm above the block. The docs mention neither the band nor the
  hover. *(probe: platen starts
  inside the contact band; dropped platen floats)*
- **API** (2026-10-06): the collider is placed by z only. The plane is
  infinite (construct.rs:225): a platen moved 0.45 m off to the side still
  presses with the same -898 N. The sphere collider's centre is pinned to
  the block's top centre (construct.rs:238), so a ball 0.45 m to the side
  presses -107 N, the same as one directly above. *(probe: contact plane
  infinite; sphere collider ignores x/y)*
- **bug** (2026-10-06): `step_kinematic` with the default plane collider
  puts the plane at z 0 (step.rs:200) and crushes the block: |f| 26,956 N
  with the platen 0.4 m clear. `set_sphere_center` is a no-op for the
  plane, and nothing stops this. *(probe: step_kinematic with the plane
  collider)*
- **bug** (2026-10-06): `step_articulated` ignores `rigid_damping` and
  `with_contact_moment` (step.rs:165), and it uses the COM `xipos` where
  `step` uses `xpos` (articulated.rs:17-21). The free-body gradients'
  forward passes route only the linear force, so with the moment on they
  don't follow `step` (freebody.rs:158-162). `coupled_step_material_vz`
  drops the damping (single_step.rs:165-176). From the source; not run.

#### Gradients

- **API** (2026-10-04): no gradients with friction, and `StaggeredCoupling`
  isn't `Clone` (lib.rs:89). Still true 2026-10-06:
  - `coupled_trajectory_material_gradient` panics with "friction-exact
    gradient requested without x_prev", at eps_v 1e-3 and at 0.1
    *(probe: gradient with friction)*;
  - each trajectory gradient advances the scene, so calling it twice on
    one coupling gives z 0.121204 and then 0.124872. You have to rebuild
    the scene, since it can't be cloned *(probe: trajectory gradients
    consume the scene)*.
- **docs** (2026-10-06): `param_idx` 0 is the partial d/dmu with lambda held,
  but the block ties lambda = 4 mu, so the design gradient is
  g0 + 4 g1. Only the single-step doc says so (single_step.rs:493-495); the
  trajectory doc (freebody.rs:74-83) doesn't. From the source; not run.
- **API** (2026-10-06): the free-body gradients don't check their
  preconditions (a free joint, no contact moment), while the articulated
  and control ones assert them (control.rs:173-190, policy_grad.rs:56-82).
  From the source; not run.
- **perf** (2026-10-06): every step rebuilds the tet mesh 2-4 times and
  builds a new `CpuNewtonSolver` (contact_readout.rs:85-87, step.rs:60,
  77). At n_per_edge 4 (384 tets) a step costs 5 ms. `BondedSandwich`
  builds its solver once (bonded.rs:330-339). From the source, with the
  measured step time.
- **works** (2026-10-06): the validated case still holds. A 0.3 kg ball
  resting on the block reads 2.875 N of support against 2.943 N of weight
  after 200 steps. *(probe: ball settles on the block)*

### `sim-ml-chassis`

`cargo run --release --example gaps_ml_chassis` checks this crate and
sim-rl in one run (5 s). On 0.9.0: 33 open, 4 fine (both crates). Paths
are under ml-chassis's `src/` unless noted.

#### SimEnv and VecEnv step differently

- **bug** (2026-10-06): the two envs disagree about `forward()`.
  `SimEnv::step` calls it after stepping (env.rs:145); `VecEnv::step`
  doesn't (vec_env.rs:170-249, only reset envs get one, 235). Either way
  something breaks:
  - in a VecEnv, sensors and other derived fields (`sensordata`, `xpos`,
    `site_xpos`, `actuator_force`, energy) describe the state before the
    last integration: after one step the shoulder sensor reads 0.000000
    while qpos is 0.002915. SimEnv reads 0.002915 for both. *(probe: VecEnv:
    sensors one step stale)*
  - in a SimEnv, the passive callback runs twice per step (20 calls in 10
    steps against VecEnv's 10), which is the thermostat double draw
    logged under sim-therm-env. *(probe: SimEnv: passive callback runs
    twice per step)*
  - under implicitspringdamper `forward()` itself moves qvel (see sim-core),
    so after 50 steps the shoulder's qvel is -1.478 in a SimEnv and 2.506
    in a VecEnv. *(probe: SimEnv and VecEnv differ (implicitspringdamper))*

  The parity test (vec_env.rs:512-554) compares only qpos/qvel under
  Euler, so it misses all three.
- **works** (2026-10-06): with Euler and no passive callback, SimEnv and a
  one-env VecEnv walk the same path bit for bit. *(probe: SimEnv and VecEnv
  agree (Euler))*
- **API** (2026-10-06): when done and truncated are both true, `SimEnv`
  returns both (env.rs:149-150) and `VecEnv` forces truncated to false
  (vec_env.rs:195-199). *(probe: done and truncated disagree)*
- **bug** (2026-10-06): SimEnv's early break inside the sub-step loop calls
  `done_fn` before any `forward()` (env.rs:139), so a done based on
  `site_xpos` (like the stock reaching tasks) is tested on stale positions
  there and on fresh ones at 149. From the source; not run.
- **works** (2026-10-06): a VecEnv truncation keeps the terminal observation
  and returns the new episode's first one. *(probe: VecEnv keeps the
  terminal obs)*

#### Divergence

sim-core resets a diverged `Data` inside `step` and returns Ok (see
sim-core). The envs handle that differently:

- **bug** (2026-10-06): `SimEnv` never checks `divergence_detected()`
  (env.rs:137-161). A NaN force at t = 0.20 returns Ok with done false, and
  the time is back to 0.01, so a time-based truncation restarts and the
  episode can run forever. *(probe: SimEnv hides divergence)*
- **API** (2026-10-06): `VecEnv` catches it (176-180), but only as a plain
  `done`: `errors[i]` stays None, though its doc (44-46) says physics
  failures land there, and the terminal obs is None. With sub-steps,
  sim-core restarts the episode in the middle of the step, so the reward is
  from the restarted episode (0.0286 here), not "before auto-reset" as the
  doc says (30). A learner can't tell a blow-up from success.
  `Trajectory` has no flag for it either (rollout.rs:20-33). *(probe:
  VecEnv: divergence is a plain done)*
- **bug** (2026-10-06): sim-core tests qpos/qvel at the start of a step, so
  a state the last integration pushed past 1e10 is returned as a normal
  step first: qpos 1.004e10 with done false, then done on the next step.
  *(probe: VecEnv: divergence seen one step late)*

#### Spaces and actions

- **bug** (2026-10-06): `SimEnv::step` and `ActionSpace::apply` don't check
  the action length (env.rs:134, space.rs:750-756): for 2 ctrls, a 1-long
  action panics ("range end index 2 out of range") and a 5-long one is cut
  silently. `step` returns a Result and could say so. VecEnv checks the
  shape (vec_env.rs:153). *(probe: SimEnv: action length unchecked)*
- **bug** (2026-10-06): `check_flat` only tests `end <= len` (space.rs:550-563),
  so `qpos(2..1)` builds with dim 0 and then `extract` panics ("slice
  index starts at 2 but ends at 1", 148). The module doc promises "never
  runtime panics during extraction" (7-9). The same goes for every range
  segment. *(probe: reversed obs range builds)*
- **API** (2026-10-06): overlapping action injectors build: `ctrl(0..2)
  .ctrl(1..2)` gives dim 3 for 2 ctrls, the last write wins (852-873).
  *(probe: overlapping action injectors)*
- **API** (2026-10-06): `mocap_pos` / `mocap_quat` name their argument
  `body_range` (829-845) but take mocap ids: the mocap body's body id (3)
  is refused as out of range (nmocap 1). An all-zero quaternion action
  normalizes to NaN (674-681), and nothing flags it, since sim-core only
  checks qpos/qvel/qacc. *(probe: mocap injectors)*
- **docs** (2026-10-06): the `energy()` observation (409-414) reads `[0, 0]`
  unless the model has `<flag energy="enable"/>`, and its doc doesn't say
  so. *(probe: energy obs needs the flag)*
- **docs** (2026-10-06): `TaskConfig::obs_scale`'s doc says "multiply raw
  observations element-wise by these scales before feeding into a
  policy" (task.rs:99-103), but `LinearPolicy` and `MlpPolicy` scale
  internally (linear.rs:71-76, mlp.rs:32-37). Doing what the doc says
  scales twice. From the source; not run.

#### Tasks and VecEnv construction

- **bug** (2026-10-06): `TaskConfig::from_build_fn` checks nothing
  (task.rs:155-172): a task that says act_dim 1 with a 1-long obs_scale
  builds over an env with 2 ctrls and obs dim 4. `VecEnv` has no
  `observation_space()` / `action_space()` getters, so the caller can't
  check either. *(probe: from_build_fn unchecked)*
- **API** (2026-10-06): `TaskConfigBuilder` has no `on_reset` and ignores
  the seed (task.rs:287-292, "accept-and-ignore"), so builder tasks can't
  randomize starts and replicates with different seeds run the same
  episodes. From the source; not run.
- **API** (2026-10-06): `VecEnv` always uses `BatchSim::new`
  (vec_env.rs:406), so per-env models (`BatchSim::new_per_env`) aren't
  reachable. `batch_mut()` would let a user swap the batch, but the env
  still applies actions and runs reset `forward()` with its own model.
  From the source; not run.
- **bug** (2026-10-06): `reset_all` returns at the first `forward()` error
  (vec_env.rs:276-285), so later envs skip their `on_reset` and `forward`.
  From the source; not run.

- **API** (2026-10-06, Step 4): the reward, done and truncated hooks are
  `Fn(&Model, &Data)` shared by every env (vec_env.rs:340-358), with no env
  index and no episode state; `on_reset` gets the index (373), the reward
  doesn't. A reward that depends on the episode so far needs a side table.
  The smart wash pays only when the i9's latch improves (the best answer
  seen so far), so it keeps a `Mutex<HashMap>` keyed by each env's `Data`
  address and spots a new episode by `time` going back (src/trade/smart.rs
  `wash_env`). It works because the batch never moves its `Data`, which
  nothing promises.

#### Rollouts

- **bug** (2026-10-06): `collect_episodic_rollout` takes act_dim from env
  0's first action (rollout.rs:108-109). Another env's short action runs
  and is zero-padded silently (recorded as `[]`); a long one on the last
  env panics ("index out of bounds: the len is 6 but the index is 6").
  *(probe: rollout: action length)*
- **bug** (2026-10-06): a VecEnv of 0 envs builds (vec_env.rs:386-419), and
  the rollout then panics in `Tensor::row`. `max_steps = 0` still takes a
  step (rollout.rs:139-158), against its doc (74-75). *(probe: rollout:
  zero envs, zero steps)*
- **perf** (2026-10-06): finished envs keep stepping with zero actions and
  auto-resetting until the slowest env finishes (rollout.rs:164-185). With
  episodes of 6 and 56 steps, env 0's `on_reset` ran 10 times in one
  rollout. That wastes physics, burns `on_reset`'s RNG, so one env's
  starts depend on the others' episode lengths, and CEM's `total_steps`
  under-reports what ran. *(probe: rollout steps finished envs)*

#### Policies and values

- **bug** (2026-10-06): `MlpPolicy`, `MlpValue`, `MlpQ` (mlp.rs:105, 259-277,
  383-403) and `AutogradPolicy::new` (autograd/policy.rs:117-177) start at
  all zeros. tanh(0) is 0, so the hidden layer never gets a gradient: only
  the output bias learns, forever. `MlpPolicy`'s `log_prob_gradient` is
  nonzero in 1 of 41 entries, `AutogradPolicy` too, and `MlpQ`'s
  `action_gradient` is `[0.0]`, so a TD3/SAC actor gets nothing. This is
  the documented recipe for PPO and TD3 (sim-rl ppo.rs:67, td3.rs:75-76,
  algorithm.rs:69). `AutogradPolicy::new_xavier` exists; the Mlp types
  have no init option and no warning. CEM is unaffected (its noise breaks
  the symmetry). *(probe: Mlp starts at zero)*
- **bug** (2026-10-06): the linear policies zip the observation with
  `obs_scale` (linear.rs:71-76), so a wrong length is silent: at obs_dim 2,
  `forward([1])` and `forward([1, 0, 9])` both give tanh(1). `LinearQ`
  (and `MlpQ`) join obs and action before the weights (linear.rs:292-296,
  mlp.rs:407-411), so a short obs shifts the action into an obs weight:
  Q([1], [1]) is 3 where Q([1, 0], [1]) is 4. *(probe: LinearPolicy: obs
  length; LinearQ: obs and action misaligned)*
- **bug** (2026-10-06): the default `forward_batch` (policy.rs:59-67) drops a
  trailing partial row: 5 floats at obs_dim 2 give 2 outputs, and obs_dim 0
  divides by zero. *(probe: forward_batch drops a partial row)*
- **bug** (2026-10-06): `log_prob_gradient` with sigma 0 returns `[inf, inf,
  inf]` (linear.rs:128-133, mlp.rs:213-217). *(probe: sigma 0 gradient)*
- **API** (2026-10-06, Step 4): `Policy` is an open trait, and a hand-written
  one is easy (five methods; the smart wash computes its own features from
  the observation and maps them to a log temperature). But `descriptor()`
  must return a `PolicyDescriptor` whose `NetworkKind` is closed (Linear,
  Mlp, Autograd; artifact.rs:47-54), so a custom policy has to claim to be
  one of them. `to_policy` then rebuilds a `LinearPolicy` of the claimed
  size (artifact.rs:399-401), not the policy that was saved, so
  checkpoints and `best_artifact` of a custom policy don't round-trip. CEM
  itself only reads the params, so training works.
- **API** (2026-10-06, Step 4 memory): a policy can't remember anything.
  `Policy::forward(&self, obs)` (policy.rs:45) gets no env index and no
  state, and there are no recurrent policies. CEM's rollout closure has
  the env index (sim-rl cem.rs:164-165) but drops it. The observation
  can't carry memory either: `ObservationSpace` only reads fixed `Data`
  fields (space.rs:293-434; no custom extractor). So the smart wash's
  memory (when the strips froze, when the latch last improved) lives
  behind a `Mutex<HashMap>` keyed by a hash of the params, which works
  only because CEM calls `set_params` right before each `forward`
  (src/trade/smart.rs `Trainee`). Passing `env_idx` to `forward`, or a
  per-env policy state, would make memory policies first-class.

#### Autograd, optimizer, replay buffer

- **bug** (2026-10-06): a second `Tape::backward` re-propagates the
  intermediate cotangents (autograd/tape.rs:748-790), so for b = 6x the
  gradient goes 6, then 18. The doc says the second call accumulates,
  which would give 12. *(probe: Tape: second backward)*
- **bug** (2026-10-06): Adam takes any settings (optimizer.rs:216-244). A
  negative `max_grad_norm` reverses the step (+0.1 becomes -0.1), 0 freezes
  it, and beta1 = 1 gives NaN. A NaN gradient skips clipping. *(probe:
  Adam settings unchecked)*
- **docs** (2026-10-06): `Optimizer::params()` says "after the most recent
  step" (optimizer.rs:56) but stays at its initial zeros under
  `step_in_place`, which every algorithm uses. `load_snapshot` ignores the
  snapshot's config (lr comes from the new one) and checks only the `m`
  length; a wrong `v` length panics in `copy_from_slice` (321-330). From
  the source; not run.
- **bug** (2026-10-06): `ReplayBuffer::new(0, ..)` builds and the first push
  panics ("range end index 2 out of range", replay_buffer.rs:103-110).
  *(probe: ReplayBuffer capacity 0)*

#### Artifacts, checkpoints, competition

- **bug** (2026-10-06): `TrainingCheckpoint::save` doesn't validate,
  though its doc says it errors on non-finite values (artifact.rs:623-634).
  serde_json writes NaN as `null`, so a checkpoint saved with a NaN
  `noise_std` saves Ok and won't load ("invalid type: null, expected f64").
  `PolicyArtifact::validate` checks `metrics[i].mean_reward` but not
  `extra`, so a NaN there does the same. The comment at best_tracker.rs:90
  ("serde_json rejects non-finite") is wrong. *(probe: checkpoint: NaN
  round trip)*
- **API** (2026-10-06): a custom `impl Policy` trains with CEM but can't be
  saved: `NetworkKind` (artifact.rs:47-54) is Linear, Mlp or Autograd, so
  `save` / `to_policy` fail or rebuild the wrong type. From the source;
  not run.
- **bug** (2026-10-06): `CompetitionResult::save_artifacts` names files
  `{task}_{algorithm}` (competition.rs:124-137), ignoring
  `replicate_index`, so with several seeds only the last replicate's files
  survive. `RunResult::best_reward` uses `max_by` with an `Equal` fallback
  (49-54): `[1, NaN]` gives NaN, `[NaN, 1]` gives 1, and ties pick the last,
  unlike the provenance's best (strict `>`, 692-707). The provenance
  `hyperparams` is always empty, and with 0 epochs `final_reward` is 0
  (711-719). From the source; not run.

### `sim-rl`

Checked by the same probe, `gaps_ml_chassis`. Paths are under sim-rl's
`src/`.

#### CEM's real job: a wash program

- **works** (2026-10-06): CEM learns a real wash program. One sock sits in
  a tilted double well (barrier 3, tilt 0.5), starting in the shallow
  well. A `LinearPolicy` on `[x, v]` sets the drum temperature (0 to 3 kT)
  through sim-therm-env's ctrl temperature, and the reward is time in the
  deep well minus heat used. In 25 epochs of 32 socks (0.8 s) it finds
  temp = tanh(-2.44 x - 0.09): heat while the sock is in the shallow well,
  cool once it's out. That scores 50.8 of 100, against 0 for cold, 20.6
  for always hot and 24.3 for the best constant. The chassis pieces
  (therm-env `build_vec`, `LinearPolicy`, `collect_episodic_rollout`)
  fit together without glue. *(probe: CEM learns a wash program)*
- **works** (2026-10-06): the same seed on a deterministic task replays bit
  for bit. *(probe: CEM: same seed replays)*
- **works** (2026-10-06, Step 4): CEM learns a wash program for the real
  machine that generalizes. It ran on 17-20 coupled strips, the whole
  board of the night, at Normal's length. Training: 66 generations of 32
  spins over 11 hard nights, 16 min single-threaded. Result, on boards it
  never saw: 93% vs Normal's 83% best set over nights 61-80, and 88% vs
  81% over nights 1-10. Each cool-down starts hotter and cools faster
  while strips are mid-flip, six cool-downs a cycle with the i9 latch
  keeping the best. That needed four things from the user:
  - our own env, because therm-env fixes the observation;
  - a side table for the latch reward;
  - one `train` call per night;
  - training only on hard nights.
  The CEM itself just worked. (src/trade/smart.rs; PLAN "Step 4 so far".)
- **API** (2026-10-06, Step 4): each candidate is scored on one episode of
  one env (162-180), and an env is one model, so one board. There's no way
  to score a candidate over several episodes or several tasks. The smart
  wash has to work on every night, so it trains one `train(Epochs(1))`
  call at a time, rotating over a `VecEnv` per night. That works: the mean
  (the policy's params) and `noise_std` carry over between calls. But each
  elite pick is one stochastic spin, and the latch outcome is nearly binary,
  so the selection is noisy. A test of Normal's own schedule against itself
  at 12 spins a night varies ±15 points per night.
- **API** (2026-10-06, Step 4): the elites of an easy task all tie (every
  candidate lands the best set), so whatever small term is left decides
  the ranking. Run 1 of the smart wash had a 0.02 heat cost as a
  tie-breaker, and CEM followed it to a drum 2.7x colder that then missed on
  hard nights (revisited nights fell from 1.6 to 1.0 reward). That's
  CEM, not a bug, but with no per-candidate repeats there's no way to tell
  a real gain from a tie.
- **API** (2026-10-06, Step 4, run 6): CEM keeps no held-out check, so a
  run can end worse than it started. Started from a program that scored 94%
  on mixed held-out boards (with wants and give-aways), 99 generations over
  33 mixed training boards ended at 91%: colder, slower cool-downs, worse
  on every kind of board. `best_artifact` can't catch it. Its tracker
  keeps the epoch with the highest `mean_reward` (cem.rs:216-217), and
  when every epoch runs on a different board that mostly measures the
  board's difficulty, not the program. A hook to score candidates on a
  fixed validation set (or keep the best by it) would let a user stop a
  drift. Here a separate `versus` run after training caught it.

#### CEM

- **bug** (2026-10-06): CEM ranks elites by reward per step (cem.rs:172-180)
  but reports reward per episode (208-213), and the docs don't say so.
  When episodes end early, it optimizes the wrong thing. With +1 per step
  alive, every episode ties at 1.0, so the stable sort picks elites by env
  index and CEM can't learn to survive: 4 of 8 seeds end with every cart
  alive, the others at 133-269 of 300. *(probe: CEM: per-step fitness)*
- **bug** (2026-10-06): `best_artifact` stores the elite mean after the
  update (200), scored with the reward of the population around the old
  mean. Those parameters were never rolled out: after 1 epoch of the wash
  job, `best_reward` is 17.8 (epoch 0's mean) for parameters that score
  45.9. REINFORCE, PPO, TD3 and SAC also snapshot after the update. There's
  no way to get the best single candidate. *(probe: CEM: best never
  evaluated)*
- **bug** (2026-10-06): `n_elites` isn't capped at the population (142), so
  elite_fraction 1.5 with 4 envs panics ("range end index 6 out of
  range"). *(probe: CEM: elite_fraction > 1)*
- **bug** (2026-10-06): nothing else is checked either. elite_fraction 0 or
  NaN silently becomes one elite (`NaN as usize` is 0), and `n_envs = 1`
  or elite_fraction 1 makes the update a random walk. A NaN noise_std
  trains on with NaN parameters, and the episodes still return finite
  rewards because sim-core resets the NaN state (-259 here). noise_min >
  noise_std or noise_decay > 1 make the noise grow. *(probe: CEM:
  hyperparameters unchecked)*
- **bug** (2026-10-06): a NaN reward compares equal to everything in the
  elite sort (183), so it can be picked: every epoch's mean_reward is NaN,
  `BestTracker` skips NaN (ml-chassis best_tracker.rs:49-52), and
  `best_reward` stays None, so `best_artifact` is the initial parameters.
  No error. (It didn't panic at 32 envs.) *(probe: CEM: NaN reward)*
- **bug** (2026-10-06): `TrainingBudget::Steps(s)` becomes `s / (n_envs *
  max_episode_steps)` epochs (138, same in every algorithm). With 10 envs
  and 300 max steps, `Steps(2999)` trains 0 epochs silently, and
  `Steps(3000)` runs one that uses 1000 steps because episodes end early.
  *(probe: CEM: Steps budget)*
- **docs** (2026-10-06): the `noise_std` in an epoch's metrics is the
  decayed value for the next epoch (219-222): epoch 0 ran at 0.3 and
  reports 0.270. *(probe: CEM: noise_std metric)*
- **API** (2026-10-06): `from_checkpoint` doesn't check `algorithm_name`
  (90-112, every algorithm): a checkpoint named PPO loads into `Cem`.
  Each `train()` numbers epochs from 0, so `best_epoch` is ambiguous after
  a resume. *(probe: from_checkpoint ignores the name)*
- **docs** (2026-10-06): this is not textbook CEM (1-6, 154-156): the noise
  is a fixed isotropic schedule on the raw parameters and is never refit
  from the elites, and the old mean isn't kept, so the mean can get worse.
  Parameters in different units need hand scaling, and the docs don't
  say so. From the source; not run.
- **docs** (2026-10-06): the constructor examples write `CemHyperparams {
  elite_fraction: 0.2, noise_std: 0.3, .. }` (53-57; reinforce.rs:55-60,
  ppo.rs:65-71, ml-chassis algorithm.rs:66-76). That isn't valid Rust, and
  no `*Hyperparams` has a `Default`. They're marked `ignore`, so the
  compiler never caught it. From the source; not run.

#### TD3, SAC, PPO

From the source; not run (the probe exercises only CEM's training loop).

- **bug** (2026-10-06): TD3/SAC with `warmup_steps > buffer_capacity` never
  train: `buffer.len()` stops at the capacity, so actions stay random and
  no update runs (td3.rs:285, 359; sac.rs:288, 352). Warmup counts
  transitions (n_envs per step), not steps as the docs say. The buffer is
  local to `train()`, so it's lost between calls and missing from
  checkpoints.
- **bug** (2026-10-06): TD3 `policy_delay: 0` never updates the actor or
  the targets (`is_multiple_of(0)` is false, td3.rs:434). PPO `k_passes: 0`
  never updates and reports `value_loss` 0/0 = NaN (ppo.rs:424).
- **bug** (2026-10-06): SAC clamps actions to [-1, 1] after sampling, but
  `log_prob` and the reparameterized gradient use the unclamped Gaussian
  (sac.rs:291, 300, 369, 446-463), so the gradient is wrong at the bounds.
  TD3/SAC hard-code [-1, 1] instead of reading the `ActionSpace`.
- **docs** (2026-10-06): a checkpoint missing its critic gives
  `ParamCountMismatch { expected: 1, actual: 0 }` (ppo.rs:127-131,
  td3.rs:162-166, sac.rs:163-167), not a "missing critic" error.

#### Earlier

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
