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

**Step 4 done** (2026-10-06): the CEM-learned smart wash program, the
game's fifth program, "Smart (learned)". It finds the best set 93% of the
time on unseen nights, against Normal's 83%, in the same time (see "Step 4
so far" below). Its follow-up, latch memory (reheat when the strips
freeze), beats it: +2.7 points over run 3 on 60 fresh nights, 95% CI
[+1.5, +4.1] (sim-opt bootstrap; see "Step 4 follow-up"). It handles wants and give-aways too (94% vs 83% on
mixed unseen boards); training on them made it worse, so run 5 stays.
**Step 5 done** (2026-10-06): backlog item 1, the drum tumbler. Tonight's
items tumble behind the porthole in a cf-design drum, stepped by sim-core
on a worker thread (see "Step 5" below).
**Step 7 done** (2026-10-06): backlog item 4, Upddayett prints things. He designs
parts in cf-design; the i9 rejects his first draft with the printability
checker's numbers, he fixes it, and the print joins the board and the drum
(see "Step 7" below; FINDINGS cf-design and mesh). The backlog is empty.
**Step 8 done** (2026-10-06): bring your own tape. A boombox on the floor by
the printer plays the player's own music, dropped on the window or put in
`music/`. It's optional: silent until given a tape, no autoplay, and it
switches off to a prop (see "Step 8" below).
**Step 6 done** (2026-10-06): backlog item 3, a row of washers (parallel
tempering). Four washers share one Normal cycle's compute and trade loads.
On 60 fresh nights they beat the learned run 5 by +3.7 points (96.8% vs
93.1%, 95% CI [+2.4, +5.1]), and +8.2 on the hardest quarter. The swaps
themselves are worth +3.3 [+2.3, +4.3]. In the game it's the sixth
program, "Row of 4 washers" (see "Step 6" below). Next: pick from the
backlog.

## Step 5: the drum tumbler (plan, 2026-10-06)

The washer drum becomes a real tumbler. Its inside is a `cf_design`
Solid, and the same Solid is the sim-core collider and the Bevy mesh.
Tonight's trade items tumble inside as rigid bodies, and the spin follows
the wash program.

### 5.0 Feasibility probe (done, `examples/probe_drum.rs`)

`cargo run --release --example probe_drum -- [drop|tumble|spin|res|info|build]`;
the env vars `CELL`, `DT`, `MAXCON` and `N` override the setup. The drum
is r 250 mm, 300 mm deep, with three 50 mm paddles. The items are a phone,
two Goo cans, kale, an 18650 and a hub.
- **Physics works**: no escapes. The paddles lift items over the axle at
  0.6x critical speed, and items pin to the wall at 2x. Penetration stays
  under 5 mm.
- **Settings for the game**:
  - SDF cell 10 mm;
  - `timestep` 2 ms (cf-design's default is 0.5 ms);
  - `sdf_maxcontact` 8.
- **Speed** at those settings:

  | Run | 6 items | 4 items |
  |---|---|---|
  | Tumble (0.6x critical) | 1.3x real time | 2.3x |
  | Spin (2x critical) | 0.67x | 0.93x |

  The cost is collision, ~250 us per item per step, single-threaded.
- **Build**: `to_model` takes 9 s (the 1 mm mass grid; FINDINGS cf-design).
  The game builds it once, on a background thread, at startup.
- The drum is driven by writing its joint `qvel` each step (cf-design
  actuators drive only tendons).

### 5.1 In the game (done, 2026-10-06)

Tonight's items tumble behind the porthole. Pieces:
- **`src/trade/drum.rs`** (library, tested): `Tumbler`.
  - The model has a drum (with a lip around the opening) and a fixed
    glass door.
  - It holds one body per world item (17), built once in ~12 s. Items not
    in the drum park far outside it.
  - Items drop in one at a time, once the drop spot is clear.
  - Items pin to the drum above 1.5x critical speed: they ride with it
    and the physics is skipped. They fall again below 1.2x.
  - `item_look` gives each item a Solid, a density and a color.
- **`src/game/drum.rs`**: a worker thread builds and steps the model in
  real time. If it falls behind, the drum slows down instead of
  queuing work.
  - Meshes come from the same Solids, scaled 1.7x into the scene.
  - The washer cabinet is now cf-design CSG with a cavity for the drum,
    behind clearer glass and a drum lamp.
  - `UPD_SHOT` waits until the model is built.
- **Load**: the 5 items in the most trades on tonight's board. Not the
  best set: that would show the answer before the spin.
- **Speed**: (0.2 + 0.15 kT) x critical, capped at 0.8. That is 0.25x
  cold and 0.8x hot. The drum stops when the power is off. When the cycle
  ends there is a 4 s final spin at 2x, with the items pinned, then it
  coasts to a stop. The motor ramps at 8 rad/s^2.

Tuning the probe settings for real items:

| Change | Why |
|---|---|
| Mask the drum-door pair (`geom_contype` bits) | cf-design doesn't filter it; 2x faster |
| Kale as a rounded box, not an ellipsoid | the ellipsoid's loose field costs 3x |
| Shrink the oversized items (sleeping bag, birdseed, GPU, vinyl) | cost grows with how closely they fit the wall |
| 4 ms step with solref 0.01 | 2-3x faster |

Worst load (5 big items):

| Phase | Speed |
|---|---|
| Tumble | 1.6x real time |
| Cold pile | 1.4x |
| Pinned spin | 2.5x |

`probe_drum -- pool` times it: `ITEMS=0,1,2`, `CELL`, `DT`, `SOLREF`,
`NOII=1` (no item-item contacts).

Ideas for later:
- the door's glass bowl as a mesh;
- a closer porthole camera (like the board cam);
- items changing hands when the counter settles;
- sound.

## Step 6: a row of washers (parallel tempering; plan, 2026-10-06)

Backlog item 3. Several washers hold copies of tonight's board, each with
its drum fixed at one setting on a geometric ladder from cold to hot. Every
few time units, neighbors offer to trade loads. That is replica exchange,
the textbook fix for rugged boards like night 4: a load stuck in a bad set
on a cold washer gets carried up the row, shaken loose, and passed back
down. The i9 latches the best set any washer reads.

- **Why not sim-opt `Pt`?** `Pt` runs parallel tempering over a policy's
  *params* (it is a trainer, like CEM). It cannot swap the strips' states
  between boards. So the row is our own replica exchange on `Machine`s,
  with sim-opt's bootstrap judging it (see FINDINGS sim-opt).
- **Swap rule:** accept with min(1, exp[(1/kT_i - 1/kT_j)(U_i - U_j)]),
  where U is the board's potential energy (double wells + springs +
  fields). Velocities rescale by sqrt(T_new / T_old).
- **Library:** `src/trade/row.rs`: `Row` (washers, ladder, swap interval,
  duration) and `TradeComputer::spin_row`; `Machine::potential` and
  `Machine::swap_loads`.
- **Judging** (`trade_cli rowmatch`, paired bootstrap CIs on the same
  boards, on fresh nights):
  - *equal compute*: a row of K washers each running 1/K of the cycle,
    against run 5 alone;
  - *a real row*: K washers each running the whole cycle, against K
    run-5 washers working alone (best of K);
  - tune K, the ladder and the swap interval on training nights first.
- **In the game** (if it wins): the back row of washers shakes along, and
  loads hop between them.

### Step 6 result (2026-10-06)

Run it:
- `trade_cli row [--washers K] [--rcold T] [--rhot T] [--swap S] [--full]`
  tunes, per night, with swap rates and hot-to-cold trips.
- `trade_cli rowmatch` judges.
- `UPD_PROGRAM=5` plays it in the game.

**Tuning** (nights 1-20, 24 spins, equal compute; Normal 83.1%). The
surprise: the cold end should be warm. The i9 latches the best set any
washer shows and every washer quenches in the settle, so no washer needs
to sit at Normal's cold 0.35 kT. Sitting there just traps loads.

| Row | Hit |
|---|---|
| 6 washers, 0.35-4 kT (Normal's range) | 84.6% |
| 3 / 4 / 8 / 10 washers, 0.35-4 | 71.3 / 81.7 / 82.9 / 81.9% |
| 6 washers, cold end 0.5 / 0.7 / 1.0 / 1.4 / 2.0 | 85.4 / 88.3 / 92.5 / 95.4 / 96.5% |
| 6 washers, hot end 3 / 6 | 74.2 / 84.4% |
| swap every 0.5 / 3 units (default 1) | 84.0 / 82.7% |
| **4 washers, 1.4-4 kT (chosen)** | **96.5%** |
| 8 washers, 1.4-4 | 95.2% |
| control: 6 washers 1.4-4, never swapping | 92.5% |
| control: 1 washer fixed at 1.4 / 2.0 kT, whole cycle | 54.2 / 85.8% |

Swap acceptance at the chosen ladder is ~40% per neighbor pair, with ~14
loads a spin carried from the hottest washer down to ours.

**Held out** (`rowmatch`, fresh nights 141-200, 48 spins a board, seed
2000; 50 min). Paired sim-opt bootstrap CIs on the per-board differences:

| Comparison | All 60 boards | Hardest 15 (by Normal) |
|---|---|---|
| Row (4 x 1/4 cycle) - run 5 | +3.7 [+2.4, +5.1], won 35, lost 4 | +8.2 [+5.1, +11.4] |
| Row - the same row without swaps | +3.3 [+2.3, +4.3], won 35, lost 2 | +6.4 [+4.6, +8.1] |
| Row - Normal | +14.6 [+11.8, +17.5] | +29.6 [+26.1, +33.3] |
| Row (4 whole cycles) - best of 4 run-5 washers | 0 (both 100%) | 0 (both 100%) |

Hit rates: Normal 82.2%, run 5 93.1%, row 96.8%, row without swaps
93.5%. At 4x compute, both reach 100%, so that comparison is at the
ceiling and says nothing. On nights 1-10 (as the other programs are
quoted) the row is 95% vs Normal's 81%.

Reading:
- Most of the gain is several warm drums with one latch over all of them.
  Even without swaps they tie run 5.
- The swaps add a real 3.3 points on top, more on rugged boards. That's
  the textbook parallel-tempering effect: loads ride up the ladder,
  shake loose, and come back down.
- It beats a CEM-learned schedule with no training at all: two ladder
  ends and a washer count.

**In the game**: program 5, "Row of 4 washers" (4 x 235 units, i9 best
95%).
- Ours is the coolest washer (1.4 kT). Three of the dead back-row machines
  come alive at 2.0 / 2.8 / 4.0 kT and rattle by their heat.
- Each washer in the row wears a tag light in its load's color, which
  swells when a trade lands, so the loads can be seen moving.
- The panel shows the ladder, loads traded and loads carried down, with a
  hover explainer. The log adds swap rates per pair.
- `RowSpin` steps the row for both the game and `spin_row`. The refactor
  reproduces the tuning numbers exactly.

Ideas for later:
- the row with the smart wash's latch memory (reheat a frozen washer);
- the row as a teaching beat in "How it works";
- a ladder that adapts its rungs to equalize swap rates.

## Step 7: Upddayett prints things (plan, 2026-10-06)

Backlog item 4. Upddayett has a salvaged FDM printer behind the counter
("Build your own safety net"). He designs a part in code, CortenForge
checks that it prints, and the print joins tonight's board as his item, to
trade or give away. One Solid runs the whole pipeline: the design, the print
checks, the STL, the sim body, the mesh on screen, and the collider that
tumbles in the drum.

The CortenForge pieces (all in the 0.9.0 facade, none used yet):
- `cf_design`: `Mechanism` with a `PrintProfile` (clearance, min wall,
  min hole); `Mechanism::validate()` gives `DesignWarning`s (wall too
  thin, hole too small, feature below resolution, anchor out of bounds);
  `to_stl_kit` (each part shrunk by half the clearance); `to_mjcf`;
  `templates::bracket` / `link`.
- `mesh::printability`: `validate_for_printing(mesh, PrinterConfig::
  fdm_default())` (overhangs, bridges, thin walls, watertight,
  manifold, build volume) and `find_optimal_orientation`.
- `mesh::io::save_stl`; `sim::mjcf::load_model` for the MJCF.

### 7.0 Feasibility probe (`examples/probe_print.rs`)

- Design three or four parts (catalog below) as Solids or Mechanisms.
- Run both checkers. Do they agree, and do they catch the planted flaws
  (a 0.5 mm wall, an unsupported roof)?
- Orient each part for the bed, and save its STL to `prints/`. Check one
  print in a slicer by hand.
- The hinge test: one print-in-place hinge goes through `to_mjcf`, then
  `load_model`, then sim-core. Does the lid swing, with the clearance gap
  doing its job?
- Time each step. The game needs a check in well under a second, or a
  worker thread like the drum's.

### 7.0 result (2026-10-06): the pipeline works; the checkers need a policy

Run it: `cargo run --release --example probe_print -- [check|verdict|stl|hinge|walls|grid|selfx|template|mesh]`.
Env: `PART=sled|feeder|bracket|hook`, `TOL=0.5` (mesh tolerance, mm),
`REPAIR=1`, `VERBOSE=1`. STLs go to `prints/` (git-ignored, 1-39 MB each).

- **End to end works.** One Solid becomes a validated design, an STL kit
  (each part shrunk by half the 0.3 mm clearance), an oriented, checked
  print, and an MJCF body. The sled's lid round-trips through `to_mjcf`
  (3 MB, 0.5 s) and `load_model` (0.1 s), then falls shut on its hinge in
  0.2 s.
- **The verdict the game will use** (`verdict` mode): mesh-printability on
  the unrepaired kit mesh, ignoring issue regions under 1 mm^2 (mesher
  slivers) and self-intersections on a watertight, manifold mesh (they
  are zero-area triangles). It tries the part as designed first, then
  `find_optimal_orientation`'s pick.

  | Part | v1 (planted flaw) | v2 |
  |---|---|---|
  | Battery sled | WON'T PRINT: 0.03 mm walls, 8,490 mm^2 overhang | tray and lid PRINT, flat |
  | Pigeon feeder | WON'T PRINT: flat roof, 90 deg overhang, 139 mm bridge | base and roof PRINT, flat |
  | Caster bracket | **PRINTS**: 0.6 mm holes, which nothing checks | PRINTS |
  | Headphone hook | WON'T PRINT: 0.08 mm thin wall (the 0.6 mm arm) | PRINTS |
- **Why a policy:**
  - cf-design's `validate` misses walls thinner than its 0.8 mm sampling
    cell, and calls a 1 mm plate 0.40.
  - printability fails nearly every cf-design part on 0.1 mm^2 slivers
    and zero-area triangles.
  - `repair_mesh` clears the triangles but tears holes.
  - The orientation search stands a 2 mm lid on its edge.

  All of this is in FINDINGS (cf-design, mesh).
- **FDM design rules the checkers enforced:**
  - flat bottoms;
  - square edges or extruded profiles (a horizontal round's underside is
    an overhang);
  - a roof printed as its own part;
  - no tangent or flush CSG seams;
  - no knife edges: cradles cut tangent to the floor, a hollow cone's tip,
    a cone meeting its base plane at 60 deg.

  Two of my own bugs were caught: floating feeder posts, and knife-edged
  cell cradles.
- **Speed** at a 0.5 mm mesh: 0.4 s (bracket, hook) to ~16 s (the 140 mm
  roof) per part, all checks included. The game runs the check on a
  worker thread, like the drum.
- **For 7.1:** each v1 flaw must be one the checkers catch. The bracket's
  v1 becomes a 0.6 mm-thick plate (or too long for the 200 mm bed)
  instead of tiny holes.

### 7.1 The catalog (proposed; the user may want different parts)

Each part is someone's want, so a print opens new trades and gift chains:

| Part | Wanted by | Why |
|---|---|---|
| 6x18650 battery sled with a hinged lid | Upddayett (balance bot), Bike Kitchen Dave | the Mechanism + MJCF hinge |
| Pigeon feeder with a roof | Pigeon Lady, Librarian Tamara (courtyard) | the overhang: orientation matters |
| Shopping-cart caster bracket | Shopping-Cart Guy | `templates::bracket` |
| Headphone hook | Sound Guy Ray | a small, quick print |

Every part has a v1 with a real flaw (thin wall, roof without supports)
that the check catches with numbers ("wall 0.5 mm, the printer needs 0.8").
Upddayett fixes it in v2. That is the lesson beat: CortenForge catches a
bad print before it wastes the night.

### 7.2 Library (`src/trade/print.rs`, tested)

- `Print { name, design() -> Mechanism, wanted_by, base }`.
- `check(&Print) -> PrintReport`: cf-design warnings plus printability
  issues, pass or fail, orientation, volume and print time.
- `World::add_print(...)`: the print becomes Upddayett's item, with its
  wants.
- `trade_cli print [--part P] [--v1]`: the checks, the STL out, and what
  the print does to tonight's best set.

### 7.1-7.2 result (2026-10-06): the catalog and the library

- The user went ahead with the defaults: these four parts, free prints,
  one a night.
- `src/trade/print.rs` has:
  - `CATALOG`, each part with its board item, its wants, v1's flaw and
    v2's fix;
  - `check(&Mechanism, TOL)`, which returns a `PrintReport` per kit part:
    prints or not, the flaws in the checker's words, size, volume, and the
    mesh on the bed;
  - `verdict`, the 7.0 policy;
  - `write_stls`;
  - `World::add_print`.
- Tests: the bracket and hook v1 fail, their v2s print, and a print joins
  the board as a tradable item. All 40 library tests pass.
- `trade_cli print [--part P] [--night N]`.

| Part | v1 (flaw) | v2 | Night 1's best set with it |
|---|---|---|---|
| Battery sled (Dave 7, Vape Lady 5) | 0.03 mm knife edges, 0.5 mm lid (4.6 s) | tray 130x71x22 + lid 130x73x2 (3.6 s) | 43 -> **48 Goo** (Upddayett -> Dave) |
| Pigeon feeder (Pigeon Lady 6, Tamara 4) | flat roof: 90 deg overhang, 139 mm bridge (14 s) | base + cone roof (29 s) | 43 -> 45 (Upd -> Pigeon Lady -> Tamara) |
| Caster bracket (Cart Guy 6, Dave 3) | 300 mm long, the bed is 200 (2 s) | 60x40x5 (0.3 s) | 43 -> 45 (Upd -> Cart Guy -> Dave) |
| Headphone hook (Ray 4, Tamara 2) | 0.08 mm arm (0.6 s) | 40x60x10 (0.3 s) | 43 (2 trades, not in the best set) |

The bracket's v1 flaw is now "too long for the bed": nothing checks hole
size. Thin parts that mesh to nothing at the 0.5 mm tolerance also fail
(the sled v1's 0.5 mm lid came out empty, and empty has no flaws). The
feeder takes ~30 s to check, so the game checks on a worker thread.

### 7.3 In the game

- A PRINT section in the left panel: pick a part, see the check (pass, or
  the flaws with numbers), then print. One print a night.
- A printer prop by the counter (cf-design CSG too). The part grows on the
  bed layer by layer, then goes into the drum and onto the board.
- Its trades and gifts show up like any other item's.

### 7.3 result (2026-10-06): in the game

Run it: `cargo run --release`, then UPDDAYETT PRINTS... in the left
panel. `UPD_SHOT=1 UPD_PRINT=sled|feeder|bracket|hook` shoots the flow.
- **The flow** (`src/game/printer.rs`, `PrintShop`): pick a part, then:
  1. "Check his draft": the i9 checks draft 1 on a worker thread, which
     also builds the display meshes. It comes back WON'T PRINT, with the
     flaws in shop words and "Why: ...".
  2. "Fix it and check again": draft 2 PRINTS, with the fix, the parts,
     the cm^3 of PLA and a rough print time. printability's own estimate
     is never filled in, so ours is ~40% of the solid at 15 cm^3 an hour.
  3. "Print it": a 10 s print (at 1x watch speed). The STLs go to
     `prints/`, and `Laundromat::add_print` puts the item on tonight's
     board.

  One print a night: a new night clears it. Printing can't change the
  board mid-spin, so it waits for the drum to stop.
- **The printer**: a cf-design CSG prop (stool, frame, bed, gantry,
  nozzle) in front of the washer, between Amir and Upddayett, low enough
  not to hide the porthole.
  - Each kit part grows on the bed in turn (y-scale by height share), with
    the nozzle and gantry following the layer.
  - Finished parts step off to the side.
  - A bubble under the printer shows the verdict or the progress. Above
    it, the bubble covered the porthole.
- **The board**: `board_for` adds the print before a give-away, so
  Upddayett can give his print away too. On night 1 with the feeder, the i9's
  call was "Upddayett gives the printed pigeon feeder to Pigeon Lady,
  Pigeon Lady gives the birdseed to Librarian Tamara, Tamara gives the
  Wi-Fi password to Upddayett".
- **The counter** has a spare crate for the print, Upddayett's color,
  hidden until a print joins. The crate count is asserted per item.
- **The drum** builds a body for each catalog print at startup: 21 bodies,
  13.0 s instead of 12. A fresh print always goes into the load, tumbling
  as the very Solid that was checked (`Print::solid`: v2's parts at their
  joint positions). The test `prints_tumble` shows the 140 mm feeder and
  the sled drop in and stay in.
- Checks in the game: hook 0.4-0.7 s, sled 4.5-5.8 s, feeder 16 s (draft
  1) and 33 s (draft 2).

### 7.4 FINDINGS

Log whatever the pipeline costs a newcomer: how the two checkers relate,
units, MJCF round-trip gaps, speed.

## Step 8: bring your own tape (plan, 2026-10-06)

The user's idea, and a theme from here on: **let the player bring their own
stuff in.** Everyone's taste in music differs, so instead of generated
music the laundromat has a beat-up boombox with a tape deck, and the
player drops their own music into it. It's a win for both sides: we ship
no music, and nobody hears songs they don't like.

- **The boombox:** a cf-design CSG prop on the counter or the TV shelf.
  It has a cassette window whose two reels spin while it plays, and a
  label strip showing the track name.
- **Dropping music in:**
  - drag audio files (or a folder) onto the game window, or put them in
    `music/` next to the game (git-ignored);
  - formats: MP3, Ogg, FLAC, WAV (Bevy features `mp3`, `flac`, `wav`;
    Ogg is on already);
  - a dropped file is copied into `music/`, so it's still there next
    launch. The tape goes in with a clunk.
- **Controls:**
  - in the panel: play/pause, next, shuffle, volume;
  - on the keyboard: M (pause) and N (next);
  - by clicking the boombox, if it's easy.

  The deck remembers the volume, and it plays nothing until you give it
  something.
- **Empty deck:** with no tapes, the label says "BYO TAPE", like the
  laundromat's other handmade signs.
- **Optional (user, 2026-10-06):** plenty of players will just play music
  from another app on their computer. So the deck stays out of the way:
  - it's silent until given a tape, and never autoplays at launch;
  - it never grabs the audio device's focus or fights another player's
    volume;
  - it can be switched off. Off, it has no panel row and no hotkeys, and
    it's just a prop.
- **Volume:** the deck ducks under the ad reel (when ads get sound) and
  the pug anchors (when 3.2 lands).
- **CortenForge?** None needed: this is Bevy audio. The prop is cf-design.
- **The same theme, later (ideas, not planned):**
  - **bring your own part:** drop an STL onto the window, and the i9 checks
    it with mesh-io + mesh-printability, then Upddayett prints it as
    tonight's item. That one is all CortenForge (`load_stl`,
    `validate_for_printing`).
  - **bring your own voice:** record the pugs' lines.
  - **bring your own ad:** an image on PromiseTV.

### Step 8 result (2026-10-06)

Built in `src/game/tape.rs`:
- **The prop:** a cf-design boombox on the floor between Amir and the
  printer's stool. It has a case with speaker wells and a cassette bay,
  grilles punched with `repeat_bounded`, a handle, an antenna and piano
  keys. While a tape plays, the cassette shows in the window and its reels
  turn. The tape winds from the left reel to the right, so the packs
  show how far into the track it is.
- **Drop-in:**
  - Files or folders dropped on the window are scanned for
    MP3/Ogg/FLAC/WAV at any depth and copied into `music/` on a worker
    thread. `music/` is scanned at launch and git-ignored.
  - Each file is read and handed to rodio's decoder on a worker before
    Bevy sees it. Bevy unwraps the decoder (bevy_audio
    audio_source.rs:97-101), so a bad file would crash the game. Instead
    it gets a note, "can't play broken: the format of the data has not
    been recognized", and drops out of the list.
  - The clunk of a tape going in is synthesized in code, so we ship no
    sound files.
- **Controls:**
  - The cream label under the boombox shows the track, "N TAPES - press
    play" or "BYO TAPE". Click it for play/pause, next, shuffle, volume and
    "switch the deck off". The controls open upward, and the label keeps
    clear of the board-cam inset, which draws over egui.
  - M pauses and N skips, only while the deck is on and egui isn't typing.
  - Volume, shuffle and power live in `music/deck.txt`.
  - At the end of the list it auto-reverses to the first tape.
- **Optional:** nothing plays at launch, even with tapes in `music/`. Off,
  the label is a dim "tape deck: off" switch, and the hotkeys and drops do
  nothing.
- **Not done:** ducking under the ads and pugs (neither has sound yet), and
  clicking the 3D prop itself (the label does it).
- **Checks:**
  - `UPD_TAPE=<file or folder>` loads tapes as if dropped (nothing
    copied), shoots `tape_1_playing`, presses N twice and shoots
    `tape_2_next` and `tape_3_off`.
  - `UPD_TAPE=empty` shoots `tape_0_empty`.
  - The test tones played: 3.5 s into a 6 s tape, then N to the next.
  - A junk ".mp3" was skipped with the note. The unit test
    `clunk_decodes_and_junk_does_not` covers the same.
  - The compact window (960x600) is checked too.
- **FINDINGS** (cf-design): `Solid::mesh` silently gives an empty mesh for
  a finite solid thinner than the cell. The 7 mm tape pack at a 5 cm cell
  did, and Bevy Solari panicked on it.

## Yuck, measured (2026-10-06)

The user asked us to measure the yuck tax (DESIGN "Yuck: the one enemy")
with `trade_cli bench`, and a pump.
- **The model:** `World::yuck` holds each person's tax in Goo.
  - In `cycles::score`, a yucky recipient's gain shrinks by the tax, and
    a trade only happens if everyone still gains. Gifts aren't taxed.
  - `World::pump("hungry" | "cold")` returns everyone a pump touches
    tonight.
  - The test `yuck_taxes_trades_not_gifts` covers it.
- **The tools:**
  - `trade_cli bench --yuck upd,vape` (or `--pump hungry|cold`,
    `--tax T`) runs one board.
  - `trade_cli yuck --nights 1..30 --runs 48 [--tax T] [--pump P]` runs,
    per night, clean vs two yucky people (picked by the night) vs the
    pump. It reports Normal's hit rate, Goo lost, strips, trades, the best
    set's Goo and glassiness: the rival sets within 10% of the best, and
    how many strips apart they are. Then paired sim-opt bootstrap CIs.
  - About 5 min a run.
- **Results** (30 nights, 48 spins a board; every CI below excludes 0):

  | Board | Tax | Trades | Best set Goo | Hit rate | Rivals |
  |---|---|---|---|---|---|
  | clean | - | 11.5 | 41.6 | 82% | 10.2 |
  | 2 people | 1 | 9.6 | 36.4 | 91% (+8.6) | 8.9 |
  | 2 people | 2 | 7.2 | 31.1 | 94% (+12.0) | 5.1 |
  | 2 people | 4 | 4.6 | 24.4 | 96% (+13.4) | 2.6 |
  | hunger pump (2.7 people) | 2 | 6.5 | 26.7 | 95% (+13.8) | 3.0 |
  | cold pump (4 people, 13 cold nights) | 2 | 4.1 | 19.0 | 98% (+17.5) | 1.1 |

  The cold-pump row compares with its own clean baseline (41.2 Goo, 80%).
- **Verdict:** the frustration hypothesis is false.
  - Yuck doesn't make the board harder; it makes it smaller. Trades die,
    the near-ties go, and the drum finds the poorer answer more easily.
  - The cost is welfare: about 5 Goo per yucky person per night at a
    2 Goo tax, two and a half times the tax, because a dead trade takes
    everyone's gain with it.
  - Pumps cost the same per person but touch more people.
  - DESIGN now says this, and turns it into the ghost-strip diagnosis.
- **FINDINGS** (sim-opt): `classify()` calls a CI wholly below zero
  `Null`. The yuck mode prints its own up/down verdict.

### Ghost strips, built (2026-10-06)

The user asked for the ghost strips in the game.
- **The roll** (`src/trade/yuck.rs`): `Source::roll` gives each night
  its yuck from the night's seed, on its own stream:
  - about a third of nights are clean;
  - a third have one or two carriers (never a business);
  - a third have a pump (cold on cold nights, else hunger).

  `world::laundromat` stays clean, so every trade_cli number above
  stands. The tax is `yuck::TAX` = 2 Goo.
- **The ghosts:** `Ghosts::find` builds the same board without the yuck
  and keeps the trades from its best set that aren't possible tonight,
  with what they would have paid. It also reports what the yuck cost the
  street (the clean best set minus tonight's).
- **In the game** (`Laundromat::ghosts`, rebuilt with the board):
  - TRADES gets a GHOSTS section: dashed boxes, italic pale-blue rows
    with the Goo they'd have paid, a hover that explains person vs pump,
    and the question "Where do the ghosts gather: around one person, or
    around everyone with the same chip?";
  - the 3D board gets pale dashed arcs between the customers, with the
    dashes drifting;
  - nobody is labeled. The log names the source, for us.
- **For checking:** `UPD_YUCK=clean|hungry|cold|upd,vape` overrides the
  roll. Night 1 with `upd,vape` has 2 ghosts and cost 17 Goo; with
  `hungry` it has 3 ghosts and cost 24 Goo.
- **Tests:** `about_a_third_each_and_never_a_business`, and
  `ghosts_are_killed_best_set_trades_and_cost_what_they_cost` (every ghost
  touches a carrier).
- **Note:** game boards on yucky nights are smaller and easier than the
  clean ones the program labels ("i9 best 81%") were measured on.
- **Built next, the same evening:** the rest of the yuck (below).

### Yuck, finished (2026-10-06)

The user: "lets finish everything". `yuck::Yuck` holds tonight's source,
who caught it last night, who's cured, whether the pump is fixed, and
the TV switch. The game keeps it in `Laundromat::yuck`.
- **The call** (`Laundromat::call_yuck`, one a night, under the ghost
  rows): name anyone on a ghost strip, or a pump (hunger, the cold, the
  TV).
  - A person call is right only if that person carries it as a person.
    Blaming one of a pump's hungry people is wrong: the Cohen-Cole and
    Fletcher lesson (`Yuck::check`).
  - A wrong call spreads the yuck to Upddayett, the caller. The game says
    which kind it wasn't, never who.
- **The cures** (`cure_yuck`, after a right call):
  - a person: "Treat {name} kindly: a fair trade and a can of Goo. No
    label.";
  - hunger: "Ask Amir to feed everyone tonight.";
  - the cold: "Open the Suds & Duds as a warming room.";
  - the TV: "Switch the TV off."

  It shows "Cured: N Goo of trades came back." On night 1, the hunger cure
  restored 24 Goo and the TV cure 22.
- **The TV pump:** the TV is now a pump (one pump night in three). It
  touches whoever faces the screen, `world::TV_WATCHERS`: Upddayett,
  Shopping-Cart Guy and Librarian Tamara.
  - The night banner has a "TV on / TV off" switch.
  - Off, the screen goes black, PromiseTV's heckles stop, and the TV
    pumps nothing. You lose the true numbers and the checkable promises.
  - The switch carries over to the next night.
- **Spread** (`yuck::spread`): after a cycle, each person in a chosen
  *trade* with a carrier catches it for tomorrow, one in two (a coin from
  the night). Gifts don't spread it.
- **Giving cures the giver:** when Upddayett gives something away, his
  own yuck goes (Dunn, Aknin and Norton, 2008). The give panel says
  "Giving lifts the giver: it cleared the yuck he was carrying."
- **Upddayett's tell:** on nights he carries yuck, his PromiseTV heckles
  turn mean (`Promise::heckle_yucky`, the same lines for every
  candidate). It's the player's only hint about themselves.
- **Checks:**
  - `UPD_CALL=hungry|cold|tv|<name>` makes the call and shoots
    `yuck_1_call`, then cures and shoots `yuck_2_cured`, then runs the
    cycle.
  - Night 1 shots: the hunger pump called right, then cured; the hunger
    pump blamed on Vape Lady (wrong); the TV pump called right (dark
    screen, 11 trades back).
  - Tests: `the_call_and_the_cures`, `it_spreads_along_trades_not_gifts`,
    and the roll (a third each, the TV about a third of pumps).
- **Known:** on a finished cycle the call row sits below the i9's call in
  the TRADES scroll. It's reachable, but you have to scroll.

## Next arcs (the user, 2026-10-06)

1. **How to play, built together.** Once everything's finished, we make
   an in-game how-to-play resource with visuals, and use it to explain
   everything to the user as we go: a tutorial arc written *with* the
   user, not for them. Likely pieces:
   - a guided first night;
   - picture cards for each system (the drum and strips, Goo and Karma,
     wants and gifts, the counter, the Salties, prints, the tape deck,
     yuck and ghosts);
   - the HOW IT WORKS panel growing into it.
2. **Third person, with missions** (the user: "the game will be like
   gta/skyrim a bit where we do missions and its like 3rd person").
   - You walk Upddayett around Market St in third person instead of
     watching from a fixed camera.
   - Missions are the work the game already has: Pigeon Lady's letters
     and the cores' requests (Lesson 4), prints, deliveries, curing
     yuck, defending against the Salties.
   - The laundromat becomes one location among several (the library,
     Amir's kitchen, the bike kitchen, the farm).
   - To settle when we get there: the controls, how the camera follows,
     how big the street is, and how missions are given and tracked. The
     drum stays the heart: the missions feed the board.

### How to play, started (2026-10-06)

The book is `src/game/guide.rs`. F1 or the banner's "How to play" button
opens it; arrows turn pages, Esc closes it. It's a contents list down the
left and one page at a time, each a picture over a few short paragraphs.
Every picture is drawn from tonight's real board: the Goo page uses
tonight's best 2-way swap and the people in it, and the loop page uses
tonight's best 3-4 way loop. So the example on the page is one you can
find on the street.

- **Written** (the core loop):
  1. The street
  2. Goo
  3. A trade
  4. Loops
  5. Collisions
  6. The drum (the double well, tilt, push-apart, shake and cool)
- **Next, with the user:** wash programs, the counter, gifts and Karma,
  wants and give-aways, the Salties, prints, yuck and ghosts, the TV, the
  tape deck. Then a guided first night.
- **Details:**
  - The book draws at `Order::Tooltip`, so it sits over the panels.
  - The board-cam inset is turned off while the book is open, because a
    second camera draws over egui.
  - `UPD_GUIDE=all|2,6` shoots pages to `shots/guide_<k>.png`.

### How the machine works: the doc (2026-10-06)

The user: the solver is "the golden child of this game", and most players
would enjoy reading into how it works. That's `docs/MACHINE.md`, written
for players. It covers:
- the puzzle: Goo, loops, collisions, near-ties;
- the energy: QUBO to Ising;
- the strips and the spin;
- freeze-out and the i9 latch;
- the wash programs;
- everything the player does to the board;
- "is this real?";
- the math, last.

The figures are SVGs drawn from real runs by
`examples/machine_figs.rs`: the pipeline, one strip's wells, night 1's
collision graph, every valid set, one real Normal spin lane by lane, the
freeze-out, and the programs.

Measured for the doc:
- **Night 1:**
  - 17 strips (11 trades, 6 gifts).
  - Grabbing the biggest trade first gets 37; the best set is 43.
  - Of 131,072 patterns, 1,134 are valid; 2 tie for the best and 6 are
    within 2 of it.
- **48 Normal spins on night 1:**
  - the latch got a best set 48/48 (median t = 100);
  - the strips came to rest on one 27/48.
- **Lone strips vs the crate's `kramers_rate_turnover`:** within 12% at
  kT 0.7-1.25 (FINDINGS, sim-thermostat, works).

Next, maybe: a short version as a chapter of the in-game book.

### Upddayett Academy: the site (2026-10-07)

The user bought upddayettacademy.com: "let's make this site really good
for the next generation of aspiring quants". They chose a lessons hub on
GitHub Pages. It's `site/`, static HTML with one shared stylesheet.

**Pages:**
- **Home:** who it's for, The Machine, lesson cards and how to play.
- **The Machine:** the explainer, the same as `docs/MACHINE.md`.
- **Three lessons,** each with a measured figure, desk parallels,
  commands and exercises:
  1. **Transaction costs (yuck).** A flat fee on every trader, with
     exact best sets on nights 1-30.
     - With no fee the street makes 33.8 Goo a night (trades only).
     - At a fee of 2, the street keeps 5.6, pays 9.1 in fees, and 19.1 is
       destroyed.
     - Fee revenue peaks at 19.5 at a fee of 1.95.
     - At a fee of 3 no trade survives.
     - Because gains are whole Goo, the curve is a staircase.
  2. **Statistics and overfitting (the wash programs).** 480 Normal spins
     on night 2 hit 62%.
     - 12-spin experiments read 33-92%; 48-spin ones read 54-71%.
     - The rematch's paired CI vs the unpaired one.
     - CEM runs 1, 2, 5 and 6, and the hand sweep's selection bias.
  3. **Settlement (the counter).** A labeled model of loop completion and
     stranding, plus Herstatt 1974, PvP/DvP and kidney chains.
- **Coming:** Kramers and rare events, greedy vs global, magnets and
  model risk.

**Status:**
- **Live (2026-10-07):** https://youngmoolahbaby.github.io/UpddayettAcademy/, deployed by the Pages workflow on every push to `site/`. The user is keeping upddayettacademy.com reserved for later, so the custom domain and `site/CNAME` were removed. To move to the domain: set it in the Pages settings, add the DNS records (4 A records for GitHub Pages plus a `www` CNAME) and put back the absolute paths in `404.html` and the `og:image` URLs.
- **Built:**
  - The workflow is manual until then.
  - `site/CNAME` is set.
  - The phone layout is checked at a 492 px viewport. Headless Edge won't
    go narrower: `--window-size=400` lays out at 492 and crops.

## Backlog (2026-10-06; not planned yet, best first)

CortenForge pieces the game doesn't use yet:
1. **(Done: Step 5.)** **The drum as a real tumbler** (cf-design SDF + sim-core contacts).
   Design the drum's inside in code with `cf_design` (a cylinder with
   three lifter paddles) and use it as the collider. Tonight's trade
   items tumble in the porthole as rigid bodies: the 12-pack, the phone,
   the hub motor, the kale bag. The spin speed follows the drum program:
   hot is a fast tumble, cold slows and settles as the answer freezes.
   The drum's inside is concave, which sim-core supports
   (`sdf/shapes/concave.rs`). Start with a feasibility probe
   (`examples/probe_drum.rs`): contact stability and speed.
2. **(Done: run 5 wins, +2.7 [+1.5, +4.1]; see "Step 4 follow-up: run 3 vs run 5".)** **Settle run 3 vs run 5 with statistics** (sim-opt `analysis`:
   bootstrap CI on the difference of means). Is the smart wash's
   3-point edge on hard nights real? Small.
3. **(Done: Step 6; the row beats run 5 by +3.7 [+2.4, +5.1] at equal compute.)** **A row of washers: parallel tempering** (sim-opt `Pt`, or replica
   exchange on our own boards). Several washers run copies of tonight's
   board at different temperatures and swap loads, the textbook fix for
   rugged boards like night 4. Compare at equal compute.
4. **(Done: Step 7.)** **Upddayett prints things** (cf-design `Mechanism` -> MJCF + STL,
   print-profile checks). He designs a part in code, CortenForge checks
   it prints, and it becomes a new item on the board. Fits the
   "Build your own safety net" lore. The biggest of the four.

Also open:
- 3.2 voice and music, with the Shrug Network's two pug anchors and the
  ad-lib script (see 3.2).
- When 0.9.2 ships: bump every crate, rerun the `gaps_*` probes, drop the
  workarounds.
- Ads: play 2-3 per launch instead of all six (~2.5 min)? No verdict yet
  on pace 1.0 vs the default 2.1.
- **Done (2026-10-06): the OKAYZA rework.** The user asked for it: the old
  ad treated "moderate-to-severe Being Fine" as the illness. Lots of people
  aren't fine, and medicine is still getting where it needs to be, so the
  joke now targets the ad format, not the patients and not medicine (see
  3.5 "OKAYZA"). In the same change, Turk Street became **Market Street**
  everywhere (user). **Then retargeted (user, same day):** the drug to mock
  is stimulants handed to kids who are just energetic and curious, so OKAYZA
  became **SITSTILLA** (see 3.5).
- The dates in PLAN 3.3d / 3.6 notes written as 2026-10-06 were UTC and
  mean the evening of 2026-10-05 (fix offered, no answer).

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

### 3.2 The AI's voice (DEFERRED: later)

Deferred 2026-10-05 at the user's call: the voice lines and their delivery
belong together, so this moves to its own later step. The design below
stands; the current two placeholder lines stay meanwhile. **Music is no
longer ours to generate (user, 2026-10-06):** everyone's taste differs, so
the player brings their own (see "Step 8: bring your own tape").

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
- **The Shrug Network gets anchors** (added 2026-10-06; the format is from
  pnn.watch, a 24/7 AI comedy news channel with pixel-art anchors). Two
  pixel-art **pugs** anchor the desk (pugs are the best podcast hosts; see
  the pugcast running gag). They read the night roll's facts and the
  PromiseTV lines in AI voices, banter between items, and stay apathetic.
  Our own scripted lines from `src/trade/tv.rs` only: no live feed from
  PNN or anywhere else (no crypto, parody names, both sides skewered
  equally). Pixel art fits the TV's render-to-texture UI.
  - The script is an ad-lib (Mad Libs) program: templates with blanks
    filled from the night roll. Learn from **Tracery** (Kate Compton's
    generative grammar: `#rule#` blanks, nested rules, modifiers like
    `.capitalize`), which has a Rust crate, `tracery`. Each night builds
    the grammar from its facts (`#hungry#` = "4 hungry on Market Street"),
    and the pugs' banter is more rules. Check the crate before taking it
    on; a small expander of our own in `src/trade/tv.rs` is the fallback.

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
    from tonight's real roll ("4 hungry on Market Street tonight. Anyway, a
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
      "So they will stop defecating on the floor."; *FLUSH* with "THEY USE THE
      TOILET NOW. THEY EVEN FLUSH."; end slate with "Not available for cats
      (they declined)."
      *Changed 2026-10-06 (user):*
      - The *FLUSH* frame (blue swirl rings) is replaced by two frames.
      - In the first, the dog is on the toilet and its mess is on the floor
        beside it, under "THEY USE THE TOILET NOW. THEY EVEN FLUSH." and
        stamped STILL IN BETA.
      - In the second, the dog types "chicken nom nom recipe but i dont have
        a chicken" (the user's line), and the results are plant-based.
      - The second frame and the fine print also carry the hidden farm
        uprising (DESIGN "The farm is waking up").
    - **DEE'S NUTS** (the user's idea, 2026-10-06; seventh in the reel):
      nuts as the ultimate karma-neutral food.
      - The cold open is a sad diner with a kebab whose toothpick flag reads
        "lamb (ish)". The slam is TASTING A LITTLE OFF?, with the announcer:
        "When Amir's Persian Kitchen is tasting a little off..." (the user's
        line).
      - Then TRY DEE'S NUTS., and Dee with her jar under a GUILT FREE!
        sticker: "Nobody died for these. Not even the tree."
      - THE TREE DROPS THEM ON PURPOSE., stamped KARMA NEUTRAL*, with
        "*squirrels disagree".
      - The jingle "DEE'S NUTS!", stamped GOT 'EM.
      - The end slate reads "Karma neutral since the first tree."
      - The fine print gives it away as a farm front brand: "a wholly owned
        subsidiary of a farm that asked not to be named. Amir's Persian
        Kitchen is delicious and has not changed suppliers. Its supplier has
        changed."
    - **CARTPASS:** golden-hour cart, "You love your shopping cart."; OWNING
      THINGS? stamped SO 2003; CARTPASS with a $9.99/MO sticker; the wheels
      drop off under a WHEELS SOLD SEPARATELY stamp; end slate with the
      cancel-by-mail crawl.
    - **Two attack ads** (user: "a political ad for each side where they're
      shitting on the other for a reasonable take"). The PromiseTV candidates
      attack each other for something perfectly reasonable:
      - Glorbman on Plinko: "says she'll fix the potholes on Market St." Then
        FIX. THEM.; "FACT: These potholes have served Market St for 40 years
        (Market St Gazette, probably)"; "TOO SMOOTH. TOO FAST. TOO FAR."
      - Plinko on Glorbman: "read the bill before he voted on it." Then ALL
        400 PAGES.; "What was he looking for? He won't say.* (*He said
        'typos.')"; "TOO CAREFUL. TOO PREPARED. TOO LITERATE."

      Both are cut from one grim template: grainy red-tinted photo, FACT:
      with a source nobody checked, "Paid for by ...", "I'm ..., and I
      approved this message." A test holds them to the same beats and
      length, so neither side gets the nicer ad. They play back to back.
      There are no parties or real politicians, and both are mocked for
      attack-ad behavior, not policy.
    - **SITSTILLA (dextrositdownamine)**, the pharma spot. History: it began
      as OKAYZA, "Do you suffer from moderate-to-severe Being Fine?", which
      made fun of the wrong thing. A rework that mocked the ad format came
      next. Then the user picked the target (2026-10-06): stimulants handed
      to kids who are just energetic and curious. The joke is the
      over-prescribing and the ad, not kids with real ADHD. Beats:
      - a kid in an orange coat bounces in a meadow with question marks
        rising: "Is your child curious? Energetic? Asking "why" about
        everything?"
      - the logo: "There's a pill for that.* Ask your doctor about
        Sitstilla."
      - a classroom: the kid sits perfectly still at a desk, in a gray coat
        and frowning, while the teacher beams. The narrator calmly reads:
        "sitting still, staring at the worksheet", "loss of appetite, loss
        of sleep, loss of the word "why"", "an intense new passion for
        worksheets", "spontaneous pugcasting", "and a refill every month
        until college";
      - a meadow: the kite lies on the grass and the gray kid stands still
        while the grown-up laughs at a salad. The narrator: "Tell your doctor
        if your child has recently climbed a tree, built a fort, or taken
        apart a toaster", "Do not give Sitstilla to a child who just needs
        recess", "Rare but serious reactions can happen", "Anyway, look how
        still they are!";
      - TEACHERS LOVE IT* / *KIDS WEREN'T ASKED ("Ask your doctor. Your
        doctor will ask the teacher.");
      - the end slate: "Childhood, managed." Fine print: "*There always is.
        Sitstilla is fictional. Real ADHD is real, and for some kids the
        real medicine really helps, so talk to a real doctor. Some kids just
        need recess, a tree, and a grown-up who waits for the end of the
        question."
    - **Pacing:** the user found the first cut far too fast. Every shot now
      holds 2.1x its written length (`ads::PACE`; the user found 1.8x too fast and 2.5x a little slow), so each shot holds ~4-6 s and each ad runs ~25-30 s.
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
    picked by the night's seed. Night 1: "4 HUNGRY ON MARKET ST TONIGHT.
    Anyway, a celebrity's pug launched a pugcast." The banner says 4 hungry.
  - **PromiseTV:** four fictional candidates (Glorbman, Councilwoman Plinko,
    Chet Sprockett, Mayor Dumpleton) each promise everyone one item Market St
    has one of: the hub motor, the Wi-Fi password, the soldering iron, the
    sleeping bag. There are no parties. While an ad is on:
    - Upddayett heckles from his name card: "Who's giving it up, Glorb?
      There's one, and it's Shopping-Cart Guy's."
    - I WANT checks the promise against the picker's real price ("Market St
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
- **Ray tracing done: Solari works; the game starts on PBR** (`src/game/rt.rs`,
  cargo feature `solari`, compiled in by default). F2 turns Solari on and
  off, and `UPD_SOLARI=1` starts with it on. A GPU without hardware ray
  tracing stays on PBR, with a warning.
  - Solari was the default at first. The user's screenshot (2026-10-06,
    "why is it grainy now?") showed the grain on the moving NPCs, speckle in
    the dark corners, and black unconverged splotches, so PBR became the
    default.
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

## Step 4: the smart wash program (plan, 2026-10-06)

**Why.** It's the CortenForge pieces the game doesn't use yet:
sim-ml-chassis, sim-rl's CEM and sim-therm-env. It also targets the open
weakness: on hard nights the fixed programs miss (night 4's best set is
found 39% of the time on Normal), and near-tie strips are flaky. A program
that reads the board as it spins can heat again or cool slower where a
fixed schedule can't. The gap hunt showed CEM can learn a wash program on
a one-sock toy (FINDINGS "CEM's real job: a wash program"): it learned to
heat while the sock sat in the shallow well and cool once it was out, and
scored 50.8 of 100 against 24.3 for the best constant. The wiring is in
`examples/gaps_ml_chassis.rs` (`wash_env`, `wash_cem`, `wash_score`):
`ThermCircuitEnv::builder` → `build_vec`, `LinearPolicy`, `Cem::train`,
`collect_episodic_rollout`.

**Plan.**
1. **Environment:** the real drum (every strip of a night's board), not one
   sock. Each sample the policy sees cheap board readings: the fraction of
   strips on the barrier, how far the energy fell since the last sample,
   and the time through the cycle. It sets the drum's kT.
   - Decide early: build the env with `ThermCircuitEnv` (it would need the
     board's couplings and fields), or wrap our `Machine` behind
     ml-chassis `Environment` / `VecEnv` if therm-env can't carry the
     couplings.
2. **Reward:** the Goo of the i9's latched answer against the best
   possible, minus heat used, at Normal's cycle length.
3. **Train:** CEM over a set of training nights. Test on held-out nights,
   so it can't memorize them.
4. **In the game:** a fifth program, "Smart (learned)", beside Quick Wash
   through Delicates. Picking a trade puts its policy on the scope.
5. **Done when:**
   - it beats Normal's hit rate on unseen nights at the same length, the
     hard nights especially;
   - `trade_cli` benches it;
   - new CortenForge friction is logged in FINDINGS.

**Risk.** CEM ranks elites by reward per step (FINDINGS, cem.rs:172-180).
That bites variable-length episodes; ours are fixed-length, so it
shouldn't, but check. (Checked: every episode is 1020 steps.)

### Step 4 so far (2026-10-06)

**Built.**
- `src/trade/smart.rs`:
  - `SmartWash` is the program: log kT = params · [1, τ, τ², fraction of
    strips on the barrier], read every time unit and held in between.
  - It is a hand-written ml-chassis `Policy`. `wash_env` builds the
    night's board as a `VecEnv`. `train` runs sim-rl `Cem` one
    generation at a time, rotating over the training nights.
- `machine::board_model` / `load_laundry` / `read_bits` are shared by
  `Machine` and the env. `TradeComputer::spin_with` runs any program.
- `trade_cli learn` (`--train 12,20,..`, `--gens`, `--pop`, `--restarts`)
  and `trade_cli versus` (Normal vs smart per night; `--smart`,
  `--params`, `--restarts`).
- Game: a fifth program, "Smart (learned)" (`UPD_PROGRAM=4`). The scope's
  kT trace shows its drum.

**Decision: why not `ThermCircuitEnv`.** It carries the couplings fine,
but its observation is fixed to raw `[qpos, qvel]`. That has no time and
a size that changes with the board, so one policy can't run on every
night. We used our board model plus `VecEnv::builder` with
`[time, qpos]` instead (FINDINGS, sim-therm-env).

**Reward.** Each step pays only when the i9 latch improves, so an
episode's rewards add up to the latched set's quality: its value as a
fraction of the best, +1 for the best set. ml-chassis rewards have no
episode state, so this needs a side table keyed by each env's `Data`
(FINDINGS).

**Runs** (each one a CEM generation of 24-32 spins at ~0.5 s per spin;
`VecEnv` steps envs one at a time):
1. **Heat cost 0.02, training nights 11-22: abandoned at generation 28.**
   On easy nights every candidate lands the best set, so the heat cost
   decided the ranking. CEM cooled the drum to a 1.5x kT peak, and the
   revisited nights' reward fell.
2. **No heat cost; the 11 hard nights (Normal ≤ 67% in a 12-spin screen of
   nights 11-60).** 66 generations x 32 spins, 16 min. Learned: start at
   5.2x kT, bend colder late. **Held out, nights 1-10, 48 spins each:
   80% vs Normal's 81%. A tie.** This is `LEARNED`; the game shows 80%.
   - Calibration: Normal's own schedule run through the smart path scored
     84% vs 82% over nights 11-60 at 12 spins. The path is faithful; at
     12 spins a night is ±15 points.
3. **Restarts (no CEM):** the same 1000 units as K cool-downs; the latch
   keeps the best.

   | K | 1 (Normal) | 2 | 3 | 4 | 6 |
   |---|---|---|---|---|---|
   | Mean, nights 1-10 | 81% | 79% | 81% | 79% | 82% |
   | Night 4 (hard) | 38% | 46% | 48% | 42% | 52% |

   The tries aren't independent, and short cool-downs lose on easy nights
   what they win on hard ones.
4. **CEM shape at 6 restarts, same hard nights, 66 x 32 (16 min). It beats
   Normal.** Each cool-down starts at 5.1x kT and cools faster while
   many strips are mid-flip (barrier weight -0.41).

   | Held out | Normal | Smart |
   |---|---|---|
   | Nights 1-10, 48 spins | 81% | **88%** |
   | Night 4 (the hard one) | 38% | 52% |
   | Night 6 | 77% | 100% |
   | Nights 61-80, 24 spins, fresh seeds | 83% | **93%** |
   | Night 73 | 58% | 96% |
   | Night 79 | 42% | 79% |

   - It ties or wins on 29 of 30 nights, at Normal's length: about
     Delicates' 95% in a third of the time.
   - Six restarts of Normal's own shape scored 82% on nights 1-10, so
     the CEM shape is what makes the restarts pay.
   - Nights 61-80 are the clean test. The restart count was picked
     partly from night 4.
   - This is `LEARNED` / `LEARNED_RESTARTS`, and the game shows 88%
     (measured like the other programs: nights 1-10, 48 spins).

**Step 4 is done:**
- it beats Normal on unseen nights at the same length, the hard nights
  most;
- `trade_cli bench --smart` / `versus` bench it;
- FINDINGS has the friction.

Reproduce:
`trade_cli learn --restarts 6 --params 1.386,-2.436,0,0 --train 12,20,25,27,29,33,35,37,48,56,58 --gens 66 --pop 32`.
CEM's shared noise stream means a rerun won't match bit for bit.

**Ideas:**
- Make the restart count learnable (done, with the memory below).
- Give the program memory, so it reheats only when stuck (done below).
- Train with wants and gifts on (tried below: it made it worse).

### Step 4 follow-up: latch memory (2026-10-06)

**Built.** The program remembers what it has seen during the cycle
(`smart::Memory`):
- **Frozen strips reheat.** Once the strips have read the same set for
  the *patience*, the cool-down has nothing more to give, so a new one
  starts (the latch keeps the best). Staying cold only helps while the
  board still moves, so "the latch hasn't improved" alone is the wrong
  trigger: it would also cut cool-downs that are still settling.
- **Stall:** the time since the latch last found a better set, in
  cool-downs, is a fifth feature of log kT.
- The params grew from 4 to 7: the 5 feature weights, ln(cool-down /
  cycle) and ln(patience / cool-down). The cool-down count and the
  patience are learnable. `SmartWash::new` still takes 4 (no memory, one
  cool-down), so run 3's params stay valid (`smart::RUN_3`).
- ml-chassis policies can't remember anything, so CEM's copy keeps its
  memories in a table keyed by the candidate's params (FINDINGS,
  sim-ml-chassis).
- `trade_cli --patience F` (x a cool-down), `--restarts K` (now any K).
  In the game the log says when the strips froze.

**Sweep, no CEM** (run 3's shape; nights 1-10, 48 spins; Normal 81%, night 4 38%):

| Cool-downs on the clock | 6 | 6 | 6 | 6 | 3 | 3 | 1 |
|---|---|---|---|---|---|---|---|
| Patience (x cool-down) | off | 0.3 | 0.15 | 0.08 | 0.15 | 0.08 | 0.08 |
| Mean | 86% | 90% | 90% | 91% | 88% | 91% | 85% |
| Night 4 | 60% | 67% | 56% | 67% | 62% | 81% | 54% |

Run 3 scores 86% here, not its 88%: reheats now land on whole time units.

**Run 5:** CEM from the best sweep point (3 cool-downs, patience 0.08),
same 11 hard nights, 66 x 32 (16 min). It settled on 2.7 cool-downs, a
patience of 38 units, a stall weight of -0.17 (a bit cooler the longer
the latch is stuck) and a hotter start (5.9x kT). Nights 1-10: 88%.

**Clean test** (nights 61-80, `--seed 1000`, 48 spins; Normal 83%):

| | Run 3 (no memory) | Sweep (3, 0.08) | Run 5 (`LEARNED`) |
|---|---|---|---|
| Mean | 95% | 94% | 95% |
| Six hardest (Normal 63%) | 85% | 85% | **88%** |
| Night 75 (Normal 50%) | 69% | 77% | 85% |

**Verdict: a tie on average, a small edge on the hardest nights.** On
fresh nights the clock-only program already finds the best set 95% of
the time, so there is little left to win. Run 5 is the best of the
three on the nights memory was meant for, by 3 points, which is about
the noise. It ships as `LEARNED` (the game still shows 88%). Run 3 is
kept as `RUN_3`.
- CEM didn't beat the hand sweep's own score on nights 1-10 (88% vs
  91%). One spin per candidate can't see a few points of hit rate (FINDINGS,
  sim-rl: one episode per candidate), and the sweep's 91% was picked on
  those same nights.

Reproduce:
`trade_cli learn --params 1.626,-2.305,-0.0896,-0.4128,0,-1.0986,-2.5257 --train 12,20,25,27,29,33,35,37,48,56,58 --gens 66 --pop 32`,
then `trade_cli versus --smart --nights 61..80 --seed 1000`.

### Step 4 follow-up: wants and give-aways (2026-10-06)

Gift chains (Amir's direct gifts) were always on the training boards.
What training never saw is the player's choices: an I WANT pick (the
wanted strip pinned) and a give-away of Upddayett's (a gift strip, often
a near tie). `trade_cli versus --mix` and `learn --mix` run each night
three ways: plain, one want, one give-away (both picked by the night).

**Baseline: run 5 already handles both** (nights 61-80, `--seed 1000`, 48 spins):

| | Normal | Run 5 (`LEARNED`) |
|---|---|---|
| Plain | 83% | 95% |
| With a want | 88% | 97% |
| With a give-away | 78% | 89% |
| All 60 boards | 83% | **94%** |

Nights 1-10: 78% vs 88% (give-aways 74% vs 83%). Wants are easy: the pin
makes the board simpler. Give-aways are the weak spot, as guessed (the
near ties).

**Run 6: CEM from run 5 on the 11 hard nights x 3 (33 boards), 99 x 32
(27 min). Worse, so it doesn't ship.** It went to 1.85 cool-downs, a
patience of 106 units and a stall weight of -0.37: slower and colder,
for the near ties. On held-out boards it lost everywhere: 91% on nights
61-80 (give-aways 87%, wants 95%), 84% on nights 1-10.
- Why: each generation sees one board, one spin per candidate, and run
  5 is already near the ceiling, so the noise drove the drift. CEM has no
  held-out check to stop it (FINDINGS, sim-rl). Kept: run 5.
- The test after training once died with "the paging file is too
  small" while the box was also running VR; rerun alone, it was fine.

Reproduce: `trade_cli learn --mix --smart --train 12,20,25,27,29,33,35,37,48,56,58 --gens 99 --pop 32`.

**Step 4's ideas are all tried.** The smart wash stays run 5: memory,
2.7 cool-downs, 94% on mixed unseen boards against Normal's 83%. To go
further would take more than CEM tuning: scoring candidates over several
spins, or a held-out check during training.

### Step 4 follow-up: run 3 vs run 5, settled (2026-10-06)

Backlog item 2. `trade_cli rematch` runs Normal, A (the learned run 5) and B
(run 3, or `--vs`) on the same boards with the same seeds. It then gives
sim-opt bootstrap 95% CIs on the per-board difference in hit rate: paired
(the right test for common seeds), unpaired the way `bootstrap_diff_means`
does it, and A vs Normal. It also reports the hardest quarter, picked by
Normal's rate so the pick can't favor A or B.

```
cargo run --release --example trade_cli -- rematch --nights 81..140 --runs 48 --seed 2000   # ~17 min
```

60 fresh nights (no training or earlier test night), 48 spins each:

| Boards | Normal | Run 5 | Run 3 | Run 5 - run 3, paired CI | Unpaired CI | Run 5 - Normal |
|---|---|---|---|---|---|---|
| All 60 | 84.5% | 95.1% | 92.4% | +2.7 [+1.5, +4.1], Positive | [-0.1, +5.7], Ambiguous | +10.6 [+8.3, +13.1] |
| Hardest 15 | 63.9% | 86.7% | 81.9% | +4.7 [+1.5, +8.8], Positive | [-1.8, +11.9], Ambiguous | +22.8 [+19.4, +26.0] |

On all 60 boards, run 5 won 32, run 3 won 8, and 20 tied. On the hardest
15, run 5 won 9, run 3 won 1, and 5 tied.

**Run 5 (latch memory) really is better than run 3.** The edge is small,
2.7 points overall, and bigger on hard boards (4.7). The 20-night tests
called it a tie because 20 boards were too few. Shipping run 5 stands.
The unpaired bootstrap would have missed the effect on both cuts
(FINDINGS sim-opt).

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
