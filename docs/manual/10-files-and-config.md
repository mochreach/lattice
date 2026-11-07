# Files, export and configuration

This chapter is about what Lattice keeps outside itself: songs, the WAV files it renders, the
config file that changes its keys and colours, and the other files it writes. It ends with every
action listed by its config name, `lattice ctl` for driving Lattice from a script, and the
example songs.

## Song files

A song is saved as one text file with the extension `.lattice`. It holds everything that makes
the song sound as it does:

- its name and tempo, and whether the metronome clicks;
- the grid: the meter (steps a beat, beats a bar and how they're grouped), and the tracks, each
  with its name, length, notes, locks and gestures, rules, instrument and settings, mix, effects
  and how they branch, choke group, macros, face and lock columns;
- the track groups, each with its bus;
- the arrangement: the patterns, each with its own lane for every track, its length in bars,
  its meter if it has its own, and its conductor; and the entries, the order the patterns play
  in, with each entry's repeats and mutes;
- the send and master buses, and their effects.

What isn't in it belongs to you rather than the song: the cursor and selections, the page, the
keyboard's octave, velocity and auto-step, follow, the undo history, and the audio devices.

### The format

The file is [RON](https://github.com/ron-rs/ron), Rusty Object Notation: a text format shaped
like Rust's own structs, with `(field: value)` for a record, `[…]` for a list and `{key: value}`
for a map. It was chosen because it reads much like what it describes, can be written by hand,
and diffs well in version control, so a song can be checked in alongside its samples.

Lattice writes it sparsely, so it reads much like the pattern does:

- a track is written as its length and a map from step number to the steps that aren't empty,
  rather than every step;
- fields at their defaults are left out: a velocity of 100, a length of one step, a rule that
  always applies, a level of 0 dB, no send, no mute, a click that's on, a pattern that's as long
  as its longest track.

Here is a small song, as Lattice saved it: one track playing a C and a G, with two rules, and
two patterns, `A` and `B`, played once each.

```ron
#![enable(implicit_some)]
(
    name: "small",
    bpm: 120.0,
    grid: (
        name: "new",
        steps_per_beat: 4,
        tracks: [
            (
                name: "track 1",
                length: 16,
                steps: {
                    0: (note: (pitch: 60)),
                    4: (note: (pitch: 67)),
                },
                rules: [
                    (transform: Drift(1)),
                    (transform: Reverse, when: Every(n: 2, offset: 1)),
                    (transform: Transpose(5)),
                ],
                instrument: (
                    kind: Synth,
                ),
            ),
        ],
    ),
    arrangement: (
        patterns: [
            (
                name: "A",
                lanes: [
                    (length: 16, steps: {0: (note: (pitch: 60)), 4: (note: (pitch: 67))}, …),
                ],
            ),
            (
                name: "B",
            ),
        ],
        current: 1,
        entries: [
            (
                pattern: 0,
            ),
            (
                pattern: 1,
            ),
        ],
    ),
)
```

Reading it from the top:

- `#![enable(implicit_some)]` tells RON that optional values can be written plainly, `bars: 2`
  rather than `bars: Some(2)`.
- `name` is the song's name, taken from the file's name when it's saved; `bpm` its tempo.
- `grid` is the tracks as they are in the pattern being edited, `current`. `steps_per_beat` is
  the meter's steps in a beat; `beats_per_bar` and `grouping` are left out at 4 and none.
- Each step's `note` has a `pitch` as a MIDI number, 60 being middle C (`C-4`) and 67 the G
  above; `velocity` (1 to 127) and `length` (in steps) are left out at 100 and 1.
- `rules` are the track's, in order. `when: Every(n: 2, offset: 1)` is the
  `every 2 1` of `:rev every 2 1`; a rule that always applies leaves `when` out.
- The `instrument` is the synth at its defaults, so its settings aren't written; other tracks
  have `params: {"cutoff": 800.0, …}` with each setting by name.
- In `arrangement`, each pattern other than the current one has a `lane` for each track: its
  length, steps and rules, written on one line (shortened with `…` above). The current
  pattern's lanes are the grid's tracks, so pattern `B` has none of its own written here.
- `entries` is the song's order. `repeats` and `mutes` are left out at once and none; `mutes`
  is a number with a bit for each track muted in that entry.

Instruments, effects and mixes add more fields as they're changed from their defaults: see the
examples' files for whole songs.

### Sound files beside the song

A `sampler` or `chop` track plays a WAV file, and the song keeps its path. A file inside the
song's folder, or in a folder inside it, is saved relative to the song (`samples/break-1.wav`),
so the song and its sounds can be moved, copied or checked in together. A file anywhere else is
saved with its full path. Either way, the file itself isn't in the song: keep it with it.

### Checking a song

Opening a song checks it as well as reading it, and refuses one the editor or engine couldn't
play, or holds a value no command could have set: a tempo outside 20 to 400 BPM, a track of no
steps or more than 256, more than 16 tracks, steps per beat out of range, a level above +6 dB, a
choke group above 8, groups that overlap, patterns of more than 256 bars, entries for patterns
that don't exist, a setting, lock or macro out of its range (or not a number at all, as `NaN`),
a lock or macro of a setting the track hasn't. A preset is checked the same way. The status line
says what's wrong and where:

```
 NORMAL  can't open bad.lattice: 5:1: Unexpected missing field named `grid` in `Song`
```

Opened from the command line (`lattice bad.lattice`), Lattice stops with the same message.

Should a bad value get past all that, the engine silences any track whose sound stops being a
number, so one track gone wrong goes quiet rather than taking the buses, and the reverb's and
delay's tails, with it.

### Saved songs may break

Lattice is in beta, and its file format changes as Lattice does, without keeping old files
working. A song saved by an older Lattice may not open in a newer one, or may open with some of
its settings back at their defaults.

That's deliberate. Keeping every old format readable means code to convert each one, and it
freezes early guesses in place at the time they're most likely to be wrong. While the design is
still moving, the format follows the code. Because the files are text, a song that no longer
opens can usually be mended by hand: the error says which line and column it stopped at. Once
Lattice settles, the format will too.

## Opening, saving and starting afresh

| Command                | What it does                                                         |
|------------------------|----------------------------------------------------------------------|
| `:write [PATH]` (`:w`) | save the song, to its own file, or to `PATH`                         |
| `:write-quit [PATH]`   | save the song, then quit (`:wq`)                                     |
| `:open PATH` (`:o`)    | open the song at `PATH`, or start a new one there                    |
| `:open! PATH` (`:o!`)  | the same, throwing away unsaved changes                              |
| `:new [PATH]` (`:n`)   | start an empty song, to be saved at `PATH`                           |
| `:new! [PATH]` (`:n!`) | the same, throwing away unsaved changes                              |
| `:quit` (`:q`)         | quit, unless there are unsaved changes                               |
| `:quit!` (`:q!`)       | quit, throwing away unsaved changes                                  |
| `:recover`             | bring back the unsaved changes kept when Lattice last stopped        |
| `:recover!`            | the same, throwing away the unsaved changes there are now            |

Paths are typed as they are, relative to the folder Lattice was started in, and `tab` completes
them: after `:open`, `:w` and the others, the files and folders that fit are listed above the
line.

### Saving

`:w` saves the song to its file and says `written song.lattice`. `:w PATH` saves it to `PATH`
instead, and from then on the song's file is `PATH`, so the next `:w` goes there too: it's
"save as", and the song takes its new file's name. A path with no extension gets `.lattice`
(`:w sketch` saves `sketch.lattice`), as `:new PATH` does. A song with no file yet, the demo or
one from `:new`, needs a path: `:w` alone says `no file name yet: :w PATH`.

Saving also gathers up samples. Samples rendered before the song had a file (see
[where samples go](#where-samples-go)) are in a scratch folder that won't last; saving copies
those the song's tracks play into a `samples` folder beside it, and the tracks play the copies
from then on. A copy keeps its name if it's free, or gets a number (`loop-1-2.wav`) if another
file has it.

`:wq` saves and then quits, but only if the save worked. If it fails, the status line says why
(`can't write /read-only/song.lattice: …`) and Lattice carries on.

### The [+] marker and [scratch]

The header shows the song's file name, and `[+]` after it while there are changes that aren't
saved. Lattice compares the song with the copy it last saved or opened, so undoing back to that
point takes the `[+]` away again. A song with no file is called `[scratch]`, as in Helix.

The click is saved with the song, but turning it on or off alone doesn't add a `[+]`. It's
saved the next time something else is. Nor does going to another pattern: the file remembers
which pattern was open, but that's saved only along with a change.

### Opening

`:o PATH` opens the song at `PATH`, replacing the one you're in: the undo history is cleared,
the cursor goes to the top left, and the tutor, if it's running, stops. With unsaved changes it
refuses, saying `unsaved changes: :w to save them, or :o! to open anyway`. `:o!` opens anyway.

If there's no file at `PATH`, `:o` starts a new song there, as `lattice PATH` does, and says
`PATH (new file)`. It has the demo's tracks, instruments and mixer, with no notes: a kit to start
from. `:w` saves it.

### Starting afresh

`:new` starts an empty song: one track of 16 steps on the synth, no effects, at the tempo and
meter you have now. `:new PATH` names the file it will be saved to, and refuses if there's
already something there (`song.lattice is already there: :o to open it`), so it can't be used
to overwrite a song by mistake. With unsaved changes, `:new` refuses as `:o` does; `:new!` goes
ahead.

```
:new sketch.lattice
 NORMAL  new song: :w saves it as sketch.lattice
```

### Quitting

`:q` quits, and refuses if there are unsaved changes; `:q!` quits anyway. `C-c` is the same
from a key: with unsaved changes it says so, and a second `C-c` straight away quits anyway.

### Unsaved changes are kept: `:recover`

A terminal is easy to close by mistake, and nothing asks first: the window goes, and Lattice
with it. So while the song has unsaved changes, Lattice keeps a copy of it every 30 seconds,
and again as it stops for any reason but quitting: the terminal closing, the computer shutting
down, or being told to stop. The copy goes in Lattice's state folder,
`~/.local/state/lattice/autosave/`, in a folder for each song, never beside the song itself,
so your own files only ever change when you save. Samples the song plays that were made before
it had a file, and so are only in the scratch folder, which a restart may clear, are copied
into that folder too.

Next time you open the song, or start Lattice with no song if it never had a file, the status
line says so: `unsaved changes from 5 minutes ago were kept: :recover to bring them back`. It
says that only when the copy is newer than the song's file. `:recover` replaces the song with
the copy, counted as unsaved (`[+]`), so `:w` keeps it; if you've changed something since
opening, it refuses, as `:o` does, and `:recover!` goes ahead anyway.

The copy is thrown away when it's no longer wanted: once the song is saved, or undone back to
how it was saved, and when you quit, since quitting with `:q!` is choosing to lose the changes.
A copy you didn't recover stays until that song is changed again, when the new changes take
its place, so open the song and `:recover` first if you want it.

Thirty seconds is a guess at the most work it's fine to lose; most changes are a few keys, and
writing the song takes a few milliseconds, so it could be more often, but there's no need while
closing the terminal still saves. Killing Lattice outright (`kill -9`) or losing power can't be
caught, and those lose up to the 30 seconds.

### If Lattice crashes

Lattice is written not to crash (it never uses Rust's `unwrap` or indexing that could panic,
and lints check it), but if it does, it catches the crash as the program unwinds, puts the
terminal back, and saves the song if it had unsaved changes: beside its file as
`song.rescue.lattice`, or as `rescue.lattice` in Lattice's state folder (beside the log) if it
was never saved. It says where, and where the log is. The song's own file is left alone, since
the song in memory may be half changed.

## Exporting audio

Lattice renders to WAV files with three commands:

| Command                 | What it renders                                                      |
|-------------------------|----------------------------------------------------------------------|
| `:export [PATH] [BARS]` | the song as arranged; or with `BARS`, this pattern for so many bars  |
| `:export-sel [PATH]`    | the selection, once, with its tail: a sample                         |
| `:export-loop [PATH]`   | the selection, exactly its length, its tail mixed into its start     |

Each renders faster than real time, on a thread of its own with an engine of its own, so you can
carry on playing and editing while it works. It uses the song, mixer and tempo as they are when
you start it. The status line says `exporting song.wav…`, then `exported song.wav (32.5s)` when
it's done, or `export failed:` and why.

### The file

Every export is a stereo WAV of 24-bit PCM, at the sample rate of your audio output (usually
48000 or 44100 Hz). That's a format every sampler, editor and DAW reads, and 24 bits keeps
quiet passages and fading tails clean where 16 would add noise. The level is the master bus's,
as you hear it: anything over full scale is clipped, so keep an eye on the master's meter.

After the music ends, the export carries on while delays and reverbs ring out, until there's
been a second of silence (below -80 dB), and trims that second off, so a tail is kept but not a
long silence. A tail is never more than ten seconds.

A path without an extension gets `.wav`. A path that's there already is written over.

### :export

`:export` renders the song as the song page shows it: each entry in order, each for its pattern's
length and as many times as it repeats, with the tracks the entry mutes muted. As it does in the
song, a pattern played again carries on round rather than starting over, so tracks of different
lengths keep drifting and rules keep counting passes. Then the tail.

With no path, it goes beside the song's file, named after it (`song.lattice` exports
`song.wav`), so a song with no file needs a path: `:export` alone says
`no file name yet: give one, or :w the song first`.

`:export PATH BARS`, or `:export BARS`, renders the pattern you're editing for that many bars
(1 to 256), whatever the song's arrangement: four bars of a loop, say, to try a mix on. A number
at the end of the line is always taken as the bars, so `:export 8` renders eight bars to the
song's own `.wav`.

```
:export mix-1 8
 NORMAL  exported mix-1.wav (15.6s)
```

The **conductor** plays in an export as it does in the song: tempo changes and slides, `X`
stopping the song, and the mutes and solos it sets. Its `P` cues are left out, since an export
plays the entries as arranged and a cue would take it somewhere else; when a pattern has them,
the messages say `as arranged: the conductor's P cues are left out`. A song with nothing
arranged exports the pattern you're on, once round.

The mixer is as you've set it: muted tracks are silent and soloed ones play alone. There's no
click in an export.

### :export-sel

`:export-sel` renders the primary selection, its steps on its tracks, once, and then its tail.
It's for making a sample: a bar of drums to play on a sampler, a chord to chop. The tracks
selected are heard even if they're muted, and the rest are silent, whatever the mixer's mutes
and solos say. Their rules apply as on the first pass. A selection plays without the conductor,
at the song's own tempo.

Once it's rendered, `:sample` or `:chop` with no path plays it, on the tracks the selections
reach, so a bar can be rendered and loaded in two commands:

```
v3j                  select four steps of the cursor's track
:export-sel          render them, once, into the samples folder
tl                   to the next track
:sample              play what was just exported, on that track
```

### :export-loop

`:export-loop` renders the selection as `:export-sel` does, but makes it exactly the
selection's length, with the tail mixed back into the start. A reverb or a crash still ringing
as the loop ends goes on ringing over its start, as it would if it went round, so the file loops
without a gap or a click. It's what `:chop` wants: the amen and mountain-king examples were both
made this way.

### Where samples go

`:export-sel` and `:export-loop` make samples, so they go in a `samples` folder beside the song,
numbered so that a new one never replaces one a track plays: `song-selection-1.wav`,
`song-loop-1.wav`, `song-loop-2.wav`, and so on.

- A name alone, `:export-loop break`, goes in the `samples` folder as `break.wav`.
- A path with a folder in it, `:export-loop loops/break.wav`, goes where it says.
- Before the song has a file, they go in a scratch folder for this run of Lattice (in the
  system's temporary folder, `lattice-` and the process's number), named `selection-1.wav`,
  `loop-1.wav` and so on. `:w` copies the ones the tracks play into `samples` beside the song,
  so the song and its sounds stay together.

`:export` itself makes a mix rather than a sample, so it goes beside the song.

## The config file

Lattice reads a config file when it starts, to change its keys and colours:
`~/.config/lattice/config.toml`, or `$XDG_CONFIG_HOME/lattice/config.toml` if that's set.
`lattice --config PATH` reads another in its place. You needn't have one: without it, Lattice
uses its own keys and the terminal's colours.

The file is [TOML](https://toml.io), as Helix's is, and laid out as Helix lays out its keys:

```toml
[keys.normal]
C-s = ":w"                   # a command
X = "none"                   # unbound
Q = "goto_mixer"             # an action, by name

[keys.normal.space]          # a new menu, opened by space
m = "goto_mixer"
s = "goto_song"

[keys.normal.g]              # adding to the goto menu
a = ":pattern A"

[keys.insert]
C-c = "escape"

[theme]
accent = "magenta"
beat-row = "none"
tracks = ["#ff8800", "cyan", 12]
```

Changes are read when Lattice starts, so restart it to try them. If something in the file is
wrong, the status line says what as Lattice opens, with how many more mistakes there were:

```
~/.config/lattice/config.toml: keys.normal.Q: no action called "fly" (and 2 more)
```

and everything else in the file still applies. A file that isn't valid TOML applies nothing.

### Binding keys

There are two tables of keys: `[keys.normal]`, for normal and select mode (which share their
keys), and `[keys.insert]`, for insert mode. Each line binds a key to one of three things:

- an **action**, by its name: `"goto_mixer"`. Every action is listed below, in
  [every action, by name](#every-action-by-name), and the help page shows each key's, dimmed,
  before what it does;
- a **command line**, after a colon: `":w"`, `":bpm 140"`, `":export-loop"`. It runs as though
  typed and entered, once, whatever count comes before it. The command's name is checked when
  the config is read;
- `"none"`, which unbinds the key.

A table under `[keys.normal]` is a **menu**, opened by its key, as `g` opens goto:
`[keys.normal.space]` above makes `space` a menu of its own (so in normal mode it no longer
plays and stops). If the key already opens a menu, the table adds to it, or changes its keys,
and leaves the rest: `[keys.normal.g]` with `a = ":pattern A"` adds `ga` to goto, and
`e = "none"` would take `ge` away. A menu's box lists what's in it, with commands shown as
`:pattern A`. Menus can't hold menus.

Insert mode takes only actions, as its other keys play notes; a command there is a mistake. A
piano key bound there does its action instead of playing its note, on the pattern page and the
instrument page alike, so `q = "escape"` gives up the note `q` played for a way out of insert
mode under the left hand.

Keys are written as Helix writes them:

| Key                      | Written                                                          |
|--------------------------|------------------------------------------------------------------|
| a letter, digit or sign  | itself: `a`, `X` (shift and `x`), `7`, `%`, `;`                  |
| control and a key        | `C-` and the key: `C-s`, `C-space`                               |
| alt and a key            | `A-` and the key: `A-;`, `A-C`                                   |
| space, `-`, backtick     | `space`, `minus`, `backtick`                                     |
| enter, escape and so on  | `ret`, `esc`, `backspace`, `del`, `tab`                          |
| the arrows               | `up`, `down`, `left`, `right`                                    |
| the page keys            | `pageup`, `pagedown`, `home`, `end`                              |
| the function keys        | `F1` to `F12`, none bound to start with                          |

TOML lets a key be written bare only if it's letters, digits, `-` and `_`, so quote any other:
`"%" = "select_all"`, `"A-;" = "flip"`. Shifted letters are written in capitals; there's no
`S-`. A control key is the same whichever case is written, `C-S` being `C-s`.

### Colours

`[theme]` sets Lattice's colours. Anything not set follows the terminal:

| Name        | What it colours                                         | Unless set               |
|-------------|---------------------------------------------------------|--------------------------|
| `text`      | values and other main text                              | the terminal's own       |
| `dim`       | labels, hints and secondary information                 | the text, dimmed         |
| `faint`     | empty steps, off-beat step numbers, scaffolding         | dark grey                |
| `accent`    | the logo, insert mode, messages, the conductor's tempos | yellow                   |
| `playing`   | the transport while playing                             | green                    |
| `visual`    | the page's name, select mode's cursor and badge         | blue                     |
| `error`     | mistakes on the command line, `REC`, the count-in       | red                      |
| `selection` | the background of selected steps                        | shaded from the terminal |
| `beat-row`  | the background of rows on the beat, or `"none"`         | shaded from the terminal |
| `tracks`    | the tracks' colours, a list used in turn                | six of the sixteen       |

The tracks' six are blue, green, magenta, cyan, yellow and red, in that order, round and round.

A colour is one of:

- a name from the terminal's sixteen: `black`, `red`, `green`, `yellow`, `blue`, `magenta`,
  `cyan`, `gray`, `dark-gray`, `light-red`, `light-green`, `light-yellow`, `light-blue`,
  `light-magenta`, `light-cyan` and `white` (`bright-` works for `light-`, and `grey` for
  `gray`);
- a number from 0 to 255, the terminal's palette by index (`12`, or `"12"`);
- `#rrggbb`, an exact colour, for terminals that show them;
- `default`, the terminal's own foreground or background.

By default every colour is one of the terminal's sixteen, by name rather than by value, so
Lattice looks right in a light theme or a dark one, Solarized or Gruvbox, without a theme of its
own: it's drawn in your terminal's colours. Highlighted blocks, the cursor and the badges, use
reverse video rather than naming a text colour, so they stay readable in any palette.

Two colours can't come from the palette, as it has no gentle shade of the background: the
selection's and the beat rows'. For those, Lattice asks the terminal what its background and
foreground are as it starts, and mixes them, a little of the way for beat rows and further for
the selection. A terminal that doesn't say leaves beat rows unshaded and the selection dark grey.
`beat-row = "none"` turns the shading off.

### Keyboard lights

My laptop's keyboard lights each key on its own, so Lattice lights the keys that matter now, as
Ableton's Push and Novation's Launchpad light their pads to show what each one plays. It needs
`qs-rgbd`, a service I wrote that shares the keyboard between programs. It isn't released, so
for now the lights are tied to my laptop; without it nothing is lit and nothing goes wrong, so
there's nothing to set up or turn off.

- **In normal and select mode** every key that does something lights up, dimly, in a colour
  for the kind of thing it does, as the actions are sorted under [Every action, by
  name](#every-action-by-name): moving blue, pages cyan, selecting yellow, editing red, modes
  grey (`i`, `a`, `:`), tracks magenta, playing green. Keys that open a menu, `g` `]` `[` `t`,
  are orange, the digits that start a count faint, and the keys that do nothing dark. On its
  own a key shows what it does unshifted: `u`, undo, rather than `U`, redo.
- **While a key waits for another**, after `g`, `]`, `t`, `f`, `"`, `q` and the like, only the
  keys that do something next light up, bright amber, and the rest go dark: the infobox, on the
  keyboard. After `r`, the notes light up, as it waits for one.
- **In insert mode** (and on the instrument page, playing), the keys that play notes light up
  by where their notes fall in the pads' scale (`:scale`, [chapter 9](09-recording.md)): the
  root orange, the scale's other notes white, and the rest dim blue. In C major the piano
  layout looks like a piano, white keys and black; `:layout` lays them out differently, and
  the lights follow, so the fourths and Wicki–Hayden layouts show where the scale lies on
  them, which is hard to remember. In a folded group the digit pads light blue. The keys that
  aren't notes light by kind, as in normal mode.
- **As the song plays**, the key for the note sounding on the cursor's track lights bright cyan,
  held as long as the note is, so you can watch a part go by under your fingers and learn it.
- **The beat** runs along the number row while the song plays: one key a beat of the bar, the
  beat playing lit bright green. In insert mode the keys that are notes stay notes.
- **F keys** holding recorded keys (`QF3`, [chapter 2](02-patterns.md)) light purple, and the
  one being recorded into blinks red.
- **`esc`** shows the mode it leaves: green in insert mode, yellow in select, white on the
  command line.
- **`space`** is bright green while the song plays, and red while it records.
- **Right Alt**, the loop pedal, breathes amber while a track's armed, pulses as it listens
  for `:threshold`, and breathes red while it records.
- **When something goes wrong**, a key flashes red for a moment: `space` when the sound broke
  up, the computer too busy to keep up (the log says how often), and right Alt when the input
  lost some of what it heard, which a loop would have recorded, so you know to take it again.

On the command line the keys are only letters, so Lattice lights nothing there but `esc` and the
F keys. Keys Lattice doesn't light keep whatever lights them otherwise.

## Every action, by name

These are all of Lattice's actions, by the name the config file binds keys to, with their keys
as Lattice comes. Some keys do other things on some pages (the mixer, fx, instrument and song
pages reuse many), explained in their chapters; the names here are what the keys are bound to.
Insert mode shares the arrows, `pageup` `pagedown` `home` `end`, `esc`, the playing keys, `{`
`}`, `tab`, `R` and `C-c` with normal mode.

### Moving

| Keys             | Action           | What it does                                         |
|------------------|------------------|------------------------------------------------------|
| `h` `left`       | `left`           | left: the open track's fields, then the track before |
| `l` `right`      | `right`          | right: the open track's fields, then the next track  |
| `k` `up`         | `up`             | up a step                                            |
| `j` `down`       | `down`           | down a step                                          |
| `C-u`            | `half_page_up`   | up half a page                                       |
| `C-d`            | `half_page_down` | down half a page                                     |
| `C-b` `pageup`   | `page_up`        | up a page                                            |
| `C-f` `pagedown` | `page_down`      | down a page                                          |
| `gg` `home`      | `first_step`     | the first step                                       |
| `ge` `end`       | `last_step`      | the last step                                        |
| `gh`             | `first_track`    | the first track                                      |
| `gl`             | `last_track`     | the last track                                       |
| `w` `]n`         | `next_note`      | the next note                                        |
| `b` `[n`         | `prev_note`      | the previous note                                    |
| `]b`             | `next_beat`      | the next beat                                        |
| `[b`             | `prev_beat`      | the previous beat                                    |
| `]m`             | `next_bar`       | the next bar                                         |
| `[m`             | `prev_bar`       | the previous bar                                     |
| `]p`             | `next_pattern`   | the next pattern (cued, while playing)               |
| `[p`             | `prev_pattern`   | the previous pattern (cued, while playing)           |
| `th`             | `prev_track`     | the previous track, on its notes                     |
| `tl`             | `next_track`     | the next track, on its notes                         |
| `f`              | `goto_field`     | then a key: to a field of the open track             |

### Pages

| Keys | Action            | What it does                         |
|------|-------------------|--------------------------------------|
| `gp` | `goto_pattern`    | the pattern page                     |
| `gm` | `goto_mixer`      | the mixer page, on this track        |
| `gf` | `goto_effects`    | the fx page, on this track's effects |
| `gi` | `goto_instrument` | the instrument page, on this track   |
| `gs` | `goto_song`       | the song page                        |

### Selecting

| Keys  | Action            | What it does                              |
|-------|-------------------|-------------------------------------------|
| `v`   | `select_mode`     | select mode: motions extend the selection |
| `x`   | `select_rows`     | select whole rows; again, one more        |
| `%`   | `select_all`      | select everything                         |
| `tt`  | `select_tracks`   | select every step of the tracks reached   |
| `;`   | `collapse`        | collapse each selection onto its cursor   |
| `A-;` | `flip`            | flip which end the cursor is              |
| `C`   | `copy_down`       | copy the selection onto the rows below    |
| `A-C` | `copy_up`         | copy the selection onto the rows above    |
| `A-s` | `split_rows`      | split the selections into rows            |
| `A--` | `merge_all`       | merge the selections into one             |
| `,`   | `keep_primary`    | keep only the primary selection           |
| `A-,` | `remove_primary`  | remove the primary selection              |
| `)`   | `rotate_forward`  | make the next selection primary           |
| `(`   | `rotate_backward` | make the previous selection primary       |
| `s`   | `select_notes`    | select the notes matching a pattern       |
| `K`   | `keep_notes`      | keep selections with a matching note      |
| `A-K` | `remove_notes`    | remove selections with a matching note    |

### Editing

| Keys              | Action                  | What it does                               |
|-------------------|-------------------------|--------------------------------------------|
| `i`               | `insert`                | insert mode, at the start of the selection |
| `a`               | `append`                | insert mode, after its end                 |
| `c`               | `change`                | delete, then insert mode                   |
| `esc`             | `escape`                | back to normal mode; close a page or box   |
| `:`               | `command_line`          | the command line                           |
| `d`               | `delete`                | delete, copying first                      |
| `A-d` `del`       | `clear`                 | delete without copying                     |
| `backspace` `del` | `clear_advance`         | insert mode: clear the step, move on       |
| `y`               | `yank`                  | copy                                       |
| `p`               | `paste`                 | paste over the selection                   |
| `P`               | `paste_insert`          | paste, pushing the steps below down        |
| `r`               | `replace`               | then a piano key: fill with that note      |
| `u`               | `undo`                  | undo                                       |
| `U`               | `redo`                  | redo                                       |
| `C-a`             | `note_up`               | up a semitone; in a field, its value up    |
| `C-x`             | `note_down`             | down a semitone; in a field, down          |
| `>`               | `note_octave_up`        | up an octave                               |
| `<`               | `note_octave_down`      | down an octave                             |
| `+` `=`           | `longer`                | longer notes                               |
| `-`               | `shorter`               | shorter notes                              |
| `~`               | `reverse_steps`         | reverse the selected steps                 |
| `*`               | `repeat_steps`          | repeat the first step (or count) through   |
| `A-)`             | `rotate_steps_forward`  | rotate the selected steps down one         |
| `A-(`             | `rotate_steps_backward` | rotate the selected steps up one           |
| `&`               | `fill`                  | fill a field evenly between the ends       |
| `}`               | `keyboard_octave_up`    | the keyboard's octave up                   |
| `{`               | `keyboard_octave_down`  | the keyboard's octave down                 |

### Tracks

| Keys | Action                | What it does                          |
|------|-----------------------|---------------------------------------|
| `tm` | `toggle_mute`         | mute or unmute (`m` alone too)        |
| `tM` | `toggle_pattern_mute` | mute or unmute in this pattern only   |
| `ts` | `toggle_solo`         | solo or unsolo                        |
| `ta` | `add_track`           | add a track after this one            |
| `td` | `remove_track`        | remove the tracks, copying them first |
| `ty` | `yank_tracks`         | copy the tracks                       |
| `tp` | `paste_tracks_after`  | paste copied tracks after this one    |
| `tP` | `paste_tracks_before` | paste copied tracks before this one   |
| `tH` | `move_track_left`     | move the track left                   |
| `tL` | `move_track_right`    | move the track right                  |
| `tg` | `toggle_group`        | group the tracks, or ungroup them     |
| `tz` | `toggle_fold`         | fold or unfold the groups             |
| `te` | `toggle_open`         | keep the tracks open, or close them   |

### The song page

| Keys  | Action       | What it does                         |
|-------|--------------|--------------------------------------|
| `o`   | `open_below` | add an entry after the cursor's      |
| `O`   | `open_above` | add an entry before it               |
| `ret` | `open`       | edit the entry's pattern, at its bar |

### Playing, and the rest

| Keys      | Action           | What it does                            |
|-----------|------------------|-----------------------------------------|
| `space`   | `toggle_play`    | play or stop                            |
| `C-space` | `play_song`      | play the song from the start            |
| `A-space` | `play_from_here` | play the song from this pattern's entry |
| `C-k`     | `hush`           | stop everything, tails and all          |
| backtick  | `play_fill`      | hold for a fill                         |
| `R`       | `toggle_record`  | record on or off                        |
| `tab`     | `toggle_view`    | the pattern as written, or as playing   |
| `L`       | `lock_setting`   | mixer: lock the settings changed        |
| `\|`      | `loop_selection` | loop round the selection, or stop       |
| `C-c`     | `quit`           | quit; again to lose unsaved changes     |

## Lattice's other files

Besides songs and the config, Lattice keeps a few files of its own:

| File                                    | What's in it                                       |
|-----------------------------------------|----------------------------------------------------|
| `~/.config/lattice/config.toml`         | keys and colours (above)                           |
| `~/.config/lattice/audio.toml`          | the audio devices, and `:latency` for each input   |
| `~/.config/lattice/midi.toml`           | MIDI controls (`:midi-learn`), clock (`:clock`)    |
| `~/.config/lattice/chains/NAME.toml`    | effect chains saved with `:chain-save`             |
| `~/.config/lattice/presets/NAME.toml`   | presets saved with `:preset-save`                  |
| `~/.local/state/lattice/lattice.log`    | what happened this run; the last run's is `.log.1` |
| `~/.local/state/lattice/history`        | the command line's history, a command a line       |
| `~/.local/state/lattice/rescue.lattice` | a never-saved song, rescued from a crash           |
| `~/.local/state/lattice/autosave/`      | unsaved changes kept for `:recover`                |

The config folder moves with `$XDG_CONFIG_HOME`, and the state folder with `$XDG_STATE_HOME`.
On Windows, which has no `~`, they're in your user folder, `%USERPROFILE%`, all the same.
The devices, chains and presets are explained in [Recording](09-recording.md) and
[Mixer and effects](08-mixer-and-effects.md).

The **log** is for when something goes wrong: the version, the devices Lattice opened, the
commands you ran and where you were, the pages you went to, playing and stopping, what the status
line said, any trouble the audio had keeping up, and, after a crash, what went wrong with its
backtrace. `lattice ctl log` shows its end.

## Driving Lattice from scripts: lattice ctl

`lattice ctl` drives a Lattice that's running, from a script, a test or an agent: it can read
the screen, ask where things are, type keys and run commands, as you would.

```sh
lattice ctl screen                  # the screen as text
lattice ctl state                   # page, cursor, tracks, transport and status line, as JSON
lattice ctl keys 'gp:bpm 140<ret>'  # type keys
lattice ctl cmd 'fx-add delay'      # run a command, as : would
lattice ctl log 20                  # the end of the log
```

Each Lattice listens on a socket of its own, `$XDG_RUNTIME_DIR/lattice/PID.sock` (or under
`~/.local/state/lattice/sockets` without a runtime folder), in a folder only you can open, so
nobody else on the machine can drive yours. With one Lattice running, `ctl` finds it. With more,
it says which are running, and `--pid PID` says which to talk to:

```sh
lattice ctl --pid 41234 screen
```

### The requests

| Request          | What it does                                                               |
|------------------|----------------------------------------------------------------------------|
| `screen`         | prints the screen as it was last drawn, as text, a line a row              |
| `state`          | prints where things are, as JSON                                           |
| `keys KEYS`      | types `KEYS`, then prints the status line's message, if it changed         |
| `cmd COMMAND`    | runs `COMMAND` (with or without its `:`), then prints the message          |
| `log [LINES]`    | prints the last `LINES` lines of the log (40 if not given)                 |

`state` gives the process's `pid`; the `song`'s name and its `path`; whether it's `modified`;
the `page` and `mode` (`normal`, `select`, `insert` or `prompt`, with the `prompt` typed); the
cursor's `track` and `step`; the `tracks` by name; whether it's `playing` and each track's step
in `positions`; the `bpm`; whether `record` is armed; and the status line's `message`.

`keys` writes keys as the tutor does: a character is itself, and any other key goes by its name
in angle brackets, as in the config: `<ret>`, `<esc>`, `<tab>`, `<space>`, `<backspace>`,
`<C-a>`, `<A-space>`, `<up>`, and the numpad's `<pad7>` or `<pad+>`. `<lt>` and `<gt>` type `<`
and `>`. A key name Lattice doesn't know is refused before anything is typed.

Keys and commands go the same way as your own, so they show on the screen, take undo steps (`u`
undoes them) and say what they did in the status line. Keys typed through `ctl` go into the
command history as yours do; commands run with `cmd` don't. A refused request makes `ctl` print
the reason and exit with an error, so a script can stop. `log` reads the file directly, so it
works with no Lattice running.

### What it can't do: --allow-agent-save

A script driving your Lattice could, by mistake, overwrite a song with `:w` or throw one away
with `:q!`. So unless Lattice was started with `--allow-agent-save`, `ctl` refuses anything that
would:

- save, open, or quit: `:w`, `:wq`, `:o`, `:q`, and `C-c`;
- write a WAV file anywhere: `:export`, `:export-sel` and `:export-loop`;
- throw the song away: `:new!`, `:open!`, `:quit!` and `:tutor!`;
- save a chain or preset to your config folder: `:chain-save` and `:preset-save`.

```
$ lattice ctl cmd 'w'
Error: saving, exporting, opening and quitting need Lattice started with --allow-agent-save
```

Everything else works, so an agent can play, edit and listen while you keep the say over your
files. Start Lattice with `--allow-agent-save` when you do want a script to save, such as one
that builds a song and writes it out.

### A worked script

```sh
tmux new-session -d lattice --no-splash song.lattice    # Lattice needs a terminal of its own
lattice ctl keys 'gg'                    # to the top of the pattern
lattice ctl keys 'izxcv<esc>'            # four notes, C D E F
lattice ctl cmd 'tr 12 every 2'          # up an octave every other pass
lattice ctl keys '<space>'; sleep 4; lattice ctl keys '<space>'
lattice ctl screen | head -20            # see what it looks like
lattice ctl state | grep modified        # "modified": true
```

## The examples

Three songs in the `examples` folder show what Lattice can do. Open them from the folder Lattice
is in (`lattice examples/amen/amen.lattice`), then `C-space` to play the song. Each folder's
`README.md` goes through it bar by bar. Copy one before changing it, so the original stays as it
was.

### amen

The Amen break, from The Winstons' "Amen, Brother" (1969), the most sampled drum break there
is, in three patterns, one for each stage of a sampler's life. `drums` programs it note by note
on Lattice's own drums at 136 BPM, ghost notes and all, in a group with drive and a compressor on
its bus. `sample` plays the same four bars rendered to `amen.wav` (with `:export-loop`) on a
`chop` track cut into 64 pieces. `chopped` edits the sample by moving pieces around as notes: a
snare roll, a reversed snare, a stutter, and a rule that shifts the edit half a bar every other
time round. It shows chopping a loop.

### mountain-king

Grieg's "In the Hall of the Mountain King", from "Peer Gynt" (1875), getting louder, higher and
faster as the trolls close in. It shows as much of Lattice as fits in one song: track groups and
a choke, a branching effects chain and a macro on the troll lead, faces, rules, mutes in the
arrangement, locks that slide, a section chopped from its own render (`hall.wav`), and the
conductor taking the tempo from 112 BPM to 176.

### scarlatti

A Switched-On Scarlatti: Domenico Scarlatti's Sonata in G, K. 455, on Krell, the alien
harpsichord, with Switched, the modular, screaming along over a drum machine, as Wendy Carlos
played Bach on "Switched-On Bach" (1968). It's in 2/2 (`:meter 2 4`), and shows Lattice's own
engines, the ensemble and the phaser, a ladder filter sliding open, gestures (trills, glides, an
arpeggiator, a run split between two tracks, chance), a canon and rolls that play only in a
fill, the conductor slowing into each half's close, and repeats orchestrated differently by
mutes in the arrangement. Its notes come from the Mutopia Project's edition, and it's shared
under the Creative Commons Attribution-ShareAlike 4.0 licence.
