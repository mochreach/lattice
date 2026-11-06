# Introduction

## How to read this manual

This manual explains everything Lattice does, in full: every key, command, argument and range,
with examples, drawings of the screen, and the reasons things are the way they are. It's not the
place to start from scratch. For that, run the tutor (`:tutor`), which teaches the keys a lesson
at a time by having you use them. Come here when you want to know exactly how something works.

Each chapter covers one part of Lattice:

- this one, what Lattice is and why it exists, the screen, the modes and pages, the command
  line, undo, playing and learning;
- [Patterns](02-patterns.md): the grid, moving, selecting and editing notes;
- [Tracks](03-tracks.md): adding, moving, grouping and folding them;
- [Rules](04-rules.md): transforms that reshape a track each time it loops;
- [Gestures](05-gestures.md): what happens once on a step, and the conductor;
- [The song](06-song.md): patterns, the arrangement and the song page;
- [Instruments](07-instruments.md): the engines and their settings;
- [Mixer and effects](08-mixer-and-effects.md): levels, sends, chains, locks and macros;
- [Recording](09-recording.md): playing in live, the pads and the looper;
- [Files, export and configuration](10-files-and-config.md): song files, rendering to WAV, the
  config file, every action by name, and driving Lattice from scripts.

Inside Lattice, `:manual` opens this manual at its contents, and `:manual TOPIC` opens it at a
heading about the topic (see [Learning Lattice](#learning-lattice)). The same files read as
plain Markdown on GitHub, in `docs/manual`.

Some conventions:

- Keys are written as Helix writes them: `C-a` is control and `a`, `A-s` is alt and `s`,
  `ret` is enter, `esc` escape, `space` the space bar. `gp` means `g` then `p`, one after the
  other. Upper-case letters mean shift: `U` is shift and `u`.
- Commands are written with their colon, `:bpm 140`, and typed after pressing `:`. In a
  command's description, `NAME` in capitals is something you fill in, and `[BARS]` in brackets
  is optional.
- Drawings of the screen are copied from the running app, trimmed to the part that matters.
- Each key also has a name, `toggle_play` for `space`, which is what the config file binds keys
  to. The names are listed in [every action, by name](10-files-and-config.md#every-action-by-name).

## What Lattice is

Lattice is a music tracker for the terminal: a program for writing music as a grid of notes,
played by its own instruments and effects, with keys modelled on the Helix text editor. It has
a sample-accurate sequencer, a sound engine written by hand, and rules that reshape a pattern
each time it comes round, so a loop can change as it plays.

### Why Lattice exists

I've always liked the idea of a tracker; they're intuitive and precise, with a wonderful
complexity bubbling below the surface. I have used both software and hardware trackers,
spending a lot of time with both Renoise and the Polyend Tracker. Unfortunately, I've
always come away with a sense of cognitive dissonance, as I would consistently come
across implementation details that defied my expectations. Now, this is just a "skill
issue", right? Well, definitely: if I had continued to spend time working with them,
I would have got to a point where it felt natural to use them. However, in my mind, there
was the shadow of a piece of software that would match my expectations and enable me to
enter a state of tracker-induced creative bliss. I wrenched this idea out from deep in my
id, and thus, Lattice was born.

The intended audience for Lattice is me: every feature and tool has been created to
satisfy some, potentially unreasonable and fleeting, creative desire. All I care about is
translating my musical ideas out of my head and into the world. That's it. Every choice
serves that, so I've been uncompromising about how easy it is for anyone
else to use. All the same, I think it's a fascinating piece of software, and an interesting
way to make music. The documentation is very comprehensive; there is a tutor mode, and it's
made to explain what options are available and what they do, so it is quite usable. I hope
other people get something from it and enjoy it, and I'm curious to see how
they use it. But that's the philosophy: Lattice is made for me first.

An interesting aside here: I have made Lattice with the diligent assistance of AI. Most of
the code has been written via an LLM. **I could not have made this before agentic programming.**
Software this complex, feature-rich and performant, written for an audience of one person,
would have been a preposterous notion until very recently. Now, not only is it possible,
but it's accessible to most people. I think that presents an exciting and important
opportunity: go and make software that works for you.

I'm Chris Wells Wood, and if you'd like to hear more from me, I make videos on my YouTube
channel, [Mo chreach!](https://www.youtube.com/@mochreach), and write at
[mochreach.dev](https://mochreach.dev).

### Trackers, from the Amiga on

A tracker shows music as a table read from top to bottom. Each column is a track, a voice that
plays one thing at a time; each row is a step, a fixed slice of time, often a sixteenth note;
and each cell holds a note, written as its name and octave (`C-4`, `F#2`), with columns beside it
for how loud it is and anything else that happens on that step. Time runs down the screen, and a
playhead runs down it as the music plays.

The form comes from the Commodore Amiga. In 1987 Karsten Obarski's Ultimate Soundtracker used
the Amiga's sound chip, Paula, which played four channels of 8-bit samples, to play music laid
out in four columns of notes. Its song format, MOD, and the programs that followed it,
NoiseTracker and ProTracker on the Amiga, then Scream Tracker, FastTracker 2 and Impulse Tracker
on the PC, carried trackers through the demoscene and into game music, and Renoise and hardware
such as the Polyend Tracker carry them on today. Lattice keeps the tracker's grid, its notes
written `C-4`, its computer keyboard laid out as a two-row piano (`z` to `m` the lower octave,
`q` to `u` the upper), and its habit of doing things on a step: Lattice's gestures are its
effect columns, and `A37` arpeggiates up three and seven semitones as ProTracker's `037` did.

What Lattice adds is mostly about patterns that change. Each track has its own length, so a
seven-step line drifts against a sixteen-step bar (polymeter); rules transpose, rotate or reverse
a track on some passes and not others; and gestures can send a track's playhead elsewhere.

### Select, then act

Lattice's keys follow Helix, a modal text editor, rather than Vim. In Vim you name an action
and then what it acts on (`d` then a motion). Helix, which took the idea from Kakoune, turns that
round: you select first, then act, so you see what will change before it does.

On a grid this fits well. There is always a selection, a rectangle of steps across one or more
tracks; with nothing selected it's just the cursor. Motions move the cursor and collapse the
selection onto it, and in select mode (`v`) the same motions stretch it instead. Then `d`
deletes what's selected, `C-a` puts it up a semitone, `>` an octave, and the selection stays
put, so you can press the key again and hear the result each time. There can be several
selections at once, as in Helix, and every action applies to each.

Lattice departs from Helix where Helix's key makes no sense on a grid: `t` is a menu of track
actions rather than "select till a character", and `space` plays and stops, as in every
sequencer, rather than opening a menu.

### Why a terminal

Trackers were always keyboard programs, and many, Impulse Tracker among them, ran in text mode.
A terminal suits that: everything is a key away, the screen is a grid of characters already,
and it runs the same in a desktop terminal, a tiling window manager or over SSH. Lattice takes
its colours from the terminal's own palette, so it looks at home in whatever theme you use (see
[colours](10-files-and-config.md#colours)).

Lattice was first tried as a graphical program, and what was learned there, such as keeping the
clock in the audio thread rather than the interface, is written up in `docs/DESIGN.md`.

## Starting Lattice

```sh
lattice                        # the demo song
lattice song.lattice           # opens the song, or starts a new one there
lattice --tutor                # the tutor
lattice ctl screen             # asks a running Lattice for its screen (see lattice ctl)
```

With no file, Lattice opens a demo song: techno at 130 BPM in F minor, in six sections, opening
on its busiest, `peak`. It has no file of its own, so the header calls it `[scratch]` and `:w`
needs a path to save it.

With a file that exists, Lattice opens it. With a path to a file that doesn't, it starts a new
song there, as Helix does, and says `song.lattice (new file)`: the demo's tracks and
instruments, with no notes, saved at that path when you `:w`. A file that can't be read stops
Lattice before it starts, with the reason (see [song files](10-files-and-config.md#song-files)).

### Command-line options

| Option                 | What it does                                                         |
|------------------------|----------------------------------------------------------------------|
| `PATH`                 | open the song at `PATH`, or start a new one there                    |
| `--tutor`              | start the tutor, on a song of its own                                |
| `--config PATH`        | read that config file in place of `~/.config/lattice/config.toml`    |
| `--no-click`           | start with the metronome off, whatever the song says                 |
| `--show-keys`          | show the keys as they're pressed, above the status line              |
| `--no-splash`          | leave out the opening animation                                      |
| `--allow-agent-save`   | let `lattice ctl` save, open and quit (see below)                    |
| `ctl …`                | talk to a Lattice that's running, rather than start one              |

The options can come in any order. The first word that doesn't start with `--` (and isn't the
path after `--config`) is the song. Options Lattice doesn't know are ignored.

- `--tutor` starts the tutor at once, in place of the song: `lattice --tutor` is the same as
  starting Lattice and typing `:tutor`.
- `--config PATH` is for trying out a config without changing your own. A missing file named
  this way is reported in the status line; a missing default config is not, since you needn't
  have one.
- `--no-click` turns the metronome off for this run. Songs save whether the click is on, so the
  examples open without it anyway; `:click` turns it back on.
- `--show-keys` is for screencasts: the keys you press appear in a box above the status line,
  with repeats counted (`j×4`) and what you type on the command line as text, fading two
  seconds after the last. `:keys` turns it on and off while Lattice runs.
- `--no-splash` skips the opening. Any key skips it too, and does nothing else.
- `--allow-agent-save` lets scripts and agents driving Lattice through `lattice ctl` save, open
  and quit. Without it they can't overwrite your files or throw away your song: see
  [lattice ctl](10-files-and-config.md#driving-lattice-from-scripts-lattice-ctl).

### The opening

Each time Lattice starts, a fractal lattice grows out from the logo in the top corner until it
fills the screen, then sweeps away from the same corner to show the editor beneath. Each launch
picks its own pattern (Sierpiński's triangle and carpet among them), scale, shape and colours.
It lasts about two seconds; any key, or `--no-splash`, skips it.

## The screen

Here is the demo song as it opens, in a terminal 92 columns wide:

```
 LATTICE  PATTERN   ■ STOP    [scratch]   pat peak   bpm 130   oct 4   vel 100   step 1   pa

    ╭─ DRUMS ▸ ────────╮ ╭─ LOW ───────────────╮
    K  C  S  H  O  P  R  BASS        ACID        STAB        PAD         PLUCK
    16 16 16 16 16 5  16 16          12          16          32          7
                            -3dB  ▌     -7dB ▉      -9dB ▌      -12dB       -13dB
                              C          R15         L25        ▐L40         R45
                   r     tr-4/4+2    drift+2/2+1 tr-4/4+2    tr-4/4+2    drift-1/2
                   r     tr-2/4+3    tr+12/4+3   tr-2/4+3    tr-2/4+3
    ── ── ── ── ── ── ── ─────────── ─────────── ─────────── ─────────── ───────────

 00  ●  ·  ·  •  ·  •  •  ···         F-2         ···         C-4   16    C-5
 01  ·  ·  ·  ·  ·  ·  ·  F-1         F-2         ···         │           ···
 02  ·  ·  ·  ·  •  ·  •  F-1         F-3      ◆  ···         │           D#5
 03  ·  ·  ·  •  ·  •  ·  F-2         ···         C-4    2 ◆  │           ···
 04  •  •  ·  •  ·  ∙  •  ···         D#3      ◆  │           │           G-5      ◆
 ...
 NORMAL               v select · x rows · g goto · ] next · i insert · d delete · : command
```

The top line is the **header**; the bottom line the **status line**; between them, the page.

### The header

From the left, the header shows:

- `LATTICE`, then the page you're on: `PATTERN`, `MIXER`, `FX`, `INST`, `SONG`, `HELP` or
  `MANUAL`.
- The transport: `■ STOP`, `▶ PLAY`, or `◆ IN 4` … `◆ IN 1` while recording counts you in, the
  beats left as a band leader counts them.
- Badges for what's switched on: `● REC` while record is armed, `◆ FILL` during a fill,
  `ARMED BASS` or `● LOOP BASS` for the looper, `REPEAT` while the numpad's note repeat is held,
  and `MUTE PADS` when the pads mute tracks.
- The song's file name, or `[scratch]` if it has none yet, and `[+]` if it has changes that
  aren't saved. Both are named as in Helix's status line. Undoing back to where you saved takes
  the `[+]` away again.
- `pat` and the pattern being edited. While the song plays, it adds where the song is:
  `pat peak 1/2 · song 5/6` is the first of two times round `peak`, the fifth of six entries;
  `pat A → B` means `B` is cued to follow.
- `bpm`, the tempo: while playing, the tempo the conductor has set, if it has changed it.
- `oct`, the keyboard's octave for entering notes (`{` `}` or `:oct`); `vel`, the velocity
  notes are played and entered at (`:vel`); `step`, how far the cursor moves after each note
  (`:step`).
- `pattern as written` or `as playing` (`tab` switches between them), `follow on` or `off`
  (`:follow`), and `click on` or `off` (`:click`).

A narrow terminal leaves fields out, whole, rather than cutting one off part way: `click` goes
first, then `follow`, `pattern`, `step`, `oct` and `vel`, so the file, the pattern and the tempo
are the last to go.

### The page

The rest of the screen is the page. The pattern page, above, has a column for each track, with
its name, length, level and pan, and its rules, over its steps; the [Patterns](02-patterns.md)
chapter explains it all. Other pages fill the same space: see [Pages](#pages).

Boxes open over the page when there's something to choose from: the menu after a prefix key
such as `g`, the commands and values that fit what you're typing on the command line, and the
tutor's lesson.

### The status line

The status line, along the bottom, has two parts:

- On the left, the **mode**: `NORMAL`, `SELECT`, `INSERT`, or `PLAY` while the instrument page
  plays the keyboard. On the command line, the mode gives way to the line being typed. With
  several selections it also says which is primary and how many there are (`2/5 sels`), and
  while you type a count, the count.
- On the right, **hints**: the keys that matter most on this page and in this mode. On the
  pattern page in normal mode they're
  `v select · x rows · g goto · ] next · i insert · d delete · : command`; in select mode, the
  keys that extend and act on the selection; on the mixer,
  the mixer's keys.

```
 NORMAL                         v select · x rows · g goto · ] next · i insert · d delete · …
```

Above it, in the bottom-right corner, a **message** says what the last key or command did, or
what went wrong, in the accent colour (`hushed`, `written song.lattice`, `nothing to undo`). It
clears on the next key. It used to sit on the status line, after the mode, but a long one
crowded out the hints, and in a narrow window there was no room for either; in a box of its
own it wraps onto as many lines as it needs, and the status line keeps its room for the keys.

```
                                       ┌──────────────────────────────────────────────────┐
                                       │ unsaved changes: :w to save them, or :q! to quit │
                                       │ anyway                                           │
                                       └──────────────────────────────────────────────────┘
 NORMAL                         v select · x rows · g goto · ] next · i insert · d delete · …
```

## Modes

Lattice is modal, as Helix and Vim are: the same key does different things depending on the
mode, so that letters can be both commands and notes.

- **Normal mode** is where you start, and where `esc` brings you back to. Letters are actions:
  `j` moves down, `d` deletes, `u` undoes.
- **Select mode** (`v`, its action `select_mode`) uses the same keys as normal mode, but motions
  stretch the selection rather than moving it. `esc` or `v` again goes back to normal mode,
  keeping the selection.
- **Insert mode** (`i` at the start of the selection, `a` after its end, `c` to clear it first)
  turns the letter keys into a piano: `z` plays and enters a C, and the cursor moves on by the
  auto-step. `backspace` clears a step and moves on. `esc` goes back to normal mode. On the
  instrument page, `i` plays the keyboard without entering notes, and the mode says `PLAY`.
- **The command line** (`:`) takes a command, typed out: see [The command line](#the-command-line).
  `s`, `K` and `A-K` use the same line to ask for notes to select (`select:`, `keep:`,
  `remove:`).

Some keys work in every mode: the arrows, `pageup` `pagedown` `home` `end`, `esc`, the transport
(`space`, `C-space`, `A-space`, `C-k`, backtick), `{` `}` for the keyboard's octave, `tab`, `R`
to arm recording, the loop pedal (right Alt tapped alone, or `!`), and `C-c` to quit. Insert
mode's piano layout can be changed with `:layout` (`piano`, `wicki`, `fourths` or `scale`),
explained in [Recording](09-recording.md).

## Pages

Lattice has five pages for making music and two for reading. The `g` menu goes between them:

| Keys     | Page            | What it shows                                                     |
|----------|-----------------|-------------------------------------------------------------------|
| `gp`     | pattern         | the grid of notes, a column a track ([Patterns](02-patterns.md))  |
| `gm`     | mixer           | every strip's level, pan and meter, and the one under the cursor  |
| `gf`     | fx              | one strip's effects, in lanes, as the pattern shows notes         |
| `gi`     | instrument      | every track's instrument, side by side, and their faces           |
| `gs`     | song            | the arrangement: patterns in the order they play                  |
| `:help`  | help            | every key and command                                             |
| `:manual`| manual          | this manual                                                       |

`gm`, `gf` and `gi` open on the cursor's track, so `gf` on the bass shows the bass's effects.
`esc` on the help page or the manual goes back to the page you came from. On every page, the
transport, the command line and the `g` keys work; keys that would change something out of
sight, such as editing the pattern from the help page, do nothing, and the status line says so
(`y does nothing on the fx page`), so a key that seems unheard was heard, just not useful there.

## Counts

A number typed before a key repeats it: `4j` goes down four steps, `3>` puts the selection up
three octaves, `12C-a` up twelve semitones. While you type the number it shows in the status
line. `0` only counts after another digit, so `10j` works; counts go up to 9999.

A count before a prefix key carries through to the key after it: `3]m` goes three bars on.
Some keys use the count their own way: `2*` repeats the first two steps through the selection
rather than repeating `*` twice. A key bound to a command in the config runs the command once,
whatever the count.

## Prefix menus

Some keys don't act at once but wait for another, as Helix's do. While they wait, a box in the
corner lists what can follow, so you needn't remember it:

```
                                        ┌ goto ──────────────────────────────────────────┐
                                        │ g  first step                                  │
                                        │ e  last step                                   │
                                        │ h  first track                                 │
                                        │ l  last track                                  │
                                        │ m  mixer page                                  │
                                        │ p  pattern page                                │
                                        │ s  song page                                   │
                                        │ i  instrument page: every track's instrument,… │
                                        │ f  fx page: the track's effects, in lanes      │
                                        └────────────────────────────────────────────────┘
```

| Key | Menu       | What follows                                                            |
|-----|------------|-------------------------------------------------------------------------|
| `g` | goto       | the ends of the pattern and the pages                                   |
| `]` | next       | `b` beat, `m` bar, `n` note, `p` pattern                                |
| `[` | previous   | the same, backwards                                                     |
| `t` | track      | select, mute, solo, add, remove, copy, paste, move, group, fold, open   |

A key that isn't in the menu closes it and does nothing. `f`, `r` and `s` also wait for what
follows (a field, a note, a pattern of notes), but say so in their own ways. The config file can
add keys to these menus, or make new ones (see
[binding keys](10-files-and-config.md#binding-keys)), and the box shows them as they're bound.

## The command line

`:` opens the command line on the status line. Type a command and its values, then `ret` to run
it or `esc` to cancel; `backspace` on an empty line cancels too. Every command is listed on the
help page (`:help`) and explained in the chapter it belongs to.

### Finding the command

As you type the name, the commands that match are listed above the line, best first, each with
its values and what it does:

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ export       [PATH] [BARS]  render the song to a WAV file, by default as arranged        │
│ export-sel   [PATH]         render the selection to a sample in the song's samples folder│
│ export-loop  [PATH]         render the selection to a sample exactly its length, its tai │
└──────────────────────────────────────────────────────────────────────────────────────────┘
:ex                                                         tab complete · ret run · esc cancel
```

The match is fuzzy: the letters you type must come in order, but not together, so `:rv` finds
`rev` and `:pn` `pattern-new`. Matches at the start of a name and runs of letters together
count for more, and shorter names win ties. `tab` fills in each match in turn. A name must be
whole (or an alias) for `ret` to run it, so that a half-typed name never runs the wrong thing:
`:ex` then `ret` says `unknown command; did you mean :export?`, naming the match `tab` would
have filled in. A slip of a letter or two that matches nothing, such as `:lyaout`, is still
caught: the status line asks whether you meant `:layout`.

### Values

Once the command is named, what's still to type is shown faintly after the cursor, and shrinks
as you type over it, with what the command does on the right:

```
:tr 12 [every N [OFFSET] | pass K]                             rule: transpose, -128 to 127
```

The values that fit the word you're typing are listed above it in turn, for `tab` to fill in:
instruments after `:inst`, effects after `:fx-add`, the choices after `:layout`, patterns after
`:pattern`, and files and folders after `:open`, `:w`, `:sample` and the rest. Typing a value's
first letters narrows the list to those starting so.

Values are checked as you type. A word that can no longer be right is underlined in the error
colour, and what was expected is shown on the right:

```
:bpm 500                                                                       bpm is 20 to 400
```

A half-typed word that could still become right, such as `4` on the way to `40`, isn't marked.
Commands that take text, a name or a path, take the rest of the line, spaces and all, so
`:name lead synth` names a track `lead synth`.

### History

`up` and `down` (or `C-p` and `C-n`) go back and forth through the commands you've run. With
something typed, they go through only those that start with it: `:ex` then `up` finds your last
export. A command run again moves to the end, and the last 500 are kept from one run to the next
in `~/.local/state/lattice/history`. Commands sent through `lattice ctl cmd` aren't added.

### Aliases

Some commands have a short name as well, as in Vim and Helix:

| Alias               | Command                | Alias        | Command          |
|---------------------|------------------------|--------------|------------------|
| `:w`                | `:write`               | `:q`         | `:quit`          |
| `:wq`               | `:write-quit`          | `:q!`        | `:quit!`         |
| `:o`                | `:open`                | `:o!`        | `:open!`         |
| `:n`                | `:new`                 | `:n!`        | `:new!`          |
| `:h`                | `:help`                | `:man`       | `:manual`        |
| `:pat`              | `:pattern`             | `:pn`        | `:pattern-new`   |
| `:pd`               | `:pattern-dup`         | `:pm`        | `:pattern-mute`  |
| `:ta`               | `:track-add`           | `:trm`       | `:track-remove`  |
| `:tmv`              | `:track-move`          |              |                  |

A `!` after a command means "even though it throws something away": `:q!` quits with unsaved
changes, where `:q` refuses.

## Undo and redo

`u` undoes the last change, and `U` redoes it. Undo takes back the song as a whole: notes,
tracks added, removed and moved (with their instruments), rules, the mix, effects, patterns and
the arrangement, and the selections as they were, all in one step. Lattice remembers the last
500 changes. A new change after undoing clears what could be redone.

What counts as one change:

- each key or command that edits, however much it touches: `d` on a selection across eight
  tracks is one undo;
- each pass recorded live, so `u` takes out a bad take and keeps the passes before;
- in insert mode, each note entered.

Moving the cursor, changing pages, playing and the view aren't changes, and neither are the
settings that belong to you rather than the song: the keyboard octave, velocity, auto-step,
follow, and whether the click is on. A tempo set with `:bpm` isn't an undo step either, but a
`:meter` that changed the tempo along with the steps per beat brings it back when undone.
Opening or starting a song clears the undo history. When there's nothing left, the status line
says `nothing to undo` (or `nothing to redo`).

## Playing and stopping

| Key        | Action name      | What it does                                                  |
|------------|------------------|---------------------------------------------------------------|
| `space`    | `toggle_play`    | play or stop: on the pattern page, the pattern, looped        |
| `C-space`  | `play_song`      | play the song from its first entry (or stop)                  |
| `A-space`  | `play_from_here` | play the song from the entry playing this pattern (or stop)   |
| `C-k`      | `hush`           | stop the song and every sound at once                         |
| backtick   | `play_fill`      | hold for a fill (see [Gestures](05-gestures.md))              |
| `ralt` `!` | `loop_pedal`     | record a loop ([looper](09-recording.md#the-loop-pedal))      |

`space` on the pattern page loops the pattern being edited, each track going round at its own
length, so tracks of different lengths drift against each other. On the song page, it plays the
song from the cursor's entry. `C-space` plays the whole song from the start, following it in the
header (`song 2/6`); `A-space` starts at the entry that plays the pattern you're on, so you hear
it in context. Pressing any of them while playing stops.

Stopping lets notes ring out as they would: a reverb's tail, a delay's echoes, a string still
sounding. `C-k`, hush, stops everything at once, held notes, echoes, tails and loops, by
building every instrument and effect afresh. The status line says `hushed`. It's the panic
button, for when a feedback loop runs away.

### Tempo

`:bpm BPM` sets the tempo in beats a minute, from 20 to 400, and may have a fraction
(`:bpm 122.5`). A beat is a number of steps (`:meter` sets how many; four, sixteenth notes, to
start), so at 120 BPM and four steps a beat each step lasts an eighth of a second. The
conductor can change the tempo as the song plays (see [Gestures](05-gestures.md)); `:bpm` sets
the song's own, which the conductor's `T--` goes back to.

### The click and count-in

`:click` turns the metronome on and off. It clicks each beat, accenting the first of the bar
(and, more softly, the start of each group in a meter such as `2+2+3`). Whether it's on is saved
with the song, so a finished piece can open without it; turning it on or off alone doesn't count
as an unsaved change, since it's how you listen rather than the music. `--no-click` starts
without it.

When record is armed (`R`), playing first counts you in: a bar of clicks, whether or not the
metronome is on, with the beats left in the header. `:count BARS` sets how many bars (0 to 4; 0
for none, 1 to start). Recording is in [Recording](09-recording.md).

### Following and the view

- `:follow` turns following on and off: on, the page scrolls to keep the playhead in view while
  playing; off, it stays where you put it. It starts on.
- `tab` (`toggle_view`) switches the pattern between the notes as written and as they're
  playing this pass, after the rules have changed them. The header says which.
- `:oct OCTAVE` sets the keyboard's octave for entering notes (0 to 9), as `{` and `}` step it
  down and up from anywhere; `:step STEPS` how far the cursor moves after each note or paste (0 to
  64; 0 stays put, for entering chords), and `C-0` to `C-9` set it from the keyboard.

## Things that look alike

A few jobs can be done more than one way, and each way is there for a reason. Here is which to
reach for.

**A part that changes from pass to pass: rules or gestures.** A rule changes the whole track on
the passes it names: `:tr 3 every 4` plays everything a third up every fourth time round, and
`:rot 1` turns the whole loop. A gesture changes one step: `?50` plays it on half the passes,
`1:4` on the first of four, `F` only in a fill. Use a rule when the idea is "the same, but
different", and a gesture when only one note should behave differently. They work together: a
rule moves the gestures with their steps. See [Rules](04-rules.md) and
[Gestures and locks](05-gestures.md).

**Saved sounds: presets, chains, faces and macros.** A *preset* is the most: an instrument and
its settings, its effects, its macros and its face, put on a track at once. A *chain* is only
the effects, for any strip, a bus too. A *macro* is a knob on the track that turns several
settings together, and a *face* is how the instrument page shows the track: a title, the
controls it puts forward, and art. Macros and the face belong to the track. `:inst` keeps the
macros and effects but takes the face, which belonged to the old instrument. See
[Presets](07-instruments.md#presets) and [Saved chains](08-mixer-and-effects.md#saved-chains).

**The tempo: `:bpm`, the conductor and `:meter`.** `:bpm` sets the song's own tempo. The
conductor's `T` gestures change it as the song plays, and `T--` goes back to it. `:meter` doesn't
mean to change the tempo, but keeps the music the same speed: give a beat fewer steps and the
same steps make more beats a minute, so the tempo shown goes up. See [Tempo](#tempo) and
[The meter](06-song.md#the-meter).

**Lengths: a track's, a pattern's and an entry's.** A track's length (`:len`) is how many steps
it has before it goes round, and tracks of different lengths drift against each other. A
pattern's length (`:bars`) is how long the pattern plays before it loops or the song moves on:
by default its longest track, to a whole bar. An entry's repeats (`+` and `-` on the song page)
are how many times the song plays that pattern before the next. See
[Polymeter](04-rules.md#polymeter-tracks-of-different-lengths) and
[How long a pattern plays](06-song.md#how-long-a-pattern-plays-bars).

**Silencing a part: `tm`, `tM`, the song page's `m`, and the pads.** `tm` mutes a track
everywhere, `tM` in this pattern only, and the song page's `m` for one entry of the song. The
numpad's mute mode (`+`) is `tm` on a pad, for playing a song live. The conductor's `M2`
mutes track 2 from a step of the song on, as part of the arrangement. See
[Mute and solo](03-tracks.md#mute-and-solo).

## Learning Lattice

There are three ways to learn Lattice, each for a different moment:

- **The tutor** (`:tutor`, or `lattice --tutor`) is a course of short lessons, in sections
  (the basics, the pattern, sound, the song, and help), on a small song of its own.
  Each lesson says what to do and checks that you've done it, by looking at the result rather
  than the keys, so any way of getting there counts. Then a box says so, and `space` (or `ret`)
  goes on to the next, carrying on from where you left off. The lessons name keys as you have
  them bound, so a changed config is taught as it is.
- **The help page** (`:help`, or `:h`) lists every key and every command, made from the keymap
  and the command table each time it's shown, so it's always up to date. Each key is followed by
  its action's name, dimmed, then what it does.
- **The manual** (`:manual`, or `:man`) is this: everything, explained.

```
  NORMAL AND SELECT MODE
  h left             left previous field, or track
  l right            right next field, or track
  k up               up previous step
  j down             down next step
  C-u                half_page_up up half a page
```

### The tutor

`:tutor` replaces the song with the tutor's own, so it refuses while you have unsaved changes:
`:w` first, or `:tutor!` to start anyway. While the tutor runs, its song never counts as
unsaved. Then:

- `:tutor next` skips to the next lesson, and `:tutor back` goes back one;
- `:tutor end` stops the tutor (opening or starting another song stops it too).

The lesson box sits under the page, titled with its number, section and name:

```
┌ TUTOR 1/35 · Basics · Moving around ─────────────────────────────────────────────────────┐
│ Welcome to Lattice. The tutor teaches the keys by having you use them: each lesson says  │
│ what to do, and once you've done it, space goes on to the next. :tutor next skips a      │
│ lesson, :tutor back goes back one and :tutor end stops.                                  │
```

### The help page and the manual

On both reading pages, `j` and `k` scroll a line, `C-d` `C-u` half a page, `C-f` `C-b` a page,
and `gg` `ge` go to the top and the end; counts work (`10j`). `esc` goes back to the page you
came from. The transport, the command line and the `g` pages still work, so you can read while
the song plays.

`/` finds words, as in Helix: type them, and the page scrolls to the first line with them as you
type, from where you were; `ret` keeps it there and `esc` goes back. Case doesn't matter. `n`
finds the next line with them and `N` the one before, coming round from the end to the top (the
status line says `round to the top`). It works on both pages, so `/` `quit` on the help page goes
to the keys that quit.

In the manual, `w` and `b` go to the next and previous heading, so `w` from the contents goes to
this chapter. Headings are numbered as they're shown (1, 1.2, 1.2.3), and the contents list the
chapters and their sections.

`:manual TOPIC` opens the manual at the topic: at the first heading that mentions it, or failing
that where it's written as a command (`:manual bpm` finds where `:bpm` is written), or failing
that anywhere it's mentioned. Case doesn't matter. The same command again goes on to the next
place, coming round to the first after the last, so `:manual export`, then `up` `ret` to run it
again, steps through every heading about exporting. A topic the manual doesn't mention says so:
`nothing in the manual about xyzzy`.

## Quitting and saving

| Command                | What it does                                                         |
|------------------------|----------------------------------------------------------------------|
| `:write [PATH]` (`:w`) | save the song, to its own file or to `PATH`                          |
| `:write-quit` (`:wq`)  | save, then quit                                                      |
| `:quit` (`:q`)         | quit, refusing if there are unsaved changes                          |
| `:quit!` (`:q!`)       | quit, throwing away unsaved changes                                  |

`:q` with unsaved changes says `unsaved changes: :w to save them, or :q! to quit anyway`, and
leaves you where you were. `:wq` quits only if the save worked. Opening another song (`:o`),
starting a new one (`:new`) and starting the tutor refuse in the same way, each with its own `!`
to go ahead.

`C-c` (`quit`) quits as `:q` does, but if there are unsaved changes it says so, and pressing it
again straight away quits anyway, as `:q!` does.

Lattice shouldn't crash, but if it does, it saves the song first if it had unsaved changes, as
`song.rescue.lattice` beside its file (or `rescue.lattice` beside the log, if it was never
saved), and says where once the terminal is back. Files are explained in full in
[Files, export and configuration](10-files-and-config.md).
