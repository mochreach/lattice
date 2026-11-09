Lattice is in beta: ready for others to try, though made for me, and shared in case it suits
you too. Expect rough edges, and songs saved now may not open in later versions.

## What's new in 0.2.0

- **Two new instruments**: Tuss, an electric piano named after Aphex Twin's alias The Tuss,
  with presets of its own and Flam, an example song after "Flim"; and Bessel, an FM synth.
- **Helix's match menu** on the pattern page: `mi` and `ma` select the note, phrase, beat,
  bar, group or chord the cursor is in, `mm` goes between a note's ends, and `ms`, `mr` and
  `md` lock a setting over a selection, change it and take it away. Mute moves to `tm`.
- **Clearer selections.** Edits act on the column the cursor is in, and that column is shown
  lighter across the whole selection, the note column included.
- **Gestures are easier to type**: a box lists what you can type while you type one.
- **Arpeggios retune** the sounding note instead of restarting it, so fast ones stay clean.
- **Presets live in folders**, and a silent Switched track costs almost nothing.
- A thank-you after the opening, with where to find more; `splash = false` in the config
  leaves both out.

## Installing

**Linux and macOS**, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/mochreach/lattice/main/install.sh | sh
```

**Windows**, in PowerShell (untried):

```powershell
irm https://raw.githubusercontent.com/mochreach/lattice/main/install.ps1 | iex
```

**NixOS**, or anywhere with Nix: `nix run github:mochreach/lattice`.

Or download the archive for your computer below, which holds `lattice`, the example songs and
the README; `SHA256SUMS` has their checksums. On Linux, `x86_64-linux-v3` is for CPUs with
AVX2 (nearly all since 2015), and plays its synths about 1.6× as fast; `x86_64-linux` runs on
any. Then start with `lattice --tutor`, and `lattice --help` if you're stuck.
