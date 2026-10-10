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
  - [x] the dog ad's pig becomes a pigeon (no farm animal is free yet)
  - [ ] Karma first at the end of a night: who went to bed fed, quietly
  - [ ] review the Shrug Network's shrugs (billionaires, yachts) for punching down
  - [ ] the site's tone: lessons as what the washer taught him, plain words
  - [ ] the how-to-play pages written so far, same check

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
