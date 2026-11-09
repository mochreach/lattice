Lattice is in beta: ready for others to try, though made for me, and shared in case it suits
you too. Expect rough edges, and songs saved now may not open in later versions.

## What's new in 0.2.1

- **`lattice update`** installs the newest release in place of the one you have. Once a day, as
  it starts, Lattice asks GitHub whether there's a newer release and the status line says if
  there is; `update-check = false` in the config stops it asking.
- **Held notes sustain on Windows.** Its terminals report keys being let go, which Lattice
  hadn't noticed, so notes played on the keyboard were blips and recorded ones a step long.
  Windows is now tried, in Windows Terminal; it has no numpad pads, and the loop pedal is `!`.

## Installing

**Linux and macOS**, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/mochreach/lattice/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/mochreach/lattice/main/install.ps1 | iex
```

**NixOS**, or anywhere with Nix: `nix run github:mochreach/lattice`.

Or download the archive for your computer below, which holds `lattice`, the example songs and
the README; `SHA256SUMS` has their checksums. On Linux, `x86_64-linux-v3` is for CPUs with
AVX2 (nearly all since 2015), and plays its synths about 1.6× as fast; `x86_64-linux` runs on
any. Then start with `lattice --tutor`, and `lattice --help` if you're stuck.
