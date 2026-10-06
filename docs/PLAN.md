# Build plan: the few-hour slice

Slice = Lesson 3, **Money Laundering (Legally)**: a laundromat trade computer
whose bits are simulated by `cortenforge::sim::thermostat`. See `DESIGN.md`.

Status (2026-10-06): **Steps 1 and 2 done** (trade computer in `src/trade/`,
CLI in `examples/trade_cli.rs`, Bevy game in `src/main.rs` + `src/game/`).
**Step 3 done** (see the Step 3 plan below), except 3.2, deferred to a later
voice-and-music step:
- 3.0, 3.1;
- all of 3.3 (3.3a-d plus the costly-want clamp);
- 3.4, the escrow counter;
- 3.5, polish: layout, title card, ads, the TV, the scope, Solari;
- 3.6, the Salties, dumb and smart.
**The 0.9.2 gap hunt is done** (CortenForge 0.9.2 will fix what FINDINGS lists,
and every crate moves to 0.9.2). There is one probe per crate we use, in
`examples/gaps_<crate>.rs`, with one check per FINDINGS entry. Open gaps per
probe:
- sim-thermostat: 26
- sim-therm-env: 10
- sim-core + sim-mjcf: 27
- sim-ml-chassis + sim-rl: 33 (CEM learned a wash program)
- sim-soft + sim-coupling: 42

Next: the user's pick. Open candidates:
- the CEM-learned smart wash program;
- 3.2 voice and music;
- when 0.9.2 ships: bump every crate, rerun the `gaps_*` probes, drop the
  workarounds.

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
iron) and 91% (Tamara wants the vinyl). Asking for the hub motor makes the
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

## Step 2 result

Run it: `cargo run --release`. Screenshot mode: `UPD_SHOT=1 cargo run --release`
runs a fast cycle, writes `shots/*.png` and exits (handy for checking the
look without watching the window).

- Bevy **0.19.1** (latest stable; 0.20 was still rc) + `bevy_egui` **0.42.0**,
  with Bevy's `area_light_luts` feature for the fluorescent `RectLight`s.
- Code: `src/main.rs` + `src/game/{sim,scene,arrows,ui,shots}.rs`. The
  `Laundromat` resource owns the `TradeComputer`, the CortenForge `Machine`
  and the i9 `Latch`; `step_sim` advances `speed x frame time / dt` steps
  per frame (cap 5,000) and feeds the latch once per time unit.
- Scene: checker floor, a row of out-of-order washers, fluorescent tubes,
  green neon "MONEY LAUNDERING (LEGALLY)" (text gizmos, flickers). The hero
  washer shakes and its drum spins with the temperature; 14 slap-bit strips
  on top arch up or sag down from live `qpos`; magnet pucks and Hall LEDs
  glow green when on; the i9 on the front panel shows the latched set.
- Overlay: portrait cards above each customer; every candidate trade is a
  coloured arc with arrowheads, alpha from its strip, with the item riding
  along when on. When the drum stops, the i9's trades become thick pulsing
  HDR loops and the trade board prints the chain in words plus an AI line.
- A **board cam** inset (second camera, bottom-left) shows the strips up
  close.
- egui: Spin Cycle window (Run spin cycle, New load, spin dial for manual
  mode, sim-speed slider, at-rest vs i9 values) and a Trades board (ON/off/~
  per trade, locked trades highlighted).
- Default speed is 40 time units/s, so a cycle plays in ~25 s.

## Step 2 plan (as written before building)

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

## Step 3: make it a lesson (plan, 2026-10-05)

Work through these in order, one at a time: build, check (tests, CLI bench,
`UPD_SHOT` screenshots, logs), show the user, then commit. Each item lists
what it's for, how it works, and when it counts as done.

### 3.0 Groundwork: wash programs and fresh loads

Why: the user's first session showed two gaps. (a) After ~33,000 time units
on the manual dial, the strips had already settled into the best set, so the
i9 latched it 2 time units into the cycle: the cycle got credit for work
done before it. (b) The sim-speed slider only changes how fast you watch;
nothing lets the player spin too fast, and that failure is the lesson.

- **Wash programs** set the anneal *length* (the physics), separate from the
  watch speed: Quick Wash (~150 time units), Normal (1000), Delicates
  (3000). Measure each with `trade_cli bench --time` and show the expected
  hit rate next to the button, so the trade-off is honest and visible.
- **Run = fresh load.** Starting a cycle re-randomizes the strips (new
  laundry), so every cycle earns its answer. Manual mode stays a sandbox.
- Watch speed auto-scales so every program takes ~15-30 s to watch.
- Done when: Quick Wash visibly misses more often than Normal (matching
  bench numbers); no latch earlier than a few time units after a fresh load
  unless the random start happens to be best (log it); logs show the program.

**3.0 done.** Bench, 192 fresh loads each (i9 latch / at rest finds the
best set): Quick Wash 150 units 43% / 26%, Permanent Press 300 65% / 39%,
Normal 1000 95% / 54%, Delicates 3000 100% / 66%. In-game (screenshot mode):
Quick Wash 2 of 4 best (misses at 28-32 Goo), Delicates 2 of 2. The game
now seeds from the clock (`UPD_SEED=<n>` replays a session; the seed is
logged), so every launch is a different night. `UPD_PROGRAM=0..3` picks the
program in screenshot mode, whose stages now follow cycle progress (it hung
on short programs before).

### 3.1 "I want..." picker (backward mode in the UI)

- Picker: customer (default Upddayett) + item, listing only items some
  candidate trade can deliver to them; the rest greyed out ("nobody's
  trading that tonight"). A Clear button.
- Choosing a want rebuilds the board (couplings can't change after
  `install`, a known gotcha). Add `TradeComputer::clear_want` and a
  `Machine` constructor that keeps the current strip positions, so the
  strips don't jump.
- Cycles that deliver the want get a gold outline on their arrows and a
  "WANTED" tag on the trade board. The result says what the want cost: the
  block's value vs the forward-mode best ("Got you the hub motor. Cost the
  block 2 Goo.").
- Done when: every deliverable want for every customer is selectable and
  delivered (unit test over all wants: the ground state delivers and is
  clash-free; spot-check a few with the CLI bench); the result line shows
  the cost; logs show the want.

**3.1 done.** 20 deliverable wants in the Tuesday world (7 cost the block
2-10 Goo; the rest are free because the best set already delivers them).
`trade_cli wants --bench` benches every one. What it took:
- **Gentlest bias.** The worst-case want bias pinned strips on 14 of 20
  wants (fields up to 20 vs a 7.7 flattening limit). Now the bias is
  bisected to the smallest value whose exact ground state delivers, plus
  a margin of 1.0 x the smallest cycle value (swept 0.5/1/2/3: 1.0 is the
  best before fields reach the limit). Max field now 7.1, no clamps.
- **Optimal by value, not bits.** Several trade sets can tie (Tamara
  wanting the kale: ~70% of runs found an equally good set that wasn't the
  brute-force one). `TradeComputer::is_optimal` judges by value + clash-free
  + delivered; the CLI, the logs and the AI line all use it (the game would
  otherwise have roasted perfect runs).
- Result over all 20 wants, 96 Normal runs each: delivered mean 100%, min
  99%; i9 optimal mean 93%, min 75% (Dave wants the cells / Ray wants the
  derailleur, both cost 5 Goo and compete for the same chain). Forward mode
  unchanged at 95%.
- In-game: picker (customer, then item; undeliverable items greyed out with
  "nobody's trading that tonight"; cost on hover and in the caption), gold
  ring + "wants:" tag on the portrait, WANTED tag on the trade board, gold
  arrow for the leg that hands the item over, result line with the cost.
  Picking a want rewires the board keeping the strips (`take_state_from`).
  `UPD_WANT=who:what` sets a want in screenshot mode.
- Seen in-game: the hot phase (kT 4 vs dV 5) isn't very hot next to the
  problem's energy scale, so the i9 sometimes latches the best set within
  the first ~10 time units. Not a bug; worth remembering if we tune.
- Layout note for 3.5: the Spin Cycle panel is now tall enough to cover
  Sound Guy Ray's portrait card.

### 3.2 The AI's voice (DEFERRED: later, together with generated music)

Deferred 2026-10-05 at the user's call: the voice lines, their delivery and
music we generate belong together, so this moves to its own later step.
The design below stands; the current two placeholder lines stay meanwhile.

- A pure `voice` module in the library (no Bevy, unit-testable): an
  `Outcome` (program, want, at-rest bits, latched bits, best bits, latch
  time, longest loop, clashes at rest, at-rest vs latched) -> a line. No
  immediate repeats. Every line quotes a real number from the run.
- Categories: perfect run, missed (froze too fast), at rest differs from
  the i9 (the "I caught it, you didn't" line), found a 4-way, clash at rest,
  want delivered/cost, fresh-load quips, a few live lines during the spin
  (first latch, freeze-out as the drum cools past ~0.15 dV).
- Upddayett gets replies ("...I'm still the pimp though."). Shown as a
  Comedy Central-style caption bar at the bottom.
- The user reviews and edits the line list before it ships (tone).
- Done when: each category triggers in a deliberately provoked run (Quick
  Wash for the misses), and unit tests cover selection and no-repeat.

### 3.3 Altruistic chains (Karma)

Decisions (user, 2026-10-05):
- **A restaurant joins** as the eighth customer (named **Amir's Persian
  Kitchen** by the user): tonight's surplus adas polo
  (plant-based) starts a food-rescue gift every night.
- **Tags go on the want, not the item** (the same plate of food is a need for
  someone hungry and a treat for someone fed): *need* (food, warmth,
  shelter, and feeding animals), *purpose* (tools and parts for making
  things), *pleasure* (records, soda). Show the user the full tag table
  before wiring it in.
- **Karma = relief + flourishing, needs first.** Need gains count 1x
  (relief). Purpose and pleasure gains count 1.5x (flourishing) only for
  people whose needs are covered tonight; otherwise 1x.
- **The machine routes gifts by Karma**: a gift chain's value in the QUBO
  uses these weights, so the drum sends the gift where it does the most
  good (the food-rescue mission from the design). Trades stay in Goo.
- **Keep "Goo"**, and explain it.
- **The rules are front and center** (user: "high signal, low noise"): a
  short HOW IT WORKS card in the game and the same rules in DESIGN.md.

**Dynamic needs (user, 2026-10-05): level 1 now, level 2 later.** Wants
and needs aren't fixed: Ray isn't always full. Tags come from rules over
tonight's conditions, not a hand-made table.
- Each want (and each owned item) has a *use*: eat, warm up, feed
  animals, lifeline (a phone), build (tools and parts), enjoy (treats).
  Tags go on the want: Pigeon Lady wants kale to feed her pigeons; Tamara
  wants it to eat.
- Each night rolls conditions from a seed: weather (cold or mild), who's
  hungry (more likely if sleeping out), whose animals need feeding.
- Rules: eat = need if hungry, pleasure if fed; warm up = need on a cold
  night if sleeping out, else pleasure; feed animals = need if they're
  hungry, else pleasure; lifeline = need if you have no phone, else
  purpose; build = purpose; enjoy = pleasure.
- Value follows condition (a hungry person values food ~1.4x their
  base, a fed one ~0.5x), and so does what owners ask for their own food
  or warmth. So the drum sends food to whoever's hungry tonight, with no
  special case.
- "Needs covered" (for the 1.5x flourishing bonus) is per night: not
  hungry, not cold outside, animals fed, has a phone. Same rule in the
  solver and the Karma score.
- Level 2 (later, its own step: the creative-to-survival shift): tonight's
  outcome carries into tomorrow (fed tonight = not hungry tomorrow, items
  that changed hands stay changed), plus Upddayett's hunger meter. That's
  also where "feed them first" pays off across nights, and where Saltie
  theft starts (see 3.6).

Sub-steps (commit and push each):
- **3.3a Nights (library):** uses, night roll, derived values and tags,
  tests; `trade_cli` shows the night. No gifts yet.
- **3.3b Gifts (library):** Amir's Persian Kitchen (eighth customer, fictional; user named it Amir's)
  with tonight's surplus adas polo (lentil rice, plant-based); open gift chains; Karma scoring;
  cap the board at 20 bits (keep the best gifts and trades); bench.
- **3.3c Game:** a 20-slot strip board (unused slots parked), a night
  banner with "Next night", condition tags on portraits (hungry, cold),
  a Karma meter beside Goo, gift chains in warm gold with a heart token,
  the Karma line on HOW IT WORKS, and tightened layout for small windows.
- **3.3d Give-away picker:** Upddayett gives one of his things away and
  it starts a chain too.

**3.3a done.** `world::laundromat(seed)` rolls a night; values and tags
come from the rules (`World::tag`, `needs_covered`, `conditions`). Over 200
nights: 11-14 candidate trades, best sets 28-38 Goo. Program odds measured
across 10 nights (480 runs each): Quick Wash 46%, Permanent Press 68%,
Normal 88% (was 95% on the hand-tuned Tuesday; some nights are harder),
Delicates 99%. Along the way:
- **Exact structural solver.** Valid trade sets are independent sets of
  the conflict graph (thousands, not 2^20 states). `qubo::best_valid_set`
  gives the best set, cached on rebuild (brute force at 20 bits was ~40 ms,
  and the UI called it every frame after a cycle). The minimum want bias
  is now closed-form: (best set without the want) - (best set with it),
  no bisection. Tests cross-check both against brute force on 24 nights.
- **Whole Goo.** Night multipliers made fractional values and near-ties
  (36.6 vs 36.7) the drum can't separate; values are rounded to whole Goo,
  so those become exact ties (which count as optimal). A night went from
  71% to 100%.
- **Phones.** Shopping-Cart Guy and Pigeon Lady carry phones now (many
  unhoused people do, e.g. the federal Lifeline program); otherwise "no
  phone" locked them out of the flourishing bonus forever, since only Vape
  Lady's phone want can be met. Her phone stays a need.
- The board caps at 20 trades (`MAX_BITS`, best kept); no night hits it yet.
- `trade_cli night --night N` prints tonight's conditions and every want's
  value and tag; `trade_cli nights` summarizes 200 nights.

**3.3b done (2026-10-05).** Amir's Persian Kitchen gives tonight's adas
polo away; gift strips are scored by Karma (needs 1x, purpose/pleasure
1.5x once needs are covered), trades by Goo. Results show both
("41 Goo + 8.0 Karma") and who the gift reached ("Upddayett gets the
adas polo (hungry)").
- **Chain length 1 (direct gifts), user-approved on my recommendation.**
  Pay-it-forward chains work but make a glassy problem (near-equal sets
  many strip-flips apart): Normal / Delicates across 10 nights, 1
  recipient 81% / 94%, 2: 54% / 86%, 3: 42% / 65% (trades only: 91% /
  99%). Longer chains are a later unlock; `cycles::MAX_CHAIN` and
  `trade_cli --chain N` keep them one number away.
- Tried and rejected: higher beta (stiffer springs: 38% at 7, 28% at 9)
  and normalizing by the biggest trade (gift strips overpowered conflict
  springs: 28-46%).
- Board: trades first (keeping >= 4 gift slots), gifts fill to 20, picking
  the best gift per different recipient first, so the drum really chooses
  who eats. Over 200 nights the food goes first to someone hungry on 187,
  and never to a fed person while a hungry one wanted it.
- Program odds re-measured with gift strips (480 runs each): Quick Wash
  26%, Permanent Press 46%, Normal 81%, Delicates 95%.
- Costly wants (cost 4-10 Goo: Dave/cells, Ray/derailleur, Tamara/kale,
  Pigeon Lady/charger, Vape Lady/Goo) delivered only 58-85% on the bigger
  boards; fixed by the want clamp below.
- Found a CortenForge bug: `ising::exact_distribution` overflows to NaN on
  big boards at low temperature (FINDINGS).
- Tests: 11, 1.6 s (brute-force cross-checks thinned from 84 s).

**Costly-want clamp done (2026-10-05).** A want now pins its strip: the
bias sets the wanted strip's idle field to `WANT_CLAMP` = 1.5x the
well-flattening field (closed form: the idle field on strip i is
beta * scale * (value_i + bias) / 2), and never less than the old minimum
(cheapest delivering bias + 1x the smallest trade). In fiction it's
Upddayett's thumb on the strip again, as in Step 1.
- **The plan was wrong about pinning.** It said to stay under ~0.9x the
  flattening tilt. Sweeping fixed margins 1/2/3/4/6/8 on the costly wants
  of nights 1-8 showed delivery rising with the pull right past the tilt
  and leveling off at margins 4-6 (fields 1.3-1.9x the tilt), with no harm
  from pinning. Pinning removes the choice; the strips around the pinned
  one settle the rest. The 3.1 sweep that picked 1.0 ran on the small
  Tuesday board, where the margin barely mattered.
- **Result,** nights 1-7, every want, 48 Normal runs each (`trade_cli wants
  --bench`, old behavior = `--clamp 0`):

  | Wants | Delivered: before -> now | i9 best set: before -> now |
  | --- | --- | --- |
  | Costly (70) | mean 91%, min 58% -> mean 100%, min 100% | 68% -> 86% |
  | Free (105) | mean 99%, min 90% -> mean 100%, min 100% | 75% -> 76% |

- Pulling harder never changes which set is best: the bias only rewards
  delivering trades, and the conflict penalty (2.4) still beats any one
  trade (<= 1.5). Test `a_want_clamps_its_strips_without_changing_the_answer`
  checks it on 7 nights, plus that unwanted strips are untouched.
- `trade_cli wants --costly` benches only wants that cost Goo; `--clamp C`
  sets the clamp (0 = the old margin-only bias). The want column now says
  "pinned" instead of "THUMB".
- **Known limitation: hard nights.** "Best set" is now limited by how jagged
  a night is, not by the want. Night 4 finds its best set only 39% of the
  time even with no want (mean 36.3 of 37 Goo: near misses). Its night mean
  rose 47% -> 60% with the clamp, but two free wants dipped (Vape Lady/RTX
  3090 40% -> 15%, Upddayett/cells 50% -> 31%, 48 runs): a pinned strip can
  block a way out of a near miss. Candidates: a slower Normal on hard nights,
  or latch-and-reheat.
- **Found while benching (game, not CortenForge; fixed in 3.3c):** Amir's
  Persian Kitchen could roll "hungry", which means nothing for a restaurant
  (it changed no values, but it showed in `trade_cli night`). Nights also repeat often:
  over 200 seeds one pattern comes up 7 times and 23 twice, because the
  skewed coins (hungry 60% / 15%) carry only ~9 bits, so the commonest
  night has ~1.2% odds. That's expected, not a broken RNG; nights 1 and 8
  are the same apart from Amir. (The fix keeps every later coin: see 3.3c.)
- Tests: 12, 3.9 s.

**3.3c done (2026-10-06).** The game shows the night and the gifts:
- **Night banner** (top center): night number, cold or mild, how many are
  hungry, what Amir's gives away, and **Next night >** (rolls tomorrow and
  rebuilds the board; a want carries over if something can still deliver
  it). `UPD_SEED=<night>` replays a night; `UPD_NEXT=1` presses Next night
  in screenshot mode.
- **Condition tags** under each portrait (hungry, cold, pigeons hungry, no
  phone), from `World::conditions`.
- **Goo and Karma meters** in SPIN CYCLE: the i9's latched set (or the
  strips, before a cycle) against the night's best set; hover for relief
  vs flourishing.
- **Gifts in warm gold:** gift arrows and rows are orange-gold, with a heart
  riding the arrow (a camera-facing gizmo curve). Trade colors skip the
  golds and oranges, so gifts and want legs never look like trades.
- **HOW IT WORKS** gains "Tonight" and "Karma" lines.
- **20-slot board:** the washer always has 20 strips; slots past tonight's
  trades sit parked flat with dark LEDs, so Next night never respawns meshes.
- **Small windows** (under 1280x760; check with `UPD_WINDOW=960x600`):
  narrower panels, trades as portrait initials (U -> VL -> U; hover for the
  full text), HOW IT WORKS folded, and both lists scroll instead of running
  under the board cam or the rules. The i9's call now sits above the trade
  list so it never scrolls away. (egui gotcha: a fixed-size window offers
  its content only last frame's height, so the scroll areas set
  `min_scrolled_height` to the room they have.)
- **Amir's never goes hungry** (`Npc::business`): the restaurant still
  draws its hunger coin and ignores it, so every other coin, and every
  benched night, stays the same (a test checks 200 nights).
- Gift lines read "Upddayett (hungry) gets the tray of ...".
- Tests: 13, 1.5 s.

**3.3d done (2026-10-06).** Upddayett can give one of his things away:
- **Picker:** UPDDAYETT GIVES AWAY... under I WANT in SPIN CYCLE: nothing,
  his 12-pack of Mtn Goo, his cracked Android phone or his kale (whatever
  someone wants tonight; hover shows what it costs him and who wants it).
  The banner lists both donors; HOW IT WORKS gains "A give-away". The pick
  carries over to the next night, and so does a want if something still
  delivers it.
- **Library:** `World::set_gift(item, on)` marks the item a gift and
  re-derives the night (owner value 0, out of the trades, a gift strip
  from him); `World::giveable(npc)` lists what has takers. Nothing else
  changes value (tested on 24 nights, and taking it back restores the night).
- **Two donors share the gift slots:** `pick_gifts` now gives every gift
  item its best strip first, then the best per other recipient, then the
  rest, so Amir's food can't crowd out Upddayett's gift (or the reverse).
  One donor picks the same strips as before.
- **What it does:** the gift costs Upddayett his own value for it (4-6
  Goo) but the block barely loses Goo: the trades that used the item are
  replaced by others, and the recipient's gain counts. Night 1, phone:
  +44 Goo + 15 Karma (vs +43 + 8 keeping it), since Vape Lady has no phone
  and his phone meets a need (relief 7, 1x). The Goo goes to a treat
  (1.5x once needs are covered); the kale to whoever is hungry, or to
  Pigeon Lady's pigeons.
- **Reliability** (Normal, 48 runs x nights 1-10, `trade_cli bench --give`):
  keeping it all 81%, giving the Goo 78%, the phone 81%, the kale 65%
  (min 31% on night 5). Night 4 stays hard (38-69%).
- **Known limit: the kale.** Its gift strips are the weakest on the board
  (night 5: 4 Karma to Pigeon Lady vs 3 to Tamara, against trades up to
  10), so the drum often settles on the wrong one of a 1-point choice
  (mean 42.7 of 44). Delicates gets night 5 to 77% (mean 43.8). Same
  family as the hard nights: small near-ties between weak strips. Fix
  candidates there apply here too (latch-and-reheat, a slower program on
  hard boards).
- **Fixed:** a gift's reason came from the recipient's first condition, so
  the phone read "Vape Lady (hungry)"; `World::why` now names the condition
  the want answers ("no phone"). Gift lines show each gift's own Karma
  (they all showed the night's total once there were two).
- `UPD_GIVE=phone` gives an item away in screenshot mode; `trade_cli
  --give upd:phone` does it in the CLI.
- Tests: 14, 1.6 s.

Order: (1) the Goo legend, per-person gains and HOW IT WORKS card: done
(`cfd6015`). (2)-(3), tags and gifts, became sub-steps 3.3a-d above.

Encoding (unchanged): a donor gives an item away; the chain passes it
along an open path (each middle person gives something they value less
than what they get); the last person keeps. Every chain is one more bit;
chains from the same donation share the item, so at most one wins. Keep
total bits <= 20 so the exact solver still checks it. Who donates: the
Amir's Persian Kitchen every night, plus a "Give away" picker for Upddayett's things.

- Visual: open chains drawn from the donor in warm gold, with a heart
  token; a Karma meter beside the Goo numbers.
- Done when: unit tests for chain enumeration (gains > 0 in the middle,
  last person gains, shared-donation conflicts) and for Karma scoring
  (needs-first rule); bench hit rate stays near the forward-mode numbers;
  a donation visibly reaches 2-4 people; the HOW IT WORKS card states the
  rules in a few lines.

### 3.4 The laundry counter is the escrow

- A folding counter in the scene. When a cycle starts, every customer's
  items fly to the counter (small crates in the owner's color); when the
  drum stops they fly to their new owners per the i9's call, and untraded
  items go home.
- One line of AI or tooltip explaining why: a 4-way swap only works if
  everyone delivers or nobody does, which is also why loops stop at 4
  (kidney exchanges cap loops for the same reason).
- Done when: screenshots show items on the counter mid-cycle and delivered
  after; item counts conserve (nothing duplicated or lost; assert it).

**3.4 done (2026-10-06).** A folding table with a cardboard ESCROW sign
stands left of the washer. Every item is a crate in its owner's coat color,
stacked at its holder's feet. When a cycle starts the crates fly onto the
counter; when the drum stops they fly to whoever the i9's call gives them
to, and the rest go home. On a battery night the cells' crate sits by the
washer. Night 1: 17 items held, 15 changed hands in 6 trades and 1 gift, 2
went home.
- **Library:** `trade::escrow::settle(world, cycles, bits)` returns who holds
  each item, which chosen trades went through and which were called off. The
  most valuable chosen trade claims its items first; a later one that needs
  a claimed item is called off whole, and everyone in it keeps their things.
  The i9's latch is clash-free in practice, so the game hasn't hit this
  yet; the strips at rest can clash.
- **Conservation, asserted:** `Settlement::check` runs on every settle. It
  checks that:
  - every item ends with exactly one person;
  - no item moves twice;
  - a moved item went where its trade said, from its real owner;
  - every per-person count adds up.

  The game also asserts one crate per item every frame. Tests:
  - every night × (open, battery, give-away) × 200 random strip patterns,
    clashes included;
  - the best set goes through whole, and trades swap one for one;
  - a clash is called off whole;
  - the check catches a counter that cheats (a missing leg, a double move).
- **The why:**
  - HOW IT WORKS has one short line: "The counter holds every item during
    the spin, then hands each trade over whole, or not at all."
  - Hovering it, or the counter's result line under THE i9 CALLS IT, gives
    the reason: a 4-way swap only works if everyone delivers, loops stop at
    4, and kidney exchanges cap their loops for the same reason.
- Seen, for 3.5: on a night with a long i9 call (night 1: 6 trades), the
  TRADES window grows down over HOW IT WORKS. Its scroll area keeps a
  minimum of 80 px.
- Tests: 25 in the library (2 new), 2.9 s.

### 3.5 Visual polish

- **Layout:** the user's screenshot (narrower window) shows the trade board
  covering the washer. Make it compact and collapsible; scale panels to the
  window.
- **Title card:** Xbox-style "Press Start": UPDDAYETT'S SCHOOL OF BIDDNESS,
  Lesson 3, chunky type, then a "powered by CortenForge" splash (real name,
  per the naming policy).
- **Fake commercial card** between cycles now and then (Mtn Goo, or
  Superintelligence for Dogs).
- **The laundromat TVs** (user, 2026-10-06; see DESIGN "The TVs"): one or
  two wall TVs (a low-poly mesh, the screen a quad with an egui/text
  texture) with a scrolling chyron, flipping between two parody channels.
  Text only; spoken lines wait for the 3.2 voice step.
  - **The Shrug Network** (comedically apathetic news): headlines built
    from tonight's real roll ("4 hungry on Turk Street tonight. Anyway, a
    billionaire bought a second moon."). Every number comes from
    `World::night` / `conditions`, like the AI-line rule.
  - **PromiseTV** (false promises): political ads for fictional candidates
    ("Vote Glorbman: every family gets a hub motor!"). The hook: the
    promised item shows in the I WANT picker as undeliverable ("nobody's
    trading that tonight") or with its real price, because there's one hub
    motor. Upddayett heckles: "Who's giving it up, Glorb?"
  - Guardrails: parody names only (no real networks, politicians or
    parties); skewer both channels equally, and behavior (apathy, empty
    promises), not policy; no crypto ads. Flavor, kept cheap: CortenForge
    stays the focus.
  - Done when: screenshots show both channels with headlines that match
    the night, and a PromiseTV promise checked against the picker; unit
    tests cover headline generation (numbers match the night, no immediate
    repeats).
- **Scope trace:** click a trade to watch its strip's `qpos` over time on a
  little green scope ("the scope", Tentzhen's in-game nickname): you see
  the Kramers hops.
- **Ray tracing (`bevy_solari`), last and riskiest:** feature `bevy_solari`,
  `RaytracingMesh3d` on meshes, camera with `Msaa::Off` and storage-texture
  usage (see Bevy's `examples/3d/solari.rs`). A key toggles back to normal
  PBR. If it fights gizmos/egui or tanks the frame rate, document why and
  keep PBR.
- Done when: screenshots of each; the frame rate is logged with and without
  Solari.

**3.5 as built (2026-10-06):**
- **Layout done.**
  - HOW IT WORKS folds into sections. "The basics" (Goo, trades, the drum,
    the counter) starts open; the rest start folded. The Salties section
    opens itself on a night the dumb ones brag.
  - THE i9 CALLS IT scrolls on its own and takes at most the room minus a
    few board rows, so a long call no longer runs over HOW IT WORKS.
  - Panel widths scale with the window (SPIN CYCLE 19%, TRADES 24%, HOW IT
    WORKS 26%, each clamped).
  - Tab hides every panel but the name cards.
  - Checked at 1600x900 and 960x600 on night 1 (6 trades): the washer stays
    in sight at both sizes.
- **Title card and commercials done** (`src/game/cards.rs`). They're egui
  painter cards with extruded type: Arial Black from `C:/Windows/Fonts`,
  falling back to a smeared egui face.
  - **The title:** UPDDAYETT'S / SCHOOL OF BIDDNESS / a pink LESSON 3 pill /
    blinking PRESS START, over the dimmed laundromat, plus "RATED M: crude
    humor, mild thermodynamics". Any key or click goes on.
  - **The "powered by CortenForge" splash** (real name, per the naming
    policy): in Corten-steel orange, with a line on what the sim does and
    Ferris waving below. It lasts 3 s and can be skipped.
  - **The ads, cut like the fake commercials that open Tropic Thunder**
    (user, 2026-10-06; `src/game/ads.rs`). After the splash, a reel plays
    all three before the game: any key moves to the next ad, Esc skips the
    rest. Every third night one ad also breaks in.
    - **The style kit:** each ad is 11-14 s of beats. Every cut lands with a
      white flash; type slams in from huge and shakes on impact; light rays
      wheel behind; there are starburst stickers, crooked rubber stamps,
      lower-third V.O. captions, a bouncing jingle, and fine print crawled
      too fast. The format is TT's; the jokes are ours (DESIGN: never lift
      jokes).
    - **MTN GOO:** DJ Hyperfocus in silhouette (shades, chain, pumping a
      can): "Yo. Can't focus? Can't sleep? Can't stop?" Then the can spins in
      under MTN GOO with a NEW! sticker; the choir sings "GET THE GOO IN YOU";
      NOW 40% GREENER "(than what?)"; end slate "Hyperfocus in a can." with
      the side-effects crawl.
    - **SUPER INTELLIGENCE FOR DOGS** (the user's words): a sad gray problem
      shot, the dog by its mess, "TIRED OF THIS?"; INTRODUCING; the title with
      the dog in glasses and a mortarboard, GENIUS!; the dog on the toilet,
      "So they will stop shitting on the floor."; *FLUSH* with "THEY USE THE
      TOILET NOW. THEY EVEN FLUSH."; end slate with "Not available for cats
      (they declined)."
    - **CARTPASS:** golden-hour cart, "You love your shopping cart."; OWNING
      THINGS? stamped SO 2003; CARTPASS with a $9.99/MO sticker; the wheels
      drop off under a WHEELS SOLD SEPARATELY stamp; end slate with the
      cancel-by-mail crawl.
    - **Two attack ads** (user: "a political ad for each side where they're
      shitting on the other for a reasonable take"). The PromiseTV candidates
      attack each other for something perfectly reasonable:
      - Glorbman on Plinko: "says she'll fix the potholes on Turk St." Then
        FIX. THEM.; "FACT: These potholes have served Turk St for 40 years
        (Turk St Gazette, probably)"; "TOO SMOOTH. TOO FAST. TOO FAR."
      - Plinko on Glorbman: "read the bill before he voted on it." Then ALL
        400 PAGES.; "What was he looking for? He won't say.* (*He said
        'typos.')"; "TOO CAREFUL. TOO PREPARED. TOO LITERATE."

      Both are cut from one grim template: grainy red-tinted photo, FACT:
      with a source nobody checked, "Paid for by ...", "I'm ..., and I
      approved this message." A test holds them to the same beats and
      length, so neither side gets the nicer ad. They play back to back.
      There are no parties or real politicians, and both are mocked for
      attack-ad behavior, not policy.
    - **OKAYZA (mehprozine)**, the pharma spot: "Do you suffer from
      moderate-to-severe Being Fine?" in the rain. Over a sunny kite montage,
      the side effects: feeling great; feeling nothing; growing a second,
      smaller, more successful you (he shows up in a tiny suit);
      uncontrollable pugcasting; sudden fluency in dolphin; "death, but in a
      chill way"; "tell your doctor if your doctor is a raccoon". End slate:
      "Because 'fine' is a diagnosis."
    - **Pacing:** the user found the first cut far too fast. Every shot now
      holds 2.5x its written length (`ads::PACE`; 1.8x was still too fast per slide), so each shot holds ~5-7 s and each ad runs ~30-35 s.
      The slams and flashes stay quick, and the fine print crawls slower.
      The reel is 6 ads (Esc skips it), and the night breaks cycle through
      all six.
  - The board-cam inset is its own camera and drew over the cards, so it's
    switched off while a card is up. Titles are sized to fit the window.
  - `UPD_CARDS=1` shoots the title, the splash and every ad beat
    (`shots/card_ad<k>_<beat>.png`); `UPD_SHOT` skips the cards.
  - Unit test (binary): `cargo test --release --bin cortenforge_play`.
- **The TV done.** One wall set hangs on arms between the neon sign and the
  back row of washers, tipped toward the room. Its screen is Bevy UI
  rendered to a 640x360 texture (`src/game/tv.rs`): a header with the
  channel's logo, a big headline with a line under it, and a crawling
  chyron. It flips channels every 14 s, with a burst of snow on each flip.
  The copy is pure and tested in `trade::tv`.
  - **The Shrug Network:** facts from tonight's roll (hungry count, cold
    and how many sleep out, hungry flocks, give-aways, trades on the board,
    the dumb Salties' brag), each followed by "Anyway, ..." and a shrug
    picked by the night's seed. Night 1: "4 HUNGRY ON TURK ST TONIGHT.
    Anyway, a celebrity's pug launched a pugcast." The banner says 4 hungry.
  - **PromiseTV:** four fictional candidates (Glorbman, Councilwoman Plinko,
    Chet Sprockett, Mayor Dumpleton) each promise everyone one item Turk St
    has one of: the hub motor, the Wi-Fi password, the soldering iron, the
    sleeping bag. There are no parties. While an ad is on:
    - Upddayett heckles from his name card: "Who's giving it up, Glorb?
      There's one, and it's Shopping-Cart Guy's."
    - I WANT checks the promise against the picker's real price ("Turk St
      has 1. Checked: for Upddayett it costs the block 2 Goo"), and a
      "Want it" button puts it in the picker.
    - Night 1 with the hub wanted: the drum got Upddayett the hub motor for
      2 Goo.
  - Tests: 3 new in the library (28 in all):
    - the news is true (hungry and cold counts, gifts, trades, nights 1-60);
    - no shrug twice in a row, looping included;
    - every promise is one item that someone other than Upddayett owns.
  - `UPD_TV=shrug|promise` holds a channel for screenshots.
- **The scope done.** Click a trade in TRADES and THE SCOPE opens bottom
  center: that strip's `qpos`, live from CortenForge, on green phosphor.
  - The sim samples every strip every 0.5 sim units inside its step loop
    (not once a frame, so no hop slips between frames) and keeps the last
    300 units.
  - Dashed levels mark ON (+1), the barrier (0) and off (-1). The drum's kT
    runs faint amber along the bottom, and a hop counter tallies well-to-well
    crossings past ±0.5.
  - Night 1, strip 1 (U -> SC -> BK -> U):
    - hot (2.76 kT): 16 hops in view, all over both wells;
    - cooling (0.90 kT): 2 hops, one dip to off and back;
    - locked: flat on ON, 0 hops, 0 kT.

    That's the whole lesson on one trace.
  - A board rebuilt with a different strip count clears the trace and
    closes the scope. `UPD_SCOPE=<strip>` opens it for screenshots.
- **Ray tracing done: Solari works and is on by default** (`src/game/rt.rs`,
  cargo feature `solari`, default on). F2 toggles back to PBR, and
  `UPD_SOLARI=0` starts on PBR. A GPU without hardware ray tracing falls
  back to PBR on its own, with a warning.
  - **Frame rate** (`UPD_BENCH=1`: 1600x900, vsync off, panels up, RTX
    4070 Ti):

    | Mode | fps | ms a frame |
    | --- | --- | --- |
    | PBR | 190-215 | 4.7-5.3 |
    | Solari | 248-275 | 3.6-4.0 |
    | PBR, toggled back | 194 | 5.2 |

    Solari is the faster one here. It drops the point light's cube shadow
    maps and the three rect lights, and traces from the emissive tubes
    instead.
  - **What it took** (Bevy friction, not CortenForge):
    - Every mesh gets a `RaytracingMesh3d` twin, fixed up the way Solari
      needs (tangents, u32 indices, no second UV set). Glass and the unlit
      TV picture stay raster-only.
    - The main camera needs `Msaa::Off` plus storage-binding texture usage.
      `Camera3d` already brings both components with default values, so a
      `Without<...>` insert never fires. The first run died with a wgpu
      validation error ("TextureView ... do not contain required usage flags
      STORAGE_BINDING").
    - `SolariLightingPlugin` makes deferred the default for every opaque
      material, PBR included. Without a `DeferredPrepass` on the main camera
      and the board cam, the PBR picture went black.
    - Solari lights only from emissive meshes and directional lights. In
      Solari mode the tubes glow 20,000x brighter (`UPD_TUBES=<x>` to tune),
      and the breaker flicker dims them like the lights.
    - The TV's picture as an emissive texture came out as a faint purple
      wash under Solari. It's now unlit and drawn forward, and reads the
      same in both modes.
    - Without DLSS Ray Reconstruction (it needs NVIDIA's SDK) the picture
      is speckled. TAA under Solari averages it out over frames. Its
      `TemporalJitter` and `MipBias` have to come off with it, or the PBR
      picture shakes.
  - **The look:** soft ray-traced shadows and bounce light off the tubes,
    darker and moodier: the "RTX remaster" from DESIGN. Static surfaces
    converge clean. Anything that moves (the NPCs' bob, the shaking washer,
    flying crates) keeps a film grain, because TAA can't average a moving
    surface. If that grates, F2 (or flipping the `Rt::on` default) gives
    the clean PBR picture.
  - Screenshots: `shots/rt_pbr.png`, `rt_solari.png`, `rt_pbr_again.png`.

**3.5 done (2026-10-06).**

### 3.6 The Salties (sabotage nights)

Design: DESIGN.md, "The Salties". Order: after 3.3c-d (it needs the night
banner and condition tags). Theft waits for Level 2 (nights that carry over).

- **3.6a Magnet (library).** A sabotage condition in the night roll: a
  magnet at a board position adds a dipole field (~1/r^3 from the magnet)
  on top of the trade biases, through the same `ExternalField`. Bench hit
  rate and Goo lost against magnet strength and position, with and
  without a shield (a fixed attenuation factor). The idle Hall offset
  (strip rest positions with the drum stopped) is the tell. A calibration
  load (a fixed night with a known best set) flags tampering when the drum
  misses it. `trade_cli night` shows the magnet; `trade_cli bench` takes
  `--magnet`.
- **3.6b Power cut (library).** The breaker trips at a rolled time: the
  temperature drops to zero and the settle phase starts early (a quench).
  Bench hit rate against cut time. A vape-cell battery finishes the cycle,
  but it uses cells Upddayett could trade for.
- **3.6c Game.** The sabotage tag on the night banner; tells in the scene
  (idle strips leaning toward the magnet, the lights flickering); a defense
  choice before the spin (shield, battery, calibration load); AI roasts
  quoting the measured numbers; the "EMP" as a dud gag (popcorn, no build
  details).
- Done when: unit tests show the dipole field falls off with distance and
  sums with the want bias; a strong magnet pins strips past
  `machine::max_safe_field`; a quench lowers the hit rate and the battery
  restores it; bench numbers logged here; screenshots of a magnet night
  and a power-cut night.
- Watch for: a magnet field that moves the drum's best set changes what
  "optimal" means. Score sabotaged runs against the clean night's best set,
  so Goo lost to the magnet shows up as a loss.

**3.6 done (2026-10-06).** The Salties show up on 3 nights in 10: a magnet
(half), the breaker (about a third) or the "EMP" (the rest). Their draws come
after every older coin, so each night keeps its weather and hunger, and
every earlier bench number stands (a test replays the old roll on 500
nights). The magnet and power-cut numbers below were measured before the
smart Salties existed, on boards with the same values; nights 1 and 16 have
since become smart coil nights (see "Smart Salties" below).
- **Library** (`src/trade/salties.rs`): `Magnet { pos, strength }` (strength
  in units of the flattening field, + pushes strips on). Its field is
  `strength * (DEPTH / r)^3` with `DEPTH` 1.5 strips: a dipole pointing
  along the swing, so the sign is the same everywhere. `Magnet::shielded`
  passes 10%. `Magnet::through(shield)` applies the shield only if the
  plate covers the magnet (within 2 strips). `Machine::with_stray_field`
  adds it as a second `ExternalField` in the stack, so the i9 still
  scores by the clean QUBO. `Machine::rest` + `Machine::stray_field` is the
  idle check: force balance on strips at rest, which reads the hidden
  field to 1e-3. `Anneal::cut` drops the temperature to 0 early (the
  quench). `World::set_held` takes the battery's cells off the market.
  The calibration load is night 1 (98% on Normal).
- **Game:** the banner shows SALTIES AROUND with their brag ("Magnet stuff
  tonight. Science, baby." / "We're calling down a solar flare." / "EMP
  tonight."). DEFENSES in SPIN CYCLE: Idle check ("strip 9 feels 0.47x the
  flattening field nobody installed"), a steel shield over a chosen strip,
  and the battery (Vape Lady's 18650s, priced every night: 7 Goo on
  night 1, 9 on night 17). On a breaker night the tubes flicker before the
  spin, the room goes dark when it trips, and the progress bar says POWER
  CUT. After the spin the magnet appears taped to the washer and the AI
  roasts with the numbers ("Their magnet pushed strip 1 at 1.10x the
  flattening field. Your shield took it to 0.11x. Steel: 1, Salt: 0.").
  A battery spent on a quiet night gets called out too. HOW IT WORKS gains
  "The Salties". `UPD_IDLE`, `UPD_SHIELD=<strip>|found`, `UPD_BATTERY`.
- **Magnet bench** (`trade_cli magnets`, 48 runs per cell, Normal, nights
  1, 2, 3, 5, 7):
  - Pushing **on** is the real attack. At mid-board, 0.6x and up drops the
    best set from 67-98% to 0-17% and costs 1-8 Goo a run (the clashing
    trades get pushed together).
  - Pushing **off** only hurts over a strip in the best set (night 1,
    strip 14 at 1.0x: 0%, 5.2 Goo lost). Over an unused strip it does
    nothing. On a hard night it can even help (night 2: 67% to 88-92%).
  - Past the end of the board the field has fallen to 0.04-0.09x and does
    little.
  - **The shield** brings easy nights back to baseline (night 1: 96-100%;
    night 3: 90-94%). On hard nights a strong "on" magnet still hurts
    through it (night 2: 35-46% vs 67%; night 7: 48-58% vs 77%), since
    10% of 1.5x is 0.15x.
  - **The idle check** always catches the magnet, even shielded (it reads
    0.03x for a shielded 0.3x).
  - **The calibration load** gives a 2% false alarm. It flags 83-100% of
    "on" magnets of 0.6x and up, but it only tests its own board: a 1.0x
    "off" magnet under strip 9 costs night 5 the best set (25%) and never
    trips it. The idle check is the reliable tell; the calibration load
    stays CLI-only.
- **Power-cut bench** (`trade_cli cuts`, nights 1-10):
  - A cut 15% in (kT 2.78) drops the best set to 17-60% and costs 1-2 Goo.
  - A cut 30% in costs a little.
  - From 50% in, the result matches the battery, because the i9 latch
    has already caught the best set (median t of about 140-300).
  - The quench shows at rest: night 17 at 27%, at rest best set 8% and
    35% clashes, while the latch still finds it 71% of the time.
  - So the battery is a gamble: it saves early cuts, but its cells cost
    Goo every night it runs (9 on night 17, more than the cut cost).
- Tests: 21 (falloff and shield, the idle check reading magnet + want, a
  strong magnet pinning the strips it overpowers, the cut schedule, the
  held cells, old nights unchanged), 1.6 s. Screenshots: night 1 open and
  shielded, night 16 idle check, night 17 cut and battery.

**Smart Salties (2026-10-06, user: "make some salties smart and we have to
battle smart salties. dumb salties brag. we can have both").** 4 in 10
Salties nights are smart (one more draw, after the others). Salties
nights among the first 40: smart coils 1, 10, 16, 20, 27; a smart quiet cut
4 (20% in); dumb magnets 21, 29; dumb cuts 17, 31.
- **Smart Salties never brag:** no banner, no flicker. A quiet night isn't
  a safe night.
- **The aimed coil** (`Sabotage::Coil`). They scout the board (`salties::aim`):
  every strip, both ways, at 0.8x (just under flattening, so it never pins
  a strip). Each spot is scored by how much worse the best set gets once
  the tilt is counted in Goo (a field `h` is worth `2h / (beta * scale)`).
  Their favorite: pushing the Upddayett ⇄ Vape Lady swap *on*, into the
  trades it clashes with. It's an electromagnet keyed to the drum's
  shaking: `salties::Coil`, our own `PassiveComponent` that only pushes
  while `ctrl[0] > 0`. So the idle check reads 0.000.
- **The quiet cut** (`Sabotage::QuietCut`): the breaker, but 12-30% in,
  where the cuts bench says it hurts, and with no flicker.
- **The counter: the spin check** (`salties::SpinCheck`). The i9 averages the
  idle check's force balance over every read while the drum shakes. The
  shaking, ringing and hops average away; the coil doesn't. It runs on
  every cycle; DEFENSES shows "Spin check: strip 12 felt 0.78x while the
  drum spun". The alarm is at 0.3x: a clean Normal spin averages out to at
  most ~0.12x on some strip. The shield defaults to the strip it names.
  The battle: lose a spin, read the numbers, defend, spin again.
- **Bench** (`trade_cli smart`, 48 runs, nights 1-10):
  - On Normal, the aimed coil drops the best set to 0-31% on 8 of 10 nights
    (69% and 81% on the other two). It costs 0.5-4.7 Goo a run (Quick
    Wash: 5.8-8.1), against "on paper" 8-17: the i9 latch claws most of it
    back.
  - Smart Salties are deadliest when you wash fast.
  - Spin-check alarm: 100% of coil runs, on the right strip 98-100%. False
    alarms on clean runs: 0% on Normal and Permanent Press, 8-17% on Quick
    Wash (fewer reads).
  - The shield on the named strip brings Normal back to about the clean
    rate (98/98, 81/81, 85/85, 92/81, 77/65).
- **Roasts:** "No brag tonight: the smart kind. Strip 12 felt 0.78x the
  flattening field while the drum spun and nothing at idle: a coil keyed
  to the shaking, aimed at Vape Lady -> Bike Kitchen Dave -> Sound Guy Ray
  -> Vape Lady. ... Shield strip 12 and spin again." After the shield:
  "Your shield took it from 0.80x to 0.08x. Steel: 1, Salt: 0." Quiet cut:
  "No brag, no flicker: they knew where the panel was, and when," plus
  what the battery would cost. On night 4 that's 7 Goo of trades against
  the cut's 2, so the honest answer is often "take the hit".
- `UPD_RESPIN=1` plays the battle in screenshot mode: it shields what the
  spin check caught (or brings the battery after a cut) and spins again.
- Tests: 23 (adds aim beats a random spot, and the coil hides from the
  idle check but not the spin check, with the clean noise floor under half
  the alarm).

## Gotchas (from the probes)

- Windows Smart App Control blocks Rust builds (os error 4551); it is now off.
- `ExternalField` stronger than about `1.54 * delta_v / x0` deletes the well;
  keep biases below it unless you mean to pin the strip (a want does, on
  purpose: see the costly-want clamp).
- `ExternalField::new` doesn't check length; indexing is unchecked.
- Couplings and fields can't change after `install`; changing a clamp means
  rebuilding the stack (cheap at this size).
- `exact_distribution` and `GibbsSampler` cap at n <= 20.
- Bit convention in `ising`: bit i set = spin +1.
- The sim's Langevin results differ from ideal Ising by 1 to 18%; don't promise
  exact answers in-game.
- `sim-thermostat` doc examples are all `ignore`; the `test-fixtures` models
  aren't reachable via the facade. Use `therm_env::generate_mjcf`.
