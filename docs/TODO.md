# What's left to do

The one list of open work, as of 2026-10-07. Details live in `PLAN.md`
and `DESIGN.md` (sections named in each item). Tick an item here when it
ships, with its commit, and keep its write-up in PLAN.

Everything planned so far is built: Steps 1-8, the yuck, the Salties (dumb
and smart), the companion site with all six lessons. What's left is two
arcs the user named, then smaller items.

## Next arcs (the user picks the order)

- [ ] **Humble pass** (DESIGN "Humble, karma first"; PLAN "Humble pass
  started"). The game looks up at people, never down.
  - [x] open straight into the laundromat; ads move to the TV (click to watch)
  - [x] Upddayett's heckles turn on himself; the AI says "we"
  - [x] soften the pimp framing (DESIGN)
  - [x] the dog ad loses the pig at the window and the bulk tab to The Farm (no farm animal is free yet)
  - [x] Karma first at the end of a night: who went to bed fed, quietly (the simple screen's result card)
  - [ ] review the Shrug Network's shrugs (billionaires, yachts) for punching down
  - [ ] the site's tone: lessons as what the washer taught him, plain words
  - [ ] the how-to-play pages written so far, same check
- [x] **A simple screen for first-timers** (PLAN "The simple screen"): Start button,
  things to do tonight, a people-first result; the rest under the hood (H).
  - [ ] plain-word AI lines on the simple screen (the Salties ones are technical)
  - [ ] a way into the yuck call from the simple screen
- [x] **The mesh handheld** (Tentzhen; DESIGN "The mesh handheld"): a give-away
  for comms with no phone bill. Later: a solar repeater, everyone on the mesh
  once nights carry over.

- [ ] **How to play, the rest of the book** (PLAN "How to play, started";
  `src/game/guide.rs`). Pages 1-6 are written (the street, Goo, a trade,
  loops, collisions, the drum). Write the rest *with* the user, explaining
  each system to them as we go:
  - [ ] wash programs
  - [ ] the counter
  - [ ] gifts and Karma
  - [ ] wants and give-aways
  - [ ] the Salties
  - [ ] prints
  - [ ] yuck and ghosts
  - [ ] the TV
  - [ ] the tape deck
  - [ ] a guided first night
  - [ ] maybe: a short "how the machine works" chapter from `docs/MACHINE.md`
  - Still unasked: what confuses the user most.
- [ ] **Third person, with missions** (PLAN "Next arcs" 2). Walk Upddayett
  around Market St; missions are the work the game already has (Wafflina's
  letters, the cores' requests, prints, deliveries, yuck cures,
  Salties defense); the laundromat becomes one location of several. To
  settle first: controls, camera follow, street size, how missions are
  given and tracked.

- [ ] **The sphere: everyone's own lane** (DESIGN "The sphere"; PLAN
  "The sphere, probed"). A picture of net worth and assets, not the map
  (user, 2026-10-13): each person's spike out of a shared ball. Distance
  still costs, as the wealth gap: too far ahead or too far behind.
  **Paused (user, 2026-10-13): the open items below are deferred to a
  future session.** Next up when it resumes: nights that carry over, then
  the pull over many nights.
  - [x] probe: Thomson placement on the thermostat (`probe_sphere`)
  - [x] the wealth-gap cost measured (`probe_sphere gap`): a sweet spot near
    the street's middle, a slope at both ends
  - [ ] the user's OK on the picture (DESIGN "The picture"): spike = stack
    of your items, shells = doublings, ball = the street's Karma,
    neighbors = trading partners; and on the cost (1 Goo a doubling past one free)
  - [x] the pull (DESIGN "The pull"): a flat-bottomed spring to the street's
    middle, steer only, a force on the board (`src/trade/spring.rs`,
    `probe_sphere pull`); gentle in one night, k = 4 recommended
  - [ ] the gap cost and the pull in the game (from tonight's net worths)
  - [ ] the pull over many nights (needs carry-over): does the spread settle,
    and how fast for each k
  - [ ] the row of washers with the spring (its swaps don't see it yet)
  - [ ] a panel under the hood: tonight, before and after the wash
  - [ ] growing over nights, once nights carry over

- [ ] **Nights that carry over** (DESIGN "Nights that carry over"; PLAN
  "Nights that carry over, probed"). Done first: the sphere's pull and the
  tab need it (user, 2026-10-23).
  - [x] library and probe (`src/trade/street.rs`, `trade_cli street`): the
    street dries up to a third (7 strips, 19 of 49 Goo + Karma)
  - [ ] the user's call on the dry street: the tab, faster restock, or both
  - [ ] the game: Next night carries over; a way to start fresh
- [ ] **The tab** (DESIGN "The tab"): IOUs on the counter, the washer clears
  the rings. Warm, never a debt collector (user, 2026-10-23).
  - [ ] probe: how much of the dry street it wins back; washer vs exact at clearing
  - [ ] the game

## Waiting on something

- [ ] **Bump to CortenForge 0.9.2** when it ships: every crate, rerun the
  `gaps_*` probes, drop the workarounds (PLAN status; FINDINGS).
- [ ] **The tape deck with real music**: the user hasn't tried drag-and-drop
  with their own files; MP3/FLAC decoding is untested (only synthesized WAV
  tones). (PLAN "Step 8 result")
- [ ] **Ad pacing verdict**: default 2.1x; the user was trying 1.0. (The
  reel at launch is gone: ads play from the TV now. PLAN "Humble pass".)
- [ ] **UTC dates**: notes in PLAN 3.3d / 3.6 dated 2026-10-06 mean the
  evening of 2026-10-05 (UTC). Fix offered, no answer.

## Small fixes

- [ ] **The call row sits below the fold on a finished cycle** (TRADES
  scroll; reachable, but you have to scroll). (PLAN "Yuck, finished")

## Deferred steps

- [ ] **3.2 voice**: the `voice` module (a line per outcome, every line
  quotes a real number), Upddayett's replies, a caption bar; the user
  reviews the lines first. Plus the Shrug Network's two pixel-art pug
  anchors reading an ad-lib (Tracery-style) script built from the night
  roll. When ads or pugs get sound, the tape deck ducks under them.
  (PLAN 3.2, "Step 8")
- [ ] **Level 2 needs, the creative-to-survival shift**: nights carry over
  (hunger meter, tonight's outcome feeds tomorrow). Salties theft waits for
  this. (PLAN 3.3, 3.6; DESIGN "Survival and barter")
- [ ] **Longer pay-it-forward gift chains** as a later unlock (2-3
  recipients drop Normal from 81% to 54% / 42%). (DESIGN "Candidate ideas")
- [ ] **Lesson 4, the underground** (the farm): open questions for the
  user: the lesson's name, the cores' names (brands are placeholders), which
  request comes first (the barn computer reuses the printer), whether heat
  joins or replaces the Salties. Requests to build: barn computer enclosure,
  pigeon capsule, sim-soft bite test, court calendar on the thermostat
  machine. (DESIGN "Lesson 4 (proposed)")

## Ideas, not planned

- **Drum:** the door's glass bowl as a mesh; a closer porthole camera;
  items changing hands when the counter settles; sound. (PLAN 5.1)
- **Row of washers:** the row with latch memory; the row as a teaching
  beat; a ladder that adapts its rungs to equalize swap rates. (PLAN
  "Step 6 result")
- **Bring your own:** drop an STL for Upddayett to print (mesh-io +
  mesh-printability); record the pugs' lines; an image on PromiseTV.
  (PLAN "Step 8")
- **Smart wash:** further gains need multi-spin candidate scoring or a
  held-out check (CEM has neither; FINDINGS). (PLAN "Step 4 follow-up")
- **Thermodynamic hardware as lore**: point at real p-bit chips in a later
  lesson or the credits. (DESIGN "Candidate ideas")
- **Site:** move to upddayettacademy.com when the user wants (DNS, Pages
  cname, `404.html` paths, `og:image` URLs; PLAN "Upddayett Academy").

## Known limits (measured, accepted for now)

- Hard nights and weak near-tie strips (night 4, the kale give-away) are
  mostly answered by latch memory (run 5) and the row of washers (96.8%);
  Normal still misses them. (PLAN 3.3a, Step 4, Step 6)
- Nights repeat: the skewed coins carry ~9 bits, so boards recur (nights 8
  and 10 = night 1). Expected, not a bug; multi-board stats use
  `distinct_nights` in `machine_figs`. (PLAN 3.3a)
