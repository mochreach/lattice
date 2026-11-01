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
cargo run --release -- --tutor            # starts the tutor
```

Debug builds abort if the audio callback ever allocates, which catches real-time mistakes early.

## Keys

There's an example of chopping a loop in [`examples/amen`](examples/amen): the Amen break
programmed on the drums, sampled, and cut up a step at a time.

New to it? `:tutor` (or `lattice --tutor`) teaches the keys a lesson at a time, on a song of its
own, as Helix's tutor does. Once you've done what a lesson asks, a box says so and `space` goes
on to the next, from where you left off; `:tutor next`,
`back` and `end` move through it.

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
| `~` | reverse the selected steps; on a chop track each piece plays backwards too |
| `*` | repeat the selection's first step through the rest of it, or with a count (`2*`) the first that many |
| `A-(` `A-)` | rotate the selected steps up / down one, wrapping round |

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
| `gi` | the mixer, on this track's instrument |
| `gf` | the fx page: this track's effects (or the mixer strip's), in lanes |

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
| `:rot N`, `:drift N`, `:rev`, `:tr N` | add a rule to the track, optionally `every N [OFFSET]` or `pass K`, or as they're shown, `/N[+OFFSET]` or `@K` |
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
| `:inst INSTRUMENT` | the instrument for every track the selections reach (or the mixer cursor's): `synth`, `string`, `sampler`, `chop`, `kick`, `snare`, `hat`, `clap` or `tom` |
| `:arm [PASSES]`, `:arm! [PASSES]`, `:disarm` | arm the track for the looper, or disarm it: with record on (`R`), playing records the audio input a pass of the track at a time into a chop track, until stopped, or for PASSES passes and then disarms (`:arm 1` for just one). `:arm` records over what a chop track plays (unless its notes have all been deleted); `:arm!` starts a new loop |
| `:input [DEVICE]`, `:output [DEVICE]` | the device the looper records, or that you hear Lattice through: type `:input ` to list them, then a number, or part of a name; 0 is the system's default. Remembered between runs; with nothing typed, says which is in use |
| `:latency MS` | move what the looper records earlier (negative) or later, if it's out of time with what you heard (-500 to 500); remembered for each input |
| `:chop [PATH]` | cut a WAV file into a piece a step, on every track the selections reach (or the mixer cursor's), each step playing its own; with no path, the last selection exported |
| `:sample [PATH]` | play a WAV file on a sampler, on every track the selections reach (or the mixer cursor's); with no path, the selection last exported with `:export-sel` or `:export-loop` |
| `:fx-add EFFECT` | add an effect (`delay`, `reverb`, `filter`, `drive`, `comp` or `pitch`) after the mixer or fx page cursor, or at the end of this track's chain |
| `:chain-save NAME` | save the strip's chain of effects (on the pattern page, the cursor's track's) as `~/.config/lattice/chains/NAME.toml`, to load on any strip of any song |
| `:chain-load NAME` | put a saved chain on the strip, in place of its own (its effects' locks go with them); Lattice comes with `shimmer` |
| `:fx-send LANE [DB]` | on the fx page, add a send after the cursor into lane `LANE` (`B`, say); the next letter after the last lane adds one. Into its own lane or one to its left it's feedback, at -6 dB unless given, and at most 0 dB |
| `:lock PARAM VALUE` | lock a setting of the track's instrument or effects on every step the selections reach: `:lock cutoff 800`, `:lock filter.mix 0.5` (or `2.mix`, the second effect's). `:lock cutoff 200..4000` sweeps it across each selection. Locked steps are marked `◆`, and the cursor's locks are shown in the status line |
| `:macro NAME` | add a macro to the track: one knob, from 0 to 1, that turns several of its settings |
| `:map NAME [DEPTH]` | on the mixer or fx page, have macro `NAME` turn the setting under the cursor, by `DEPTH` of its range (-1 to 1, 0.5 if left out) at full turn, adding the macro if it's new |
| `:unmap NAME` | stop macro `NAME` turning the setting under the cursor |
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
| `:export-loop [PATH]` | render just the selection, exactly its length, with its tail mixed back into the start, so it loops cleanly; `:chop` or `:sample` with no path then plays it |
| `:export-sel [PATH]` | render just the selection (its steps, on its tracks, heard even if muted) once, with its tail, for a sample |
| `:o PATH`, `:o! PATH` | open a song, or start a new one at PATH; `!` discards unsaved changes |
| `:n [PATH]`, `:n! [PATH]` | start an empty song: one track of 16 steps, no effects, at the same tempo, to be saved at PATH (or wherever `:w PATH` says); `!` discards unsaved changes |
| `:q`, `:q!` | quit, refusing if there are unsaved changes; `!` quits anyway |
| `:track-add [NAME]` (`:ta`) | add a 16-step track after this one (up to 16 tracks; also `ta`) |
| `:track-remove` (`:trm`), `:track-move ±N` (`:tmv`) | remove this track, move it left or right (also `th` `tl`) |

`:export-sel` and `:export-loop` make samples, so they go in a `samples` folder beside the song,
numbered (`song-loop-1.wav`, `song-loop-2.wav`, …) so a new one never replaces one a track plays; a
name alone (`:export-loop break`) goes there too, and a path where it says. Before the song's been
saved they go in a scratch folder, and `:w` copies the ones its tracks play into `samples`, so the
song and its sounds stay together.

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
`filter`, `drive`, `comp`, `pitch`) adds an effect after the cursor and `d` removes it. `tm` `ts` still
mute and solo, and `gp` goes back to the pattern. Instruments, effects and bus levels are saved
in the song.

The **fx page** (`gf`) shows one strip's effects the way the pattern shows notes: the sound goes
down through them, each effect's settings on the lines under its name, and they can branch into up to four **lanes**, side by side. A **send** row,
`→ B -3.0 dB`, copies the sound at that point into lane B; added with `:fx-send B`, it starts lane
B if there isn't one. A send into its own lane or one to its left is **feedback** (`↺`), heard
again 256 samples later and kept to 0 dB or below, so it dies away. Each lane ends in an **out**,
its level into the strip, along the bottom. `j` `k` move line by line, so `^a` `^x` (or `c`)
turn each setting, send or out as you pass it, `w` `b` jump from effect to effect past their
settings, and `h` `l` move across the lanes on the same line. `:fx-add` adds an effect after the
cursor's, and `d` on an effect's name or a send removes it, or on an out, the whole lane. `L`
locks settings as on the mixer. `(` `)` change strip, and `gm` and `gp` go to the mixer and the
pattern.

**Macros** are knobs that each turn several of a track's settings at once. `:map bloom` on a
setting, on the mixer or fx page, has the macro `bloom` turn it, by half its range when `bloom`
is all the way up (`:map bloom -1` turns it down by all of it). The fx page shows a track's macros
in a column left of lane A, each with the settings it turns under it: `h` from lane A goes there,
`^a` `^x` turn a macro or how far it turns a setting, and `d` removes either. Settings keep their
own values and the macros move them from there, so turning a setting by hand still works. A macro
locks like a setting, by its name (`:lock bloom 1`), with `L`, or by turning it with record armed.

`:chain-save wash` saves a chain to load on any strip with `:chain-load wash`. Lattice comes with
one, `shimmer`: a long reverb in lane A that sends into lane B, which high-passes it and shifts it
up an octave with the `pitch` effect, then feeds it back into the reverb, so every trip round adds
an octave above, rising as it fades, as on Eno and Lanois's records. Put it on a slow pad, and try
the feedback send's level, the reverb's decay, or `pitch` at 7 for fifths.

**Locks** automate settings by step, as on Elektron machines: a step can lock up to four of its
track's instrument or effect settings with `:lock`. A lock holds from its step until the next step
with a note or a lock, and then the setting goes back to the mixer's. A locked step is marked `◆`, and the
steps it holds over `╎`; while it plays, the mixer page shows a held lock's value, in the accent
colour and marked `◆`, in place of the track's own. Or select the steps, go to
the mixer (`gm`), press `L`, and change the track's settings by ear; `L` again locks each setting
that changed onto the selected steps and puts the track's own settings back (`esc`, or leaving the
mixer, cancels). To perform them instead, arm record (`R`) on the mixer page and play: each
setting you change is locked on the step that's playing, and heard at once, so a sweep played on
`^a` goes in step by step. Each pass is one undo, as with recorded notes. A lock on a step with no
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
`:export-sel`, then `:sample` on another track, turns a bar into a sample to play.

`chop` is for loops: `:export-loop` on a selection, then `:chop` on an empty track, or `:chop
break.wav`, cuts the file into as many equal pieces as the track has steps and has each step play
its own, so it sounds as recorded. The track shows each step's piece number and its waveform, drawn
down the column in braille, and the pieces are notes like any other: `y` and `p` copy them, `d`
silences one, `C-a` and `C-x` move a step to the next or last piece, and rules such as `:rot` and
`:rev` rearrange the loop each pass. A note plays its piece at the file's speed until the note ends,
playing on into the next if it's longer, so the loop follows the tempo without stretching. `~`
reverses the selected steps, turning each piece round too (marked `◂`), so the slice plays
backwards; `*` repeats the first step, or the first few, through the selection, for a stutter; `A-(`
and `A-)` rotate the selection. `:lock direction reverse` turns a single step's piece round;
`pieces`, `start`, `end`, `tune`, `release` and `gain` are on the mixer.

The **looper** records the audio input into a chop track. `:arm` a track (the header shows `○`, and
the top bar `ARMED`), turn record on with `R` and play: from the track's first step, each pass of it
is recorded (`●`, and `● LOOP`), saved in `samples` as a new take, and chopped onto the track, so
the next pass plays it back. Each pass after the first is recorded over the last, so the loop
builds up layer by layer until you stop or turn record off, or `:arm 1` records one pass and
disarms (`:arm 4`, four); `u` takes a layer back off. `:arm` on a
chop track records over its loop, `:arm!` starts a new one, and `:disarm` disarms it; delete all of
the track's notes and the next pass starts a new loop too, filling the steps again. Lattice
doesn't play the input back to you, so listen through your interface. If the recording sits late or
early against what you heard, `:latency -20` (in ms) moves it.

`:input` chooses what the looper records, and `:output` what you hear Lattice through, even while
it plays: type `:input ` and the devices are listed in the corner, then a number (`:input 2`), part
of a name (`:input scarlett`), or 0 for the system's default picks one. The choices, and `:latency`
for each input, belong to the computer rather than the song, so they're remembered in
`~/.config/lattice/audio.toml`. On Linux, Lattice goes through PulseAudio (which PipeWire serves),
so the devices have the names the desktop gives them, falling back to ALSA without it.

Lattice keeps a log in `~/.local/state/lattice/lattice.log` (or under `$XDG_STATE_HOME`), with the
last run's as `lattice.log.1`: the devices it opened, the commands you ran and where you were,
pages and playing, what the status line said, any trouble the audio had keeping up (and, at the
end, how close it came), and a crash's backtrace. If the audio keeps falling
behind, or a device fails, the status line says so too.

Scripts and agents can drive a running Lattice with `lattice ctl`, through a socket only you can
open (`$XDG_RUNTIME_DIR/lattice/<pid>.sock`):

```sh
lattice ctl screen                  # the screen as text
lattice ctl state                   # page, cursor, tracks, transport and status line, as JSON
lattice ctl keys 'gp:bpm 140<ret>'  # type keys; <ret>, <esc>, <C-a>, <space>, <lt> for <
lattice ctl cmd 'fx-add delay'      # run a command, as : would
lattice ctl log 20                  # the end of the log
```

What it types goes the same way as your own keys, so `u` undoes it. It can't save, open or quit,
or start over with `:new!`, unless Lattice was started with `--allow-agent-save`. With more than
one Lattice running, `--pid` says which.

The demo song is techno at 130 BPM in F minor, in six sections (`gs` shows them; `^space` plays the
lot). It opens on `peak`, which has everything: drums folded into a group with drive and a
compressor on its bus (`tz` unfolds it), hats sharing a choke, a five-step tom and a seven-step
sitar drifting against the bar, a bowed steel pad, dub stabs into the send's dotted-eighth echo, and
an acid line through a drive. The bass, stabs and pad follow a chord change every four passes with
`transpose` rules.

Tracks can be added, removed and moved while playing, and undo brings a removed track back with
its notes and its instrument. When there are more than fit across the terminal, the grid scrolls
sideways to keep the cursor's track in view, with `‹` `›` showing there are more.
