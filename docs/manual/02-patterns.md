# The pattern page

The pattern page is where you write music. It's the page Lattice opens on, and the one `gp`
brings you back to from anywhere else. It shows one pattern of the song as a grid: time runs
down the screen, a row for each step, and each track is a column across it. You move a cursor
over the grid, select steps, and act on them, much as you would on the text of a file in Helix.

This chapter covers the grid itself, moving round it, selecting, entering notes, and every edit
that works on steps. Tracks themselves (adding, naming, muting, grouping) are in
[chapter 3](03-tracks.md); rules, which reshape a track on each pass, in [chapter 4](04-rules.md);
gestures and locks, the step's other columns, in [chapter 5](05-gestures.md); and patterns, the
song and the conductor in [chapter 6](06-song.md).

A word on where this comes from. Trackers began on the Commodore Amiga: Karsten Obarski's
Ultimate Soundtracker (1987) laid music out as rows scrolling up a screen, four channels side by
side, notes written as `C-3` and typed on a computer keyboard laid out like a piano. Lattice keeps
that shape (rows of steps, note names, the two-row piano) and edits it the way Helix edits text.

## The grid

Here is a small pattern of three tracks, sixteen steps long, with the cursor on the lead:

```
    BASS        LEAD                      HAT
    16          16                        16
        0dB                0dB        ▊       0dB
         C                  C                  C
    no rules    no rules                  no rules
    ─────────── ───────────────────────── ───────────
                            vel g    G
 00  C-2         C-4    4   78   ··   ··   C-4
 01  ···         │               ··   ··   ···
 02  ···         │               ··   ··   C-4
 03  C-2         │               ··   ··   ···
 04  ···         G-4    3   78   ··   ··   C-4
 05  ···         │               ··   ··   ···
 06  D#2         │               ··   ··   C-4
 07  ···         ···             ··   ··   ···
 08  C-2         B-4    2   78   ··   ··   C-4
 09  ···         │               ··   ··   ···
 10  ···         C-5        78   ··   ··   C-4
 11  C-2         ···             ··   ··   ···
 12  ···         ···             ··   ··   C-4
 13  ···         ···             ··   ··   ···
 14  F#2         G-4        78   ··   ··   C-4
 15  F-2         ···             ··   ··   ···
```

### Rows are steps, columns are tracks

Each row is a **step**, the smallest slice of time the pattern has. The numbers down the left,
the **gutter**, count the steps from `00`. Each column is a **track**, which plays one
instrument; its name is at the top, in capitals, in the track's colour.

A step on a track can hold a note, or nothing (`···`). What a step lasts in time depends on the
tempo and the meter: at the start a beat is four steps, so each step is a sixteenth note, and at
130 BPM a step lasts 60 / 130 / 4 of a second, about 115 ms. `:meter` and `:bpm` change that
([chapter 6](06-song.md)).

Tracks needn't be the same length. Each one loops on its own, so a track of 12 steps beside one
of 16 drifts against it, coming back into line every 48 steps. That's **polymeter**, and it's
one of the oldest ideas Lattice kept from its first version. A shorter track's column ends,
and the rows below it are blank. The longest track sets how many rows there are; when that's
more than fit on the screen, the grid scrolls to keep the cursor in view.

When there are more tracks than fit across the terminal, the grid scrolls sideways too, keeping
the cursor's track in view, with `‹` and `›` by the names to show there are more off to that side.

### Beats, bars and the gutter

The gutter marks where you are in the bar, by how the step numbers are drawn:

- the first step of each **bar** is bold;
- the first step of each **group** of beats, in a bar with groupings (`:meter 2+2+3`), is bold
  in the accent colour;
- the first step of every other **beat** is in the normal text colour;
- the steps between beats are faint.

If the terminal says what its colours are, rows on the beat are also shaded as a band across the
whole grid, so the beats read at a glance even far from the gutter. (The `beat-row` theme setting
changes or removes the shade: see [chapter 10](10-files-and-config.md).)

A **meter** is how music groups its beats: 4/4 is four beats in a bar, 3/4 a waltz's three, 7/8
seven quick beats usually felt as 2+2+3. The grid follows whatever meter the pattern is in, so
the bold bar lines move when you change it. [Chapter 6](06-song.md) covers `:meter` and
`:pattern-meter`.

### Reading a step

A note is drawn as its name (`C-4`), then its length if it's longer than one step (`4`). Below a
long note, the steps it's held over show a faint `│`, in the track's colour, so you see where it
ends. In the drawing, the lead's `C-4` sounds for four steps, from 00 to 03.

The track under the cursor is **open**: it shows each step's fields beside its notes. In order,
they are:

| Field | Header  | What it holds                                                         |
|-------|---------|-----------------------------------------------------------------------|
| note  |         | the note's name and length                                            |
| vel   | `vel`   | how hard the note is played, `00` to `99`                             |
| g, G  | `g` `G` | two gesture fields: things that happen once on the step (`R3`, `J12`) |
| locks | a name  | up to four lock columns, settings that change on the step             |

Other tracks are **closed**, showing only their notes; a closed step with gestures or locks is
marked `◆` at its end. `te` keeps tracks open after the cursor leaves ([chapter 3](03-tracks.md)).
Gestures and locks are in [chapter 5](05-gestures.md). A few tracks draw their steps differently:
a `chop` track shows each step's piece of the sample and its waveform
([chapter 7](07-instruments.md)), and a folded group shows each track as a column of hits,
`●` `•` `∙` by velocity ([chapter 3](03-tracks.md)).

### Notes, octaves and MIDI numbers

Lattice names notes as trackers do: a letter, a sharp or a dash, and an octave, so every note
is three characters wide and the columns line up. `C-4` is C in octave 4, `C#4` the C sharp above
it. Sharps are always used, never flats: the E flat a minor key would write is shown `D#`.

An **octave** is the distance from one note to the next of the same name, twice the frequency;
it's split into twelve **semitones**, the steps from one key of a piano to the next, black keys
included. Octave numbers go up at each C, so `B-3` is one semitone below `C-4`.

Under the names, each note is a **MIDI note number**, from 0 to 127, one per semitone. MIDI, the
standard instruments have spoken to each other since 1983, numbers middle C 60, which Lattice
names `C-4`, as scientific pitch notation does (some makers, Yamaha among them, call it C3). So:

| Number | Name  | Note                                                    |
|--------|-------|---------------------------------------------------------|
| 0      | `C--` | the lowest; octave -1 is drawn as a dash                |
| 36     | `C-2` | two octaves below middle C, a bass note                 |
| 60     | `C-4` | middle C; also where drums and samples play as recorded |
| 69     | `A-4` | the A that orchestras tune to, 440 Hz                   |
| 127    | `G-9` | the highest                                             |

The number is what's saved in the song file (`pitch: 60`), and what transposing counts in.

### Velocity

**Velocity** is how hard a note is played. The name is MIDI's: a keyboard measures how fast each
key goes down, from 1 to 127. Instruments use it as they see fit, usually for loudness and often
for brightness too: a harder pluck is louder and brighter, a softer drum hit duller.

The `vel` field shows it as two digits, `00` to `99`, so it fits beside the note as a tracker's
columns do, and so it's typed the same way as a lock's value. `99` is 127, the hardest; `00` is 1,
the softest that still sounds. Notes go in at 100 unless you choose otherwise (`:vel`, below), and
100 shows as `78`. The scale is rounded, so a typed value can read back one off (see the notes at
the end of the chapter).

### Length and gate

A note's **length** is how many steps it lasts, 1 to 256. It's the note's **gate**: on an analogue
synthesiser the keyboard sends a gate signal that's on while a key is held, and the envelopes
open while it's on and let go when it goes off. In Lattice the gate opens on the note's step and
closes when its length runs out.

What an instrument does with that is up to it. A synth holds its note for the length and then
releases it; a bowed string is bowed while the gate is open; a plucked string or a harpsichord
rings on after the gate closes, as a harp's strings do, and the note-off only stops a bow or
drops a damper; a drum mostly ignores it; a sampler in `one-shot` mode plays to the end whatever
the length, while `gate` and `loop` follow it. [Chapter 7](07-instruments.md) says what each does.

`+` and `-` lengthen and shorten the selected notes a step at a time.

### The header

Above the grid, each track's header shows, line by line: its name with a flag (`M` muted, `m`
muted in this pattern only, `S` soloed, `─` muted by the song's entry, `○` `●` armed for the
looper); its length in steps, and while playing, which pass round it is (`16 · pass 3`); its level
as a bar from -60 to +6 dB; its pan as a bar from the centre; its rules, one a line, or
`no rules`; and a meter, a line that lights up as the track plays. Muted tracks, and tracks
silenced by another's solo, are dimmed. [Chapter 3](03-tracks.md) explains each. On a short
screen, under 30 lines or so, the level and pan are left out (the mixer has them), and so is
`no rules`, to leave more room for the steps; rules a track has still show.

The top line of the screen is the song's: the page, whether it's playing, the song's file (with
`[+]` for unsaved changes), the pattern (`pat A`), the tempo, then the keyboard octave (`oct`),
the velocity notes go in at (`vel`), the auto-step (`step`), the view (`pattern as written` or
`as playing`), and whether the view follows the playhead.

## Modes

Lattice is modal, as Helix and Vim are: the same key does different things in different modes,
so that most actions get a single key. The mode is shown at the bottom left.

| Mode    | Shown    | What keys do                                                         |
|---------|----------|----------------------------------------------------------------------|
| normal  | `NORMAL` | move, select and act; where you spend most of your time              |
| select  | `SELECT` | the same keys, but motions extend the selection instead of moving it |
| insert  | `INSERT` | letters play and enter notes; `esc` goes back to normal              |
| command | `:`      | type a command; `ret` runs it, `esc` cancels                         |

The right of the status line lists the keys that matter most in the mode you're in. Messages,
what an action did or why it didn't, appear in the accent colour in a box in the bottom-right
corner, above the status line, until the next key.

Prefix keys (`g`, `]`, `[`, `t`, `f`, `r`) wait for a second key, and show what can follow in a box
in the corner:

```
                                                         ┌ goto ──────────────────────────
                                                         │ g  first step
                                                         │ e  last step
                                                         │ h  first track
                                                         │ l  last track
                                                         │ m  mixer page
                                                         │ p  pattern page
                                                         │ s  song page
                                                         │ i  instrument page: every trac
                                                         │ f  fx page: the track's effect
                                                         └────────────────────────────────
 NORMAL                                           v select · x rows · g goto · ] next · i
```

`esc` or any key that isn't on the menu closes it.

## Moving the cursor

The cursor is a block on one step of one track (in insert mode it's in the accent colour, in
select mode in the select colour). Motions move it, and, in normal mode, collapse the selection
onto it, so after a motion the selection is only the cursor.

| Keys              | Action           | What it does                                      |
|-------------------|------------------|---------------------------------------------------|
| `j`, `↓`          | `down`           | next step                                         |
| `k`, `↑`          | `up`             | previous step                                     |
| `h`, `←`          | `left`           | previous field, then on to the previous track     |
| `l`, `→`          | `right`          | next field of the open track, then the next track |
| `th`              | `prev_track`     | previous track, landing on its notes              |
| `tl`              | `next_track`     | next track, landing on its notes                  |
| `f` then a key    | `goto_field`     | to a field of the cursor's track (below)          |
| `w`, `]n`         | `next_note`      | next step with a note on the cursor's track       |
| `b`, `[n`         | `prev_note`      | previous step with a note on the cursor's track   |
| `]b`              | `next_beat`      | next beat                                         |
| `[b`              | `prev_beat`      | previous beat                                     |
| `]m`              | `next_bar`       | next bar                                          |
| `[m`              | `prev_bar`       | previous bar                                      |
| `]p`              | `next_pattern`   | next pattern (see [chapter 6](06-song.md))        |
| `[p`              | `prev_pattern`   | previous pattern                                  |
| `C-d`             | `half_page_down` | down 8 steps                                      |
| `C-u`             | `half_page_up`   | up 8 steps                                        |
| `C-f`, `pagedown` | `page_down`      | down 16 steps                                     |
| `C-b`, `pageup`   | `page_up`        | up 16 steps                                       |
| `gg`, `home`      | `first_step`     | to step 00                                        |
| `ge`, `end`       | `last_step`      | to the last step                                  |
| `gh`              | `first_track`    | to the first track                                |
| `gl`              | `last_track`     | to the last track                                 |

The cursor stays on the grid: `j` on the last step stays there, as `k` does on the first, and
nothing wraps round. "The last step" is the end of the longest track the selection spans, so the
cursor can sit below the end of a shorter track only while the selection reaches a longer one.

`w` and `b` are Helix's word motions, and notes are the grid's words. They look only at the
cursor's track; if there's no note further on, the cursor stays where it is. The beat and bar
motions go to the next multiple of a beat or bar: `]b` from step 05 goes to 08, and from 08 to
12. `[b` from 05 goes back to 04, and from 04 to 00.

`]` and `[` are menus of "next" and "previous", as Helix borrowed from Vim's unimpaired plugin.
`]p` and `[p` change the pattern being edited; while the song plays, the new one is cued and
takes over at the next bar.

### Fields: `h`, `l` and `f`

`h` and `l` don't go straight from track to track: they go through the open track's fields
first. From the lead's note, `l` goes to its velocity, then its two gesture fields, then any lock
columns, and only then on to the next track's note. `h` goes back the same way, arriving on the
previous track's last field. Left of the first track's note, `h` goes into the conductor's
fields, the pattern's own lane for tempo changes and cues ([chapter 6](06-song.md)).

To jump, `f` then a key:

| Key      | Field                                                               |
|----------|---------------------------------------------------------------------|
| `n`      | the note                                                            |
| `v`      | the velocity                                                        |
| `g`      | the first gesture field                                             |
| `G`      | the second gesture field                                            |
| a letter | the lock column whose header underlines that letter (`c` for `cut`) |
| `C`      | the conductor's first field, from any track                         |

The letters `g`, `G` and the lock columns' are underlined in the header line above the steps.
A track folded into a group has only its notes; `fg` there says to unfold it with `tz`. Asking for
a field the track hasn't got says `x isn't a field on this track`.

`th` and `tl` are for when you want the next track rather than the next field: they always land
on the track's notes.

### Counts

Type a number before a key and it's done that many times, as in Helix: `4j` goes down four steps,
`3]m` three bars, `2w` two notes on. The count shows at the bottom left as you type it, up to
9999. `0` on its own isn't a count (there's no `0` motion), but after another digit it is, so
`10j` goes down ten.

Counts work on edits too: `12C-a` transposes up twelve semitones, `3p` pastes three times, `3C`
makes three copies of the selection, `2*` repeats the first two steps. Where an action has no
sensible "more", such as `u` or a track command, the count does nothing extra.

## Selections

### Select, then act

Lattice edits the way Helix does: you **select first, then act**, so you see what a change will
touch before it happens. Vim works the other way round (`d3j`: delete, then say what); Helix,
following Kakoune, turned it about (`v3jd`: select, see, delete).

There is always a selection. It's a rectangle of steps across one or more tracks, with two ends:
the **anchor**, where it started, and the **cursor**, the end that moves. With nothing selected,
the anchor and cursor are the same step, and the selection is only the cursor. Every edit acts on
the whole selection; most leave it selected afterwards, so `C-a C-a C-a` goes on transposing the
same notes.

The selected steps are shaded in the selection colour, and the cursor is a solid block.

### Making a selection

| Keys  | Action          | What it does                                                        |
|-------|-----------------|---------------------------------------------------------------------|
| `v`   | `select_mode`   | select mode on or off: motions extend the selection                 |
| `x`   | `select_rows`   | select the selection's rows across every track; again, one row more |
| `%`   | `select_all`    | select every step of every track                                    |
| `tt`  | `select_tracks` | select every step of the tracks the selections reach                |
| `;`   | `collapse`      | collapse each selection onto its cursor                             |
| `A-;` | `flip`          | swap each selection's anchor and cursor                             |
| `esc` | `escape`        | back to normal mode                                                 |

**Select mode.** `v` leaves the anchor where it is, and every motion then moves only the cursor,
so the selection stretches. From step 00 of the bass, `v3j` selects 00 to 03; `tl` takes in the
next track too; `]m` stretches to the next bar. `h` and `l` work as usual, so they move through
the open track's fields before extending onto the next track. Deleting, copying, pasting and
filling (`d` `A-d` `y` `p` `P` `r`) finish the selection and go back to normal mode, as in Helix;
other edits stay in select mode so you can go on. `v` again, or `esc`, leaves it.

**Rows.** `x` is Helix's "select line": it selects whole rows, every track from the first to the
last, over the steps the selection covers. Pressed again on a selection that already spans every
track, it adds the next row, so `xxx` selects three rows, as does `3x`.

**`;` and `A-;`.** `;` throws the selection away, leaving the cursor. `A-;` flips it, putting the
cursor at the other end, so you can stretch it the other way: select 04 to 07, `A-;`, then `k`
in select mode takes in 03.

Here the bass's first four steps are selected (`gh` `gg` `v3j`), shown by the bracket on the right:

```
 00  C-2         C-4    4    C-4      ┐
 01  ···         │           ···      │  anchor at 00, cursor at 03
 02  ···         │           C-4      │
 03  C-2         │           ···      ┘
 04  ···         G-4    3    C-4
```

### Many selections

As in Helix, there can be several selections at once. Each motion moves every one of them, and
each edit acts on all of them, as one undo step. The status line shows how many there are, and
which is the **primary**: `2/5 sels`. The view follows the primary, whose cursor is the solid
block; the others' cursors are marked but not filled. Selections that come to overlap merge into
one, the rectangle covering both.

| Keys  | Action            | What it does                                               |
|-------|-------------------|------------------------------------------------------------|
| `C`   | `copy_down`       | add a copy of each selection on the rows below it          |
| `A-C` | `copy_up`         | add a copy of each selection on the rows above it          |
| `s`   | `select_notes`    | select the notes matching a pattern, inside the selections |
| `K`   | `keep_notes`      | keep only the selections containing a matching note        |
| `A-K` | `remove_notes`    | remove the selections containing a matching note           |
| `A-s` | `split_rows`      | split each selection into one per row                      |
| `A--` | `merge_all`       | merge the selections into one covering them all            |
| `,`   | `keep_primary`    | keep only the primary selection                            |
| `A-,` | `remove_primary`  | remove the primary selection                               |
| `)`   | `rotate_forward`  | make the next selection primary                            |
| `(`   | `rotate_backward` | make the previous selection primary                        |

**`C` and `A-C`.** Helix's `C` copies the selection onto the next line; here it copies it onto
the rows below, as far down as it's tall. Select two steps and press `C` three times (or `3C`),
and you have four selections, two steps each, one after the other: a quick way to put something
on every half-bar. The furthest copy becomes the primary. Copies that would go past the end of
the track aren't made. There's no sideways copy onto the next track: a selection already spans
tracks, so widening it does that job.

**`s`, `K` and `A-K`.** These are the grid's stand-in for Helix's regular expressions. Each opens
a prompt at the bottom (`select:`, `keep:`, `remove:`), and as you type, the selections change to
show what you'll get; `ret` keeps the result and `esc` puts the selections back. A pattern is one
or more note names separated by spaces, any of which may match:

| Pattern      | Matches                                        |
|--------------|------------------------------------------------|
| `C`          | every C, in any octave                         |
| `C#`, `Db`   | every C sharp (`b` after the letter is a flat) |
| `Eb4`, `D#4` | E flat in octave 4 only                        |
| `C-4`        | C in octave 4, written as the grid writes it   |
| `*`          | any note                                       |
| `*4`         | any note in octave 4                           |
| `C E G`      | any C, E or G                                  |

The letters can be either case. `s` makes a one-step selection on every matching note inside the
selections; `K` keeps the selections that contain at least one; `A-K` removes those that do, but
won't remove every one (`that would remove every selection`). With nothing matching, `s` says
`no matches` and leaves the selections as they were.

So `%`, then `s` `C` `ret`, selects every C in the pattern, and `C-a` then sharpens them all:

```
 00  C#2        78   ··   ··   C#4    4    C#4
 01  ···             ··   ··   │           ···
 02  ···             ··   ··   │           C#4
 03  C#2        78   ··   ··   │           ···
 04  ···             ··   ··   G-4    3    C#4
 ...
 NORMAL  1/14 sels
```

**Splitting and merging.** `A-s` turns each selection into one per row, each still as wide as it
was: select four rows across two tracks and you have four selections, each a row of two steps,
which is how you'd act on each row on its own. `A--` does the reverse, merging every selection
into the one rectangle that covers them all.

**The primary.** `,` keeps only the primary and drops the rest, the usual way back to one
cursor. `A-,` drops the primary instead, and the next becomes primary; it won't drop the only
selection. `(` and `)` move which selection is primary, round and round, so you can bring each
into view, or choose which `,` keeps.

## Entering notes

### Insert mode

`i` starts **insert mode** at the start of each selection, and `a` straight after its end
("append", as in Helix); `c` deletes the selection, copying it, and starts inserting where it
was. In insert mode the letter keys are a piano: each key you press puts its note on the cursor's
step and moves the cursor on. `esc` goes back to normal mode.

| Keys               | Action                 | What it does                                       |
|--------------------|------------------------|----------------------------------------------------|
| `i`                | `insert`               | insert at the start of each selection              |
| `a`                | `append`               | insert after the end of each selection             |
| `c`                | `change`               | delete the selection (copying it) and insert there |
| `esc`              | `escape`               | back to normal mode                                |
| `backspace`, `del` | `clear_advance`        | in insert mode: clear the step and move on         |
| `{`                | `keyboard_octave_down` | the keyboard octave down                           |
| `}`                | `keyboard_octave_up`   | the keyboard octave up                             |

With several selections, each gets a cursor, and a note typed goes in at every one of them, so
you can enter the same line on four tracks at once, or on every bar.

A note goes in at the keyboard octave, at the velocity in the header (`vel`), one step long,
replacing any note that was there; the step's gestures and locks stay. You hear it as you type
it ("Hearing it", below). The cursor then moves on by the **auto-step**, and wraps round to the
top of the track at the end, so you can go on round a loop.

Keys that aren't notes still do what they do in normal mode where it makes sense: the arrows,
`pageup` `pagedown` `home` `end` move, `space` plays and stops, `tab` switches the view, `{` `}`
change octave, `R` records. Ctrl-C quits Lattice from either mode, at once.

### The piano keyboard

The default layout is the tracker's two-row piano. The bottom row of letters is the white keys
of one octave, and the row above it the black keys, as they'd sit on a piano; the `q` row and the
number row above it are the next octave up, and the keys from `i` go on into a third, as far as G:

```
   2 3   5 6 7   9 0   =            C# D#    F# G# A#    C# D#    F#
  q w e r t y u i o p [ ]          C  D  E  F  G  A  B  C  D  E  F  G      (octave +1, +2)

   s d   g h j   l ;                C# D#    F# G# A#    C# D#
 \ z x c v b n m , . /          B  C  D  E  F  G  A  B  C  D  E          (octave 0, +1)
```

At the keyboard octave 4, `z` is `C-4`, `q` is `C-5`, `i` is `C-6` and `]` is `G-6`; `\`, left
of `z` on a UK keyboard, is `B-3`, the note below. The keys right of `m` overlap the `q` row: `,`
is `C-5`, the same note as `q`. A note above `G-9`, MIDI's top, isn't entered.

This is why the keyboard octave is on `{` and `}` rather than `[` and `]` as in some trackers:
`[` and `]` are notes. On a US keyboard, `\` is above `ret` rather than beside `z`, but it plays
the same B.

### Other layouts: `:layout`

The piano layout makes a C major scale easy and every other key awkward, and each chord has a
different shape in each key. `:layout NAME` lays the notes out another way, over all four rows
of the keyboard, `z` (the keyboard octave's C) at the bottom left to `=` at the top right:

| Layout    | To the right     | A row up                 | Range        | Like                   |
|-----------|------------------|--------------------------|--------------|------------------------|
| `piano`   | the piano's      | the next octave          | 2½ octaves   | a tracker              |
| `wicki`   | a whole tone     | fourth left, fifth right | 37 semitones | a concertina           |
| `fourths` | a semitone       | a fourth                 | 26 semitones | a guitar, Linnstrument |
| `scale`   | the scale's next | about a fourth           | scale notes  | Push's in-key mode     |

The `wicki`, `fourths` and `scale` layouts are **isomorphic**: an interval is the same shape
wherever you play it, so once you know where a major chord is under your fingers, every major
chord is the same shape, moved. That's the idea behind button accordions, the Wicki–Hayden
system (Kaspar Wicki patented it in 1896; Brian Hayden came to it again for the concertina in the
1950s), the guitar's strings tuned mostly in fourths, and grid controllers like the LinnStrument.

In `wicki`, with the keyboard octave at 4:

```
 1:D#5  2:F-5  3:G-5  4:A-5  5:B-5  6:C#6  7:D#6  8:F-6  9:G-6  0:A-6  -:B-6  =:C#7
  q:A#4  w:C-5  e:D-5  r:E-5  t:F#5  y:G#5  u:A#5  i:C-6  o:D-6  p:E-6  [:F#6  ]:G#6
   a:F-4  s:G-4  d:A-4  f:B-4  g:C#5  h:D#5  j:F-5  k:G-5  l:A-5  ;:B-5  ':C#6
    z:C-4  x:D-4  c:E-4  v:F#4  b:G#4  n:A#4  m:C-5  ,:D-5  .:E-5  /:F#5
```

A major triad is `z` `c` (a third, two keys right) and `s` (a fifth, up-right): C, E, G. Move the
same shape anywhere and it's still a major triad.

`:layout scale` plays only the notes of the scale the numpad's pads play, `:scale NAME [ROOT]`,
C major to start, from its root at `z`: each key right is the next note of the scale, and each row
up is three notes of a seven-note scale (two of a five- or six-note one), close to a fourth. So
nothing is out of key. `:scale minor A` then `:layout scale` puts A minor under your fingers, A at
`z`; changing `:scale` while in this layout lays the keys out again. The scales are `major`,
`minor`, `dorian`, `mixolydian`, `pentatonic`, `minor-pentatonic`, `blues` and `chromatic`
([chapter 9](09-recording.md) has more on the pads).

`:layout piano` goes back. In every layout, `{` and `}` change the octave, and the status line in
insert mode says which layout you're in (`z–] notes`, `wicki notes`, `A minor notes`). The layout
lasts until Lattice quits; it isn't saved with the song.

### The keyboard octave: `{` `}` and `:oct`

The **keyboard octave**, `oct` in the header, is the octave the piano's `z` plays: 4 to start,
so `z` is middle C. `{` and `}` move it down and up one, from 0 to 9, in any mode and on any
page (they work while choosing `r`'s note, too); `:oct N` sets it outright. The message says
`octave 3`. Like the layout, it's not saved.

### Auto-step: `:step`

`:step N` sets how many steps the cursor moves on after each note, 0 to 64; it starts at 1 and is
shown as `step` in the header. With `:step 2`, typing eight notes puts one on every other step;
`:step 4` puts one on each beat. `:step 0` doesn't move at all, for trying notes on one step.
The cursor wraps round to the top of the track, so with `:step 2` and eight `z`s, a 16-step hat
track fills with `C-4` on every even step:

```
:step 2
i z z z z z z z z esc
```

```
    BASS        LEAD        HAT
 00  C-2         C-4    4    C-4        78   ··   ··
 01  ···         │           ···             ··   ··
 02  ···         │           C-4        78   ··   ··
 03  C-2         │           ···             ··   ··
 04  ···         G-4    3    C-4        78   ··   ··
 ...
 14  F#2         G-4         C-4        78   ··   ··
 15  F-2         ···         ···             ··   ··
```

Auto-step also moves the cursor after clearing a step in insert mode and after typing a value
into a field.

### Velocity: `:vel`

`:vel N` sets the velocity notes are entered and played at, `00` to `99` as the `vel` field shows
it, so the number you type is the number you'll see on the notes; it starts at `78` (100 of 127)
and is shown as `vel` in the header. It applies to every note you enter, from the piano keys, `r`,
or the numpad's pads, and to what you hear. The numpad's `/` `*` `-` set it to `47`, `78` and
`99`, soft, medium and hard ([chapter 9](09-recording.md)). To change notes already entered,
edit their `vel` field (below).

### Typing into fields

With the cursor in a field rather than on the notes, insert mode types into the field:

- in the **velocity** field, two digits set the value, `00` to `99`, and move on (`i` `5` `0` sets
  50). Only a step with a note has a velocity; on an empty step the digits do nothing but move on;
- in a **gesture** field, type a gesture (`R3`, `J12`, `?50`); it goes in once it's whole, or with
  `ret` or `esc` ([chapter 5](05-gestures.md));
- in a **lock column**, two digits set the value, `~` first makes it slide there, and `-` puts the
  setting back to the mixer's ([chapter 5](05-gestures.md)).

The status line shows what you've typed so far (`5_`), and `backspace` takes back a gesture's
last key.

### Clearing in insert mode

`backspace` (or `del`) clears the step at each cursor, its note, gestures and locks, and moves on
by the auto-step, so you can type through a line, entering some steps and clearing others.

## Editing the selection

Every edit here acts on all the selected steps, on every selection, as one undo step. Most keep
the selection, so you can repeat them.

| Keys                | Action                  | What it does                                     |
|---------------------|-------------------------|--------------------------------------------------|
| `d`                 | `delete`                | clear the selected steps, copying them first     |
| `A-d`, `del`        | `clear`                 | clear the selected steps without copying         |
| `c`                 | `change`                | clear (copying) and start inserting              |
| `y`                 | `yank`                  | copy the selected steps                          |
| `p`                 | `paste`                 | paste over the steps from the start              |
| `P`                 | `paste_insert`          | paste at the start, pushing the steps there down |
| `r` then a note key | `replace`               | fill every selected step with that note          |
| `C-a`               | `note_up`               | notes up a semitone                              |
| `C-x`               | `note_down`             | notes down a semitone                            |
| `>`                 | `note_octave_up`        | notes up an octave                               |
| `<`                 | `note_octave_down`      | notes down an octave                             |
| `+` (or `=`)        | `longer`                | notes a step longer                              |
| `-`                 | `shorter`               | notes a step shorter                             |
| `~`                 | `reverse_steps`         | turn the selected steps back to front            |
| `*`                 | `repeat_steps`          | repeat the first step (or first N) through it    |
| `A-)`               | `rotate_steps_forward`  | rotate the steps down one, the last to the top   |
| `A-(`               | `rotate_steps_backward` | rotate the steps up one, the first to the bottom |
| `&`                 | `fill`                  | in a field, fill evenly between the two ends     |
| `u`                 | `undo`                  | undo                                             |
| `U`                 | `redo`                  | redo                                             |

### Deleting

`d` clears every selected step (its note, velocity, gestures and locks) and copies them first, so
`d` then `p` elsewhere moves them. `A-d`, or `del`, clears without copying, leaving what you
copied before. Either collapses each selection onto its first step, as Helix does after a delete.
The steps stay: deleting empties them, and the track keeps its length. (To shorten a track, use
`:len`, in [chapter 3](03-tracks.md).)

`c` is `d` and then `i`: clear the selection, copying it, and start inserting at its first step.

### Copying and pasting

`y` copies the selected steps, whole: notes, velocities, lengths, gestures and locks. The message
says how much (`copied 8 steps`). As in Helix, each selection is copied as its own clip, so with
three selections you have three clips (`copied 3 selections`).

`p` pastes over the steps already there, from the top left of each selection: the first column
of the clip goes onto the selection's first track, the next onto the track to its right, and so
on. Columns that would go past the last track, and steps past the end of a track, are left out.
`3p` pastes three times, one after the other down the track. Afterwards the selection is what was
pasted, so you can see it and act on it again.

`P` inserts instead: the steps from the selection down are pushed down to make room, as a
tracker's insert does, and whatever is pushed past the end of the track falls off; the track
doesn't get longer. Here the first row was copied across all three tracks (`gg` `x` `y`), then
inserted at step 04 of the bass (`gh` `4j` `P`), pushing everything below down a step, so the
bass's last note fell off the end:

```
 00  C-2         C-4    4    C-4        78   ··   ··
 01  ···         │           ···             ··   ··
 02  ···         │           C-4        78   ··   ··
 03  C-2         │           ···             ··   ··
 04  C-2         C-4    4    C-4        78   ··   ··      <- pasted
 05  ···         G-4    3    C-4        78   ··   ··      <- pushed down from 04
 06  ···         │           ···             ··   ··
 07  D#2         │           C-4        78   ··   ··
 ...
 15  F#2         G-4         C-4        78   ··   ··
```

With several selections and several clips, the selections take the clips in order, as Helix
hands out one value per selection; if there are more selections than clips, the last clip is
used for the rest. So `y` on one selection and `p` on four puts the same steps in four places.
`nothing to paste` means nothing's been copied yet.

The clipboard is Lattice's own, not the system's, and it's kept until you copy again. Steps,
values copied from a field, and whole tracks (`ty`) each have their own.

### Filling with a note: `r`

`r` then a piano key fills every selected step with that note, empty steps included: the note at
the keyboard octave and the `:vel` velocity, one step long. Any length the old notes had goes, but
the steps' gestures and locks stay. The box in the corner says `z–] fill with this note`; `{` and
`}` change the octave first if you need to, and `esc` cancels. It's Helix's `r`, which replaces
each selected character with the one you type. `x` then `r` `z`, say, puts a C on every track of
a row.

### Transposing

`C-a` and `C-x` move every selected note up or down a semitone; with a count, that many
(`7C-a`, up a fifth). `>` and `<` move them an octave. Empty steps are left alone, and a note at
either end of MIDI's range stops there (0 or 127) rather than wrapping. You hear the last note
changed, so `C-a` repeated is a way of finding the note you want by ear.

The keys come from Helix and Vim, where `C-a` and `C-x` add to and subtract from a number, and
`>` and `<` indent. In a field, they change the field's values instead (below).

To have a track transposed every time round, or every fourth time, without changing what's
written, use a rule: `:tr 7` ([chapter 4](04-rules.md)).

### Lengths

`+` (or `=`, the same key without shift) makes every selected note a step longer, and `-` a step
shorter, from 1 to 256 steps; a count changes it by that many. The faint `│` under each note
shows where it now ends.

### Rearranging: `~`, `*`, `A-(` and `A-)`

These move the selected steps round within the selection, on each track it reaches, notes,
velocities, gestures and locks together.

`~` **reverses** them: turns them back to front, so the selection plays backwards. A note keeps
its length and ends where it used to start: one that started `k` steps into the selection ends
`k` steps from its end. A note held past the end of the selection is cut at it, since only the
selection is turned round. Here the lead's whole track (`tt`) reversed:

```
      before                    after ~
 00  C-4    4               00  ···
 01  │                      01  G-4
 02  │                      02  ···
 03  │                      03  ···
 04  G-4    3               04  ···
 05  │                      05  C-5
 06  │                      06  B-4    2
 07  ···                    07  │
 08  B-4    2               08  ···
 09  │                      09  G-4    3
 10  C-5                    10  │
 11  ···                    11  │
 12  ···                    12  C-4    4
 13  ···                    13  │
 14  G-4                    14  │
 15  ···                    15  │
```

On a `chop` track, `~` also turns each piece of the sample round, with a `<` gesture, so the
slice plays backwards too and the loop sounds reversed rather than reordered
([chapter 7](07-instruments.md)). Helix's `~` switches case; reversing is the grid's nearest
thing.

`*` **repeats** the selection's first step through the rest of it; with a count, the first that
many steps, over and over. Select a bar of hats with one hit on step 00 and `*` puts a hit on
every step; `4*` repeats the first beat through the bar; `3*` on a bar makes a three-step figure
that cuts across the beat. It's how you'd make a stutter on a `chop` track.

`A-)` **rotates** the selected steps down one, the last coming round to the top; `A-(` rotates
them up. With a count, by that many: `4A-)` moves a bar of steps a beat later. Notes keep their
lengths. Helix's `A-(` and `A-)` rotate the contents of the selections among themselves; here they
rotate the steps within each selection. (The rules `:rot` and `:drift` do the same each pass
without changing what's written: [chapter 4](04-rules.md).)

### Editing a field

With the cursor in a field (velocity, a gesture or a lock column) and every selection on that
one track, the editing keys act on the field alone, leaving the notes as they are:

| Keys        | In a field                                                                  |
|-------------|-----------------------------------------------------------------------------|
| `C-a` `C-x` | each value up / down by one (of 99); a gesture's amount by one              |
| `>` `<`     | each value up / down by ten                                                 |
| `d` `A-d`   | clear the values (with `d`, copying them); a velocity goes back to `:vel`'s |
| `c`         | clear the values (copying them) and start inserting                         |
| `y`         | copy the values, step by step (`copied 8 values`)                           |
| `p` `P`     | paste the copied values down from each selection's start                    |
| `&`         | fill each selection evenly between the values at its ends                   |
| `r`         | says to type the value in insert mode instead                               |

If the selections reach other tracks, edits go back to acting on whole steps.

`&` **fills** a field evenly: it takes the values at the first and last steps of each selection
and sets every step between to a value on the straight line from one to the other. It needs a
value at both ends (`& needs a value at each end of the selection`). For a crescendo on the hats,
set the first velocity to 20 and the last to 99, select from one to the other, and `&`:

```
fv  gg  i 2 0 esc      (first hat: velocity 20)
14j  i 9 9 esc         (last hat, step 14: velocity 99)
gg  v 14j  &
```

```
                                        vel g    G
 00  C-2         C-4    4    C-4        21   ··   ··
 01  ···         │           ···             ··   ··
 02  ···         │           C-4        32   ··   ··
 03  C-2         │           ···             ··   ··
 04  ···         G-4    3    C-4        43   ··   ··
 05  ···         │           ···             ··   ··
 06  D#2         │           C-4        54   ··   ··
 07  ···         ···         ···             ··   ··
 08  C-2         B-4    2    C-4        67   ··   ··
 09  ···         │           ···             ··   ··
 10  ···         C-5         C-4        78   ··   ··
 11  ···         ···         ···             ··   ··
 12  ···         ···         C-4        89   ··   ··
 13  ···         ···         ···             ··   ··
 14  F#2         G-4         C-4        99   ··   ··
```

(The first reads `21`, not `20`: see the notes at the end of the chapter.)

Velocities only go on notes, so the empty steps between are skipped. In the conductor `&` fills
tempos, for a gradual change of speed ([chapter 6](06-song.md)); gesture fields don't fill, and
`&` says to copy them with `y` and `p` instead. Helix's `&` lines selections up in a column;
filling a column of values is the grid's version.

## Undo and redo

`u` undoes the last change and `U` redoes it. Every edit is one undo step, however many
selections it touched, and undo puts the selections back as they were before it, so you see
what came back. Lattice keeps the last 500 steps; making a new change after undoing clears what
could have been redone. `nothing to undo` means you're back at the start.

Undo covers the whole song, not only the pattern page: notes, tracks added or removed (with
their instruments and every pattern's notes), the mixer and effects, groups, rules, the
arrangement, and changes to the meter. While recording, each pass round the track is one step
([chapter 9](09-recording.md)).

What isn't undone is what isn't part of the song's music: the keyboard octave, auto-step,
velocity and layout, the view, which tracks are kept open, and `:bpm` (a tempo change only comes
back with an undone change of meter, since `:meter` changes the tempo with it). Undoing leaves
insert or select mode for normal.

## Hearing it

### Auditioning

Every note you enter plays as you type it, on the cursor's track, through its instrument and
effects, so you hear what goes in. In a terminal that reports key releases (kitty, WezTerm, foot,
Ghostty, recent Alacritty; not inside tmux), the note sounds for as long as you hold the key, so a
pad or bowed string sustains and several keys held ring together as a chord. Elsewhere each note
is a short blip.

`r`'s note, and the last note changed by `C-a` `C-x` `>` `<`, play too, briefly. Moving the cursor
doesn't play anything. The numpad's pads play the cursor's group's drums or a scale whenever you
press them, in any mode ([chapter 9](09-recording.md)), and the instrument page's `i` plays a
track from the keyboard without entering notes ([chapter 7](07-instruments.md)).

### Playing and following

`space` plays the pattern and stops it, in any mode. It loops round until stopped; each track
loops on its own length, so a playhead, a block in the track's colour, runs down each column
separately. While playing, `:follow` (on to start) keeps the view scrolled to the playhead; with
it off, the view stays where you put it, which helps when editing one part of a long pattern
while the rest plays. Edits are heard on the next pass over the step, even mid-loop.

| Keys      | Action            | What it does                                              |
|-----------|-------------------|-----------------------------------------------------------|
| `space`   | `toggle_play`     | play / stop                                               |
| `C-space` | `play_song`       | play the song from its start (or stop)                    |
| `A-space` | `play_from_here`  | play the song from this pattern's entry (or stop)         |
| `C-k`     | `hush`            | stop the song and every sound at once, tails and all      |
| `tab`     | `toggle_view`     | show the pattern as written, or as it's playing this pass |
| `:`       | `command_line`    | the command line                                          |
| `gm`      | `goto_mixer`      | the mixer page                                            |
| `gs`      | `goto_song`       | the song page                                             |
| `gi`      | `goto_instrument` | the instrument page                                       |
| `gf`      | `goto_effects`    | the effects page                                          |
| `gp`      | `goto_pattern`    | back to the pattern page                                  |
| `C-c`     | `quit`            | quit; again to quit with unsaved changes                  |

Playing the song, rather than looping this pattern, is in [chapter 6](06-song.md).

### Looping some steps: `|`

To work on a few steps, loop playback round them: select the rows (`v` and `j`, or `x`) and
press `|`, `loop_selection`. With only the cursor, `|` loops round its beat. The gutter marks the
loop, `↻` on its first step and `┃` down the rest, and the status line says
`looping steps 04 to 07`, numbered as the gutter numbers rows. `|` on a looping step stops.

```
 04↻  C-4  ··
 05┃  ···  ··
 06┃  E-4  ··
 07┃  ···  ··
 08   G-4  ··
```

At the end of the loop playback jumps back to its first step, each track to the step it plays
there and going forward at the song's speed, as from the top, whatever its gestures had it doing.
Notes still sounding ring on. Started from outside the loop, or with play stopped, it jumps in on
the next beat. While it loops, the pattern stays, even in the song or with another cued. The
loop is the pattern's: going to another pattern leaves it, and coming back finds it again. It
isn't saved. `|` is a repeat bar, as in printed music; the song page loops bars and entries the
same way ([chapter 6](06-song.md)).

### As written, or as playing: `tab`

The grid normally shows the pattern **as written**. A track's rules change what it plays each
pass without changing what's written (a rotation, a transposition, every fourth time), so `tab`
switches to the pattern **as playing**: each track as its rules make it on this pass. Steps the
rules have changed are shown bold and italic. The header says which you're looking at
(`pattern as written`, `pattern as playing`). Here the lead has `:rot 2`, and `tab` shows it
rotated two steps on:

```
 LATTICE  PATTERN   ■ STOP    pat.lattice [+]   pat A   bpm 130   ...   pattern as playing

    BASS        LEAD
    16          16
    no rules    rot+2
    ─────────── ─────────────────────────
                            vel g    G
 00  C-2         G-4        78   ··   ··
 01  ···         ···             ··   ··
 02  ···         C-4    4   78   ··   ··
 03  C-2         │               ··   ··
 04  ···         │               ··   ··
 05  ···         │               ··   ··
 06  D#2         G-4    3   78   ··   ··
```

You still edit what's written, whichever you're looking at. The rules, `:tr`, `:rot`, `:drift`
and `:rev`, with `:pop` and `:clear` to take them off again, are in [chapter 4](04-rules.md).

## A worked example

A four-on-the-floor kick, a bass line, and a hat on the off-beats, from an empty song (`:n`):

```
:name kick<ret>  :inst kick<ret>         the song's one track becomes the kick
:step 4<ret>  i z z z z <esc>            a C-4 on each beat: auto-step 4 skips to the next
:track-add bass<ret>                     a track after it; the cursor goes to it
:oct 2<ret>  :step 1<ret>
gg  i z <down> z <down> d <esc>          C-2 on 00 and 02, D#2 on 04
gg  v 7j  y                              copy the first half of the bar
gg  8j  p                                and paste it over the second
:track-add hat<ret>  :inst hat<ret>  :oct 4<ret>
gg  2j  i z <esc>                        one hat, on step 02
gg  2j  v 13j  4*                        repeat 02 to 05 (the hat and three empty steps) to the end
```

In insert mode each note moves the cursor on a step, so `<down>` skips one. The result:

```
    KICK        BASS        HAT
    16          16          16
 00  C-4         C-2         ···
 01  ···         ···         ···
 02  ···         C-2         C-4        78   ··   ··
 03  ···         ···         ···
 04  C-4         D#2         ···
 05  ···         ···         ···
 06  ···         ···         C-4        78   ··   ··
 07  ···         ···         ···
 08  C-4         C-2         ···
 09  ···         ···         ···
 10  ···         C-2         C-4        78   ··   ··
 11  ···         ···         ···
 12  C-4         D#2         ···
 13  ···         ···         ···
 14  ···         ···         C-4        78   ··   ··
 15  ···         ···         ···
```

`space` plays it. From here, `x` `>` on a row moves it up an octave, `w` hops between the bass
notes, and `:tr 5 every 4 3` would move the bass up a fourth every fourth bar.

## Notes and oddities

- If letters in insert mode don't enter notes, the cursor is in a field, typing values: `fn`
  goes back to the notes. The field the cursor was last in is remembered, even across `:n`.
- Velocity is stored as MIDI's 1 to 127 but shown and typed as 00 to 99, and the conversion
  rounds both ways, so a typed value can read back one higher: typing `20` gives velocity 26,
  shown as `21`, and `70` shows as `71`.
- Every note entered from the keyboard in insert mode is one step long; `+` lengthens it.
  Recording live, a held key sets the length ([chapter 9](09-recording.md)).
- One step of one track holds one note. A chord is several tracks, one note each.
