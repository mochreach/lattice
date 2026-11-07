Lattice is pre-alpha: made for me, shared in case it suits you too. Expect rough edges, and
songs saved now may not open in later versions.

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
