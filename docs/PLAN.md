# Build plan: the few-hour slice

Slice = Lesson 3, **Money Laundering (Legally)**: a laundromat trade computer
whose bits are simulated by `cortenforge::sim::thermostat`. See `DESIGN.md`.

Status (2026-10-05): **Step 1 done** (headless trade computer in `src/trade/`,
CLI in `examples/trade_cli.rs`). Next action: **Step 2**.

## Step 1 result

Run it: `cargo run --release --example trade_cli -- [run|bench|cycles] [--want upd:hub] [--seed N]`.

- The Tuesday-night laundromat (7 NPCs, 16 items) yields **14 candidate
  trades** (2- to 4-way loops). The best set is 6 trades worth 35 Goo, and it
  skips the single most valuable loop (10 Goo), so greedy picking loses.
- One spin cycle takes 0.38 s wall on one thread (14 bits, 1020 time units,
  dt 0.01), or about 0.8 s when 12 run in parallel.
- **Two answers per spin.** *At rest* is where the strips physically settle.
  The *i9 latch* is the lowest-energy configuration the i9 reads during the
  spin (every time unit, only when every strip sits in a well). That's
  standard p-bit practice and fits the fiction: the i9 watches through the
  Hall sensors.
- Defaults (tuned by sweep): `beta` 5, `delta_v` 5, penalty 1.6 x max
  value, `gamma` 1, kT multiplier 4 -> 0.35 geometric over 1000, then 20
  units with the drum stopped.

| 192 seeded runs, forward mode | Best set | Clash-free |
| --- | --- | --- |
| At rest | 54% | 91% |
| i9 latch | **95%** | 100% |

Backward mode (96 runs each): the want is delivered in 100% of runs, and the
i9 latch reaches the best set 89% (cells), 93% (hub motor), 98% (soldering
iron) and 91% (Tamara wants the Ledger). Asking for the hub motor makes the
machine drop the 3-way that sent it to Dave and do a direct Goo-for-hub swap.

What tuning taught us (good in-game material):
- **Soft spins.** A strip pushed by strong springs sits well past |x| = 1
  (seen: -1.86). Then the linear compensation in `h` over-pushes its
  neighbours, and clashes freeze in. Raising the penalty made clashes worse
  (up to 100%). Keep couplings small next to the barrier.
- **Freeze-out.** At a given barrier, strips stop flipping around kT ~ 0.1
  to 0.2 x dV. Anneal time buys accuracy only logarithmically: 3x longer
  moved the at-rest rate from 62% to 65%. Spinning too fast is the in-game
  failure ("You spun it too fast, Daddy").
- The i9 first latches the best set early (median t of about 70 of 1020),
  but cutting the cycle to 300 or 500 units dropped the latch rate to 64% or
  80%, because the early hot phase explores.
- Backward-mode bias can exceed the well-flattening field (17.6 vs 7.7 for
  "Upddayett wants the cells"). That clamps the strip on, which in fiction is
  Upddayett holding it down with his thumb. It works; the field warning in
  the plan is advisory.
- Fixed bug: two cycles that both deliver the wanted item both get the bias,
  so their conflict penalty must include it or the machine takes both.

## Step 1 plan (as written before building)

Goal: prove the magic before any graphics. A trade problem goes in, the
thermostat sim spins up and winds down, and the bits settle on the best set of
trade chains. Print the settling in the terminal and check it against the
crate's exact solver.

### Encoding (maximum-weight independent set)

1. **World:** 6 to 8 NPCs (Upddayett, Shopping-Cart Guy, Vape Lady, ...), each
   with a few items they *have* and a few they *want*, plus a per-NPC value for
   each item.
2. **Candidate trades:** build the want graph (edge i -> j when j has an item i
   wants). Enumerate its 2-, 3- and 4-cycles. Each cycle is one candidate trade,
   one **bit** (do it or not). Value of a cycle = sum over participants of
   (value of item received - value of item given); drop cycles where anyone
   loses. Aim for 12 to 20 candidate cycles so the exact solver still works
   (it caps at n <= 20).
3. **QUBO:** minimize `-sum_c v_c x_c + P * sum_{c,c' conflict} x_c x_c'`, where
   two cycles conflict if they use the same item (or the same person twice, if
   we keep it simple). P must exceed the largest value so conflicts never pay.
   Normalize values to roughly [0.5, 1.5].
4. **Convert to Ising** with the probe's `Qubo::to_ising(beta)` (copy it from
   `examples/probe_therm.rs`, lines ~205-251). Convention: s = 2x - 1,
   H = -sum J s s - sum h s.
5. **Backward mode ("I want a hub motor"):** add a bias toward every cycle that
   delivers the wanted item to Upddayett, or a one-hot penalty
   `P * (1 - sum x_c)^2` over those cycles. Keep total |h| on any bit below
   ~1.5 * delta_v (see gotchas).

### Simulation recipe (proven in `examples/probe_therm.rs`)

```rust
use cortenforge::sim::core::{DVector, Data, Model};
use cortenforge::sim::mjcf::load_model;
use cortenforge::sim::therm_env::generate_mjcf;
use cortenforge::sim::thermostat::ising::{exact_distribution, tv_distance};
use cortenforge::sim::thermostat::{
    DoubleWellPotential, ExternalField, LangevinThermostat, PairwiseCoupling, PassiveStack, WellState,
};

// n bits, 1 actuator slot so ctrl[0] can drive temperature, timestep dt.
let xml = generate_mjcf(n, 1, dt, (0.0, 10.0));
let mut model: Model = load_model(&xml)?;
let mut data: Data = model.make_data();
let mut b = PassiveStack::builder();
for i in 0..n { b = b.with(DoubleWellPotential::new(delta_v, 1.0, i)); }   // x0 = 1
b = b.with(PairwiseCoupling::new(j.clone(), edges.clone()));
b = b.with(ExternalField::new(h.clone()));                                  // len must == n
b = b.with(LangevinThermostat::new(DVector::from_element(n, gamma), k_b_t, seed, 0)
    .with_ctrl_temperature(0));                                             // kT_eff = k_b_t * ctrl[0].clamp(0, 10)
b.build().install(&mut model);
data.forward(&model)?;
// anneal: data.ctrl[0] from high to low over the run; data.step(&model)? each step
// read bits: WellState::from_position(data.qpos[i], 0.5) -> Left / Right / Barrier
```

Starting parameters (from the probe): `delta_v` 3 to 6, `gamma` 1.0,
`k_b_t` 1.0, `dt` 0.005 to 0.01 (0.01 gave the same Kramers rate as 0.001),
`beta` 2 to 4. Cost is about 0.35 us per bit per step on one thread.

### Anneal schedule

Spin-up: `ctrl[0]` around 3 to 5 (hot, bits flip chaotically). Wind-down:
lower it to ~0.2 to 0.3 over a few hundred to a few thousand time units. Too
fast freezes the bits in a random state (the probe saw this at kT 0.3), which
is also the lesson the player learns.

### Acceptance

- [x] Final bits match the exact ground state (via `exact_distribution`) in at
      least 80% of seeded runs. *Via the i9 latch, 95%; at rest, 54%.*
- [x] One anneal finishes in under ~1-2 s wall time, single thread. *0.38 s.*
- [x] Terminal output: per-tick list of which trades are "on", then the final
      chain in words ("Vape Lady gives battery to Shopping-Cart Guy ...").
- [x] Backward mode finds a chain delivering the wanted item. *100% delivered.*

Put it in `src/trade/` as a library module (no Bevy) so Step 2 reuses it, plus
a small `examples/trade_cli.rs` or a `src/main.rs` mode.

## Step 2: Bevy laundromat scene (~1.5 h)

- Pin the latest stable Bevy (check crates.io; avoid release candidates).
  `bevy_egui` matching that Bevy version.
- Scene: laundromat box, washing machine, a board of slap bits on top. Each
  bit's strip deflection = `data.qpos[i]` (live from the sim), so you see real
  thermal twitching. The machine shakes harder as `ctrl[0]` rises.
- Trade overlay (required): NPC portraits in a ring around the machine; each
  candidate cycle drawn as arrows between portraits, alpha driven by its bit's
  state; chosen cycles lock and glow as the anneal finishes.
- Spin-cycle dial (egui) for temperature, plus a "Run cycle" button that plays
  a full anneal.
- Run the sim on a fixed timestep, several sim steps per frame. At 0.38 s per
  102k steps, a 60 fps frame fits ~1,700 steps (17 time units), so a full
  spin cycle plays in about 60 s of real time at 1x; offer a speed dial.
- API to use: `TradeComputer::new(world, beta, penalty)`, `.want(npc, item)`,
  `.machine(physics, seed)`, then drive `Machine::set_temperature` and
  `step()` per frame and read `positions()` / `well(i)`. The latch logic is
  `Latch::observe` (make it public or mirror it). `TradeComputer::spin`
  runs a whole cycle in one call (headless only).
- Show both the at-rest state (arrows) and the i9's latched best (an LED
  readout on the i9).

## Step 3: backward mode, voice, polish (as time allows)

- "I want..." picker that clamps a want and reruns.
- Two or three AI lines tied to results (froze too fast; found a 4-way chain).
- `bevy_solari` lighting under laundromat fluorescents; scope trace of one bit;
  one fake commercial card; Xbox-style title card.

## Gotchas (from the probes)

- Windows Smart App Control blocks Rust builds (os error 4551); it is now off.
- `ExternalField` stronger than about `1.54 * delta_v / x0` deletes the well;
  keep clamps and biases below it.
- `ExternalField::new` doesn't check length; indexing is unchecked.
- Couplings and fields can't change after `install`; changing a clamp means
  rebuilding the stack (cheap at this size).
- `exact_distribution` and `GibbsSampler` cap at n <= 20.
- Bit convention in `ising`: bit i set = spin +1.
- The sim's Langevin results differ from ideal Ising by 1 to 18%; don't promise
  exact answers in-game.
- `sim-thermostat` doc examples are all `ignore`; the `test-fixtures` models
  aren't reachable via the facade. Use `therm_env::generate_mjcf`.
