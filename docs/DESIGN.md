# Lattice design notes

## Lessons from the first attempt

The iced prototype (`mochreach/lattice-legacy`, 2023 to January 2024) was reviewed in October 2026
before this restart. What went wrong, so we don't repeat it:

- **The clock lived in the UI.** Steps were driven by an iced timer subscription, and each step
  reset to the time the tick happened to arrive (`last_step = now`). As a result, lateness
  accumulated and tempo always ran slow. Notes then crossed two threads and landed at whatever
  buffer boundary came next.
- **One audio stream per instrument.** Each synth opened its own cpal output and slept in a loop.
  Mixing was left to the OS, and there was no master bus.
- **Level-sensitive triggers.** Instruments watched a shared `trig` value going from -1 to +1. A
  note-off followed by a note-on in the same tick could cancel out, so repeated notes didn't
  retrigger.
- **The GUI went all the way down.** Song, grid, track and step each had an iced `view()` and
  imported the UI's message type. The cursor computed pixel scroll offsets.
- **Playback state lived inside the song data.** Playheads and repeat counters were part of the
  model, and some of them ended up in save files.
- **Ad hoc modes.** Visual mode, the help screen and the command bar were each a separate flag.
  Keybindings were hard-coded maps, `example_config.toml` was never loaded, and there was no undo.

What was worth keeping (as ideas, not code):

- The `Song → Grid → Track → Step` structure, with a separate length for each track (polymeter).
- Effects as columns on each step.
- Typed editor actions with built-in help strings, and a chromatic keyboard layout for note entry.
- Pending-operator modes, auto-step, follow-playhead, a command bar, and `.` to repeat.
- Instrument parameters exposed as named controls, plus MIDI out to drive external gear.

## Principles

1. **The audio callback owns time.** The transport counts samples. Step boundaries are absolute
   sample positions computed from a fractional samples-per-step value, so they never drift and
   don't depend on buffer size. Each buffer is split at event boundaries so events are
   sample-accurate.
2. **Nothing blocks or allocates on the audio thread.** The UI talks to the engine through
   lock-free queues (`rtrb`). Large objects such as replacement patterns are built off the audio
   thread and handed over, and freed off the audio thread too. Debug builds wrap the callback in
   `assert_no_alloc`.
3. **Song data is pure.** `lattice-core` has no UI, audio or playback state. Playheads, pass
   counters and arrangement position belong to the sequencer.
4. **The editor is a library.** `lattice-editor` takes its own `Key` type and returns effects
   (send MIDI, transport, save, quit) for the frontend to carry out. It is tested by feeding in
   key sequences.
5. **Deterministic playback.** Anything random is seeded from (song seed, grid, track, pass), so a
   song sounds the same every time it plays.

## Pattern transforms (the "playfield" idea)

A track has a list of **rules**, each pairing a **transform** with a **condition**. Before
each pass through the track, the sequencer calls

```rust
realise(track, pass) -> Vec<Step>
```

`realise` applies, in order, every rule whose condition holds for that pass. The written
pattern is never changed. The UI can show either the realised pattern or the source.

Transforms include `Rotate`, `Drift` (rotation that builds up each pass), `Reverse` and
`Transpose`. Planned: seeded thinning and fills, octave jumps, swaps, per-step conditions
(Elektron-style `1:4`, `FILL`, `PRE`), and arrangement rules that replace the old global
"repeat grid" flag.

## Instruments and plugins

**Plugin support is non-negotiable, and the standard is CLAP.** It's an open specification
with a sample-accurate event model and per-note parameter changes, and it suits Linux. We'd
rather build on and contribute to an open standard than depend on a closed one. Hosting uses
`clack-host`. VST3 can come later; LV2 only if it's needed.

**Built-in engines, effects and hosted plugins share one interface,** modelled on CLAP's
processing:

```rust
trait Processor: Send {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]);
    fn params(&self) -> &[ParamInfo] { &[] }
}
```

The buffers hold the input on entry (silence, for an instrument) and the output on return. The
sequencer turns steps into events (note on/off, parameter changes) stamped with their sample
offset in the block, and each processor renders the whole block. Parameters are described by
`ParamInfo` (id, name, range, default, unit) and set by `Param` events in their own units, as in
CLAP, not normalised. Processors smooth their own parameters (`Smoothed`, a 20 ms linear glide) so
changes never click. This has several consequences:

- **Effect columns:** a step's effect columns become timestamped parameter events, like Elektron
  parameter locks, whether the target is a built-in engine or a plugin.
- **Exporting:** the built-in engines can later be packaged as CLAP plugins with `clack-plugin`,
  for use in other software.
- **Plugin windows:** plugins with their own interfaces open them in separate windows alongside
  the terminal.
- **Crash isolation:** later, plugins may run in a separate process so a crashing one can't take
  Lattice down.

## Built-in engines: opinionated, with character

Lattice is a personal instrument first. The built-in engines should be interesting and quirky,
in the spirit of Teenage Engineering's gear: powerful enough, but not a wall of options.

- **No generic synth with the usual controls.** Each engine's controls should be specific to
  what it is. For example, rather than a sub-oscillator level, a control that sweeps through
  different kinds of sub-oscillator. A filter or envelope appears only where it suits the
  engine.
- **A small set of main controls per engine,** each doing something characterful. Deeper
  parameters can be opened up later without changing the main ones.
- **Physical modelling is a priority:**
  - modal synthesis (struck bars, bells, plates and glass)
  - plucked and bowed strings (waveguides)
  - membranes
  - blown pipes
- **References:** Mutable Instruments' Rings, Elements and Plaits (MIT-licensed) for
  opinionated models with few controls, and the OP-1's engines for character.

The current `Synth` is a plain test instrument for exercising the engine, not a flagship.

### Rhythmicon, the drum engine

The first engine, named after the Rhythmicon, the first drum machine, which Leon Theremin built
for Henry Cowell in 1931. It has five drums, each an instrument of its own (`:inst kick`), so one
track plays one drum and keeps its own rules, length and mixer strip:

| Drum | Sound | Controls |
| --- | --- | --- |
| `kick` | a sine whose pitch drops from up to 12× its tuning, held at full level for part of its decay (up to 80 ms) before it falls, with a 2 ms click of noise, through off-centre `tanh` drive for even harmonics | model, tune, punch (how far and fast the pitch drops, and the click), decay, drive |
| `snare` | two shell modes (1 and 1.59×, a membrane's first two) with a small pitch drop, driven, and a burst of high-passed noise for the wires | model, tune, decay, snap (noise against the shell), tone (the noise's brightness) |
| `hat` | the 808's six detuned square waves, mixed with noise, through a high-pass | model, tune, decay (short closed, long open), tone (the high-pass), noise |
| `clap` | bursts of noise through a band-pass, a spread apart as hands never quite clap together, with a tail fading in under them | model, tune (the band), decay (the tail), spread, room (the tail's level) |
| `tom` | a sine bending down onto its pitch, through `tanh`, with a short burst of softened noise for the stick | model, tune, decay, bend, noise |

Each drum's first control is its **model**, a choice that changes the drum's insides; the other
controls keep their meaning within it, so a kick's punch is still its punch in every model. The
middle model is the default. Models are named for how they sound rather than after machines:

| Drum | Models |
| --- | --- |
| `kick` | `tight`: no hold, a shorter decay, a faster, shallower drop and more click, the small clicky kick of a Pocket Operator · `boom`: an 808's, holding for a fifth of its decay · `hard`: a 909's, a deeper, faster drop, a loud click and extra drive |
| `snare` | `tight`: a clean, short shell and noise high-passed at 900 Hz · `fat`: a driven shell that rings nearly as long as the wires, with noise kept down to 180 Hz · `crack`: a 909's, a bigger pitch drop, a hard-driven shell and louder, brighter wires |
| `hat` | `metal`: the squares multiplied in pairs (XOR, as the 606's and 909's do) for a clang · `ring`: the 808's, the six summed · `sizzle`: mostly noise, through a steeper 18 dB/octave high-pass |
| `clap` | `tight`: two quick bursts and little tail · `room`: an 808's four, slightly uneven, and its tail · `crowd`: nine bursts at random gaps through a wider band, a room clapping out of time |
| `tom` | `tight`: short, a small bend and more stick · `round`: an 808's, a long, gentle bend · `hard`: a 909's, a second shell mode, a deeper, faster bend and drive |

Each model is a table of constants (`KickModel` and so on) the voice reads, levelled so each
model's hit peaks at or below 1 and the three sit at much the same loudness.

Each drum is one voice: every note retriggers it, note-offs are ignored, and it rings out by its
decay. C-4 (`DRUM_ROOT`) plays it at its tuning and other notes tune it up or down. Velocity
scales the level. Settings apply from the next hit, except decay, which also changes the hit
that's ringing. Decay is the time to fall by 60 dB. The drums share a `Drum<V>` processor, so
each `Voice` is only its trigger and a per-sample tick; they're scalar, as one voice is cheap.

A choke (`EventKind::Choke`, after CLAP's note choke) fades whatever a drum is playing over a
few milliseconds, quick but not a click; the synth releases its voices. See choke groups, below.

### Monochord, the string engine

The second engine, named after the monochord, the single string over a movable bridge with which
Pythagoras worked out the ratios of musical intervals. One instrument, `:inst string`, with eight
voices, each a string:

- **The string** is a waveguide in two halves, as in the Synthesis ToolKit's `Bowed`: one delay
  line from the bow to the bridge and back, one from the bow to the nut and back, with the bow
  12.7% of the way from the bridge. Each end inverts the wave; the bridge's end also has a
  two-point low-pass, `(1 - d)·x[n] + d·x[n-1]`, and a loss. The bridge's line is read with
  third-order Lagrange interpolation, so the string plays in tune without dulling the top, and is
  `d` shorter to make up for the low-pass's delay. The loss per trip is set so every string fades
  by 60 dB over the decay, whatever its pitch; the low-pass then makes high notes and high
  harmonics die sooner, as on a real string. Tests check plucked pitches from A-1 to A-6 are
  within 3 cents, and a bowed A-3 within 10.
- **A pluck** fills both lines with one period of noise, low-passed from the note itself (a soft
  thumb, nearly a sine) up to near Nyquist (a hard pick) on an even, logarithmic sweep, brighter
  for harder notes, then subtracted from itself a fraction *position* of a period on, which
  notches out the harmonics with a node where the string is plucked. Its mean is removed and its
  level set by its RMS, so pluck and position change the tone, not the loudness. A hard pick also
  clicks: a 1.5 ms burst of differenced noise outside the string, as loud as the pluck squared.
  Plucking a string that's still ringing keeps 30% of it.
- **A bow** is STK's friction junction: where the lines meet, the bow's speed less the string's
  gives the slip, and the friction curve `min(1, (|(slip + 0.001)·grip| + 0.75)^-4)` turns it
  into a push into both lines. Slow slip sticks, so the bow drags the string along until it
  snaps back: Helmholtz motion, the sawtooth of a real bowed string. The bow setting raises its
  speed (0.06 to 0.3) and its grip (slope 2 to 4.2) together, so light bowing skates and heavy
  bowing grips and grits; its speed wavers by 5%, as a hand's does. The string's velocity at the
  bridge is a train of pulses, so, as far as the bow is on the string, the output is tilted down
  6 dB an octave from 1.5 times the note, into the sawtooth's spectrum. The bow glides on and off
  over about 30 ms, takes the place of the pluck as it's turned up, and swells for a second or
  two on low, long-ringing strings before it holds. A note-off lifts the bow; a plucked string
  rings on by its decay, as a harp's does. A choke fades every string out, as it does a drum.
- **The models** set the pluck's brightest, the loop's least damping, and two things a plain
  string doesn't do. `steel` plays each note on a pair of strings 7 cents apart, spread a little
  left and right, so they beat. `sitar` shortens its bridge's line by up to 6% when the string's
  displacement is large, as a sitar's string does against its curved bridge (jawari): a buzz that
  brightens the attack and settles as it fades. `steel` has a trace of the same, for the slight
  fall in pitch of a hard pluck.

| Control | Range | |
| --- | --- | --- |
| model | `nylon` `steel` `sitar` | |
| pluck | 0–1 | soft thumb to hard, clicking pick |
| position | 0.02–0.5 | near the bridge, thin and nasal, to the middle, hollow |
| decay | 0.2–20 s | before the low-pass's extra loss |
| tone | 0–1 | how much of the top survives each trip |
| bow | 0–1 | 0 only plucks; above, bows while held, light to heavy; 1 only bows |

Decay, tone and bow change the strings already sounding; the rest apply from the next note. The output
goes through a DC blocker, since the buzz can shift its centre. Each voice is scalar: a string's
work is two reads, a filter, the friction curve while bowed and two writes per sample, cheap
enough not to need SIMD lanes.

## Track groups

A group is a named run of adjacent tracks, such as a drum kit with one drum on each track. Each
track stays a real track, with its own rules, length and strip; the group adds:

- **Folding (done).** A folded group narrows each of its tracks to two cells and shows each hit
  as a glyph by velocity (`●` 110 and up, `•` 80 and up, `∙` below), like a drum machine's step
  view, so a kit takes little room. The header keeps each track's initial, length, a mark per
  rule and its meter; level and pan are on the mixer page. Editing works folded as unfolded: the
  cursor moves between the narrow columns. `tz` folds and unfolds, borrowing Helix's `z` for
  view, and is undoable, as the fold is saved with the song. The pattern page lays columns out by
  their widths, scrolling so the cursor's track stays in view.
- **A group bus (done).** The group's tracks mix into a strip of their own, with a level and an
  effect chain, before the master: a drum bus for compression and drive. The mixer page shows
  each group's bus after the tracks, beside the send and master buses, in its first track's
  colour. Sends still go straight from each track to the send bus.
- **Choke (done).** Tracks in a group can cut each other off, so a closed hat stops an open one.
  Each track has a choke number (`Track::choke`, 0 for none, up to 8), set with `:choke N` or
  the mixer page's `choke` row. When a track hits, the engine sends a choke, at the same sample,
  to the other tracks in its group with the same number. Tracks that hit on the same step don't
  choke each other, so a hat and an open hat played together both sound. The number only counts
  within a group; an ungrouped track never chokes. Previews choke too, so the kit plays by hand as
  it does from the pattern.
- **Drum pads on the numpad (done).** Pad `n` plays the `n`th track of the cursor's group at
  C-4, a drum at its tuning, so the kit can be played like a pad controller; in insert mode it
  also enters the note on the cursor's step, without moving on, so a step can take several
  drums. Terminals send numpad digits as plain digits unless asked for more: when the terminal supports the kitty protocol
  (also in WezTerm, foot, Ghostty and recent Alacritty), the TUI asks for every key as a code
  (`REPORT_ALL_KEYS_AS_ESCAPE_CODES`: disambiguating alone still sends the numpad's digits as
  text), with the shifted character (`REPORT_ALTERNATE_KEYS`, so `%` and `C` arrive as typed)
  and with repeats and releases (`REPORT_EVENT_TYPES`). Digits that arrive with the keypad state
  become `Key::Pad`. Repeats of keys that play notes (`Editor::plays`) are dropped, so a held pad
  or piano key plays once, while held motions still repeat. With releases, a note key's preview
  is `Effect::Hold` rather than `Effect::Preview`: the engine sounds it until `Command::Release`
  (15 s at most, in case the release is lost), and a track can sound up to eight at once, so
  chords ring. The editor keeps which key holds which note and emits `Effect::Release` when that
  key comes up. Only note keys hold, as they're plain keys whose releases match their presses;
  an edit's preview (`C-a`, `>`) stays at 0.3 s, since its modifier may come up first. On the command line they type digits as usual. The fallback, for
  other terminals and inside tmux: in insert mode in a folded group, the number row (`1`–`9`)
  is the pads instead of the piano's sharps, as a folded group is a drum machine's view.

- **More of a pad controller (done).** Outside a group the pads play a scale on the cursor's
  track (`:scale`, from `SCALES`, with a root), degree `n - 1` up from the keyboard octave, so a
  bass or string part can be played one-handed. The numpad's `/` `*` `-` set `Editor::velocity`
  (60, 100, 127), which every note played or entered takes, piano keys too; `+` switches the
  pads to muting the first nine tracks, an undoable change like `tm`, for building a song up and
  down live. The non-digit keys arrive as `Key::PadOp`. The pads live in `pads.rs`.
- **Isomorphic keyboards (done).** Numpads often register only three keys at once, so chords
  belong on the main keyboard. `:layout wicki` and `:layout fourths` replace the tracker piano
  with a grid over the four rows (`NoteLayout`): a key's pitch is `row * 5 + column * step`, a
  step of 2 (Wicki–Hayden, where the rows' leftward stagger makes up-left a fourth and up-right
  a fifth) or 1 (fourths, as on a guitar). `[` `]` are left out so they still change octave.
  `:layout scale` is the same grid in the steps of the pads' scale rather than semitones, from
  its root: a step to the right, and a row up the number of steps nearest a fourth (3 of seven
  notes, 2 of five or six). Setting `:scale` lays the keys out again.
- **Note repeat (done).** Holding the numpad's `0` makes the pads played meanwhile repeat, every
  1, 2 or 4 steps (`.`), as on an MPC. The editor emits `Effect::Repeat`, held like a note key's
  preview, and the engine's `Command::Repeat` is a preview with an `every`: it plays it again on
  every step that's a multiple, after the note-off. The engine's clock now runs while stopped,
  only sequencing while playing, so repeats keep time either way and, playing, fall on the song's
  grid (a count-in is whole bars, so it doesn't shift it). A pad or `0` coming up ends the
  repeats. Recording, the first hit goes in like any note, and the rest are written when the pad
  comes up, on each step of the grid it was held through. It needs key releases, as a repeat
  without its release would never end.

`tg` groups the tracks the selections reach, and ungroups them if they're already a group. Groups
are saved in the song and undone like everything else.

**How groups are kept (done).** `Grid::groups` holds each group as a run of track indices (`first`,
`len`), in track order and never overlapping, with its name, whether it's folded and its bus. Tracks
are inserted, removed and moved only through `Grid::insert_track`, `remove_track` and `move_track`,
which keep the groups in line: a track inserted between two members joins; at either end it stays
out; a member moved within its group stays in, and leaves when it moves past either end; a group
whose last track goes goes with it. At most `MAX_GROUPS` (8), as the engine preallocates a bus for
each: `Engine::groups` holds a `GroupBus` per slot (chain, gain and buffers), and each
`TrackPlayer` carries the slot it mixes into, set with `Command::SetGroup`, so the route moves
with the track. A bus runs only when a track feeds it or it has effects (whose tails ring out).
The TUI's `Mixer` uses a group's index in `Grid::groups` as its slot, and syncs routes by track
id, chains and levels as it does the other buses. New groups are named `group N`; `:group-name` renames one. The pattern page draws a bracket
with the group's name over its tracks.

**Names.** Each engine is named with an indirect nod to a pioneer of electronic music: a studio,
a record or an idea rather than the person's name, so it doesn't read as an endorsement. The name
should hint at the engine's character. The docs can say where each name comes from. Candidates:

| Name | Nod to | Engine idea |
| --- | --- | --- |
| Folly | Tower Folly, Daphne Oram's studio | drawn wavetables |
| Maida | Maida Vale, home of the Radiophonic Workshop (Delia Derbyshire) | test oscillators and tape |
| Switched | *Switched-On Bach* (Wendy Carlos) | big modular, ladder filter |
| Nemo | Nemo Studios (Vangelis) | expressive brass and pads, shaped by pressure |
| Oxygen | *Oxygène* (Jean-Michel Jarre) | string machine and phaser |
| Seven | *Seven Waves* (Suzanne Ciani) | west-coast: wavefolding, low-pass gates, plucks |
| Oblique | Oblique Strategies (Brian Eno) | generative pads that evolve on their own |
| Snowflake | *Snowflakes Are Dancing* (Isao Tomita) | orchestral imitation, whistles and choirs |
| Kepler | Laurie Spiegel's piece on the Voyager Golden Record | harmonic, slow-moving tones |
| Klang | Kling Klang (Kraftwerk) | vocoder and speech |

Physical-modelling engines (bars, strings, membranes, pipes) will want names in the same spirit.

## Live recording (done)

`R` or `:rec` arms recording, and arming starts insert mode. While the song plays, a piano key or
pad in insert mode writes its note at the playhead of the cursor's track (and of each other
selection's) rather than at the cursor, which stays put. The editor doesn't know about the engine,
so before each key the frontend gives it a `Transport`: every track's step and pass, from
`Status`, and how far into the step playback is. The engine only publishes whole steps, so the TUI
times that fraction itself, from when it saw the first track's step change against the step's
length at the current tempo. It looks every frame (16 ms), so the fraction errs a little early,
which roughly cancels the audio's own latency, as players press a little late to what they hear.

The note goes on the nearest step. Rounded back, the step has played, so the note is previewed
now; rounded on, it isn't, and the sequencer plays it on the next step, since `SetTrack` is heard
from the current step on. Each pass through the primary track is a take with one undo
checkpoint, so `u` removes the last pass's notes together; any other edit, undo or stopping
starts a new take. Tracks of other lengths take the step from their own position, so polymeter
records correctly.

With record armed, play starts with a count-in: `:count` bars (one to start, up to four, or none) of
clicks, accenting each bar's first beat, heard even with the click off. `Command::Play` carries the
bars; the engine runs its clock through them without sequencing, so the song's first step lands
exactly a whole number of bars later, and publishes the count-in steps still to come in `Status`,
which the header shows as beats (`IN 4`). The `Transport` carries them too, and a track's position
during the count-in is negative, so a note half a step early rounds onto step 0 and the sequencer
plays it there; one earlier is heard but not entered. The click is now silenced by not sending it
notes rather than by not mixing it, so the count-in's clicks can sound with it off.

Lengths come from key releases. Where the kitty protocol is available the TUI also pushes
`REPORT_EVENT_TYPES`; a release calls `Editor::release`, which sets the length of the notes that
key recorded to the steps it was held (at least one, at most the track). Repeats are ignored while
recording, so a held key isn't recorded again and again. Without releases, notes are one step
long. A preview still lasts its fixed 0.3 s, so a held note isn't heard held until the next pass.

## Performance

How many voices and effects we can run, and on what hardware, depends on performance. So it's
designed in from the start.

**Targets:**
- **Main target:** a laptop.
- **Supported:** Raspberry Pi 4 (64-bit OS).
- **Best effort:** Raspberry Pi 3, with fewer voices and effects. We don't compromise the
  design to fit it.

**Foundations:**
- **SIMD across voices and partials.** Voice state is laid out as structure-of-arrays
  (`f32x8` lanes from the `wide` crate), so eight voices, or a bank of resonator modes, are
  computed together. `wide` compiles to AVX on x86, NEON on ARM, or scalar code where there's no
  SIMD.
- **Block processing, branch-free inner loops.** Blocks are at most 256 frames. Decisions such
  as envelope stage changes are made outside the per-sample loop.
- **Control rate.** Expensive maths (filter `tan()`, envelope stages) runs every 16 samples,
  counted from an absolute sample clock so output doesn't depend on buffer size.
- **Denormals flushed to zero** at the start of every callback (MXCSR on x86, FPCR on ARM).
- **Silence skipping.** Idle instruments do almost no work.
- **Benchmarks:** `cargo bench -p lattice-engine` reports what share of one core each
  instrument needs. Run it on every target machine.
- **Later:** tracks are independent until the mixer, so they can run in parallel on a
  real-time thread pool (not rayon). Raise the audio thread to real-time priority. Limit the UI
  to 30–60 redraws a second.
- **Not GPUs.** Their latency and scheduling don't suit real-time audio at small buffer sizes.

Lay the foundations now, and optimise only where benchmarks show a need.

## DSP

We write our own DSP rather than using fundsp. A tracker's signal flow is fixed:

```
instruments → track inserts → sends → master
```

So we don't need a dynamic audio graph, and owning the code makes the real-time guarantees easy
to check. Things to get right:

- **Oscillators:** band-limited (PolyBLEP/PolyBLAMP), wavetables later.
- **Envelopes:** triggered by events, never by watching a level.
- **Filters:** a TPT/ZDF state-variable filter, later a ladder filter.
- **Effects:** drive (to be oversampled), a compressor, delay, and a reverb (a Freeverb or FDN to
  start).
- **Housekeeping:** parameter smoothing, polyphony with voice stealing.

fundsp and similar libraries are still useful as references, and as reference output in tests.

## Releases (planned)

Not needed until there's something to release; this is the plan for when there is.

- **Prebuilt binaries on GitHub Releases**, built by GitHub Actions when a version tag is pushed,
  one archive per target:

  | Build | For |
  | --- | --- |
  | `x86_64-linux` | most Linux laptops and desktops (baseline, SSE2) |
  | `x86_64-linux-v3` | the same with AVX2 (x86-64-v3), about 1.6× faster DSP |
  | `aarch64-linux` | Raspberry Pi 4, and the Pi 3 on a 64-bit OS |
  | `aarch64-macos`, `x86_64-macos` | Macs, if wanted |

- **An install script on top** (`curl … | sh`) that checks the OS and architecture and downloads
  the matching archive, and on x86_64 picks the v3 build if `/proc/cpuinfo` lists `avx2`.
- **Use `dist`** (formerly cargo-dist) to generate the workflow, build matrix, checksums and
  installers rather than writing them by hand. Check whether it can choose between CPU-level
  variants such as v3; if not, extend its installer or write a small script of our own.
- **Linux details:** binaries link the system ALSA library, which is fine on desktop Linux and
  Raspberry Pi OS. Build against an older glibc (an older Ubuntu runner, or `cargo-zigbuild`) so
  binaries run on older distributions.
- **Building from source** stays supported: `cargo install`, or a package in the Nix flake, with
  `-C target-cpu=native` for a build tuned to the exact CPU. The repository's
  `.cargo/config.toml` sets this for every local build, so release builds must set `RUSTFLAGS`
  (which replaces it) to their CPU level.
- **AVX2:** shipping a separate v3 build chosen by the installer replaces switching at runtime,
  which `wide` can't do (it picks SSE2 or AVX2 at compile time) and which would mean rewriting
  the DSP on a library such as `pulp`.

## Stack (versions as of October 2026)

| Purpose | Crate | Version |
| --- | --- | --- |
| Terminal UI | ratatui | 0.30 |
| Terminal backend | crossterm, with the kitty keyboard protocol | 0.29 |
| Audio output | cpal | 0.18 |
| SIMD | wide | 1.7 |
| Plugin hosting | clack-host (CLAP) | 0.2 |
| UI ↔ audio queues | rtrb | 0.4 |
| MIDI | midir | 0.11 |
| MIDI messages | wmidi | |
| Song files | serde, ron | 1.0, 0.12 |

**Key-release events:** most terminals send key presses only. Kitty, foot, WezTerm, Ghostty and
Alacritty also send releases through the kitty keyboard protocol. Note entry must work with
presses alone, so holding a key to sustain a note is an extra for terminals that support it. A
MIDI keyboard avoids the problem entirely.

## First milestone (done October 2026)

- One cpal stream, one polyphonic band-limited synth, and the sample-counted clock, checked
  against a metronome click.
- `realise()` covered by tests for each transform and each pass number.

## Second milestone (done October 2026): the terminal UI

- `lattice-editor`: normal, insert, select and command modes, counts, undo and redo, and
  `:` commands for rules, length and tempo. It's tested with key sequences.
- The engine has a transport and takes commands (play, stop, tempo, replace a track, preview a
  note) over an `rtrb` queue. Replaced tracks go back to the UI thread to be freed. Playheads
  and pass numbers are published through atomics.
- `lattice-tui`: a ratatui grid with one colour per track and a playhead for each track (so
  polymeter is visible), plus a live view that shows the pattern realised for the current pass
  and highlights what the rules changed.

Colours come from a `Theme`. The default uses the terminal's own 16-colour palette and default
foreground and background. Highlighted blocks (cursor, playheads, badges) use reverse video, so they
stay readable on light and dark terminals alike. At startup Lattice asks the terminal for its actual
background and foreground colours (OSC 10/11) and shades beat rows a few percent from one towards
the other, and the selection further; if the terminal doesn't answer, beat rows are left
unshaded and the selection is dark grey. Themes that set every colour can come later.

Editing follows Helix rather than vim: select, then act. The selection is always present, a
rectangle across tracks and steps, so every action shows what it will touch before it happens,
and edits such as transposing keep the selection so they can be repeated. Prefix keys (`g`, `r`)
open an infobox listing what can follow, which makes the keymap discoverable as it grows. Keys
are data (`Keymap`, `Binding::Menu`), so new menus need no new code in the frontend.

There can be several selections, as in Helix: a list of rectangles, one of them primary, kept in
order down the grid and merged when they overlap. Motions and edits apply to each, undo restores
them all in one step, and yanking keeps one value per selection. `s` narrows the selections to the
steps whose notes match a pattern, the grid's stand-in for Helix's regex; it reuses the command
line's prompt and previews the result as you type. Helix's `C` copies a selection onto the next
line; here it's the rows below, offset by the selection's height. There's no sideways copy onto
the next track: a selection already spans tracks, so widening it does the same job.

Tracks can be added, removed and moved while playing. The engine keeps room for `MAX_TRACKS` (16)
players, each a boxed pattern, instrument and set of buffers. The UI thread builds a player and
sends it over the command queue, and removed players go back over the garbage queue to be freed,
so the audio thread never allocates or frees. Positions are published per track index, so the
engine republishes them for every index that shifted. A track inserted mid-song joins in step, as
though it had been playing from the start.

The editor gives each track a `TrackId` that stays with it, including through undo, which
snapshots the ids along with the grid. Changing the set or order of tracks produces
`Effect::TracksChanged`, and the frontend works out the engine commands by comparing the engine's
ids with the editor's. Instruments are part of each track (`Track::instrument`, a kind and its
settings, like an effect), so a track brought back by undo, or pasted, sounds as it did. The
frontend keeps the instrument it last sent each engine track: if the kind has changed it builds a
new one and sends `SetInstrument`, and the old one comes back over the garbage queue; otherwise
it sends `SetParam` for each setting that differs. New tracks get the test synth.

## Mixer and effects

The signal path, all inside `Engine::process`:

```
instrument → track effects → strip (level, pan, mute/solo) ─┬────────────────→ master bus
                                                            └→ send ──→ send bus effects ─┘
master bus → master effects → + click → tanh
```

- **Strips** are per track and stored in the song (`Track::mix`: level and send in dB, pan from -1
  to 1, mute, solo), so they're saved, undone and synced like the notes: `SetTrack` carries them,
  and the engine glides each strip to its settings once per block. Mute and solo go through the
  same smoothed gain, so they fade in and out over 20 ms instead of clicking. A send at -60 dB is
  off, which is the default.
- **Pan** is a balance control: centred is unity gain on both sides, and panning turns the other
  side down along a quarter cosine. Instruments are stereo, so a constant-power law would only
  make centred tracks quieter.
- **The send is post-fader,** so muting a track mutes its echoes too.
- **Effect chains** (up to `MAX_EFFECTS`, 8) exist per track and on the send and master buses.
  `InsertEffect` and `RemoveEffect` work like tracks do: the slot is built off the audio thread
  and sent over, and removed ones come back over the garbage queue. `SetParam` targets an
  instrument or an effect slot and arrives as a `Param` event at the start of the next block.
- **Meters:** each track's peak after its strip is published as `f32` bits in an `AtomicU32`
  with `fetch_max`; the UI swaps it for 0 each frame and lets its meter fall at 30 dB/s.

There are three effects so far, each usable on any track or bus:

- **`delay`:** a plain stereo delay (time, feedback, a lowpass in the feedback path, and mix).
  New songs get one on the send bus, set fully wet; anywhere else it starts at 30% mix.
- **`drive`:** a saturator: `tanh` with up to +36 dB going in (exponential in the control, so it
  feels even), a little off centre for even harmonics with a 5 Hz DC blocker after, then a
  one-pole low-pass (tone), mix and an output level. It's scaled so a full-scale input still
  peaks at 1. It isn't oversampled yet, so it aliases when pushed hard on bright material.
- **`comp`:** a compressor: threshold, ratio, attack, release, makeup and mix (below 1 for
  parallel compression). Both channels share one gain from the louder of the two, so the image
  stays put. The gain reduction in dB follows a 6 dB soft knee with the attack going down and the
  release coming back.

Together they're a drum bus. The demo's drums are a folded group, `drums`, whose bus has a
little drive (mixed at 50%) and then a compressor with a 10 ms attack, so the hits' fronts get
through, mixed in at 60%.

The chains and the bus levels are part of the song (`Track::effects`, `Song::buses`), so they're
saved and undone with everything else. An effect is stored as its kind and its parameter values,
in the parameter's own units; `ParamInfo` (range, default, unit, linear or log curve) lives in
`lattice-core`, so the editor can show and nudge values without knowing about DSP. After a change
the TUI's `Mixer` brings the engine in line by diffing each chain: it keeps the slots that still
match at either end, removes and inserts the ones between, and sends `SetParam` for values that
differ.

### The mixer page

`gm` swaps the grid for the mixer (`gp` goes back), opening on the pattern cursor's track. It's
in two halves, so a balance is a matter of moving sideways and anything about one strip is a few
keys away:

- **The overview**, above: a column per strip (the tracks, each group's bus, then the send and
  master buses, which always stay in view while the tracks scroll), with its name, level, pan
  and meter. `h` `l` move between strips and `^a` `^x` set the level.
- **The detail**, below, of the strip under the cursor: a rule with its name and where its sound
  goes (`→ drums → master, and → send`; a bus lists what feeds it), a mix row (level, pan, send
  and choke; a bus has only a level), its chain from the instrument through the effects in the
  order the sound passes through them, and the settings of the device on the chain under the
  cursor, always shown, in a grid of three columns filled a row at a time.

`j` `k` move from the overview down through the mix, the chain and the rows of settings, and `h`
`l` move along whichever row the cursor is on. `(` `)` change strip from anywhere, keeping the row
(Helix uses them to cycle selections, which the mixer doesn't have). `tab` jumps between the
overview and the chain. `gi` and `gf` jump to the instrument and the effects, from the pattern
page too, on the cursor's track. `^a` `^x` nudge the value under the cursor (0.5 dB, 5% of pan, 1
of choke, or 1% of a setting's range, with counts), and `c` opens the command line with `set `
ready for an exact value (or `inst ` on the instrument). Settings that pick from a list, like the
synth's wave, use `Curve::Choice` and nudge one choice at a time. `:fx-add` inserts after the
device under the cursor (first, after the instrument) and `d` removes the effect under it. The `t`
menu still works on the cursor's track. The mixer cursor (`MixerCursor`: a strip, a `Row` and the
device shown) is the editor's state, like the pattern's, and isn't saved.

## Song files

Songs are saved as RON in `.lattice` files, written by `lattice_core::to_ron` and read by
`from_ron`, which also rejects songs the editor and engine can't handle (no tracks, lengths outside
1–256, and so on). A file holds the `Song`: its tempo, its grid of tracks, and its arrangement:
the patterns and the order they play in. The tracks carry the steps of the pattern being edited,
and each other pattern has a lane per track, written on one line each. Tracks are written
sparsely, as a length and a map from step index to the steps that aren't empty, and fields at
their defaults (velocity 100, length 1, a rule that always applies) are left out, so a file reads
much like the pattern does:

```ron
#![enable(implicit_some)]
(
    name: "demo",
    bpm: 120.0,
    grid: (
            name: "demo",
            steps_per_beat: 4,
            tracks: [
                (
                    name: "bleep",
                    length: 6,
                    steps: {
                        0: (note: (pitch: 79)),
                        3: (note: (pitch: 75, velocity: 80, length: 2)),
                    },
                    rules: [
                        (transform: Drift(1)),
                        (transform: Reverse, when: Every(n: 2, offset: 1)),
                    ],
                ),
            ],
    ),
    arrangement: (
        patterns: [
            (name: "A"),
            (
                name: "B",
                bars: 2,
                lanes: [
                    (length: 6, steps: {2: (note: (pitch: 72))}),
                ],
            ),
        ],
        current: 0,
        entries: [
            (pattern: 0, repeats: 4),
            (pattern: 1),
        ],
    ),
)
```

As principle 4 says, the editor makes no I/O decisions: `:w` and `:o` return `Effect::Save` and
`Effect::Open`, and the frontend reads or writes the file, then calls `Editor::saved` or
`Editor::load`. The editor keeps a copy of the song as last saved, and the song counts as
modified whenever it differs, so undoing back to the saved state clears the `[+]` marker. Loading
gives every track a new id and returns `Effect::TracksChanged`, so the frontend rebuilds the
engine's tracks, however many the song has (up to `MAX_TRACKS`).

Exporting works the same way. `:export` and `:export-sel` return `Effect::Export` with the
tracks to render and the parts to play on them, each a lane per track and a number of steps: the
song's entries in order, this pattern for some bars, or the selection cut out as a part of its
own, with the other tracks muted. The frontend renders it on a thread of its own, with a fresh
engine and mixer, so playing carries on, cueing each part while the one before plays and then a
stop. It renders on until a second of silence (at most ten seconds), trims that and writes 24-bit
PCM at the audio device's rate.

## Patterns and the arrangement

A song has one set of tracks (instruments, mixer, effects, groups, chokes) and any number of
patterns, each with its own steps and rules for every track: a lane per track. Sharing the
tracks keeps one mixer, and lets delays, reverbs and held notes ring on across a change of
pattern; a different sound for a section is a track left empty in the other patterns.

The pattern being edited keeps its lanes in the grid's tracks, so everything that works on tracks
works on it unchanged; `Arrangement::select` swaps lanes in and out. Adding, removing and moving a
track does the same to every other pattern's lanes. Undo snapshots the arrangement with the grid,
so undoing a change in another pattern goes back to it.

The pattern being edited is always the one playing. A pattern's length is `:bars`, or by default
as long as its longest track, rounded up to a whole bar. (Once round its whole loop, until every
track and rule lines up again, was tried first: a polymetric pattern ran for a minute or more
before the song moved on.) Looping, the engine leaves the tracks running, so polymeters carry on
drifting; a change of pattern starts every track from its top. The frontend's `Sequencer` sends the
engine that length and a cue for when it ends: loop, change to a pattern (its lanes, built off the
audio thread), or stop. The editor decides which, through `Editor::next`: the pattern picked by
hand (which takes over at the next bar rather than waiting out a long loop), or the next entry in
the song. At the change the engine swaps the lanes in, starts every track from its first step and
pass, and counts it in `Status`; the frontend then tells the editor with `Editor::boundary`, which
goes to the new pattern. A `SetTrack` carries the number of changes the editor had seen, so an
edit made in the frame before the editor caught up is dropped by the engine rather than played in
the wrong pattern (the editor keeps it, in the pattern it was made in).

The song page edits `Arrangement::entries`, with its cursor in the editor like the mixer's.
Playing from it calls `Editor::start_song`, which goes to the entry's pattern at once and sets
`song_position` before the frontend's play, so the cue the `Sequencer` sends next follows the
entries. Adding or removing an entry before the one playing shifts `song_position` with it. The
engine publishes the step it's on from the top of the pattern (`Status::pattern_step`), which with
`song_position` places the playhead on the page's strip.

Not in files yet:
- **Changing steps per beat while running.** The engine's clock is set up with it, so `:o` refuses a
  song with a different value; opening it from the command line works.

Next for the UI: effect columns, choosing a track's instrument, a help screen, loading
keymaps from a config file, and themes from a config file.
