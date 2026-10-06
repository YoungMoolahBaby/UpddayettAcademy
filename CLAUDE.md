# cortenforge-play

A game prototype, **Upddayett's School of Biddness**, built to evaluate the
published `cortenforge` 0.9.0 crates from an end user's point of view. Work
as an outside user would: someone who has only the published crates.

Read first:
- `docs/PLAN.md`: the build plan and current status. **Start here.**
- `docs/DESIGN.md`: the game design (repo snapshot of the living Claude Doc
  linked at its top).
- `docs/FINDINGS.md`: end-user findings about CortenForge. Add new friction here.

## Rules of the role-play

- Use CortenForge only as an end user would: the `cortenforge` facade
  (`cortenforge::sim::*`, `cortenforge::mesh`, `cortenforge::cf_design`, ...)
  and the published crate sources/docs under `~/.cargo/registry/src/*/cortenforge-*-0.9.0/`.
  Don't look for or use the CortenForge git repo.
- Keep CortenForge the focus; other crates (Bevy, egui, ...) only render,
  display and wire things up.
- Favor CortenForge's strength, simulation quality, over raw speed. No
  brute-force GPU RL.
- Log friction (missing docs, panics, confusing APIs) in `docs/FINDINGS.md`
  with file:line references to the crate sources: under its crate in the
  friction log, starting with its type (bug, docs, API, perf, setup, works) and
  the date.

## Build notes

- Windows 11, RTX 4070 Ti, Ryzen 5 5600 (12 threads). Smart App Control has
  been turned off (it blocked every Rust build script).
- Always run sims in release: `cargo run --release ...`. The first release
  build of the dependency tree takes ~2-3 minutes.
- Feasibility probes: `cargo run --release --example probe_therm -- [scale|kramers|invert|multiply|temp]`
  and `cargo run --release --example probe_soft -- [drop|couple|grad|all]`.
  `examples/probe_therm.rs` holds the proven thermostat setup and the
  QUBO-to-Ising converter that Step 1 reuses.
- Gap probes for the 0.9.2 release (one per crate): `cargo run --release --example gaps_thermostat` (and `gaps_therm_env`, `gaps_sim_core` for sim-core + sim-mjcf, `gaps_ml_chassis` for ml-chassis + rl, `gaps_sim_soft` for sim-soft + sim-coupling)
  re-checks every probed FINDINGS entry and prints OPEN / FIXED / ok, with the locked crate version.
- Trade computer (Step 1): `cargo run --release --example trade_cli -- [run|bench|cycles] [--want upd:hub]`;
  library in `src/trade/`, unit tests via `cargo test --release --lib`.
- The game (Step 2): `cargo run --release` (Bevy 0.19 + bevy_egui). `UPD_SHOT=1 cargo run --release`
  plays a fast spin cycle, saves `shots/*.png` and exits; use it to check visuals.
  `UPD_PROGRAM=0..4` picks the wash program (4 = Smart, the learned one); `UPD_WANT=upd:hub` sets a want; `UPD_SEED=<n>` replays a logged
  session (it also picks the night); `UPD_NEXT=1` presses Next night first; `UPD_GIVE=phone` has
  Upddayett give that away; `UPD_IDLE=1` runs the idle check, `UPD_SHIELD=<strip>|found` places the
  shield, `UPD_BATTERY=1` runs on the battery, `UPD_RESPIN=1` fights back and spins again (Salties nights:
  smart coils 1, 10, 16; smart quiet cut 4; dumb magnets 21, 29; dumb cuts 17, 31); `UPD_WINDOW=960x600` checks the compact layout. `UPD_CARDS=1` shoots the title, splash and every beat of the ad reel (`shots/card_*.png`); `UPD_PACE=<x>` tries another ad speed (default 2.1x); Tab hides the panels in play; `UPD_TV=shrug|promise` holds the TV on one channel; `UPD_SCOPE=<strip>` opens the scope on that trade. Ray tracing (Bevy Solari, cargo feature `solari`, compiled in but off at start): F2 toggles it, `UPD_SOLARI=1` starts with it on, `UPD_BENCH=1` times PBR vs Solari (vsync off) and saves `shots/rt_*.png`.
- `trade_cli wants [--bench]` lists every want the picker offers, with cost, field strength and hit rate;
  `trade_cli night --night N` shows a night's conditions; every mode takes `--night N`; `nights` summarizes
  200 nights; `--give upd:phone` gives an item away; `--gifts N` / `--chain N` change the gift strips
  (experiments).
- The smart wash (Step 4, `src/trade/smart.rs`): `trade_cli versus [--nights 61..80] [--seed S]` pits Normal
  against the learned program per night (~2 min for 10 nights x 48 spins); `bench --smart` benches it;
  `learn --params ... --train 12,20,.. --gens 66 --pop 32` retrains it (~16 min, one core;
  the full command is in PLAN "Step 4 follow-up"); paste the printed params into `LEARNED`. It has latch
  memory (reheats when the strips freeze): `rematch [--vs a,b,..]` puts run 5 vs run 3 (or any pair) through sim-opt bootstrap CIs on the
  same boards and seeds (~17 min for 60 nights x 48 spins; PLAN "run 3 vs run 5, settled"); `--restarts K` sets the cool-downs on the clock, `--patience F`
  how long the strips must sit still (x a cool-down) before it reheats. `--mix` makes `versus` / `learn` also
  run each night with one want and one give-away (3x the time).
- The drum tumbler (Step 5, `src/trade/drum.rs` + `src/game/drum.rs`): tonight's items tumble in a cf-design drum on a
  worker thread (the model takes ~12 s to build at startup; `UPD_SHOT` waits for it). Probe: `cargo run --release --example probe_drum --
  [drop|tumble|spin|res|info|build|pool]` (env `CELL`, `DT`, `SOLREF`, `MAXCON`, `N`, `ITEMS=0,1,2`, `NOII=1`).
- The Salties (3.6): `--salty` applies tonight's sabotage, `--magnet POS:S` (S x the flattening field,
  + pushes on) with `--shield`, `--cut F`, `--coil` (the smart Salties' aimed coil); `magnets`, `cuts` and `smart`
  sweep them (`magnets` takes ~4 min a night, `smart` ~20 s).
