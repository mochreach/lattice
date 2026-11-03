# Lattice

A music tracker for the terminal, with keys modelled on the [Helix](https://helix-editor.com)
editor. You write music as a grid of notes and it plays them through its own instruments and
effects. It has a sample-accurate sequencer, a sound engine written by hand, and rules that
reshape a pattern each time it loops, so a part can change as it plays.

## Why it exists

Lattice is maximalist software, and its intended audience is me. I wanted all of the features and
all of the tools, fast enough to get musical ideas out of my head and into songs. Everything
serves that, so I've been uncompromising about how easy it is for anyone else to use.

I couldn't have made this before agentic programming. Software this large, this involved and this
fast, written for one person, wasn't something one person could build in their spare time. Now it
is, and I think that's exciting and important.

All the same, I think it's a fascinating piece of software and an interesting way to make music.
I hope other people get something from it, and I'm curious to see how they use it. There's more
on this at the start of [the manual](docs/manual/01-introduction.md#why-it-exists).

I'm Chris Wells Wood. I make videos on my YouTube channel,
[Mo chreach!](https://www.youtube.com/@mochreach), and write at [mochreach.dev](https://mochreach.dev).

## The manual

Everything Lattice does is explained in [the manual](docs/manual), with every key and command,
drawings of the screen and the reasons behind them. It's built into the app too: `:manual` opens
it, and `:manual TOPIC` goes to what it says about a topic. To learn the keys by using them, start
with the tutor: `lattice --tutor`, or `:tutor` from inside. `:help` lists every key and command
as they're bound.

## Building from source

Lattice is pre-alpha, and it's built and run on Linux. You need a Rust toolchain recent enough for
the 2024 edition, `pkg-config`, and ALSA's development files (`alsa-lib`, or `libasound2-dev` on
Debian and Ubuntu). It plays through PulseAudio, which PipeWire also serves, or ALSA without it.

With Nix, `nix develop` gives you all of that. Then:

```sh
cargo build --release
target/release/lattice                 # a demo song
target/release/lattice song.lattice    # open a song, or start a new one there
target/release/lattice --tutor         # the tutor
```

To put `lattice` on your path, `cargo install --path crates/lattice-tui`. The other options
(`--no-splash`, `--no-click`, `--config`, …) are in
[Starting Lattice](docs/manual/01-introduction.md#starting-lattice).

## Examples

- [`examples/amen`](examples/amen): the Amen break programmed on the drums, sampled, and chopped
  up a step at a time.
- [`examples/mountain-king`](examples/mountain-king): Grieg's *In the Hall of the Mountain King*,
  using most of what Lattice does in one song.
- [`examples/scarlatti`](examples/scarlatti): Switched-On Scarlatti, his Sonata in G, K. 455, on
  Krell, the alien harpsichord, with a modular screaming along over a drum machine.

Open one with `lattice examples/scarlatti/scarlatti.lattice`, then `C-space` to play the song.

## How it's made

| Crate            | Role                                                                        |
| ---------------- | --------------------------------------------------------------------------- |
| `lattice-core`   | Song data, the rules that transform patterns, `realise()`. No UI or audio. |
| `lattice-engine` | The sequencer's clock, voices, DSP and mixer. Real-time safe.               |
| `lattice-editor` | The modal editor's state and keymaps, independent of the frontend.          |
| `lattice-tui`    | The terminal frontend: the `lattice` binary.                                |

The audio thread never allocates, locks or waits, and debug builds abort if it allocates, so a
real-time mistake shows up at once. Outside tests the code can't panic: the workspace's lints deny
`unwrap`, `expect` and indexing, since a panic would take the song with it. Scripts and agents can
drive a running Lattice through `lattice ctl`
([Driving Lattice from scripts](docs/manual/10-files-and-config.md#driving-lattice-from-scripts-lattice-ctl)).

The design, and what was learned from a first attempt as a GUI
([mochreach/lattice-legacy](https://github.com/mochreach/lattice-legacy), archived), are in
[docs/DESIGN.md](docs/DESIGN.md). How to work on the code is in [AGENTS.md](AGENTS.md).

```sh
cargo test --workspace
cargo clippy --all-targets
```

## Licence

MIT.
