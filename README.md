# Lattice

The keyboard hacker's audio tracker: a Helix-flavoured terminal tracker in Rust, with a
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

Modelled on [Helix](https://helix-editor.com): select first, then act, so you see what will
change before it does. There is always a selection, a rectangle of steps across one or more
tracks; with nothing selected it's just the cursor. Motions move the cursor and collapse the
selection onto it. In select mode (`v`) the same motions extend it instead. Counts work as in
Helix (`4j`, `3+`). Prefix keys such as `g` show what can follow them in a box in the corner.

As in Helix, there can be several selections at once. Every motion and action applies to each
of them, and selections that come to overlap merge. The view follows the primary selection,
whose cursor is the solid block. `y` copies one value per selection, and `p` hands them back out
in order, repeating the last if there are more selections than values. So `%`, then `s C` and
`ret`, then `^a` sharpens every C in the song. Patterns can list several notes (`C E G`), and
`s` shows what it will select as you type.

| Moving and selecting | |
| --- | --- |
| `h j k l`, arrows | move between tracks and steps |
| `w` `b` | next / previous note |
| `^u` `^d`, `^b` `^f` | up / down half a page, a page |
| `gg` `ge` `gh` `gl` | first step, last step, first track, last track |
| `]b` `[b`, `]m` `[m` | next / previous beat, bar |
| `]n` `[n` | next / previous note, like `w` `b` |
| `v` | select mode: motions extend the selection |
| `x` | select whole rows across every track; again to extend a row |
| `%` | select everything |
| `;` `A-;` | collapse the selection, flip which end the cursor is |

| Multiple selections | |
| --- | --- |
| `C` `A-C` | copy the selection onto the rows just below / above it |
| `s` | select the notes matching a pattern inside the selections: `C`, `C#`, `Eb4`, `C-4`, `*` |
| `K` `A-K` | keep / remove the selections containing a matching note |
| `A-s` | split the selections into rows |
| `A--` | merge the selections into one |
| `,` `A-,` | keep only the primary selection, remove it |
| `(` `)` | make the previous / next selection primary |

| Acting on the selection | |
| --- | --- |
| `d` `A-d` | delete (and copy), delete without copying |
| `c` | delete and start inserting |
| `y` | copy |
| `p` `P` `R` | paste after, before, or over the selection |
| `r` then a piano key | fill the selection with that note |
| `^a` `^x` | note up / down a semitone |
| `>` `<` | note up / down an octave |
| `+` `-` | longer / shorter note |
| `i` `a` | insert at the start, or after the end |
| `u` `U` | undo, redo |

| Everywhere | |
| --- | --- |
| `space` | play / stop |
| `tab` | show the pattern as written, or as it's playing this pass |
| `:` | command line |

One deliberate departure from Helix: `space` is play / stop, as in every DAW, rather than a menu.

In **insert mode**, the bottom two letter rows are a piano: `z`–`m` from C in the current
octave, `q`–`u` an octave up, and `[` `]` change the octave. Each note is entered and the cursor
moves on. `⌫` clears a step, and `esc` returns to normal mode.

On the **command line**, commands that fuzzy-match what you've typed are listed above it, and
`tab` cycles through them. Once the command is named, the parameters still to type are shown
faintly after the cursor, and shrink as you type over them.

| Commands | |
| --- | --- |
| `:rot N`, `:drift N`, `:rev`, `:tr N` | add a rule to the track, optionally `every N [OFFSET]` or `pass K` |
| `:pop`, `:clear` | remove the last rule, or all of them |
| `:len N` | track length (1–256) |
| `:bpm N`, `:step N`, `:oct N` | tempo, auto-step, keyboard octave |
| `:click`, `:follow` | toggle the metronome, following the playhead |
| `:name NAME`, `:q` | rename the track, quit |
