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
cargo run --release       # opens the tracker with a demo song; add `-- --no-click` to mute the metronome, `-- --show-keys` to show keys as they're pressed, `-- --no-splash` to skip the opening animation (any key skips it too)
cargo run --release -- song.lattice   # opens a song, or starts a new one there
cargo run --release -- --config my.toml   # reads that config in place of the usual one
```

Debug builds abort if the audio callback ever allocates, which catches real-time mistakes early.

## Keys

Modelled on [Helix](https://helix-editor.com): select first, then act, so you see what will
change before it does. There is always a selection, a rectangle of steps across one or more
tracks; with nothing selected it's just the cursor. Motions move the cursor and collapse the
selection onto it. In select mode (`v`) the same motions extend it instead. Counts work as in
Helix (`4j`, `3+`). Prefix keys such as `g` show what can follow them in a box in the corner.
`:help` lists every key and command, made from the keymap and the command table, so it's always
up to date; `j` `k` scroll it and `esc` goes back.

Keys and colours can be changed in `~/.config/lattice/config.toml` (or under `$XDG_CONFIG_HOME`),
written as in Helix. Keys are bound to actions by name (`:help` shows each one's, dimmed, before
what it does), to a command line after a `:`, or to `none` to unbind them. A table under
`keys.normal` is a menu opened by that key, adding to one that's already there. Colours are
names (`light-blue`), palette indexes, `#rrggbb`, or `default` for the terminal's own; anything
not set follows the terminal's palette. Mistakes are shown when Lattice starts, and the rest
still applies.

```toml
[keys.normal]
C-s = ":w"
X = "none"

[keys.normal.space]          # a new menu, opened by space
m = "goto_mixer"
s = "goto_song"

[keys.insert]
C-c = "escape"

[theme]
accent = "magenta"           # also text, dim, faint, playing, visual, error, selection
beat-row = "none"            # unshaded, or a colour
tracks = ["#ff8800", "cyan", 12]
```

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
| `]p` `[p` | next / previous pattern; while playing, it takes over at the next bar |
| `v` | select mode: motions extend the selection |
| `x` | select whole rows across every track; again to extend a row |
| `%` | select everything |
| `tt` | select every step of the tracks the selections reach |
| `tm` `ts` | mute / solo every track the selections reach (not Helix's `t`, which has no use on a grid) |
| `tM` | mute / unmute in this pattern only: the notes stay, ready to bring back, and the other patterns play them (shown `m`) |
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
| `^space` | play the song from the entry playing this pattern (or stop) |
| `R` | record on / off (starts insert mode): see below |
| `tab` | show the pattern as written, or as it's playing this pass |
| `:` | command line |
| `gm` `gs` `gp` | the mixer page (on this track), the song page, back to the pattern |
| `gi` `gf` | the mixer, on this track's instrument, or its effects |

One deliberate departure from Helix: `space` is play / stop, as in every DAW, rather than a menu.

In **insert mode**, the bottom two letter rows are a piano: `z`–`m` from C in the current
octave, `q`–`u` an octave up, and `[` `]` change the octave. `:layout` lays the notes out
isomorphically instead, over all four rows from `z` (C) to `=`, so a chord has the same shape
wherever you play it: `wicki` (Wicki–Hayden: a tone to the right, a fourth up-left, a fifth
up-right, three octaves across the keyboard) or `fourths` (a semitone to the right, a fourth a row
up, as on a guitar). `:layout scale` plays only the notes of the pads' scale (`:scale`), from its
root at `z`: the next note of the scale to the right and about a fourth a row up, as in Push's
"in key" mode, so nothing is out of key. `:layout piano` goes back. Each note is entered and the cursor
moves on. `⌫` clears a step, and `esc` returns to normal mode. In a terminal that reports key
releases (see below), a note sounds for as long as you hold its key, so a bowed string or a pad
sustains under your finger, and chords ring together; elsewhere you hear a short blip.

The **numpad is a set of performance pads**, in any mode; in insert mode a pad also enters its
note on the cursor's step, without moving on, so a step can take several drums.

| Numpad | |
| --- | --- |
| `1`–`9` | in a group, its drums: `1` plays the first track at C-4, `2` the second, and so on; on any other track, the notes of a scale up from the keyboard octave (`:scale`, C major to start) |
| `0` (held) | note repeat: pads played while it's down repeat in time until let go (`REPEAT` in the header), and record each hit; needs key releases |
| `.` | how often pads repeat: every 16th, 8th or quarter |
| `/` `*` `-` | play and enter notes soft, medium or hard (velocity 60, 100, 127; `:vel N` for any other), shown as `vel` in the header |
| `+` | mute mode (`MUTE PADS` in the header): pads `1`–`9` mute and unmute the first nine tracks |

`:scale NAME [ROOT]` chooses the scale: `major`, `minor`, `dorian`, `mixolydian`, `pentatonic`,
`minor-pentatonic`, `blues` or `chromatic`, with a root such as `A` or `F#` (`:scale minor A`).
The pads need a terminal that tells the numpad apart (kitty, WezTerm, foot, Ghostty, recent
Alacritty). Elsewhere, and inside tmux, the number row stands in for pads `1`–`9` in insert mode
while the cursor is in a folded group, and `:vel` sets the velocity.

To **record live**, press `R` (a red `REC` shows in the header) and then `space`. A bar of clicks
counts you in, with the beats left in the header (`IN 4` … `IN 1`), whether or not the metronome
is on; a note played in the last half-step of it goes on the first step. While the song plays, the notes you play in insert mode, piano keys and pads alike, go in where the playhead is,
on the nearest step, on the cursor's track (or each selection's), over whatever was there. A note
rounded back onto a step that has just played sounds as you play it; one rounded on to the next
step is played by the sequencer there, in time. Each pass round the track is one undo, so `u`
takes out a bad take and keeps the passes before it. In a terminal that reports key releases
(kitty, WezTerm, foot, Ghostty, recent Alacritty; not tmux), a note lasts as long as the key was
held; elsewhere each is one step. Stopped, insert mode enters notes at the cursor as usual.

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
| `:help` (`:h`) | the help page: every key and command |
| `:click`, `:follow` | toggle the metronome, following the playhead |
| `:rec` | toggle recording, like `R` |
| `:layout NAME` | how notes lie on the keyboard in insert mode: `piano`, `wicki`, `fourths` or `scale` |
| `:vel N`, `:scale NAME [ROOT]` | the velocity notes are played and entered at (1–127); the scale the numpad's pads play outside a group |
| `:count N` | bars counted in before recording (0–4, 0 for none; 1 to start) |
| `:keys` | toggle showing the keys as they're pressed, above the status line, for screencasts |
| `:name NAME` | rename the track |
| `:group-name NAME` | rename this track's group |
| `:level DB`, `:pan P`, `:send DB` | mixer, for every track the selections reach: level (-60 to 6 dB), pan (-100 left to 100 right), send to the send bus (-60, off, to 6 dB); on the mixer page, `:level` sets a bus too |
| `:set VALUE` | on the mixer page, set the value under the cursor (also `c`) |
| `:inst INSTRUMENT` | the instrument for every track the selections reach (or the mixer cursor's): `synth`, `string`, `sampler`, `kick`, `snare`, `hat`, `clap` or `tom` |
| `:sample [PATH]` | play a WAV file on a sampler, on every track the selections reach (or the mixer cursor's); with no path, the selection last exported with `:export-sel` |
| `:fx-add EFFECT` | add an effect (`delay`, `reverb`, `filter`, `drive` or `comp`) after the mixer cursor, or at the end of this track's chain |
| `:lock PARAM VALUE` | lock a setting of the track's instrument or effects on every step the selections reach: `:lock cutoff 800`, `:lock filter.mix 0.5` (or `2.mix`, the second effect's). `:lock cutoff 200..4000` sweeps it across each selection. Locked steps are marked `◆`, and the cursor's locks are shown in the status line |
| `:unlock [PARAM]` | clear a setting's locks, or all of them, from the steps the selections reach |
| `:choke N` | choke group (1–8, 0 for none) for every track the selections reach: within a track group, a hit cuts off the other tracks with the same number, as a closed hat stops an open one |
| `:mute`, `:solo` | toggle mute or solo for every track the selections reach (also `tm`, `ts`) |
| `:w [PATH]`, `:wq [PATH]` | save the song (to a `.lattice` file), save and quit |
| `:pattern NAME` (`:pat`) | go to a pattern, by name or number; while playing, it's cued and takes over at the next bar (the header shows `pat A → B`) |
| `:pattern-new [NAME]` (`:pn`), `:pattern-dup [NAME]` (`:pd`) | add an empty pattern after this one, or a copy of this one, and go to it; it's added to the end of the song, played once. Named A, B, C… unless named |
| `:pattern-mute` (`:pm`) | mute or unmute every track the selections reach in this pattern only (also `tM`) |
| `:pattern-name NAME`, `:pattern-remove` | rename this pattern; remove it, and its places in the song |
| `:bars [N \| auto]` | how long this pattern plays in the song: N bars, or `auto` (the default), as long as its longest track, rounded up to a whole bar. Looping on its own, a pattern never restarts its tracks, so tracks of different lengths keep drifting against each other |
| `:export [PATH] [BARS]` | render the song to a 24-bit WAV, faster than real time, while you carry on: each pattern in the song's order, for its length, then the tail until it's quiet; or with BARS, this pattern for that many bars. With no path, it goes beside the song's file |
| `:export-sel [PATH]` | render just the selection (its steps, on its tracks, heard even if muted) once, with its tail, for a sample |
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
types an exact one (or, on the instrument, another instrument), `:fx-add delay` (or `reverb`,
`filter`, `drive`, `comp`) adds an effect after the cursor and `d` removes it. `tm` `ts` still
mute and solo, and `gp` goes back to the pattern. Instruments, effects and bus levels are saved
in the song.

**Locks** automate settings by step, as on Elektron machines: a step can lock up to four of its
track's instrument or effect settings with `:lock`. A lock holds from its step until the next step
with a note or a lock, and then the setting goes back to the mixer's. A lock on a step with no
note changes the setting on its own, so a few locks down an empty stretch move an effect along.
In the demo, the second stab in each bar is brighter and rings, from its locked cutoff and
resonance.

The **song page** (`gs`) is the arrangement: the patterns in the order they play, each entry
repeated some number of times. A strip across the top shows the whole song, each entry as wide as
it plays for, coloured by its pattern; below it, the entries with when each starts, and the
patterns there are. `j` `k` move between entries, `h` `l` change the entry's pattern, `+` `-` (or
`^a` `^x`) how many times it repeats, `o` `O` add an entry after or before, `d` removes one (`y`
copies, `p` `P` paste after and before, so `d` then `p` moves an entry), and `ret` goes to the
entry's pattern to edit it. `space` plays the song from the cursor's entry to the end, following
along in the header (`song 2/3`) and on the page, where the playhead moves along the strip;
`:export` renders it. Anywhere else, `^space` plays the song from the entry playing the pattern
you're on.

**Instruments:** each track has one. `synth` is a plain test synth. `kick`, `snare`, `hat`, `clap` and `tom` are
Rhythmicon, the drum engine: one drum per track, played at its tuning by C-4 and tuned up or down
by other notes. Each has a model (`tight` `boom` `hard` for the kick, `tight` `fat` `crack` for
the snare, `metal` `ring` `sizzle` for the hat, `tight` `room` `crowd` for the clap, `tight`
`round` `hard` for the tom) and four controls within it (see
[docs/DESIGN.md](docs/DESIGN.md)). `string` is Monochord, a plucked or bowed string, modelled
as a waveguide: models `nylon`, `steel` (pairs of strings a few cents apart, spread in stereo)
and `sitar` (buzzing against its bridge), with pluck (soft thumb to hard, clicking pick), position (along
the string), decay, tone and bow (a real bowed string while the note is held, light to heavy;
it takes over from the pluck as it's turned up, and long notes suit it). Strings ring on after a
note ends, as a harp's do; a note-off only lifts the bow. `sampler` plays a WAV file (`:sample kick.wav`; 8 to 32-bit, or float, mono or
stereo, any rate). C-4 plays it as recorded and other notes pitch it, or with `slices` above 1 it's
chopped into equal slices, C-4 playing the first and each note up the next, at the file's pitch. Its
`mode` is `one-shot` (to the end, whatever the note's length), `gate` (until the note ends) or
`loop` (round the region until the note ends); `start` and `end` set the region, `direction`
plays it backwards, and `tune`, `attack`, `release` and `gain` do what they say. So
`:export-sel`, then `:sample` on another track, turns a bar into a sample to chop up.
The demo song
is techno at 130 BPM in F minor, in six sections (`gs` shows them; `^space` plays the lot). It
opens on `peak`, which has everything: drums folded into a group with drive and a compressor on
its bus (`tz` unfolds it), hats sharing a choke, a five-step tom and a seven-step sitar drifting
against the bar, a bowed steel pad, dub stabs into the send's dotted-eighth echo, and an acid line
through a drive. The bass, stabs and pad follow a chord change every four passes with
`transpose` rules.

Tracks can be added, removed and moved while playing, and undo brings a removed track back with
its notes and its instrument. When there are more than fit across the terminal, the grid scrolls
sideways to keep the cursor's track in view, with `‹` `›` showing there are more.
