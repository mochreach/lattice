# Patterns, the song and the conductor

A song in Lattice is a set of tracks, a set of **patterns** that play on them, and the
**arrangement**: the order the patterns play in, shown on the song page. Each pattern also has a
**conductor**, a lane for what belongs to the whole song: tempo changes, stopping, cueing
another pattern, bringing a part in or out. This chapter covers all three, and the **meter** the
song counts its bars in.

## Patterns

A pattern is a section of the song: a verse, a chorus, a breakdown. Every pattern has the same
tracks, with the same instruments, mixer settings and effects; what each pattern has of its own,
for every track, is a **lane**: the track's steps, its rules (see [Rules](04-rules.md)) and
whether it's muted in this pattern. A pattern also has its own conductor, its own length in bars
if you set one, and its own meter if you give it one.

Sharing the tracks is deliberate. There's one mixer to set up, not one per section, and delays,
reverbs and long notes ring on across a change of pattern, as they would in a band. A part that
plays in only some sections is a track left empty, or muted, in the others.

The grid always shows the pattern being edited, and **the pattern being edited is always the one
playing**: everything that works on tracks works on it. The header names it: `pat A`.

### Making and removing patterns

| Command               | Short | What it does                                                   |
|-----------------------|-------|----------------------------------------------------------------|
| `:pattern-new [NAME]` | `:pn` | add an empty pattern after this one, and go to it              |
| `:pattern-dup [NAME]` | `:pd` | add a copy of this one after it, and go to the copy            |
| `:pattern-name NAME`  |       | rename this pattern (on the song page, the cursor's)           |
| `:pattern-remove`     |       | remove this pattern, and its places in the song                |
| `:pattern-mute`       | `:pm` | mute or unmute the selected tracks in this pattern only (`tM`) |

- A new pattern is named `A` to `Z` in turn, the first letter not taken (then `A2`, `B2`…),
  unless you give it a name. Names must be different: `there's already a pattern verse`.
- `:pattern-new` gives each track an empty lane as long as the track is now, with no rules, and
  an empty conductor. `:pattern-dup` copies every lane, steps, rules and mutes, and the
  conductor. Either way the new pattern keeps this one's `:bars` and meter.
- Both add the new pattern to the end of the song, played once, so it's heard when the song is
  played through. A song has up to 64 patterns (`songs have up to 64 patterns`) and 256 entries.
- `:pattern-remove` goes to the pattern before (or after, for the first), then removes this one
  and every entry in the song that plays it. A song needs at least one:
  `a song needs a pattern`. It's one undo, like the rest.
- `:pattern-mute`, or `tM`, mutes the tracks the selections reach in this pattern alone. Their
  notes stay, ready to bring back, and the other patterns play theirs. The header shows `m` by
  the track's name, where the mixer's mute shows `M`. Notes already sounding ring on.

### Going between patterns

| Key / command   | Action                        | What it does                                |
|-----------------|-------------------------------|---------------------------------------------|
| `]p` `[p`       | `next_pattern` `prev_pattern` | the next / previous pattern, wrapping round |
| `:pattern NAME` |                               | go to a pattern, by name or number from 1   |

`:pattern` (`:pat`) looks for a pattern by its name first, then by its number in the list on the
song page: `:pattern chorus`, `:pattern 3`.

Stopped, you go to the pattern at once. **Playing, the pattern is cued**: the status line says
`chorus cued, after verse`, and the one playing carries on to the end of its bar, so the change
lands on the next bar line. Then the new pattern starts, every track from its first step and its
first pass. `]p` while one is cued steps on from the cued one, so `]p]p` cues the pattern two
along. Picking the one that's playing cancels the cue (`staying on verse`).

Picking a pattern by hand while the song plays **leaves the arrangement**: the song stops
following its entries, and the pattern you picked loops until you pick another. This is how to
play live, jumping between sections as the room wants, rather than in a set order.

### How long a pattern plays: `:bars`

`:bars N` makes the pattern play for N bars (1 to 256) before it loops or the song moves on.
`:bars auto`, or `:bars` alone, goes back to the default: as long as its longest track, rounded
up to a whole bar. The status line says what it came to:
`A plays 2 bars, as long as its longest track`. On the song page, `:bars` sets the cursor's
pattern rather than the one being edited.

A pattern shorter than a track cuts the track off; a track shorter than the pattern goes round
again. When a pattern loops, its tracks don't restart: they carry on round their own lengths, so
tracks of different lengths keep drifting against each other for as long as it loops (see
[Polymeter](04-rules.md)). Only a change of pattern starts every track from the top.

## The meter

### Beats, bars and steps

Music is counted in **beats**, the pulse you tap your foot to, grouped into **bars**. A **time
signature** says how: 4/4 is four quarter-note beats a bar, 3/4 three (a waltz), 6/8 six eighth
notes felt as two beats of three. Lattice sets the meter with two numbers: **beats in a bar**
and **steps in a beat**. A song starts with four beats of four steps: 4/4 counted in sixteenth
notes, the usual grid of drum machines and techno.

The tempo, `:bpm`, counts beats per minute, so how long a step lasts depends on both: at 120 BPM
with four steps a beat, a beat is half a second and a step an eighth of a second.

| Time           | Command          | Beats × steps | Each step is     |
|----------------|------------------|---------------|------------------|
| 4/4            | `:meter 4 4`     | 4 × 4         | a sixteenth note |
| 3/4 (a waltz)  | `:meter 3 4`     | 3 × 4         | a sixteenth note |
| 2/2 (cut time) | `:meter 2 4`     | 2 × 4         | an eighth note   |
| 6/8            | `:meter 2 3`     | 2 × 3         | an eighth note   |
| 12/8           | `:meter 4 3`     | 4 × 3         | an eighth note   |
| 5/4 as 3+2     | `:meter 3+2 4`   | 5 × 4         | a sixteenth note |
| 7/8 as 2+2+3   | `:meter 2+2+3 2` | 7 × 2         | a sixteenth note |

### `:meter`

`:meter BEATS [STEPS]` sets the song's meter: beats in a bar (1 to 16) and, optionally, steps in
a beat (1 to 16; left out, they stay as they are). `:meter` alone says what it is:
`4 beats a bar, 4 steps a beat`.

**The music doesn't move.** Patterns keep their steps, now counted in the new bars, and the
notes play at the same speed as before. So a change of steps per beat changes the tempo with it:
4/4 at 120 BPM with four steps a beat, made `:meter 2+2+3 2`, says
`7 beats a bar (2+2+3), 2 steps a beat at 240.0 BPM`, as each beat is now half as long. Every
conductor tempo is scaled the same way, rounded to a whole BPM.

It refuses what it can't do, and changes nothing:

- a tempo that would come out beyond 20–400 BPM: `that makes the tempo 552.0, outside 20–400`;
  likewise any of the conductor's tempos;
- a pattern of so many `:bars` whose steps don't make whole bars of the new meter:
  `end is 32 steps, which isn't 1 to 256 bars of 9` (give it `:bars auto` first, or another
  length);
- more than 16 beats: `beats: 1 to 16, or groups of them as 2+2+3`.

Undo puts back the meter, and the tempo with it; an undo of anything else leaves the tempo
alone.

### Groupings and accents

A bar of five or seven beats isn't felt as five or seven equal beats but in **groups** of twos
and threes. Write the groups with `+` and Lattice adds them up: `:meter 2+2+3` is seven beats a
bar, grouped two, two, three. The metronome's click accents the first beat of the bar most, and
the first beat of each other group a little less, and the grid shows them: the step numbers of
each bar's first step are bold, those of each group's first beat are in the accent colour, and
those between beats are dim. A plain number of beats is one group, as before.

```
 00  ·  ·  ·  ·  ·  ·  ·  ···       bold: the bar's first beat
 01  ·  ·  ·  ·  ·  ·  ·  ···       dim: between beats
 02  ·  ·  ·  ·  ·  ·  ·  ···
 03  ·  ·  ·  ·  ·  ·  ·  ···
 04  ·  ·  ·  ·  ·  ·  ·  ···       the accent colour: the second group
 05  ·  ·  ·  ·  ·  ·  ·  ···
 06  ·  ·  ·  ·  ·  ·  ·  ···
 07  ·  ·  ·  ·  ·  ·  ·  ···
 08  ·  ·  ·  ·  ·  ·  ·  ···       the accent colour: the third group
 …
 13  ·  ·  ·  ·  ·  ·  ·  ···
 14  ·  ·  ·  ·  ·  ·  ·  ···       bold: the next bar
```

### Simple and compound time

**3/4 and 6/8** both have six eighth notes in a bar, but they aren't the same. 3/4 is three
beats of two eighths: ONE-and TWO-and THREE-and, a waltz. 6/8 is two beats of three: ONE-and-a
TWO-and-a, a jig, or a lilting ballad. Music whose beats split in three is in **compound time**;
in two, **simple time**. In Lattice, compound time is a beat of three steps: `:meter 2 3` is
6/8, `:meter 4 3` 12/8 (a slow blues, or a shuffle). The tempo then counts the dotted-quarter
beats, two to a bar of 6/8, as a conductor would beat it.

### Odd meters

Bars of five, seven, nine or eleven beats are **odd meters**, and the groups make them dance
rather than stumble. Dave Brubeck's Take Five (1959) is in 5/4, felt 3+2 (`:meter 3+2`). Pink
Floyd's Money is in 7/4. Much of the music of the Balkans and Turkey is built on them: Turkish
musicians call these rhythms **aksak**, "limping", for the long group that breaks the even
pulse. Bulgarian dances name their meter: the rachenitsa in 7/8 (2+2+3), the daichovo horo in
9/8 (2+2+2+3) and the kopanitsa in 11/8 (2+2+3+2+2). Brubeck's Blue Rondo à la Turk (1959) took
its 9/8, grouped 2+2+2+3, from Turkish street musicians.

Odd meters are usually counted in eighth notes, so make the beat an eighth, a few steps each:

```
:meter 2+2+3 2        rachenitsa: 7/8, each eighth two sixteenth-note steps
:meter 2+2+2+3 2      daichovo horo: 9/8
:meter 2+2+3+2+2 2    kopanitsa: 11/8
```

With the beat an eighth note, `:bpm` counts eighths: 240 BPM is 120 quarter notes a minute.

### A pattern's own meter: `:pattern-meter`

A song can change time: a verse in 4/4 and a bridge in 7/8. `:pattern-meter 2+2+3` gives the
pattern being edited bars of its own, in beats and groups, while the steps in a beat stay the
song's, so the tempo means the same in every pattern and a step lasts as long in the 7/8 as in
the 4/4. Its length, the grid's bar lines and accents, the bar motions (`]m` `[m`), the click
and the song page's bars all follow it, and the engine switches meter as the pattern comes in.
`:pattern-meter song` puts it back in the song's; `:pattern-meter` alone says which it's in:
`cave is in its own meter: 7 beats a bar (2+2+3), 2 steps a beat`.

To go from 4/4 into 7/8, the beat has to be an eighth in both, so count the song in eighths and
give the 7/8 pattern its own grouping:

```
:meter 2+2+2+2 2      the song: 4/4 as eight eighth-note beats, each quarter accented
:pattern-meter 2+2+3  this pattern: 7/8
```

A pattern of so many `:bars` keeps its steps, and refuses a meter they don't make whole bars of,
as `:meter` does.

## The song page

`gs` opens the **song page**: the arrangement, a list of **entries**, each playing a pattern
some number of times running. This is the Mountain King example's, at 90 columns:

```
 7 entries · 38 bars · 1:06
  cave      cave     march              hall              frenzy   frenzy             en

            entries         troll bassoopizz  drone bass  kick  snare hat   open  clap
  0:00    1  ┌ cave          ····  ▄▄▄▂  ────  ▂···  ····  ▂···  ····  ····  ····  ····
  0:02    2  │               ····  ▄▂▄▂  ────  ····  ····  ▂···  ····  ····  ····  ····
  0:03    3  │               ····  ▄▄▄▄  ────  ····  ····  ▂···  ····  ····  ····  ····
  0:05    4  └               ····  ▄▄▂·  ────  ····  ····  ▂···  ····  ····  ····  ····
  0:07    5  ┌ cave          ····  ▄▄▄▂  ▄▄▄▂  ▂···  ····  ▂···  ····  ····  ····  ····
  …
  0:14    9  ┌ march ×2      ▄▄▄▂  ····  ▄▄▄▂  ····  ▄▄▄▄  ▂▂▂▂  ·▂·▂  ▂▂▂▂  ····  ····
  0:16   10  │               ▄▂▄▂  ····  ▄▂▄▂  ····  ▄▄▄▄  ▂▂▂▂  ·▂·▂  ▂▂▂▂  ····  ····
  0:17   11  │               ▄▄▄▄  ····  ▄▄▄▄  ····  ▄▄▄▄  ▂▂▂▂  ·▂·▂  ▂▂▂▂  ····  ····
  0:19   12  │               ▄▄▂·  ····  ▄▄▂·  ····  ▄▄▄▄  ▂▂▂▂  ·▂·▂  ▂▂▂▂  ···▂  ····
  0:21   13  ├ 2/2           ▄▄▄▂  ····  ▄▄▄▂  ····  ▄▄▄▄  ▂▂▂▂  ·▂·▂  ▂▂▂▂  ····  ····
  …
  0:42   25  ┌ frenzy        █▆▆▆  █▆▆▆  █▆▆▆  ····  ▄▄▄▄  ────  ────  ────  ────  ────

 patterns  cave 4 bars   march 4 bars   hall 4 bars   frenzy 4 bars   end 2 bars
```

From the top:

- **The summary**: how many entries, bars and minutes the song has. While it plays, how far in
  it is (`0:12 / 1:06`); while it loops, which entries (`looping entry 2`).
- **The strip**: the whole song in one line, each entry as wide as it plays for, in its
  pattern's colour and named where there's room. While the song plays, the playhead moves along
  it.
- **The bars**, one to a row, time going down. On the left, each bar's time from the start and
  its number (the cursor's in the accent colour, and `▶` by the one playing). Then the entries,
  each a bracket down its bars in its pattern's colour, named on its first (`march ×2` when it
  repeats), with `├` and `2/2` where a repeat starts, and `╶` for an entry one bar long.
- **A column for each track**, showing how busy it is in each beat of the bar, or each half beat
  where there's room: a block as tall as the share of steps with a note (from `▁` to `█`), `·`
  where there are none, and `─` where the track is muted, by the entry or by the pattern. A
  track shorter than its pattern is drawn going round, as it plays. Columns scroll sideways to
  keep the cursor's in view.
- **The patterns** there are, each with its length; the one being edited is underlined.

The times are worked out at the song's `:bpm`; they don't follow the conductor's tempo changes.
Each bar has its own row however long it is, so a pattern in a meter of its own has bars of its
own length.

In the Mountain King, the first `cave` entry mutes the pizz (`────`), so it comes in only the
second time; the first `frenzy` mutes the drum kit.

### Keys

On the song page the cursor is on a bar, and on the entries column or a track's column.

| Key       | Action                     | What it does                                            |
|-----------|----------------------------|---------------------------------------------------------|
| `j` `k`   | `down` `up`                | the next / previous bar                                 |
| `w` `b`   | `next_note` `prev_note`    | the next entry, the start of this / the previous one    |
| `gg` `ge` | `first_step` `last_step`   | the first / last bar                                    |
| `h` `l`   | `left` `right`             | across the columns: the entries, then each track        |
| `^a` `^x` | `note_up` `note_down`      | the entry plays the next / previous pattern             |
| `+` `-`   | `longer` `shorter`         | the entry repeats once more / less (1 to 64)            |
| `o` `O`   | `open_below` `open_above`  | add an entry after / before                             |
| `d`       | `delete`                   | remove the entries selected, copying them               |
| `A-d`     | `clear`                    | remove them without copying                             |
| `y`       | `yank`                     | copy the entries selected                               |
| `p` `P`   | `paste` `paste_insert`     | paste copied entries after / before                     |
| `ret`     | `open`                     | edit the entry's pattern, at the cursor's bar and track |
| `c`       | `change`                   | on the entries column, rename the entry's pattern       |
| `m`       |                            | on a track's column, mute it in the entries selected    |
| `s`       |                            | on a track's column, solo it in the entries selected    |
| `v`       | `select_mode`              | select bars and columns from the cursor                 |
| `x`       | `select_rows`              | select the cursor's entry; again, the next too          |
| `;`       | `collapse`                 | collapse the selection onto the cursor                  |
| `L`       | `lock_setting`             | loop the song round the entries selected                |
| `space`   | `toggle_play`              | play the song from the cursor's entry, or stop          |
| `u` `U`   | `undo` `redo`              | undo, redo                                              |
| `gs` `gp` | `goto_song` `goto_pattern` | the song page; back to the pattern page                 |

The track menu (`t`) isn't used here: `m` and `s` are the song page's own keys, and `t` says so.

### Entries

An entry plays one pattern a number of times running, from 1 to 64 (`+` and `-`; `=` too, as
it's `+` without shift). `^a` and `^x` change which pattern it plays, going round all of them. A
song has up to 256 entries.

`o` adds an entry after the cursor's, playing the same pattern once, and `O` one before; in an
empty song, which says `nothing to play yet: o adds an entry`, it adds the pattern being edited.
`d` removes the entries selected, copying them, and `p` puts copied entries back after the
cursor's (`P` before), so `d` then `p` moves an entry and `y` then `p` repeats a section. Copied
entries hold on to their patterns and muted tracks themselves, not their places, so they paste
the same after tracks or patterns are added, removed, moved or brought back with `u`; an entry
whose pattern has gone is left out. The cursor stays on its entry as entries around it change
length. Removing the entry that's playing lets its pattern play to its end, then goes on with
the entry after it, from that entry's first repeat.

`ret` goes to the cursor's entry's pattern, with the grid's cursor on the bar of it the song
page's cursor was on, and on its track if it was on a track's column. `c` on the entries column
opens the command line at `:pattern-name `, to rename the entry's pattern; `:bars` and
`:pattern-name` typed on the song page also change the cursor's pattern rather than the one
being edited.

### Mutes in an entry

A track can sit out one entry and play in the rest, without a pattern of its own. On a track's
column, `m` mutes the track in the cursor's entry: its bars there show `─`, and while the entry
plays, the track's name shows `─` on the pattern, mixer and instrument pages. `s` solos it there
instead, muting every other track in the entry. Either again undoes it: `m` unmutes if the
tracks are all muted in the entries selected, and `s` lets every track play again if they're
already soloed. The status line says what happened: `pizz muted in this entry`,
`3 tracks play alone in 2 entries`.

This is how one pattern played twice can be orchestrated differently each time. The Scarlatti
example plays each half of the sonata twice, as Scarlatti's repeats ask, with different tracks
muted each time: the harpsichord alone the first time, the lead doubling it the second.

An entry's mutes are apart from the mixer's (`tm`, for the whole song) and the pattern's (`tM`,
for every entry of it). The engine takes them in at the entry's first step, even when the
pattern doesn't change, so a mute starts exactly on the entry.

### Selecting

`v` starts a selection at the cursor; moving extends it over bars and columns, as on the grid.
`x` selects the cursor's entry, all its bars, and pressing it again takes in the next entry. `;`
collapses it. Then `m` and `s` act on every track and entry selected, and `d`, `y`, `p`, `L`,
`^a`, `^x`, `+` and `-` on every entry it reaches. An edit ends select mode, as in Helix.

### Looping a section

To work on one part of the song, loop it. `L` loops the song round the cursor's entry. On
another entry, `L` stretches the loop to take it in; on an entry inside the loop, it stops
looping. With several entries selected, it loops round them. A bar down the left marks the loop,
`↻` at its first bar and `┃` down the rest, and the summary says `looping entries 2 to 3`.

```
  0:04    5↻ ┌ A1            ▄▄▄█  ────  ▄▄▄█  ────  ▄▄▄▄  ▄▄▄▄
  0:05    6┃ │               ▄▄▄█  ────  ▄▄▄█  ────  ▄▄▄▄  ▄▄▄▄
```

The song plays from the cursor as ever, and at the end of the loop's last entry goes back to its
first. The loop isn't saved with the song. Undo takes it with the entries, and adding or
removing entries keeps it round the same ones.

`L` is `lock_setting`: on the mixer it locks settings (see [Gestures and
locks](05-gestures.md)), and here, having no settings to lock, it loops.

## Playing the song

| Key       | Action           | What it does                                                     |
|-----------|------------------|------------------------------------------------------------------|
| `space`   | `toggle_play`    | play the song from the cursor's entry (song page) or the pattern |
| `^space`  | `play_song`      | play the song from its start, or stop                            |
| `A-space` | `play_from_here` | play the song from the entry playing this pattern, or stop       |

`A-space` starts from the song page's cursor's entry if it plays the pattern you're on, or else
the first entry that does, or the top. `space` on the pattern page plays the pattern being
edited, looping, outside the song.

Playing the song, Lattice goes to each entry's pattern in turn: the header shows where it is
(`pat cave · song 1/7`), and the pattern page follows along, as does the song page, where the
playhead moves along the strip and down the bars. `:follow` turns following off, to stay where
you are while the song plays. After the last entry the song stops, unless it's looping.

Passes run on through an entry's repeats, and through entries of the same pattern back to back,
as the pattern only loops. So a rule with `pass 1` on a pattern played in two entries, once and
then twice, applies on the second time through, in the second entry. In the Mountain King, the
march's second time is an octave up by a rule, `tr+12/2+1`.

Picking a pattern by hand (`]p`, `:pattern`), or a conductor's `P3`, leaves the song: the
pattern takes over at the next bar and loops, and the header no longer says `song`.

`:export` renders the song as it plays it, entry by entry, with its mutes and conductors (see
[Files and settings](10-files-and-config.md)).

## The conductor

Some things belong to the whole song rather than a track: its tempo, stopping, going to another
pattern, bringing a part in or out. They're gestures, things that happen once on a step, in the
**conductor**: a lane of each pattern drawn left of the first track, beside the step numbers,
with two fields on each step and no notes. It's named for an orchestra's conductor, who sets the
tempo, slows the players into a cadence, and brings sections in and cuts them off.

```
               ╭─ RIGHT HAND ──────────────────
    CONDUCTOR  KRELL                     LEAD
               112                       112
     C                     vel g    G
 80  T120  ··   G-4        88   ··   ··   G-4
 81   ··   ··   D-4        75   ··   ··   D-4
 …
 96   ··   ··   G-4    8   88   ··   ··   G-4    8
 97   ··   ··   │               ··   ··   │
 …
103  ~86   ··   │               ··   ··   │
104   ··   ··   ···             ··   ··   ···
 …
110  X     ··   ···             ··   ··   ···
111   ··   ··   ···             ··   ··   ···
```

That's the end of the Scarlatti example's last pattern, `fine`: `T120` sets 120 BPM on step 80,
the tempo slides (`~86`) down to 86 BPM over the next 23 steps as the last chord comes in and
rings, and `X` stops the song on step 110, once it has rung.

The conductor is shown when it has gestures, or when the cursor is in it. From the first track's
notes (`gh`), `h` goes into its fields; `fC` goes straight there from any track, and `l` comes
back out.

### Conductor gestures

| Gesture | Range        | What it does                                                  |
|---------|--------------|---------------------------------------------------------------|
| `T140`  | 20–400 BPM   | the tempo from this step, until the next                      |
| `~140`  | 20–400 BPM   | slides from the last tempo to this one, arriving on this step |
| `T--`   |              | back to the song's own tempo, `:bpm`                          |
| `X`     |              | stops the song at the end of this step                        |
| `P3`    | pattern 1–64 | cues pattern 3 for the next bar, as `]p` does                 |
| `M2`    | track 1–16   | mutes track 2 from this step                                  |
| `M-2`   | track 1–16   | unmutes track 2                                               |
| `S2`    | track 1–16   | solos track 2                                                 |
| `S-2`   | track 1–16   | unsolos track 2                                               |

Patterns and tracks are numbered from 1, as the song page and the grid show them.

**Tempo.** `T140` sets the tempo from its step, and it holds until the next tempo, as a lock
does, outlasting the pattern: a tempo set in one pattern carries on into the next until
something changes it. While the song plays, the header shows the tempo the conductor has set.
`T--` goes back to the song's own `:bpm`, and so does stopping. A `:bpm` typed while the
conductor's tempo holds waits for `T--`.

**Slides.** `~140` changes the tempo gradually, starting on the step after the last tempo before
it (or the conductor's first step, if there's none), each step taking its share of the way, so
it arrives at 140 on its own step. That's a **ritardando** when it slows, the gradual slowing
into a cadence that marks the end of a phrase or a piece, and an **accelerando** when it speeds
up.

**Stop.** `X` stops the song where the next step would start: the end of the piece, after the
last chord has rung, rather than wherever the pattern happens to end.

**Cue.** `P3` cues pattern 3 for the next bar, as `]p` or `:pattern` would. Like them, it leaves
the arrangement, so the cued pattern loops from then on. It's for a pattern that should go
somewhere other than the song's next entry: a loop that breaks out into another section on its
own. Stopped, it does nothing. A cue follows its pattern: add a pattern before pattern 3 and the
cue becomes `P4`; remove pattern 3 and the cue goes too. `M` and `S` follow their tracks the same
way as tracks are added, removed and moved.

**Mute and solo.** `M2` mutes track 2 and `M-2` unmutes it; `S2` and `S-2` solo and unsolo it.
They set rather than toggle, so a step says what's heard whichever way the track was. They last
until the song stops, laid over the song's own mix, which is left as it was: the header and the
mixer show them while they hold. Muting or soloing the track yourself (`tm`, `ts`, a pad) takes
over from the conductor's.

### Typing and editing

In insert mode, type a gesture in either case (`t140` is `T140`). It goes in once it can't go on
(`T140`, `X`, `T--`, `M-16`), or with enter or `esc` (`P3`, which might have been `P31`; `M2`,
which might have been `M12`). A letter that can't follow puts in what was there before it.

`^a` and `^x` nudge a tempo by 1 BPM, and `>` and `<` by 10. `&` fills tempos evenly between a
selection's ends, both of which need a tempo. `d`, `y` and `p` work as in any field.

### Its own length

The conductor has its own length, as long as the pattern until you change it: with the cursor in
it, `:len N` sets it (1 to 256 steps; the status line says `the conductor's 32 steps long`). It
reads its step from how far into the pattern the song is, counting on through the pattern's
loops and repeats, round its own length. So a tempo change can come round every bar, or once in
eight, or a conductor three times the pattern's length can shape three times through it. A new
pattern starts it from its first step again.

The Mountain King's `frenzy` is four bars, 64 steps, played three times running; its conductor
is 192 steps long, one for all three, starting at `T144` and sliding to 152, 160 and 176 BPM,
arriving at the end of each time round. Its `cave` starts at `T112` and slides to 126 by the end
of its second time, over 128 steps.

### Examples

A slowing into each cadence, as the Scarlatti does at the end of each half:

```
 104  T120        the tempo, 120, from the start of bar 14
 …
 119  ~101        sliding down to 101 BPM by the last step
```

The next pattern then sets `T120` on its first step, a tempo again.

A tempo that picks up through a build, then snaps back:

```
  00  T124
  63  ~132        speeding up over four bars
  64  T--         back to the song's :bpm
```

A part brought in for the last bar of every four, in a conductor 64 steps long:

```
  48  M-5         track 5 in
  63  M5          and out again at the end of the last step
```

A song that ends itself: an `X` on the last step of the last pattern, so playing it through
stops cleanly where the music does.

### Rendering

`:export` plays each pattern's conductor as the song does, tempos, stops, mutes and solos, all
but its `P` cues, as the export already follows the arrangement; it says when it leaves them
out. `:export-sel` and `:export-loop` render a selection without the conductor.
