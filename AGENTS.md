# Working on Lattice

Guidance for anyone, person or agent, changing the code. The design itself is in
[docs/DESIGN.md](docs/DESIGN.md); what the crates do is in the [README](README.md).

## Lattice doesn't panic

A panic takes the player's song with it, so code outside tests must not be able to panic.

- **No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!`.** The workspace's
  clippy lints deny them; don't `allow` them to get round it.
- **Use `.get()` rather than indexing** (`v[i]`, `&s[a..b]`), which the lints deny too, wherever an
  index or range could be wrong: one from the cursor, the song, a file, the engine or the user.
  Slicing panics as indexing does. So do `Vec::remove`, `insert`, `swap` and `split_at` with an
  index out of range: check first, or use a form that can't fail.
- **Handle the failure; don't hide it.** `.get(i).unwrap_or_default()` turns a crash into a quiet
  wrong answer, which is worse to track down. For each place, decide what's right:
  - pass an error up with `?`, so the status line says what went wrong (commands return
    `Result<_, String>`; the frontend uses `anyhow`);
  - give up on the action with `let … else`, and a message if the player would wonder why
    nothing happened;
  - fall back to a default only where the default really is right, and say why in a comment.
- **Keep indices valid at the source.** Most out-of-range indices are stale ones: a cursor,
  selection, loop or cue left pointing where tracks, entries or patterns used to be. When
  something is added, removed, moved, undone or loaded, bring every index that refers to it up to
  date, rather than relying on later `.get()`s to catch it.
- **The audio thread** must also never allocate, lock or wait (`tests/realtime.rs` checks the
  allocation). There, prefer iterators (`zip`, `chunks_exact`, `iter_mut`) over indexing: they
  can't go out of bounds and usually run faster. Where indexing a fixed buffer really is clearest,
  put `#[expect(clippy::indexing_slicing, reason = "…")]` on the function, saying why it's in
  bounds.
- Tests may unwrap and index freely: that's how they fail.

## Checks

Run these before committing, through `nix develop -c`:

```sh
cargo fmt
cargo clippy --all-targets   # no warnings
cargo test --workspace
cargo build --release
```

Then try the change in the running app; `lattice ctl` (see the manual's last chapter) can drive it from a
script.

## Conventions

- **Keys follow Helix**, not vim. Each page's own actions get single keys on the cursor or the
  `v` selection, rather than going through menus that make little sense there.
- **Teach what's new.** Every new action or command goes in the tutor, the manual
  (`docs/manual`, explained in full), the help page and, where it changes the design,
  DESIGN.
- **Saved songs may break.** Lattice is pre-alpha: change the file format freely, without
  keeping old files working.
- **Colours** come from the terminal's palette, through the theme.
- **One commit per feature**, its message starting "Adds…" or "Makes…" and saying what it does
  for the player.
