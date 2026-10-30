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

**Built-in engines and hosted plugins share one interface,** modelled on CLAP's processing:

```rust
trait Instrument: Send {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]);
}
```

The sequencer turns steps into events (note on/off, parameter changes) stamped with their sample
offset in the block, and each instrument renders the whole block. This has several
consequences:

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
- **Effects:** drive/waveshaping (with oversampling), delay, and a reverb (a Freeverb or FDN to
  start).
- **Housekeeping:** parameter smoothing, polyphony with voice stealing.

fundsp and similar libraries are still useful as references, and as reference output in tests.

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

## Song files

Songs are saved as RON in `.lattice` files, written by `lattice_core::to_ron` and read by
`from_ron`, which also rejects songs the editor and engine can't handle (no tracks, lengths outside
1–256, and so on). A file holds the `Song`: its tempo, grids and arrangement. Tracks are written
sparsely, as a length and a map from step index to the steps that aren't empty, and fields at
their defaults (velocity 100, length 1, a rule that always applies) are left out, so a file reads
much like the pattern does:

```ron
#![enable(implicit_some)]
(
    name: "demo",
    bpm: 120.0,
    grids: [
        (
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
    ],
    order: [
        0,
    ],
)
```

As principle 4 says, the editor makes no I/O decisions: `:w` and `:o` return `Effect::Save` and
`Effect::Open`, and the frontend reads or writes the file, then calls `Editor::saved` or
`Editor::load`. The editor keeps a copy of the song as last saved, and the song counts as
modified whenever it differs, so undoing back to the saved state clears the `[+]` marker.

Not in files yet:
- **Instrument patches.** They're still defined in `lattice-tui`'s `demo.rs`, and track `i` plays
  the demo's instrument `i`. A song can have fewer tracks than there are instruments (the rest
  are silenced), but not more. Patches belong in the song once instruments can be chosen per
  track.
- **More than one grid.** Files can hold several, but only the first in the arrangement is
  edited and saved back.
- **Changing steps per beat while running.** The engine's clock is set up with it, so `:o` refuses a
  song with a different value; opening it from the command line works.

Next for the UI: adding and removing tracks, effect columns, a help screen, loading keymaps from
a config file, and themes from a config file.
