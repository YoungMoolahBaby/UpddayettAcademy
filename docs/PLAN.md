# Build plan: the few-hour slice

Slice = Lesson 3, **Money Laundering (Legally)**: a laundromat trade computer
whose bits are simulated by `cortenforge::sim::thermostat`. See `DESIGN.md`.

Status (2026-10-05): **Steps 1 and 2 done** (trade computer in `src/trade/`,
CLI in `examples/trade_cli.rs`, Bevy game in `src/main.rs` + `src/game/`).
Next action: **Step 3.3** (see the Step 3 plan below; 3.0 and 3.1 done, 3.2
deferred to a later voice-and-music step; 3.3a and 3.3b done). Next: adaptive
want margin (costly wants), then **3.3c Game** (Karma meter, night banner).
After 3.3c-d: **3.6 The Salties** (sabotage nights; planned below).

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
- **Known limitation, fix next:** costly wants (cost 4-10 Goo: Dave/cells,
  Ray/derailleur, Tamara/kale, Pigeon Lady/charger, Vape Lady/Goo) deliver
  58-85% on the bigger boards, down from ~99%; wants overall deliver
  92-97%. Plan: let a want pull as hard as the strips can take without
  pinning (largest margin that keeps the idle field under ~0.9x the
  flattening tilt), instead of a fixed 1.0 margin.
- Found a CortenForge bug: `ising::exact_distribution` overflows to NaN on
  big boards at low temperature (FINDINGS).
- Tests: 11, 1.6 s (brute-force cross-checks thinned from 84 s).

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

### 3.5 Visual polish

- **Layout:** the user's screenshot (narrower window) shows the trade board
  covering the washer. Make it compact and collapsible; scale panels to the
  window.
- **Title card:** Xbox-style "Press Start": UPDDAYETT'S SCHOOL OF BIDDNESS,
  Lesson 3, chunky type, then a "powered by CortenForge" splash (real name,
  per the naming policy).
- **Fake commercial card** between cycles now and then (Mtn Goo, or
  Superintelligence for Dogs).
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
