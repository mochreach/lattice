# Lattice

The keyboard hacker's audio tracker: a vim-flavoured terminal tracker in Rust, with a
sample-accurate sequencer, a hand-written sound engine, and transforms that reshape
the pattern each time it loops.

This is a restart. The first attempt, an iced GUI, lives on at
[mochreach/lattice-legacy](https://github.com/mochreach/lattice-legacy) (archived).
What we learned from it, and the plan for this version, are in
[docs/DESIGN.md](docs/DESIGN.md).

## Layout

| Crate            | Role                                                              |
| ---------------- | ----------------------------------------------------------------- |
| `lattice-core`   | Song data, pattern transforms, `realise()`. No UI or audio types. |
| `lattice-engine` | Sequencer clock, voices, DSP and mixer. Real-time safe.           |
| `lattice-editor` | Modal editor state and keymaps, frontend-independent.             |
| `lattice-tui`    | The terminal frontend (`lattice` binary).                         |

## Building

```sh
nix develop               # or bring your own Rust toolchain plus alsa-lib
cargo test
cargo run --release       # opens the tracker with a demo song; add `-- --no-click` to mute the metronome
```

Debug builds abort if the audio callback ever allocates, which catches real-time mistakes early.

## Keys

Vim-flavoured. Counts work in normal mode (`4j`, `3+`).

| Normal mode | |
| --- | --- |
| `h j k l`, arrows | move between tracks and steps |
| `g` `G`, `^u` `^d` | first / last step, up / down 16 |
| `space` | play / stop |
| `tab` | show the pattern as written, or as it's playing this pass |
| `i` | insert mode |
| `v` | select steps (then `y`, `d`, `^a`, `^x`, `<`, `>`, `+`, `-`) |
| `x` / `d` | clear |
| `y` `p` | copy, paste |
| `^a` `^x` | note up / down a semitone |
| `>` `<` | note up / down an octave |
| `+` `-` | longer / shorter note |
| `[` `]` | keyboard octave |
| `u` `^r` | undo, redo |
| `f` | follow the playhead |
| `:` | command line |

In **insert mode**, the bottom two letter rows are a piano: `z`–`m` from C in the current
octave, `q`–`u` an octave up. Each note is entered and the cursor moves on. `⌫` clears a step,
and `esc` returns to normal mode.

| Commands | |
| --- | --- |
| `:rot N`, `:drift N`, `:rev`, `:tr N` | add a rule to the track, optionally `every N [OFFSET]` or `pass K` |
| `:pop`, `:clear` | remove the last rule, or all of them |
| `:len N` | track length (1–256) |
| `:bpm N`, `:step N`, `:oct N` | tempo, auto-step, keyboard octave |
| `:click`, `:name NAME`, `:q` | toggle the metronome, rename the track, quit |
