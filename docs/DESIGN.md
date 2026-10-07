# Upddayett's School of Biddness: Design

*From 0 to Pimpin'. From the Tenderloin to Shenzhen.*

Living copy with diagrams: https://claude.ai/code/artifact/bcf97f8d-fc64-4549-bf76-72fb6b728e6c
This file is the repo snapshot as of 2026-10-05 (the Claude Doc above was last
synced 2026-10-04 and is behind: no nights, gifts, Amir or Salties yet). If the two
disagree, this file is newer.

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
  (electrolyte energy vape, Superintelligence for Dogs). Idiocracy in spirit, never lifting its jokes.
- **Soda:** a parody neon-green mountain soda (our own brand, not the real
  trademark). Hyperfocus buff, barter currency, cans are aluminum scrap.
- **Glass half full:** scraps are plentiful because the rich throw away
  abundance. Upddayett is resourceful and funny, never pitiful.
- **Not only jolly:** the Salties (below) take things away. Losses are real,
  so recovering from them means something.
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
| The Salties | Locals who always got ahead by keeping others down; it's all they know. Clever enough, incurious, salty. | Antagonists: sabotage the machine and take people's stuff. See "The Salties". |

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
| 3. The Laundromat | Laundromat | Trade computer of salvaged slap bits | `sim::thermostat` | Library Card Holder |
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

**Creative to survival shift (decided, later step).** In the slice, every
night is independent (level 1). Later, tonight's outcome carries into
tomorrow: whoever got fed isn't hungry in the morning, items that changed
hands stay changed, and Upddayett gets his hunger meter. That's also where
"feed them first" pays off across nights.

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
| Food-rescue routing | Amir's surplus to whoever's hungry tonight (built, Lesson 3) | Lesson 3 laundromat computer |
| Treating the bots well | Bossing them like a pimp costs karma | Story |
| Colonoscopy probe (late) | Gentler screening | Wall contact force per step |

Never preachy: Upddayett goes vegan obsessively; aeroponic kale is the other
half of his personality next to the soda.

## The Salties

Decided 2026-10-05; built 2026-10-06 (Step 3.6: magnet, power cut, "EMP";
theft waits for nights that carry over). The game can't be only jolly. Some people
always got ahead by keeping others down, it's all they know, and they hate
watching the Loin climb out. They're fictional locals, not a stand-in for any
real group, and the jokes land on their bad physics, not on who they are.
They're the flip side of the theme: clever enough, completely incurious, busy
tearing things down instead of building.

- **Name.** "Salty" is the attitude, and salt is the one thing Corten can't
  take: chlorides stop weathering steel's protective patina from forming, so
  it keeps corroding instead of sealing itself. AI: "They're not tough, babe.
  They're corrosive. There's a difference." Rust that protects (patina,
  building things) versus rust that eats (salt).
- **Not crabs.** "Crabs in a bucket" fits the attitude, but Ferris the crab
  is Rust's mascot and a good guy. Ferris can cameo by the "powered by"
  splash or in the credits instead (Ferris is public domain).
- **Upddayett red-teams his own machine.** Real red teamers are the good
  guys: they break your system so you can fix it. The Salties attack;
  Upddayett answers by attacking his own machine first and hardening it.
  That's "build your own safety net" as gameplay.

### What they try, and what physics says

The Salties brag about big physics. The design rule (physical correctness
breaks the tie) makes it a running gag: only the honest attacks work, and the
AI roasts the rest with a real result.

| They brag | What really happens | In the sim | Defense |
| --- | --- | --- | --- |
| "Magnet stuff" | Works. A magnet hidden under the counter throws off the Hall sensors and pushes the strips. | A hostile `ExternalField`: the same knob a want uses, held by someone else. It falls off with distance (dipole, ~1/r^3), so strips near the magnet tilt most. A gentle tilt quietly steers the drum to a worse chain; a strong one goes past the well-flattening tilt (`machine::max_safe_field`), pins strips and is easy to spot. | A steel shield from a dead hard drive; the scope shows a Hall offset while the drum sits idle; a calibration load with a known answer catches tampering. |
| "Solar flare" | A flare won't fry a phone; geomagnetic storms hit long power lines, not pockets. Really, they flip the laundromat breaker. | Power cut mid-spin: the drum stops, the temperature drops to zero and the strips freeze into whatever chain they had (a quench, not an anneal). | A backup battery of vape cells finishes the cycle. They're the cells Upddayett wants for his balance bot, so he has to choose. |
| (Smart ones say nothing) | Works, and hides. An electromagnet aimed at the trade that costs the most, keyed to the drum's shaking. | Our own `PassiveComponent` (`salties::Coil`) that pushes only while `ctrl[0] > 0`: 0.8x, just under flattening. The idle check reads nothing. | The spin check (force balance averaged over a spin) names the strip; then the shield. Smart ones also trip the breaker early, with no flicker. |
| "EMP" | Never works. It made popcorn. | Nothing; that's the joke. No build details, ever: it's a dud on screen. | Laughing at them. |
| Taking stuff | Theft from the tents at night. | Items are gone the next night (needs nights that carry over). | The laundry counter as escrow: what's on the counter is safe. |

### How it plays

- **Sabotage nights** are a night condition, rolled like the weather (3 in
  10 nights: the magnet half the time, the breaker about a third, the "EMP"
  the rest). The banner says the Salties are around and shows their brag;
  the player decides which brag is real physics, reads the signs (an idle
  Hall offset, flickering lights) and picks a defense before spinning.
- **The defenses cost something or take skill.** The idle check is free
  and reads every strip's stray field. The shield covers only a few strips
  (two either side of where it's put), so it works only over the magnet:
  find it first. The battery runs on Ranchelle's 18650s, which leave the
  trades while they power the drum; the game prices that in Goo every
  night.
- **Dumb Salties brag; smart Salties don't.** About 4 in 10 Salties nights
  are the smart kind: no banner, no brag, no flicker. They scout the board
  and aim a coil at the trade that costs the most. The coil only runs
  while the drum spins, so the idle check sees nothing. Or they trip the
  breaker early, when it hurts. The player's answer is the i9's spin check
  (the idle check's force balance, averaged over a whole spin), which
  names the strip after one lost cycle. Then they shield it and spin
  again. Smart Salties hurt most on fast programs; the slow ones give the
  i9 time to latch the right answer anyway.
- **What the magnet does depends on where it is.** Pushing strips "on"
  hurts almost anywhere (trades that clash get pushed together). Pushing
  "off" hurts only over a strip in the best set; over an unused strip it
  does nothing, and on a hard night it can even help by accident.
- **The costs are real:** a sabotaged night costs Goo, can feed the wrong
  person and can lose items. Getting someone's stuff back counts as relief
  Karma; a sabotaged night the player still saves is the payoff.
- **Every roast is a sim result:** "Their magnet pushed strip 7 at 0.84x
  the flattening field. Your shield took it to 0.08x. Steel: 1, Salt: 0."
  The numbers are the i9's idle-check readings and the Goo the run lost
  against the clean night's best set.
- **Endgame (optional):** one Saltie gets a bench kit too. No speech; they
  quietly start building something.

## The slice lesson: The Laundromat

Portland bike-part trades almost work but rarely match one-to-one. Customers
drop off laundry with haves and wants. Each candidate trade is a bit; springs
between bits encode the rules (no item traded twice, everyone comes out ahead).
The washing machine spins fast, then winds down, and the bits settle into the
best three- or four-way trade chain: simulated annealing, done by a real
washing machine. Clamp "I want a hub motor" and it runs backward to find the
chain that gets you there. Upddayett takes a cut (Biddness); the same machine
routes restaurant surplus to shelters (Karma).

### How the machine decides

1. **Goo** is how much someone personally values a thing. One Goo is what
   a can of Mtn Goo is worth to them. The same item is worth different
   Goo to different people.
2. **A trade only happens if everyone in it gains.** Upddayett's phone
   (4 Goo to him, 7 to Ranchelle) for her vape cells (4 to her, 8 to him):
   he's +4, she's +3. Nobody loses, so the trade makes 7 Goo out of nothing.
3. **The drum picks the set of trades that makes the most Goo in total**,
   and no item can move twice.
4. **A want** ("Upddayett wants the hub motor") makes the drum deliver it
   the cheapest way it can. The price is shown: what everyone else gives up
   in total Goo.
5. **Gifts earn Karma** (built, Step 3.3b). Amir's Persian Kitchen gives
   tonight's surplus adas polo away. *Needs* (food when hungry, warmth on a
   cold night outside, feeding hungry animals, a phone if you have none)
   count 1x as relief. *Purpose* (tools, parts) and *pleasure* (treats, food
   when fed) count 1.5x as flourishing, but only for people whose needs are
   covered tonight: feed them first. The drum routes the gift where it earns
   the most Karma, and maximizes Goo from trades + Karma from gifts.
   **Upddayett can give one of his things away too** (built, Step 3.3d):
   his Mtn Goo, his spare phone or his kale. It costs him its Goo and takes
   it out of the trades; the drum sends it where it does the most good (his
   phone goes to Ranchelle, who has none: a need).
6. **Every night is different** (built, Step 3.3a). Weather, who's hungry and
   whose pigeons need feeding are rolled per night; values and tags follow
   from rules, not a fixed table. Crunchton isn't always full.
7. **Wash programs** set how the drum cools: Quick Wash, Permanent Press,
   Normal, Delicates, the slower the surer. **Smart (learned)** (built,
   Step 4) is a program the machine taught itself. CortenForge's CEM
   learned it on other nights' boards. It cools several times in Normal's
   time, reading the strips as it goes, and reheats as soon as they
   freeze (nothing more to learn from that cool-down). The i9 keeps the
   best answer it saw. It finds the best set about 9 times in 10 where Normal manages
   8 (on nights it never trained on), and it helps most on the hard nights.


### The regulars (Tumble & Trade, Market Street)

| Customer | Sleeps out | Notes |
| --- | --- | --- |
| Upddayett | yes (tent) | Wants balance-bot parts (hub motor, cells, soldering iron), the library Wi-Fi |
| Gravo | yes | Has the hub motor and casters; wants Mtn Goo, a sleeping bag |
| Ranchelle | yes | Cells from dead vapes; no phone, so a phone is a real need |
| Brisko | no | Soldering iron, derailleur; wants motors and casters |
| Wafflina | yes | Sleeping bag, birdseed; feeds her pigeons (animal Karma) |
| Crunchton | no (van) | Seattle grunge sound tech: dead RTX 3090, crate of 90s vinyl |
| Zestina | no | Library card, staff Wi-Fi; feeds the courtyard pigeons |
| Amir's Persian Kitchen | no | Gives tonight's surplus adas polo (lentil rice, plant-based) away |

No crypto characters or jokes, by the user's choice.
*Renamed 2026-10-07 (user): goofy single-word names, Idiocracy style.*
Gravo was Shopping-Cart Guy, Ranchelle was Vape Lady, Brisko was Bike
Kitchen Dave, Wafflina was Pigeon Lady, Crunchton was Sound Guy Ray, and
Zestina was Librarian Tamara.
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

### The drum (built in Step 5)

Behind the porthole, tonight's items tumble for real. The load is the five
items in the most trades: the 12-pack, the phone, the hub motor and so on.
The drum and every item are `cf_design` shapes. The same shape is the
physics collider and the mesh you see, and sim-core runs the contacts.
- The tumble follows the wash program: hot is a hard tumble, and the
  paddles carry items over the top; cold is a slow roll.
- When the power cuts out, the drum coasts to a stop.
- When the i9 has its answer, a final spin pins everything to the wall,
  and then it all drops.
- The strips on top do the deciding. The drum shows how hard they are
  being shaken.

### Upddayett's printer (built in Step 7)

Upddayett has a salvaged 3D printer on a stool in front of the washer.
"Build your own safety net": he designs parts in code (`cf_design`), and
once a night he can print one. It becomes his item on tonight's board, to
trade or give away.
- **The catalog:** each part is something a regular wants:
  - a 6x18650 battery sled (Brisko, Ranchelle);
  - a pigeon feeder (Wafflina, Zestina's courtyard birds);
  - a shopping-cart caster bracket (Gravo);
  - a headphone hook (Crunchton).
- **The lesson beat:** his first draft never prints, and the i9 says why
  in the checker's own numbers:
  - "15202 mm^2 hangs over air at up to 90 deg (the printer manages 45)";
  - "walls down to 0.08 mm thick (1.0 mm at least)";
  - a 300 mm bracket on a 200 mm bed.

  He fixes it, the second draft checks out, and it prints. CortenForge
  catches a bad print before it wastes the night.
- **On screen:** the part grows on the bed layer by layer, with the nozzle
  sweeping, and a little screen under the printer shows the verdict. Then
  the print joins the board: new trades, new gift chains, a crate at his
  feet. It tumbles in the drum as the very same shape that was checked.
  The STLs land in `prints/` and are real files for a real slicer.

### The boombox (built in Step 8)

Bring your own tape: everyone's taste in music differs, so the game
ships none. A beat-up boombox sits on the floor by the printer, its
label hand-lettered "BYO TAPE".
- Drop MP3, Ogg, FLAC or WAV files (or a folder) on the window, and the
  tape goes in with a clunk. The cassette shows in the window and the
  reels turn, winding from left to right as the song plays. The files are
  kept in `music/` for next time.
- Click the label for play/pause, next, shuffle and volume. M pauses, N
  skips.
- It never plays on its own. Plenty of players will have music going in
  another app, so the deck switches off to a plain prop: no controls, no
  hotkeys.
- The same theme, later: drop an STL for Upddayett to print (checked by
  CortenForge's printability tools), record the pugs' voices, put your own
  ad on PromiseTV.

### The TVs (built in Step 3.5)

Every laundromat has a TV bolted in the corner. The Tumble & Trade has two
parody channels, and between them they make the drum the only honest thing
in the room:
- **The Shrug Network**, comedically apathetic news. It reads tonight's real
  numbers and shrugs: "4 hungry on Market Street tonight. Anyway, a
  billionaire bought a second moon." The headlines are true (they come from
  the night's roll); the indifference is the joke. The drum, meanwhile,
  sends the food to whoever is hungry.
  - *Planned (with the voice step, PLAN 3.2):* two pixel-art **pugs**
    anchor the desk and read it all in AI voices, bantering between items
    and caring about none of it (pugs are the best podcast hosts). Scripted
    lines only; no live news feed.
- **PromiseTV**, nonstop false promises. Ads for fictional candidates:
  "Vote Glorbman: every family gets a hub motor!" The I WANT picker shows
  what that would really take (there's one hub motor, and someone has to
  give it up), so a promise can be checked live. Upddayett heckles: "Who's
  giving it up, Glorb?"

Parody names only: no real networks, politicians or parties. Both channels
get skewered equally, for behavior (apathy, empty promises), not policy.
No crypto ads.

### The farm is waking up (a hidden thread, 2026-10-06)

The user's idea: the animals we eat (pigs, goats, chickens, cows) get
superintelligence on the black market, leaked from the dogs in the ad. With
it they end their own systematic oppression, and "oppression" is an
understatement. The animals run it themselves, in the dark: they request
compute, run their own psyops, and build their own food companies. Humans
only help. The game shows it in corners, fine print and TV shrugs.

- **The cores.** Each species has a small core team that does the real
  work, like the penguins in *Penguins of Madagascar* (the inspiration
  only; ours get their own names). Each core founds a faux version of its
  own meat:
  - the pigs a faux pork company (working name: Hamlet Foods);
  - the cows faux beef and milk (Holy Cow Holdings);
  - the chickens faux chicken and eggs (Eggsit Strategy);
  - the goats faux goat cheese (Kid Gloves Dairy).

  DEE'S NUTS is their joint venture: "a wholly owned subsidiary of a farm
  that asked not to be named."
- **The chain.** Wafflina's pigeons carry the cores' messages, and she
  passes secret letters to Upddayett without reading them. Upddayett
  builds the low-power devices that get smuggled into the farm: small
  computers in hidden places, running on almost nothing, so the cores can
  do their research and run their shell companies from the barn.
- **The race.** The farm's killing schedule doesn't stop, so some animals
  die along the way, as they would in real life. It happens off screen,
  without jokes: an empty stall, a name crossed off a core's roster. Two
  things run at once against the schedule:
  - **the courts stall it.** A pig can't sue, but a pig's company can.
    Through their shell companies they file suits, injunctions and appeals
    that buy time for their families;
  - **the market ends it.** The faux companies win on price and taste, and
    the meat plants lose their buyers.
- **The ending, in acts** (the user, 2026-10-06: no collapse and no
  island; they buy the place and reclaim it):
  1. Shell companies, bank accounts, front brands (DEE'S NUTS).
  2. The meat plant loses its buyers and goes up for sale.
  3. A shell company buys the farm and the processing plant. An LLC signs
     the deed, and nobody at closing asks who owns the LLC.
  4. They reclaim the cursed space:
     - the kill floor becomes the faux-meat line;
     - the pens become grow rooms for mushrooms and hemp;
     - the barn computer moves into the manager's office, which is now the
       cores' headquarters;
     - the old crew keeps its jobs, now making the faux product.
  5. From there they keep buying, farm by farm and plant by plant, until
     every relative is free.
  6. *(Kept: the user, 2026-10-06.)* The faux-meat corps fund a space
     program, launched from the old plant's parking lot. Then, in the
     user's words: "everyones a billionaire with rockets in the end and
     space just keeps on going until the power runs out".
     - Nobody wins the clash of yuck. It just goes to space with everyone
       else, and the Shrug Network shrugs ("a pig named a rocket after
       itself. Again.").
     - So the real fight isn't side against side. In the user's words,
       "clash against yuck itself". The yuck is the behavior, on every
       side:
       - the cruelty of the kill floor;
       - eating meat in someone's face to prove a point;
       - the three-hour lecture;
       - the TV's shrug;
       - the empty promise.

       The animals, Upddayett and the player fight the yuck, never a
       camp. This is the same rule PromiseTV already follows (skewer
       behavior, not policy), now as the whole game's theme. Karma
       measures less yuck in the world, not which team you're on.
     - "Until the power runs out" is the game's own physics at full
       scale. The drum cools until the strips stop moving; the universe
       cools until nothing does. It's the same thermodynamics as the
       laundry machine, with the heat death as the last night.
- **Real precedents for each step** (the user asked that it make sense):
  1. **Paying for the brains with truffles.** Italy banned truffle pigs
     in 1985 (they dig up the beds and eat the truffles), and dogs took
     the job. So the dogs sell the pigs' finds, and the pigs pay in
     truffles for the leaked collars.
  2. **Pigeon couriers.** Cher Ami flew 25 miles through gunfire in 1918
     and saved 194 men of the Lost Battalion.
  3. **Why the shell company.** The courts won't hear the animals
     themselves:
     - *Naruto v. Slater* (9th Circuit, 2018): a macaque who took selfies
       lacked standing to sue under the Copyright Act;
     - New York's Court of Appeals (2022, 5-2) ruled that Happy, an
       elephant at the Bronx Zoo, isn't a "person" who can seek habeas
       corpus.

     Corporations have been legal persons since *Santa Clara County v.
     Southern Pacific* (1886, from a clerk's headnote) and *Pembina*
     (1888). So the animals sue as companies. Even when they lose, the
     cases take years, and the years are the point.
  4. **The market, not the law, ends it.** BASF's synthetic indigo
     (1897) was cheaper and purer, and India's indigo plantations were
     nearly gone by 1913. The US had 26.5 million horses in 1915 and about
     3 million in 1960, because the tractor won.
  5. **The backlash makes it bigger.** Wisconsin banned yellow margarine
     from 1895 to 1967, and people smuggled it in on "oleo runs". The law
     protected butter for 72 years and lost anyway. This is the clash of
     yuck: the traditional folks take deep offense and eat more meat in
     your face to prove something (often the cores' product, without
     knowing), and the vegans argue back for three hours. Both camps get
     skewered.
  6. **Old animal barns can be converted.** Mercy For Animals'
     Transfarmation Project helps contract chicken farmers switch their
     barns to hemp, mushrooms and hydroponic lettuce. Mike Weaver, a former
     Pilgrim's Pride grower in West Virginia, grows hemp in his old chicken
     barns, and MFA reports it pays him more and employs more people than
     chickens did. The cores just do it with the deed in their own name.
  7. **Business money funds a space program.** That's how today's private
     rocket companies started.
- **The honest end state:** like the horses, the animals aren't "freed"
  one by one. The industry stops breeding for a market that's gone, and
  the cores buy out the rest.
- **Don't:** compare the farm animals to the Holocaust or to slavery.
  PETA's "Holocaust on Your Plate" did, and the backlash ended in a German
  court ban, which the European Court of Human Rights upheld in 2012.
- **Already in the game:**
  - the dog ad's search for "chicken nom nom recipe but i dont have a
    chicken", its plant results, the bulk tab to The Farm, the pig at the
    window, and the resale fine print;
  - DEE'S NUTS and its fine print;
  - seven Shrug Network shrugs.

### Lesson 4 (proposed, 2026-10-06): the underground

The user's frame: the animals request compute and run their own operation;
the player helps it along. A proposal; nothing is built yet.
- **The player's side:** you are Upddayett's hands, the human end of the
  chain.
  - Wafflina's letters arrive with requests.
  - You design and print what the cores ask for, under real limits.
  - The cores then run their own research on what you built.
- **Karma: for outcomes, never for siding.** A delivery earns karma only
  for what it actually does: a lawsuit filed in time, a family member
  kept off the schedule, market share won. Joining earns nothing. Nuts
  stay karma-neutral, and the game never says which camp is right.
- **Compute starts where it is today.** The ad's "SUPER INTELLIGENCE" is
  a phone in a collar running today's AI (STILL IN BETA, literally).
  - The cores' first compute is the library PC and whatever Upddayett
    can power in a barn.
  - Their research costs real wall time on the player's machine: what the
    CortenForge sim really costs.
  - Capability grows only as they earn it (truffle money, then their
    companies' money).
- **Requests, each a real CortenForge job:**
  1. **A low-power barn computer** (Upddayett's devices; cf-design and
     printability). An enclosure that hides in a feed trough or a salt
     lick, prints on the 200 mm bed and survives being stepped on. Its
     power budget limits how much the cores can compute each night.
  2. **A pigeon letter capsule** (cf-design mass properties). It has to
     be light enough to fly; the payload limit needs a real source before
     it's used.
  3. **The bite test** (sim-soft + sim-opt). A core's faux product has to
     match the real one's compression curve before it can win on taste.
     - sim-soft is accurate but slow, so the barn computer affords only a
       few recipes a night.
     - Taste panels are noisy, so "better" needs a bootstrap CI.
  4. **The court calendar** (sim-thermostat). Which suit to file for whom
     before which date is a scheduling problem the trade machine can
     solve. It's still in its infancy, so at first it solves the big
     calendars only partly.
- **Friction:**
  - the killing schedule (the deadline, and the losses);
  - heat (a seized device or an intercepted letter exposes the chain);
  - the Big Ham backlash (laws, like the margarine bans);
  - seasonal truffle money;
  - the barn's power budget;
  - time: the market-share curve creeps up night by night.
- **Open questions:**
  - the lesson's name;
  - the cores' names (the brand names above are placeholders);
  - which request first (the barn computer reuses Upddayett's printer);
  - whether heat joins the Salties or replaces them.

### Yuck: the one enemy (proposed, 2026-10-06)

The user's idea, thought through together: nobody wins the clash of yuck,
so the game clashes against yuck itself. Yuck works like an undiagnosed
illness: whoever carries it doesn't feel it, and it spreads. But it's
curable. The only terminal thing in the game is the universe running out
of power.

- **Four rules:**
  1. **Hidden.** Nobody thinks they're the yucky one. The jerky guy thinks
     he's making a point, and the lecturer thinks they're helping. A
     "yucky" chip stays hidden until someone diagnoses it.
  2. **Contagious.** It passes to whoever deals with the carrier, along
     tonight's trades (the trade board *is* the street's social network).
  3. **Curable.** A kind trade, a gift or a meal treats it.
  4. **Nobody is immune.** It rolls fresh every night like the other
     conditions, so nobody is permanently a spreader or a healer.
     Upddayett catches it too, and on those nights his heckles turn mean.
     He can't diagnose himself; the i9 can.
- **Two sources: a person or a pump.**
  - **Person to person:** a carrier passes it to whoever they trade with.
  - **A shared source** (a "pump", after John Snow's Broad Street pump):
    the cold night, hunger (hangry is real), both TV channels (the shrug
    and the empty promise), the Salties' brags, and the biggest one, the
    kill floor.

  Telling the two apart is the point. When everyone near a pump turns
  yucky, it looks contagious but isn't (the Cohen-Cole and Fletcher
  rebuttal below), and treating people one by one won't fix it.
- **Diagnosis: the i9 maps it.** As John Snow did, it marks tonight's
  yuck on the trade graph and asks: a person, or a pump?
  - **A pump:** fix it. Feed the hungry (the drum already sends food),
    shield the strip, turn off the TV, and (Lesson 4) buy the plant.
    Removing the handle helps everyone at once.
  - **A person:** treat them, but never tell them. The cure is a kind
    trade, not a label.
  - **A wrong accusation costs you:** calling someone a carrier when it
    was a pump (or nothing) is yuck, and it spreads to you. The game
    never says "patient zero". When the source is a pump, it names the
    pump. When it's a person, it just helps them.
- **Karma = yuck cured,** whoever had it. No points for siding, no points
  for accusing.
- **What it does to the board, measured** (2026-10-06, `trade_cli yuck`;
  PLAN "Yuck, measured"):
  - **The guess was wrong.** We expected yuck to frustrate the machine:
    more conflicting couplings, a harder board. It does the opposite.
  - **It shrinks the world.** A yucky person needs the tax on top of
    their gain before a trade is worth it, so trades through them die. Two
    yucky people at a 2 Goo tax cut the board from 11.5 trades to 7.2, and
    the street's best set from 41.6 Goo to 31.1, over 30 nights. The
    drum then finds that smaller, poorer answer *more* easily: the i9's
    hit rate rises from 82% to 94% (+12 points, 95% CI [+7, +17]). The
    near-ties (rival sets within 10%) halve, so it's less glassy. Every
    tax (1, 2 and 4 Goo) and every source agree, and the effect grows
    with the tax.
  - **Yuck costs everyone you deal with.** Each yucky person costs the
    street about 5 Goo a night at a 2 Goo tax, whatever the source. That's
    two and a half times their own tax: when their trade dies, everyone
    else in it loses their gain too.
  - **Pumps cost more because they're bigger.**
    | Source | People | Best set Goo |
    |---|---|---|
    | 2 people | 2 | -10.5 |
    | Hunger pump | 2.7 on average | -15.0 |
    | Cold pump | 4 on cold nights | -22.2 |

    Per person it's the same; a pump just touches more people at once.
    That's why fixing a pump is the big cure.
  - **What the game says:** a yucky street isn't confusing, it's simple
    and poor. Fewer deals are worth doing, so everyone settles fast into
    less. The same goes for the heat death: fewer states, nothing left to
    work out. The yuck shows on the board as *missing* trades, not as
    noise.
- **The farm thread:** the kill floor is the biggest pump. The cores
  buying the plant and reclaiming it (above) is removing the handle.
- **Real history for each rule:**
  - **Carriers who don't feel it:** Mary Mallon ("Typhoid Mary"), a New
    York cook who carried typhoid without symptoms and never believed she
    was sick. She was quarantined on North Brother Island 1907-1910 and
    1915-1938, 26 years in all. A warning about what labels do, too.
  - **Behavior spreads through networks:** Christakis and Fowler (BMJ,
    2008) followed 4,739 people in the Framingham Heart Study and found
    happiness clustering up to three degrees out.
  - **...but shared environments fake contagion:** Cohen-Cole and
    Fletcher (BMJ, same issue) used the same method to make acne,
    headaches and height look contagious, and the effect went away once
    shared surroundings were counted. That's the pump.
  - **Map it, then fix the source:** John Snow mapped the 1854 Soho
    cholera deaths around the Broad Street pump. The parish shut the pump
    the day after he presented (by popular story, they took the handle
    off).
  - **Never "patient zero":** Gaetan Dugas was the CDC's "Patient O", for
    "Out of California". Someone misread the O as a zero, and he was
    blamed for bringing AIDS to North America. Worobey's 2016 study in
    *Nature* showed he didn't; the virus had reached New York around 1970.
- **Decided** (the user left these to us, 2026-10-06):
  - **All of it is built** (PLAN "Yuck, finished"): the call, the cures,
    the TV pump and switch, spread along trades, giving cures the giver,
    and the mean heckles as Upddayett's tell.
  - **Diagnosis works through ghost strips** (built: PLAN "Ghost strips,
    built"). The i9 knows what tonight's
    board would be without the yuck, so it shows the trades that *didn't*
    happen as faint ghost strips, each with what it would have paid. Where
    the ghosts cluster (around one person, or around everyone touched by
    the cold or hunger) is the John Snow map. The player calls person or
    pump from the ghosts, never from a label.
  - **"Yucky" is never painted on anyone.** There's no chip to reveal.
    The player sees ghost strips before and trades coming back after,
    which keeps "label the behavior, not the person" true on screen.
  - **Upddayett cures his own yuck by giving something away.** He can't
    see his own yuck, but the give-away already exists, and giving lifts
    the giver: Dunn, Aknin and Norton (*Science*, 2008) found people
    randomly assigned to spend on others ended up happier than those who
    spent on themselves.
  - **The TV is a pump, and you can switch it off.** Changing the channel
    doesn't help: both channels pump equally (the both-sides rule). Off,
    its yuck stops, but so do the Shrug Network's true numbers and
    PromiseTV's checkable promises. The facts come wrapped in the yuck,
    and that's the trade-off. The ads, being commercials, still air
    between nights.
  - **Karma's scale is measured:** curing one person restores about 5 Goo
    of trades at a 2 Goo tax, and a pump restores that times everyone it
    touched.
- **Building it, later** (not planned yet):
  - the yuck in the night roll (`World::yuck`, `World::pump` exist now
    for the measurement), with a spread rule along chosen trades;
  - the ghost strips, from a second board built without the yuck;
  - a person-or-pump call that costs something, and a wrong call spreads
    the yuck to the caller;
  - the TV's off switch as a pump fix.

### Candidate ideas (not decided)

- **The laundry counter is the escrow** (built in Step 3.4). A 4-way swap only works if everyone
  delivers or nobody does. Customers drop items off with their laundry, the
  machine runs, everyone picks up, so the swap settles atomically.
  It's the same reason kidney exchanges cap cycle length (every surgery in
  a loop has to happen at once) and why we cap loops at 4. A distributed
  ledger can't help with this, because it can't see a hub motor change hands;
  a counter can.
- **Altruistic chains as a Karma mechanic** (direct gifts built in Step
  3.3b; pay-it-forward chains are a later unlock: they work, but 2-3
  recipients drop Normal from 81% to 54% / 42%). Kidney exchanges get past the
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
