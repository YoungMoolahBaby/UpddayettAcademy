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
  with file:line references to the crate sources.

## Build notes

- Windows 11, RTX 4070 Ti, Ryzen 5 5600 (12 threads). Smart App Control has
  been turned off (it blocked every Rust build script).
- Always run sims in release: `cargo run --release ...`. The first release
  build of the dependency tree takes ~2-3 minutes.
- Feasibility probes: `cargo run --release --example probe_therm -- [scale|kramers|invert|multiply|temp]`
  and `cargo run --release --example probe_soft -- [drop|couple|grad|all]`.
  `examples/probe_therm.rs` holds the proven thermostat setup and the
  QUBO-to-Ising converter that Step 1 reuses.
- Trade computer (Step 1): `cargo run --release --example trade_cli -- [run|bench|cycles] [--want upd:hub]`;
  library in `src/trade/`, unit tests via `cargo test --release --lib`.
- The game (Step 2): `cargo run --release` (Bevy 0.19 + bevy_egui). `UPD_SHOT=1 cargo run --release`
  plays a fast spin cycle, saves `shots/*.png` and exits; use it to check visuals.
