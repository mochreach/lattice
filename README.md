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
cargo run --release -- song.lattice   # opens a song, or starts a new one there
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
| `tt` | select every step of the tracks the selections reach |
| `tm` `ts` | mute / solo every track the selections reach (not Helix's `t`, which has no use on a grid) |
| `ta`, `th` `tl` | add a track after this one, move it left / right |
| `ty`, `td` | copy the tracks the selections reach; remove them, copying them first |
| `tp` `tP` | paste copied tracks after / before this one, with their notes, rules, mix and instrument |
| `tg` | group the tracks the selections reach (from the first to the last), or ungroup the group they're in; a group's tracks mix into its bus, a strip on the mixer page with a level and effects |
| `tz` | fold or unfold the groups the selections reach: each track narrows to a column of hits (`●` `•` `∙` by velocity), like a drum machine's steps |
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
| `p` `P` | paste over the selection, from its first step; or insert there, pushing the notes below down (the end of the track falls off) |
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
| `gm` `gp` | the mixer page (on this track), back to the pattern |
| `gi` `gf` | the mixer, on this track's instrument, or its effects |

One deliberate departure from Helix: `space` is play / stop, as in every DAW, rather than a menu.

In **insert mode**, the bottom two letter rows are a piano: `z`–`m` from C in the current
octave, `q`–`u` an octave up, and `[` `]` change the octave. Each note is entered and the cursor
moves on. `⌫` clears a step, and `esc` returns to normal mode.

In a group, the **numpad is drum pads**: `1` plays the group's first track at C-4, `2` the
second, and so on, in any mode; in insert mode it also enters the hit on the cursor's step,
without moving on, so a step can take several drums. This needs a terminal that tells the numpad
apart (kitty, WezTerm, foot, Ghostty, recent Alacritty). Elsewhere, and inside tmux, the number
row does the same in insert mode while the cursor is in a folded group.

On the **command line**, commands that fuzzy-match what you've typed are listed above it, and
`tab` cycles through them. Once the command is named, the parameters still to type are shown
faintly after the cursor, and shrink as you type over them. Values are checked as you type: a
word that can no longer be right is underlined, and what was expected is shown on the right.

| Commands | |
| --- | --- |
| `:rot N`, `:drift N`, `:rev`, `:tr N` | add a rule to the track, optionally `every N [OFFSET]` or `pass K` |
| `:pop`, `:clear` | remove the last rule, or all of them |
| `:len N` | track length (1–256) |
| `:bpm N`, `:step N`, `:oct N` | tempo, auto-step, keyboard octave |
| `:click`, `:follow` | toggle the metronome, following the playhead |
| `:name NAME` | rename the track |
| `:group-name NAME` | rename this track's group |
| `:level DB`, `:pan P`, `:send DB` | mixer, for every track the selections reach: level (-60 to 6 dB), pan (-100 left to 100 right), send to the send bus (-60, off, to 6 dB); on the mixer page, `:level` sets a bus too |
| `:set VALUE` | on the mixer page, set the value under the cursor (also `c`) |
| `:inst INSTRUMENT` | the instrument for every track the selections reach (or the mixer cursor's): `synth`, `kick`, `snare`, `hat`, `clap` or `tom` |
| `:fx-add EFFECT` | add an effect (`delay`, `drive` or `comp`) after the mixer cursor, or at the end of this track's chain |
| `:choke N` | choke group (1–8, 0 for none) for every track the selections reach: within a track group, a hit cuts off the other tracks with the same number, as a closed hat stops an open one |
| `:mute`, `:solo` | toggle mute or solo for every track the selections reach (also `tm`, `ts`) |
| `:w [PATH]`, `:wq [PATH]` | save the song (to a `.lattice` file), save and quit |
| `:o PATH`, `:o! PATH` | open a song, or start a new one at PATH; `!` discards unsaved changes |
| `:q`, `:q!` | quit, refusing if there are unsaved changes; `!` quits anyway |
| `:track-add [NAME]` (`:ta`) | add a 16-step track after this one (up to 16 tracks; also `ta`) |
| `:track-remove` (`:trm`), `:track-move ±N` (`:tmv`) | remove this track, move it left or right (also `th` `tl`) |

The header shows the song's file name, and `[+]` when it has unsaved changes.

Each track's header shows its level as a fader bar (-60 to +6 dB) and its pan as a bar from the
centre, each with its value written over it; `M` when it's muted and `S` when it's soloed; and a
meter in place of the underline. Muted tracks, and tracks silenced by another's solo, are dimmed.
Every track can send to a delay on the send bus; sends start off.

The **mixer page** (`gm`) is in two halves. Above, an overview of every strip (each track, each
group's bus, and the send and master buses) with its level, pan and meter: `h` `l` move between
them and `^a` `^x` set the level. Below, the detail of the strip under the cursor: where its sound
goes, its mix (level, pan, send, choke group), its chain from instrument to effects, and the
settings of the instrument or effect under the cursor. `j` `k` move down into the detail and
through its rows, `h` `l` move along a row, `(` `)` change strip, and `tab` jumps between the
overview and the chain. `^a` `^x` nudge the value under the cursor (with counts, so `10^a`), `c`
types an exact one (or, on the instrument, another instrument), `:fx-add delay` (or `drive`,
`comp`) adds an effect after the cursor and `d` removes it. `tm` `ts` still mute and solo, and
`gp` goes back to the pattern. Instruments, effects and bus levels are saved in the song.

**Instruments:** each track has one. `synth` is a plain test synth. `kick`, `snare`, `hat`, `clap` and `tom` are
Rhythmicon, the drum engine: one drum per track, played at its tuning by C-4 and tuned up or down
by other notes. Each has a model (`tight` `boom` `hard` for the kick, `tight` `fat` `crack` for
the snare, `metal` `ring` `sizzle` for the hat, `tight` `room` `crowd` for the clap, `tight`
`round` `hard` for the tom) and four controls within it (see
[docs/DESIGN.md](docs/DESIGN.md)). The demo song
is the Amen break at 136 BPM, on a kick, snare, ride and crash, under the synths. The drums are a
folded group, `amen`, with drive and a compressor on its bus; `tz` unfolds it.

Tracks can be added, removed and moved while playing, and undo brings a removed track back with
its notes and its instrument. When there are more than fit across the terminal, the grid scrolls
sideways to keep the cursor's track in view, with `‹` `›` showing there are more.
