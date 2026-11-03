# Lattice

The keyboard hacker's audio tracker: a Helix-flavoured terminal tracker in Rust, with a
sample-accurate sequencer, a hand-written sound engine, and transforms that reshape
the pattern each time it loops.

This is a restart. The first attempt, an iced GUI, lives on at
[mochreach/lattice-legacy](https://github.com/mochreach/lattice-legacy) (archived).
What we learned from it, and the plan for this version, are in
[docs/DESIGN.md](docs/DESIGN.md), and how to work on the code, such as never panicking,
in [AGENTS.md](AGENTS.md).

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
programmed on the drums, sampled, and cut up a step at a time. For most of what Lattice does in
one song, [`examples/mountain-king`](examples/mountain-king) is Grieg's *In the Hall of the
Mountain King*: groups, a branching effects chain, a macro, faces, choke, rules, mutes in the
arrangement, locks that slide, a section chopped from its own render, and the conductor speeding
it up. [`examples/scarlatti`](examples/scarlatti) is a Switched-On Scarlatti, his Sonata in G,
K. 455, shot into space on Krell, the alien harpsichord, with Switched screaming along over a
drum machine: the ensemble and phaser, a ladder sliding open, gestures (trills, glides, an
arpeggiator, a run split between two tracks, chance), a canon, rolls and timpani that play only
in a fill, and repeats orchestrated differently by mutes in the arrangement.

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
up to date; `j` `k` scroll it, `/` finds words in it (`n` `N` the next and the one before, in the
manual too) and `esc` goes back. The [manual](docs/manual) explains everything
in full, and is built in: `:manual` opens it at the contents, `:manual TOPIC` at a section about
the topic (again for the next place), `w` `b` go from heading to heading.

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
| `h j k l`, arrows | move between fields and steps: `h` `l` go through the open track's fields (note, velocity, gestures, lock columns), then on to the next track; left of the first track's notes, into the conductor's |
| `th` `tl` | previous / next track, on its notes |
| `f` then a key | to a field of the open track: `n` note, `v` velocity, `g` `G` gestures, a lock column by the letter its header underlines; or `C` the conductor's, from any track |
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
| `m`, `tm` `ts` | mute / solo every track the selections reach (`m` alone on every page, `s` alone where there are no notes to select; not Helix's `t`, which has no use on a grid) |
| `\|` | loop playback round the selected steps, or the cursor's beat; on the loop, stop (the song page loops bars or entries) |
| `tM` | mute / unmute in this pattern only: the notes stay, ready to bring back, and the other patterns play them (shown `m`) |
| `ta`, `tH` `tL` | add a track after this one, move it left / right |
| `ty`, `td` | copy the tracks the selections reach; remove them, copying them first |
| `tp` `tP` | paste copied tracks after / before this one, with their notes, rules, mix and instrument |
| `tg` | group the tracks the selections reach (from the first to the last), or ungroup the group they're in; a group's tracks mix into its bus, a strip on the mixer page with a level and effects |
| `tz` | fold or unfold the groups the selections reach: each track narrows to a column of hits (`●` `•` `∙` by velocity), like a drum machine's steps |
| `te` | keep the tracks the selections reach open, showing their velocity, gestures and lock columns as the cursor's track does; again, close them |
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
| `^a` `^x` | note up / down a semitone; in a field, its values up / down by one |
| `&` | in a field, fill each selection evenly between the values at its ends |
| `>` `<` | note up / down an octave |
| `+` `-` | longer / shorter note |
| `i` `a` | insert at the start, or after the end |
| `u` `U` | undo, redo |

| Everywhere | |
| --- | --- |
| `space` | play / stop |
| `^space` | play the song from its start (or stop) |
| `A-space` | play the song from the entry playing this pattern (or stop) |
| `^k` | hush: stop the song and every sound at once, held notes, echoes, reverb tails and loops |
| `{` `}` | keyboard octave down / up, in any mode and on any page |
| `` ` `` (backtick) | hold for a fill: steps marked `F` play, and `F-` don't (see gestures) |
| `R` | record on / off (starts insert mode): see below |
| `tab` | show the pattern as written, or as it's playing this pass |
| `:` | command line |
| `gm` `gs` `gp` | the mixer page (on this track), the song page, back to the pattern |
| `gi` | the instrument page, on this track |
| `gf` | the fx page: this track's effects (or the mixer strip's), in lanes |

One deliberate departure from Helix: `space` is play / stop, as in every DAW, rather than a menu.

In **insert mode**, the bottom two letter rows are a piano: `z`–`m` from C in the current
octave, with the B below it on `\` (left of `z` on a UK keyboard), `q`–`u` an octave up and
`i`–`]` the octave over that, as far as G; `{` `}` change the octave. `:layout` lays the notes out
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
| `/` `*` `-` | play and enter notes soft, medium or hard (velocity 47, 78, 99 of 99; `:vel N` for any other), shown as `vel` in the header |
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
faintly after the cursor, and shrink as you type over them, and the values that fit the word
you're typing are listed above it in turn, for `tab` to fill in: a rule's amount (`12`, an octave
up) and when it applies (`every 4`, then which of the four), the effects after `:fx-add`
(with their settings), instruments after `:inst`, the track's settings with their values after
`:lock`, then a setting's choices, macros after `:map`, saved chains, patterns, scales, layouts,
devices, and files and folders for `:open`, `:w`, `:sample` and the rest. Typing a value's first
letters narrows the list to the values starting so; letters further in find it when none do.
`↑` and `↓` (or `^p` `^n`) go back through the commands you've run, those starting with what's
typed, and they're kept from one run to the next in `~/.local/state/lattice/history`. Values are checked as you type: a
word that can no longer be right is underlined, and what was expected is shown on the right.

| Commands | |
| --- | --- |
| `:rot N`, `:drift N`, `:rev`, `:tr N` | add a rule to the track, optionally `every N [OFFSET]` or `pass K`, or as they're shown, `/N[+OFFSET]` or `@K` |
| `:pop`, `:clear` | remove the last rule, or all of them |
| `:len N` | track length (1–256); in the conductor, its length |
| `:bpm N`, `:step N`, `:oct N` | tempo, auto-step, keyboard octave |
| `:help` (`:h`) | the help page: every key and command |
| `:manual [TOPIC]` (`:man`) | the manual, at its contents or at TOPIC: a section about it, else where it's a command, else where it's mentioned; again for the next place |
| `:click`, `:follow` | toggle the metronome (saved with the song), following the playhead |
| `:rec` | toggle recording, like `R` |
| `:layout NAME` | how notes lie on the keyboard in insert mode: `piano`, `wicki`, `fourths` or `scale` |
| `:vel N`, `:scale NAME [ROOT]` | the velocity notes are played and entered at (00–99, as the `vel` field shows it); the scale the numpad's pads play outside a group |
| `:count N` | bars counted in before recording (0–4, 0 for none; 1 to start) |
| `:keys` | toggle showing the keys as they're pressed, above the status line, for screencasts |
| `:name NAME` | rename the track |
| `:group-name NAME` | rename this track's group |
| `:level DB`, `:pan P`, `:send DB` | mixer, for every track the selections reach: level (-60 to 6 dB), pan (-100 left to 100 right), send to the send bus (-60, off, to 6 dB); on the mixer and fx pages, `:level` sets a bus too |
| `:set VALUE` | on the mixer, fx or instrument page, set the value under the cursor (also `c`): a number, or a choice by name |
| `:inst INSTRUMENT` | the instrument for every track the selections reach (or the mixer cursor's): `synth`, `switched`, `monochord`, `krell`, `sampler`, `chop`, `kick`, `snare`, `hat`, `clap` or `tom` |
| `:arm [PASSES]`, `:arm! [PASSES]`, `:disarm` | arm the track for the looper, or disarm it: with record on (`R`), playing records the audio input a pass of the track at a time into a chop track, until stopped, or for PASSES passes and then disarms (`:arm 1` for just one). `:arm` records over what a chop track plays (unless its notes have all been deleted); `:arm!` starts a new loop |
| `:input [DEVICE]`, `:output [DEVICE]` | the device the looper records, or that you hear Lattice through: type `:input ` to list them, then a number, or part of a name; 0 is the system's default. Remembered between runs; with nothing typed, says which is in use |
| `:latency MS` | move what the looper records earlier (negative) or later, if it's out of time with what you heard (-500 to 500); remembered for each input |
| `:chop [PATH]` | cut a WAV file into a piece a step, on every track the selections reach (or the mixer cursor's), each step playing its own; with no path, the last selection exported |
| `:sample [PATH]` | play a WAV file on a sampler, on every track the selections reach (or the mixer cursor's); with no path, the selection last exported with `:export-sel` or `:export-loop` |
| `:fx-add EFFECT` | add an effect (`delay`, `reverb`, `filter`, `ensemble`, `phaser`, `drive`, `comp` or `pitch`) after the mixer or fx page cursor, or at the end of this track's chain |
| `:chain-save NAME` | save the strip's chain of effects (on the pattern page, the cursor's track's) as `~/.config/lattice/chains/NAME.toml`, to load on any strip of any song |
| `:chain-load NAME` | put a saved chain on the strip, in place of its own (its effects' locks go with them); Lattice comes with `shimmer` |
| `:preset NAME` | put a preset on the track (the instrument page's or the cursor's): its instrument, effects, macros and face, in place of the track's own; Lattice comes with 23: type `:preset ` to see them |
| `:preset-save NAME` | save the track's instrument, effects and macros, and its face, as `~/.config/lattice/presets/NAME.toml`, giving the track a face titled after `NAME` if it has none |
| `:face TITLE` | title the track's face |
| `:face-art [PATH]` | draw the track's face from a text file, up to 8 frames of 8 rows of 31 characters, each after an empty line, giving the track a face titled after the file if it has none; with no file, take the art away |
| `:face-show [SETTING]`, `:face-hide [SETTING]` | put one of the instrument's settings on the track's face, beside its macros, or take it off; the one under the cursor if none's named |
| `:fx-send LANE [DB]` | on the fx page, add a send after the cursor into lane `LANE` (`B`, say); the next letter after the last lane adds one. Into its own lane or one to its left it's feedback, at -6 dB unless given, and at most 0 dB |
| `:lock PARAM VALUE` | lock a setting of the track's instrument or effects on every step the selections reach: `:lock cutoff 800`, `:lock filter.mix 0.5` (or `2.mix`, the second effect's). `:lock cutoff 200..4000` sweeps it across each selection, `:lock cutoff ~4000` slides there, arriving on the step, and `:lock cutoff --` puts it back to the mixer's. With no value, `:lock cutoff` gives the track a column for it. The cursor's locks are shown in the status line |
| `:macro NAME` | add a macro to the track: one knob, from 0 to 1, that turns several of its settings |
| `:map NAME [DEPTH]` | on the mixer or fx page, have macro `NAME` (or its number, `1`) turn the setting under the cursor, by `DEPTH` of its range (-1 to 1, 0.5 if left out) at full turn, adding the macro if it's new |
| `:unmap NAME` | stop macro `NAME` turning the setting under the cursor |
| `:unlock [PARAM]` | clear a setting's locks, or all of them, from the steps the selections reach; along a whole track, its column goes too |
| `:choke N` | choke group (1–8, 0 for none) for every track the selections reach: within a track group, a hit cuts off the other tracks with the same number, as a closed hat stops an open one |
| `:mute`, `:solo` | toggle mute or solo for every track the selections reach (also `tm`, `ts`) |
| `:w [PATH]`, `:wq [PATH]` | save the song (to a `.lattice` file, added if PATH has no extension), save and quit |
| `:pattern NAME` (`:pat`) | go to a pattern, by name or number; while playing, it's cued and takes over at the next bar (the header shows `pat A → B`) |
| `:pattern-new [NAME]` (`:pn`), `:pattern-dup [NAME]` (`:pd`) | add an empty pattern after this one, or a copy of this one, and go to it; it's added to the end of the song, played once. Named A, B, C… unless named |
| `:pattern-mute` (`:pm`) | mute or unmute every track the selections reach in this pattern only (also `tM`) |
| `:pattern-name NAME`, `:pattern-remove` | rename this pattern; remove it, and its places in the song |
| `:pattern-meter [BEATS\|song]` | this pattern's own beats in a bar (`5`, or `2+2+3`), so a song can change time: its bars, the grid's bar lines and the click follow it, while steps a beat stay the song's, so the tempo means the same throughout (for 4/4 into 7/8, make the beat an eighth). `song` puts it back in the song's meter; on its own, it says which it's in. A pattern of so many `:bars` keeps its steps |
| `:meter [BEATS [STEPS]]` | the song's meter: beats in a bar (4 at first; 3 for a waltz, 5 or 7 for odd times, or a grouping like `2+2+3`, whose first beats the click accents and the grid marks) and steps in a beat, three of which make compound time (`:meter 2 3` is 6/8, `:meter 4 3` is 12/8). The music doesn't move: patterns keep their steps, and a change of steps per beat changes the tempo with it, so 200 BPM at two steps a beat becomes 100 at four. On its own, it says what the meter is |
| `:bars [N \| auto]` | how long this pattern plays in the song: N bars, or `auto` (the default), as long as its longest track, rounded up to a whole bar. Looping on its own, a pattern never restarts its tracks, so tracks of different lengths keep drifting against each other |
| `:export [PATH] [BARS]` | render the song to a 24-bit WAV, faster than real time, while you carry on: each pattern in the song's order, for its length, then the tail until it's quiet; or with BARS, this pattern for that many bars. With no path, it goes beside the song's file |
| `:export-loop [PATH]` | render just the selection, exactly its length, with its tail mixed back into the start, so it loops cleanly; `:chop` or `:sample` with no path then plays it |
| `:export-sel [PATH]` | render just the selection (its steps, on its tracks, heard even if muted) once, with its tail, for a sample |
| `:o PATH`, `:o! PATH` | open a song, or start a new one at PATH; `!` discards unsaved changes |
| `:n [PATH]`, `:n! [PATH]` | start an empty song: one track of 16 steps, no effects, at the same tempo, to be saved at PATH (or wherever `:w PATH` says); `!` discards unsaved changes |
| `:q`, `:q!` | quit, refusing if there are unsaved changes; `!` quits anyway (`C-c` asks, and quits if pressed again) |
| `:track-add [NAME]` (`:ta`) | add a 16-step track after this one (up to 16 tracks; also `ta`) |
| `:track-remove` (`:trm`), `:track-move ±N` (`:tmv`) | remove this track, move it left or right (also `tH` `tL`) |

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
`filter`, `ensemble`, `phaser`, `drive`, `comp`, `pitch`) adds an effect after the cursor and `d`
removes it. `ensemble` is a string machine's chorus, three wavering copies that make one player a
section and spread it in stereo; `phaser` sweeps notches through the sound, 4 to 12 stages, with
feedback to sharpen them. `tm` `ts` still mute and solo, and `gp` goes back to the pattern.
Instruments, effects and bus levels are saved in the song.

The **instrument page** (`gi`) shows every track's instrument side by side, a column each: its
name, its settings, a line each with a bar, the track's macros, and last its effects. `h` `l` move
across the tracks on the same line, `j` `k` down a column, and `w` `b` from part to part. `^a` `^x`
turn the setting or macro under the cursor and `c` sets it, or on the instrument's name, changes
the instrument; `:map`, `L` and recording locks work as on the mixer, and `enter` on the effects
opens them on the fx page. A track with a preset's face shows that instead: its title, with what
it's played on, its macros and the settings it puts forward, under art drawn from the title, one
of the opening's fractals mirrored to look back at you. It's a faint outline at rest; playing lights it
from the middle out as loud as the track is, each note sends a ring out, and the note's pitch
picks its colour, round the colour wheel as it goes round the circle of fifths. The first macro
zooms in on it and the second hurries its flow. `enter` on the title opens all the instrument's
settings, and again goes back to the face.

`i` on the instrument page plays the cursor's track live on the computer keyboard, laid out as in
insert mode (`{` `}` change octave, as they do everywhere, and the numpad's pads play too), to try a sound out without
entering any notes. The status line says `PLAY`. The arrows still move across the tracks and down
the column, and `^a` `^x` still turn the setting or macro under the cursor, so a sound can be
shaped while it's played; `esc` stops. With record armed (`R`, which starts playing too) and the
song playing, the notes go in at the playhead, and the settings you turn are locked as you go.

The **fx page** (`gf`) shows one strip's effects the way the pattern shows notes: the sound goes
down through them, each effect's settings on the lines under its name, and they can branch into up to four **lanes**, side by side. A **send** row,
`→ B -3.0 dB`, copies the sound at that point into lane B; added with `:fx-send B`, it starts lane
B if there isn't one. A send into its own lane or one to its left is **feedback** (`↺`), heard
again 256 samples later and kept to 0 dB or below, so it dies away. Each lane ends in an **out**,
its level into the strip, along the bottom. `j` `k` move line by line, so `^a` `^x` (or `c`)
turn each setting, send or out as you pass it, `w` `b` jump from effect to effect past their
settings, and `h` `l` move across the lanes on the same line. `:fx-add` (or `o`, which starts
typing it) adds an effect after the cursor's, and `d` on an effect's name or a send removes it, or on an out, the whole lane. `L`
locks settings as on the mixer. `(` `)` change strip, and `gm` and `gp` go to the mixer and the
pattern.

**Macros** are knobs that each turn several of a track's settings at once. `:map bloom` on a
setting, on the mixer or fx page, has the macro `bloom` turn it, by half its range when `bloom`
is all the way up (`:map bloom -1` turns it down by all of it). The fx page shows a track's macros
in a column left of lane A, each with the settings it turns under it: `h` from lane A goes there,
`^a` `^x` turn a macro or how far it turns a setting, and `d` removes either. A setting a macro
turns is marked with the macro's number (`*` if more than one turns it), on the fx page and the mixer, and lit while the cursor's on it among the
macros. Settings keep their
own values and the macros move them from there, so turning a setting by hand still works. A macro
locks like a setting, by its name (`:lock bloom 1`), with `L`, or by turning it with record armed.

`:chain-save wash` saves a chain to load on any strip with `:chain-load wash`. Lattice comes with
one, `shimmer`: a long reverb in lane A that sends into lane B, which high-passes it and shifts it
up an octave with the `pitch` effect, then feeds it back into the reverb, so every trip round adds
an octave above, rising as it fades, as on Eno and Lanois's records. Put it on a slow pad, and try
the feedback send's level, the reverb's decay, or `pitch` at 7 for fifths.

**Presets** make an instrument into one of its own: `:preset night-bus` puts an instrument, its
settings, the effects after it and the macros that turn them on a track all at once, in place of
the track's own, and `:preset-save cold-iron` saves a track's to put on any track of any song.
Lattice comes with 23, each with its effects, a macro or two and a face:

- synths: `night-bus` (a dark saw pad in the shimmer, which `dawn` opens), `paper-lanterns` (a slow
  pad through a swaying band-pass), `cathedral` (a square organ in a stone nave), `sodium` (a bright
  plucked lead trailing echoes), `wasp` (a driven square lead), `acid-rain` (a 303's squelch),
  `rubber` (a squelchy square bass) and `sub-zero` (a low, round sub bass)
- strings: `glasshouse` (a bright steel string in a glassy room), `dust-harp` (a soft, dark nylon
  string, far off), `cello-ghost` (a bowed string, an octave's ghost above it), `tidewater` (a
  twangy steel string off a slapback echo) and `monsoon` (a sitar in an echo, with `rain` to
  thicken it)
- drums: `foundry` (a hard, hot kick), `velvet-boom` (an 808's long boom), `tin-roof` (a 909
  crack), `brushfire` (a snare like brushes), `cicada` (a ticking, swarming hat), `brass-bell` (the
  hat's metal rung like a bell), `crowd-pleaser` (a clap from a crowd), `thunderhead` (a low,
  rolling tom in a storm) and `hollow-log` (a knock on wood)
- Krell: `great-machine` (all five choirs drifting apart over a glass body, down echoing halls;
  `alien` drifts them further)

A preset gives its track a **face**: a title (`:face Cold Iron`), and the controls it puts
forward, its macros and whichever settings `:face-show cutoff` adds. The track keeps it, and the
song saves it, until its instrument changes for another kind. Its art, lit by what the track
plays, is a fractal its title picks, unless it's drawn by hand: a preset's file can give it up to
8 frames of 8 rows of 31 characters, `art = ["⢠⣶⣦ ⣿", …]`, each frame after an empty row (`""`),
as `great-machine` draws the Great Machine of the Krell from *Forbidden Planet*, and
`:face-art lamp.txt` draws a track's face from a text file the same way, each frame after an
empty line. A track with no
face shows its engine's own: the Rhythmicon's disc spinning in front of its lamp, the monochord's
string ringing, Switched's patch cables swaying, an oscilloscope's wave for the synth, a tape
machine's reels for the sampler and Chop's slices lighting their pads. The art moves on a frame
with each beat, and with each note the track starts, and slowly when the song's stopped.

**Locks** automate settings by step, as on Elektron machines and in trackers' columns: a track
can lock up to four of its instrument's or effects' settings with `:lock`, each in a column of its
own. A lock holds from its step until that setting's next lock; `--` puts the setting back to the
mixer's, and `~` slides to a value, arriving on its step. The track under the cursor opens out to
show its columns beside its notes and their velocity: each value from 00 to 99 across the
setting's range, and `╵` where one carries on. Where two columns' settings share a name, an
effect's column starts with the effect's number, as `:lock 2.cutoff` would (`2cu`). Other tracks mark a step with locks `◆`, and `te`
keeps tracks open. In a field, `i` and two digits set its value on each cursor's step and move
on, `~` first slides, and `-` puts the setting back; `^a` `^x` nudge the values the selections
reach, `d` `y` `p` work on the field alone, and `&` fills evenly between a selection's ends. While
a lock plays, the mixer page shows its value, in the accent colour and
marked `◆`, in place of the track's own. Or select the steps, go to
the mixer (`gm`), press `L`, and change the track's settings by ear; `L` again locks each setting
that changed onto the selected steps and puts the track's own settings back (`esc`, or leaving the
mixer, cancels). To perform them instead, arm record (`R`) on the mixer page and play: each
setting you change is locked on the step that's playing, and heard at once, so a sweep played on
`^a` goes in step by step. Each pass is one undo, as with recorded notes. A lock on a step with no
note changes the setting on its own, so a few locks down an empty stretch move an effect along.
In the demo, the second stab in each bar is brighter and rings, from its locked ladder and
scream, and the first puts them back.

**Gestures** are what a tracker's effect columns do: things that happen once on a step. An open
track has two gesture fields on each step, after the velocity (`fg` `fG`); in insert mode, type
one, and it goes in once it's whole, or with `enter` or `esc`. Note gestures act on the step's
note, or on the one still sounding when the step has none, timed in 24ths of a step:

| Gesture | |
|---|---|
| `R3` | plays the note 3 times (2–9) within the step |
| `r3` | as `R3`, each hit quieter |
| `D8` | starts the note 8/24 of a step late (1–23), for flams and pushed feels |
| `X6` | cuts the note 6/24 of a step in (1–23) |
| `A37` | plays the note, then 3 and 7 semitones up, in turn, a sixth of a step each, for as long as it's held |
| `!` | plays the note at full velocity |

Sound gestures change how one note sounds, for that note alone. Where the track's instrument
can't do one, the field shows it dimmed, the status line says so, and it does nothing:

| Gesture | |
|---|---|
| `<` | on a `sampler` or `chop` track, plays the note's sound, or its piece, the other way round from its `direction`: backwards, usually |
| `O8` | on a `sampler` or `chop` track, starts the note 8/16 of the way into its sound, or its piece (0–15) |
| `G6` | on a `synth`, `switched` or `monochord` track, slides the note from the last one's pitch over 6/24 of a step (1–23) |

Conditions decide whether the step plays at all, its note, locks and other gestures with it:

| Gesture | |
|---|---|
| `?50` | plays on 50% of passes (1–99), the same ones each time the song plays |
| `1:4` | plays on the first of every 4 passes; `2:4` the second, `3:3` the third of three (up to 9) |
| `F` | plays only during a fill |
| `F-` | plays only when there's no fill |

A **fill** plays while the backtick key's held down with the song playing, and the header shows
`FILL`. It starts and ends at once, on any step, so `F` steps can be a roll, or a jump (`F` with
`J12`) somewhere else in the track, and `F-` the steps it takes over from. Terminals that don't
report keys coming up (Lattice asks for the kitty protocol) can't tell it's held: there,
backtick starts a fill and pressing it again ends it. Stopping ends one too. `F` might become
`F-`, so it goes in with enter.

Playhead gestures move the track's own playhead, so each track can loop, wait or turn round on its
own, and are shown in the accent colour:

| Gesture | |
|---|---|
| `J12` | after this step, carries on from step 12 (`J0` loops the track there) |
| `J?` | after this step, carries on from any step, the same one each time the song plays |
| `W2` | plays this step 2 more times before going on |
| `B` | plays the track backwards from here, or forwards again |
| `*2` `/2` | plays the track 2 (to 4) times as fast, or as slow, until the next speed or the top of its loop; `*1` is the song's speed |

The status line says what the gestures under the cursor do, `^a` `^x` change their amounts, and
`d` `y` `p` work on them as in any field. Closed tracks mark a step with gestures `◆`. A muted
track's playhead still goes where its gestures send it, so it's in place when it comes back. In
the demo, the hats ratchet into each bar, the second clap is pushed late, the acid's octave is
gated, it slides down to its E♭ (`G8`) and its A♭ goes round its chord, and the pluck turns round
on its G, going back and forth, and glides into its F. In the peak, hold backtick for a snare
roll, the second clap giving way to it, while the ride drops its last hit on half the passes.

The **conductor** is a lane of each pattern for what belongs to the whole song: left of the
first track, beside the step numbers, two fields on each step. `h` from the first track's notes
(`gh`) goes into them, or `fC` from any track. In insert mode, type a gesture: `T140` sets the tempo from that
step, `~140` slides there, arriving on the step, `T--` goes back to the song's `:bpm`, `X` stops
the song at the end of the step, `P3` cues pattern 3 for the next bar, `M2` mutes track 2 and `M-2`
unmutes it, and `S2` `S-2` solo and unsolo it. Those mutes and solos last until the song stops,
leaving the song's own mix as it was; `tm` or `ts` on the track takes over from them. Each goes in once it's whole, or with `enter` or `esc`. `^a` `^x` nudge a tempo, `d` `y`
`p` work as in any field, and `&` fills tempos between a selection's ends. The conductor has its
own length, as long as the pattern until `:len` in it changes it, so a tempo change can come round
every bar or once in four. While it plays, the header shows the tempo it's set. It's shown when
it has gestures or the cursor's in it, and `:export` plays it, all but its `P` cues, as it
plays the arrangement: it says when it leaves them out.

The **song page** (`gs`) is the arrangement: the patterns in the order they play, each entry
repeated some number of times. A strip across the top shows the whole song, each entry as wide as
it plays for, coloured by its pattern; below it, the song a bar to a row with time going down:
each bar's time and number, the entries as blocks of rows in their pattern's colour (`├` where a
repeat starts), and a column for each track showing how busy it is in each beat of the bar (`·`
for nothing, `─` where it's muted). Under it are the patterns there are. `j` `k` move a bar, `w`
`b` from entry to entry, `gg` `ge` to the ends, `h` `l` across the tracks (the columns scroll to
keep the cursor's in view), `^a` `^x` change the entry's pattern, `+` `-` how many times it
repeats, `o` `O` add an entry after or before, `d` removes one (`y`
copies, `p` `P` paste after and before, so `d` then `p` moves an entry), and `ret` goes to the
entry's pattern to edit it, with the cursor at the bar it was on, and on its track if it was on a
track's column. On a track's column, `m` mutes the track in the cursor's entry alone, so it can
sit out a section without a pattern of its own; its bars there show `─`, and while the entry
plays, so does its name on the pattern, mixer and instrument pages. `s` solos it there
instead, muting the rest, and either again undoes it. `v` selects bars and tracks from the
cursor, as on the grid, and `x` the cursor's entry (again, the next too); then `m` `s` act on
every track and entry selected, and `d` `y` `p` `|` `^a` `^x` `+` `-` on every entry. The track
menu (`t`) isn't used here. `c` on
the entries renames the cursor's pattern, and on the song page `:bars` and `:pattern-name` change
the cursor's pattern rather than the one being edited. `|` loops the song round the
selected entries, or the selected bars of one (with nothing selected, the cursor's bar); on the
loop, it stops. A bar down the left (`↻` `┃`) marks the bars it covers, and the song goes back to
its start at its end. `space` plays the song from the cursor's entry to the end, following
along in the header (`song 2/3`) and on the page, where the playhead moves along the strip and
down the bars;
`:export` renders it. Anywhere else, `A-space` plays the song from the entry playing the pattern
you're on, and `^space` from its start.

**Instruments:** each track has one. `synth` is a plain test synth. `kick`, `snare`, `hat`, `clap` and `tom` are
Rhythmicon, the drum engine: one drum per track, played at its tuning by C-4 and tuned up or down
by other notes. Each has a model (`tight` `boom` `hard` for the kick, `tight` `fat` `crack` for
the snare, `metal` `ring` `sizzle` for the hat, `tight` `room` `crowd` for the clap, `tight`
`round` `hard` for the tom) and four controls within it (see
[docs/DESIGN.md](docs/DESIGN.md)). The other engines go by their own names, each a nod to a
pioneer of electronic music. `switched`, after *Switched-On Bach*, is a modular: a stack of saws (`one`, a `pair`, a `swarm` of seven, organ `stops` or
`fifths`) detuned by `spread` and spread in stereo by `width`, hard-synced by `grit` into a sync
sweep, through a four-pole ladder filter (`ladder`) whose `scream` drives its resonance until it
sings on its own; `contour` sets how far the envelope opens it, and `snap` shapes the envelope
from a pluck through an organ's held note to a slow swell. `monochord`, after Pythagoras'
one-stringed instrument, is a plucked or bowed string, modelled as a waveguide: models `nylon`, `steel` (pairs of strings a few cents apart, spread in stereo)
and `sitar` (buzzing against its bridge), with pluck (soft thumb to hard, clicking pick), position (along
the string), decay, tone and bow (a real bowed string while the note is held, light to heavy;
it takes over from the pluck as it's turned up, and long notes suit it). Strings ring on after a
note ends, as a harp's do; a note-off only lifts the bow. A `G` gesture glides a synth's, a switched's or a
monochord's note in from the last one's pitch, a string plucked there and slid along. `krell`,
after the vanished civilisation of *Forbidden Planet*, is an alien harpsichord: each key plucks a
choir of strings, as the `stops` choose (`8'`, `8' 8'`, `8' 4'`, `16' 8' 4'` or `16' to 2'`,
each choir somewhere different in stereo), which `drift` pulls out of tune with each other and
sets wavering, from a real harpsichord's shimmer to something stranger. `quill` plucks them from
soft leather to a hard metal plectrum, `decay` sets how long they ring while the key's held, and
`damper` how fast they stop when it comes up. Under them, the `body` rings in sympathy, its lowest
mode at `hull` (tune it to the song's key, and it rings with the notes in it), from wood, in
tune and soon still, to `glass`, out of tune and ringing on. `sampler` plays a WAV file (`:sample kick.wav`; 8 to 32-bit, or float, mono or
stereo, any rate). C-4 plays it as recorded and other notes pitch it, or with `slices` above 1 it's
chopped into equal slices, C-4 playing the first and each note up the next, at the file's pitch. Its
`mode` is `one-shot` (to the end, whatever the note's length), `gate` (until the note ends) or
`loop` (round the region until the note ends); `start` and `end` set the region, `direction`
plays it backwards (and the `<` and `O` gestures turn one note round or start it part way in), and `tune`, `attack`, `release` and `gain` do what they say. So
`:export-sel`, then `:sample` on another track, turns a bar into a sample to play.

`chop` is for loops: `:export-loop` on a selection, then `:chop` on an empty track, or `:chop
break.wav`, cuts the file into as many equal pieces as the track has steps and has each step play
its own, so it sounds as recorded. The track shows each step's piece number and its waveform, drawn
down the column in braille, and the pieces are notes like any other: `y` and `p` copy them, `d`
silences one, `C-a` and `C-x` move a step to the next or last piece, and rules such as `:rot` and
`:rev` rearrange the loop each pass. A note plays its piece at the file's speed until the note ends,
playing on into the next if it's longer, so the loop follows the tempo without stretching. `~`
reverses the selected steps, turning each piece round too with a `<` gesture (marked `◂`), so the slice plays
backwards; `*` repeats the first step, or the first few, through the selection, for a stutter; `A-(`
and `A-)` rotate the selection. A `<` gesture turns a single step's piece round, and `O8` starts it half way in;
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

Lattice shouldn't crash, but if it does, it saves the song first if it had unsaved changes: beside
its file as `song.rescue.lattice`, or as `rescue.lattice` beside the log if it was never saved, and
says where once the terminal's back. The song's own file is left as it was.

Scripts and agents can drive a running Lattice with `lattice ctl`, through a socket only you can
open (`$XDG_RUNTIME_DIR/lattice/<pid>.sock`):

```sh
lattice ctl screen                  # the screen as text
lattice ctl state                   # page, cursor, tracks, transport and status line, as JSON
lattice ctl keys 'gp:bpm 140<ret>'  # type keys; <ret>, <esc>, <C-a>, <space>, <lt> for <
lattice ctl cmd 'fx-add delay'      # run a command, as : would
lattice ctl log 20                  # the end of the log
```

What it types goes the same way as your own keys, so `u` undoes it. It can't save, export, open or quit,
or start over with `:new!`, unless Lattice was started with `--allow-agent-save`. With more than
one Lattice running, `--pid` says which.

The demo song is techno at 130 BPM in F minor, in six sections (`gs` shows them; `^space` plays the
lot). It opens on `peak`, which has everything: drums folded into a group with drive and a
compressor on its bus (`tz` unfolds it), hats sharing a choke, a five-step tom and a seven-step
sitar drifting against the bar, a harpsichord pad over a ringing glass body, dub stabs on the
modular into the send's dotted-eighth echo, and an acid line through a drive. The bass, stabs and
pad follow a chord change every four passes with `transpose` rules.

Tracks can be added, removed and moved while playing, and undo brings a removed track back with
its notes and its instrument. When there are more than fit across the terminal, the grid scrolls
sideways to keep the cursor's track in view, with `‹` `›` showing there are more.
