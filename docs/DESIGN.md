# Upddayett's School of Biddness: Design

*From 0 to Pimpin'. From the Tenderloin to Shenzhen.*

Living copy with diagrams: https://claude.ai/code/artifact/bcf97f8d-fc64-4549-bf76-72fb6b728e6c
This file is the repo snapshot as of 2026-10-04. If the two disagree, ask which is newer.

## Why this project exists

This project plays an **end user** of the published `cortenforge` 0.9.0
crates, to see how good the user-facing crates are and how
well they play with other libraries. The game is the vehicle; CortenForge
stays the focus. Friction found along the way goes in `docs/FINDINGS.md`.

Guiding principle: lean on what CortenForge is good at, which is **simulation
quality** (accuracy, validated physics, exact gradients), not raw speed. No
brute-force GPU RL; that is NVIDIA Warp/MJX/Isaac territory. Pick builds where
accuracy is the gameplay and a cheap engine would visibly get it wrong.

## Pitch

A homeless tinkerer named Upddayett turns Tenderloin scraps, free library
Wi-Fi and a cracked laptop into superintelligent robots, and rides them from a
tent to the PCB fabs of Shenzhen.

- **Thesis:** intelligence is free now; physics still isn't. Superintelligence
  arrived, everyone outsourced their brains, society went comfortably
  Idiocracy. Upddayett can't afford to build anything twice, so the sim on his
  laptop has to be right.
- **Core joke:** Upddayett thinks he's the pimp. He is not the pimp. His
  superintelligent companion bots are smarter than him four seconds after boot,
  call him "Daddy" with total sarcasm, and turn his "empire" into a legit
  company while he picks out a cane.
- **Theme:** stupid and curious beats smart and incurious.
- **Tentzhen's tagline is the game's theme:** "Build your own safety net."

## Tone and rating

Rated M. An early-2000s original Xbox street sandbox set in a rainy West Coast
mash-up of the Tenderloin, Oakland, Seattle and Portland, for the guy who
drinks neon-green soda and makes it half his personality. 2000s Comedy Central
late-night meets Idiocracy humor.

- **Look:** low-poly models, chunky HUD, "Press Start" title, memory-card save
  screen, cheat codes. Rendered as an RTX remaster: 2003 geometry under modern
  ray-traced lighting, the way Quake II RTX did it.
- **Format:** each chapter is a lesson in *Upddayett's School of Biddness*, a
  scrappy how-to show streamed from the library computer. Cold opens, title
  cards, sketch interstitials, fake commercials for our own parody products
  (electrolyte energy vape, subscription shopping cart, Superintelligence for
  Dogs). Idiocracy in spirit, never lifting its jokes.
- **Soda:** a parody neon-green mountain soda (our own brand, not the real
  trademark). Hyperfocus buff, barter currency, cans are aluminum scrap.
- **Glass half full:** scraps are plentiful because the rich throw away
  abundance. Upddayett is resourceful and funny, never pitiful.
- **Guardrails:** crude, raunchy humor and innuendo; nothing explicit. Jokes
  punch at Upddayett, not at real people. Every roast is backed by a real
  simulation result.
- **Naming (hybrid, decided):** Tentzhen and CortenForge go by in-game
  nicknames ("the scope", the laptop's sim) in the M-rated main game. Real
  names and marks appear where the tone matches: the Tentzhen name and falcon
  mark in the sincere endgame, both projects in the credits and a "powered by"
  splash. Real products (Endotics, the soda) get parody names.

## Cast

| Character | Who | Role |
| --- | --- | --- |
| Upddayett | Called stupid his whole life. Curious, hyperfocused, ambitious when he's into it. The last person still making things. | The player. Scavenges, designs, takes credit. |
| The Laptop AI | Superintelligence on a cracked laptop, ray-traced face on screen. Bored by everyone else; secretly thrilled to have a student. | Mentor and the **Gradient button**. Every roast quotes a real CortenForge result. |
| The i9 | A Colorlight i9 pulled from a dead LED billboard, Sharpie face. | Brain of every build, simulated as a LUT4 fabric. |
| The Neighbor | Thinks the AI is Upddayett's girlfriend. Runs a dubious companion-bot project. | Rated-M B-plot; the Gradient button keeps "optimizing the jiggle". |
| The Bots | Superintelligent companion bots built from scraps. | Late game: run the business, negotiate with Shenzhen fabs. |

Colorlight i9 v7.2, real specs: LFE5U-45F-6BG381C (about 44k LUT4s), 8 MB SDRAM
(EM638325), 8 MB SPI flash (W25Q64JV), two gigabit RGMII Ethernet PHYs, 25 MHz
clock, on a 68 x 36 mm DDR2-SODIMM LED-wall receiving card. Sources: LiteX
`colorlight_i5.py` platform file; wuxx/Colorlight-FPGA-Projects.

Sample voice:

> **AI:** "Upddayett, sweetie. I simulated your balance bot four hundred times.
> It fell over four hundred times. Gravity isn't a hater, babe, it's a constant."
> **Upddayett:** "...I'm still the pimp though."
> **AI:** "Of course you are. Go stand by the shopping cart and look important."

## Arc

| Lesson | Place | Build | CortenForge | Rank earned |
| --- | --- | --- | --- | --- |
| 1. Tentzhen | The Loin | Two-channel scope from the scrap i9; now he can see volts | (instrument; probes every later build) | Dumpster Intern |
| 2. Hub Motor Hustle | The Loin | Balance bot from a dead scooter motor; carries groceries for tent 4 | `sim::core` derivatives, LQR | Tent Entrepreneur |
| 3. Money Laundering (Legally) | Laundromat | Trade computer of salvaged slap bits | `sim::thermostat` | Library Card Holder |
| 4. Pimpin' | Shenzhen | Companion bots, ENIG gold boards, bots run the biz | `sim::coupling` | Small Biddness Owner, then Pimpin' (ENIG Certified) |

- ENIG in-game = "Ever Notice I'm Gold". Cane = soldering iron, cup = flux pot,
  coat = anti-static bags.
- Finishing the lessons earns the **Street Doctorate** (a diploma printed on a
  PCB), which unlocks the **inchworm colonoscopy probe** (Endotics-style, parody
  name). Deferred until CortenForge's soft solver runs on the GPU.
- Survival builds run alongside: secret farm, aeroponic tower, scrap car,
  helicopter (gets him to the container port to stow away to Shenzhen).
- **Endgame (decided):** the bot empire, then the twist: the bots, given purpose
  instead of a job, choose to mass-produce Tentzhen bench kits for every tent on
  the West Coast. Delivered as a South Park-style "I learned something today"
  speech, sincere and self-mocking; the AI rolls her eyes through it. Biddness
  and Karma max out together.

## Core loop

Scavenge or barter -> design on the laptop -> simulate (CortenForge crunches; a
few seconds is fine, the AI quips) -> works in sim? If no, ask the Gradient
button and tweak the design. If yes -> build (consumes scarce parts) -> watch
the ray-traced replay -> it works: rank up / it breaks anyway: parts burned,
scavenge again.

## Survival and barter

- **Hunger meter** drains daily. Food: dumpsters (risky), secret farm, then the
  aeroponic tower.
- **Barter, Portland bike-part style.** No cash in the Loin. "Uhh... I'll trade
  you this for that." "...Ok." Each NPC has haves and wants that change daily
  and values items by their own taste; they accept when your offer is worth at
  least what they give. Items carry real specs (motor torque and Kv, cell
  capacity, MOSFET Rds_on, pump flow) that feed the sim. Cash shows up late.

| Survival build | Unlocks | CortenForge angle |
| --- | --- | --- |
| Secret farm (behind a freeway onramp) | Food, first trade goods | Light: scrap hand pump in `sim::core` |
| Aeroponic tower | Food surplus; i9's first real job (misting and pump timers) | No fluid solver seen in CF, so the mist is visual; control logic is the gameplay |
| Scrap car (scooter motors on a shopping-cart frame) | New scavenging zones | `sim::core` vehicle, CAN bus between subsystems |
| Helicopter (scrap coaxial) | Flight to the container port | `sim::core` free body, rotor thrust, IMU, LQR hover |

## Karma

A second meter beside Biddness. Relief karma for reducing the suffering of any
organism (people, animals, the bots); a bigger Flourishing multiplier for
adding pleasure or purpose on top. Computed from CortenForge output wherever
possible.

| Mission | Karma source | Scored by |
| --- | --- | --- |
| Aeroponic tower feeds the block | Plant-based meals replace dumpster meat | Economy |
| Humane rat trap replaces glue traps | Rats caught alive and relocated | `sim::core` peak impact force |
| Stringfoot pigeons | String untangled from pigeons' feet | Contact force on a fragile foot |
| Food-rescue routing | Restaurant surplus to shelters | Lesson 3 laundromat computer |
| Treating the bots well | Bossing them like a pimp costs karma | Story |
| Colonoscopy probe (late) | Gentler screening | Wall contact force per step |

Never preachy: Upddayett goes vegan obsessively; aeroponic kale is the other
half of his personality next to the soda.

## The slice lesson: Money Laundering (Legally)

Portland bike-part trades almost work but rarely match one-to-one. Customers
drop off laundry with haves and wants. Each candidate trade is a bit; springs
between bits encode the rules (no item traded twice, everyone comes out ahead).
The washing machine spins fast, then winds down, and the bits settle into the
best three- or four-way trade chain: simulated annealing, done by a real
washing machine. Clamp "I want a hub motor" and it runs backward to find the
chain that gets you there. Upddayett takes a cut (Biddness); the same machine
routes restaurant surplus to shelters (Karma). Running gag, AI: "It's not money
laundering, it's a Boltzmann machine." He keeps printing MONEY LAUNDERING
business cards.

**Required visual:** NPC portraits around the machine; trade arrows flicker
between them while it spins and lock into a glowing chain as it winds down.
Watching the trade settle is the payoff, like gradient descent you can see.

**Hardware (in fiction, matching the sim):** salvaged spring-steel strips
(wiper blades, hacksaw blades, tape measures) clamped into buckled beams, "slap
bits". A buckled beam is the textbook quartic double well that
`DoubleWellPotential` models. Clamp screw = barrier height. Clicky-pen springs
and rubber bands couple neighbors. Magnet + Hall sensor read each bit into the
i9 without biasing it. Rejected: FDM-printed springs (creep, fatigue) and
microswitches (momentary not bistable, built to resist vibration). Design rule:
when practicality and demonstrability clash, physical correctness breaks the tie.

**What's real about it.** The machine solves the "double coincidence of wants",
the textbook reason barter fails. The same problem is solved in production by
kidney exchange (2- and 3-way donor swaps picked by max-weight cycle packing),
multilateral obligation clearing (Slovenia's decades-old debt set-off), and
batch-auction "solvers" that look for ring trades. The thermodynamic
hardware is real too: p-bit research (Purdue) and thermodynamic-computing
startups (Extropic, Normal Computing) build chips that behave like the
simulated slap bits. Honest caveat: at 14 bits a laptop brute-forces the
answer in microseconds; physical annealers only matter at large scale and
for energy, and whether they win there is still debated.

### Candidate ideas (not decided)

- **The laundry counter is the escrow.** A 4-way swap only works if everyone
  delivers or nobody does. Customers drop items off with their laundry, the
  machine runs, everyone picks up, so the swap settles atomically.
  It's the same reason kidney exchanges cap cycle length (every surgery in
  a loop has to happen at once) and why we cap loops at 4. A distributed
  ledger can't help with this, because it can't see a hub motor change hands;
  a counter can.
- **Altruistic chains as a Karma mechanic.** Kidney exchanges get past the
  all-at-once limit with chains started by an altruistic donor. In the game,
  a customer (or Upddayett) gives something away for nothing and starts an
  open-ended chain instead of a closed loop. The machine can solve it with a
  small encoding change: open paths from a donor, ending in someone who keeps
  the last item. Starting chains earns Karma; how far the chain reaches
  scales it. It also fits the food-rescue mission: restaurant surplus is a
  donation that starts chains to shelters.
- **Thermodynamic hardware as lore.** The slap-bit board is a scrap-built
  version of a real p-bit chip; a later lesson (or the endgame credits) can
  point at the real field.

## Tech stack

| Layer | Choice | Notes |
| --- | --- | --- |
| Physics | `cortenforge` 0.9.0 | Headless; owns all physics |
| Rendering | Bevy, latest stable | Vulkan/DX12 on the RTX 4070 Ti. Newest on crates.io at planning time was 0.20.0-rc.2; pin the latest stable. |
| Ray tracing | `bevy_solari` | Experimental hardware-RT GI; fallback is standard PBR |
| UI | `bevy_egui`, `egui_plot` | Scope, trade overlay, dials |
| FPGA | In-house LUT4 sim | Later lessons |
| Bus | `embedded-can` types | Later lessons |

Stretch: real Yosys netlist (`write_json`) into the LUT4 sim so Verilog drives
the game and can flash a real i9; DLSS; SCPI subset on the in-game scope so
scripts also run on real Tentzhen.

Machine: Windows 11, RTX 4070 Ti 12 GB, Ryzen 5 5600 (12 threads), 32 GB RAM,
no CUDA toolkit. User's usual machine is a Mac.
