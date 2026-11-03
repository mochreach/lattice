# Rules

A track's notes are what you write. Its **rules** are what it does with them each time round:
turn the pattern a step later, a step further every pass, play it backwards, or move it up a
fourth on every fourth pass. The written notes never change. Before each pass through the track,
Lattice works out what that pass plays from the notes and the rules, and plays that.

This chapter covers what a pass is, the four transforms and the three conditions, how rules
stack, the commands, how rules look on screen, and how tracks of different lengths drift against
each other.

## Why rules

A loop that plays the same four bars forever gets dull, and the usual cure, copying the pattern
and changing the copy, leaves you with eight versions of the same idea to keep in step. Rules
keep one written pattern and describe how it changes: "every fourth time, down a third" is one
line above the track, and if you change a note, every variation changes with it.

The design notes call this the **playfield** idea. The first, abandoned version of Lattice had a
single global "repeat grid" flag; rules replaced it with something per track, per pass and
stackable. Underneath, it's one function: given a track and a pass number, `realise` returns the
steps that pass plays. It's pure (the same track and pass always give the same steps), so the
song sounds the same every time it plays, and the engine can work a pass out without allocating
memory on the audio thread.

Rules belong to a track in a pattern, like its notes: each pattern has its own rules for every
track (see [Patterns, the song and the conductor](06-song.md)), and `:pattern-dup` copies them
with the notes.

## Passes

A **pass** is one time through a track, from its first step to its last. Passes count from 0,
and each track counts its own:

- A track starts at pass 0 when the song starts playing and whenever a new pattern takes over.
- Each time its playhead comes round to its first step it starts the next pass. That's at the
  end of its last step usually, but also on a jump to step 0 (`J0`), or going backwards past
  step 0 (see [Gestures and locks](05-gestures.md)).
- When a pattern loops, its tracks don't restart; they carry on round, still counting. So do
  they when the song plays the same pattern again in the next entry, or an entry repeats.
- Stopping resets everything.

So a 16-step track in a 16-step pattern counts the times round the pattern, but a 5-step tom in
the same pattern counts much faster: by the end of the pattern's first bar it's already on pass
3. A rule's condition is always about the track's own passes.

While the song plays, the second line of each track's header shows its length and the pass it's
on:

```
    BASS        ACID
    8 · pass 2  16 · pass 1
```

## The transforms

Every rule pairs a **transform**, what's done to the steps, with a **condition**, which passes
it's done on. There are four transforms. A step moves as a whole: its note, velocity, gestures
and locks go with it (a transpose changes only the note).

The examples below use one 8-step track:

```
 step   0    1    2    3    4    5    6    7
        C-4  D-4  E-4  F-4  G-4  A-4  B-4  C-5
```

### Rotate (`:rot N`)

`:rot N` turns the track N steps later, wrapping round: what was on the last step comes round to
the first. A negative N turns it earlier. N goes from -256 to 256 (a track has at most 256
steps); a rotation by the track's length, or a multiple of it, changes nothing.

`:rot 2` on the example:

```
 step   0    1    2    3    4    5    6    7
        B-4  C-5  C-4  D-4  E-4  F-4  G-4  A-4
```

Rotation is how a part answers another a step behind, or how a drum pattern moves its accent. In
the Mountain King example, the `pizz` strings play the bassoon's notes with `rot+1`, an echo a
step late. On a `chop` track, where each step plays a piece of a loop, rotating moves the loop's
pieces round, as cutting tape and splicing it back on.

### Drift (`:drift N`)

`:drift N` is a rotation that builds up: N steps on the first pass the rule applies, 2×N on the
second, 3×N on the third. Every pass sounds different until it has turned all the way round. The
first pass is already turned, as the rule has applied once by then.

`:drift 1` on the example, pass by pass (the app's own live view showed these):

```
 pass   step 0    1    2    3    4    5    6    7
  0          C-5  C-4  D-4  E-4  F-4  G-4  A-4  B-4     turned 1
  1          B-4  C-5  C-4  D-4  E-4  F-4  G-4  A-4     turned 2
  2          A-4  B-4  C-5  C-4  D-4  E-4  F-4  G-4     turned 3
  …
  7          C-4  D-4  E-4  F-4  G-4  A-4  B-4  C-5     turned 8: as written
```

On an 8-step track it comes back to the written pattern every 8 passes. Steve Reich's Clapping
Music (1972) works this way: two players clap the same 12-beat pattern, and one moves it a beat
on every so many bars until the two are back in unison. `:drift 1` on a 12-step copy of the
other track moves it on every pass.

### Reverse (`:rev`)

`:rev` plays the track backwards: the last step first. It takes no amount.

```
 step   0    1    2    3    4    5    6    7
        C-5  B-4  A-4  G-4  F-4  E-4  D-4  C-4
```

Reversing moves whole steps, so a note keeps its length and still starts on its step: a long
note at the end of the bar becomes a long note at the start. It doesn't play the sound
backwards; for that, see the `<` gesture in [Gestures and locks](05-gestures.md), or `~` on a
chop track.

### Transpose (`:tr N`)

`:tr N` moves every note N semitones up, or down if negative, from -128 to 127. Notes are kept
within MIDI's range, 0 to 127 (G-9 at the top): a note that would go past either end stops
there. Velocities, gestures and locks are left alone.

Twelve semitones is an octave, seven a fifth, five a fourth, four a major third and three a
minor third; the command line suggests these as you type. On a drum track, a transpose tunes the
drum. On a `chop` track, where a note's pitch picks which piece of the loop plays, it moves each
step on to a later piece.

## Conditions

Without a condition, a rule applies on every pass. Add one after the amount to choose which
passes:

| Condition        | Written     | Applies on                                     |
|------------------|-------------|------------------------------------------------|
| (none)           |             | every pass                                     |
| `every N`        | `/N`        | passes 0, N, 2N…: the first of every N         |
| `every N OFFSET` | `/N+OFFSET` | the passes that leave OFFSET when divided by N |
| `pass K`         | `@K`        | pass K only, counting from 0                   |

So, over the first nine passes:

```
 pass        0  1  2  3  4  5  6  7  8
 always      ●  ●  ●  ●  ●  ●  ●  ●  ●
 every 4     ●           ●           ●
 every 4 2         ●           ●
 every 2 1      ●     ●     ●     ●
 pass 3               ●
```

N is 1 or more (`every 1` is the same as no condition). OFFSET counts from 0, so `every 4 0` is
the first of every four and `every 4 3` the last; an offset of N or more wraps round, so
`every 4 6` is `every 4 2`. `pass K` applies once, on that pass, and never again until the
passes start over (a new pattern, or playing again).

You can type a condition either way: `:tr 5 every 4 2` and `:tr 5 /4+2` make the same rule. The
short forms are how the rule is shown above its track.

### A condition and drift

A drift counts only the passes its condition held on. `:drift 1 every 2` turns the track one
step on pass 0, plays it as written on pass 1 (the rule doesn't apply), turns it two steps on
pass 2, plays it as written on pass 3, and so on: the turn builds up on alternate passes, and
the passes between are the written pattern, not the last turn held over.

```
 pass   step 0    1    2    3    4    5    6    7
  0          C-5  C-4  D-4  E-4  F-4  G-4  A-4  B-4     turned 1
  1          C-4  D-4  E-4  F-4  G-4  A-4  B-4  C-5     as written
  2          B-4  C-5  C-4  D-4  E-4  F-4  G-4  A-4     turned 2
  3          C-4  D-4  E-4  F-4  G-4  A-4  B-4  C-5     as written
```

## Stacking rules

A track can have any number of rules. On each pass they're applied **in the order they were
added**, each to what the ones before it made, skipping those whose condition doesn't hold.
Order matters wherever a rule moves steps. Take four steps, C-4, nothing, E-4, G-4:

```
 written            C-4  ···  E-4  G-4
 :rev, then :rot 1  C-4  G-4  E-4  ···      reversed to G E · C, then turned a step later
 :rot 1, then :rev  E-4  ···  C-4  G-4      turned to G C · E, then reversed
```

Transposes add up, whatever the order, as long as no note hits the top or bottom of the range on
the way.

Stacked transposes with different conditions make a chord progression from one bar. The demo
song's bass, stabs and pad each have two:

```
    BASS        STAB        PAD
    tr-4/4+2    tr-4/4+2    tr-4/4+2
    tr-2/4+3    tr-2/4+3    tr-2/4+3
```

Each part plays its bar in F minor twice, then down four semitones on the third pass of every
four (to D♭) and down two on the fourth (to E♭): F minor, F minor, D♭, E♭, round and round, with
nothing written but the one bar.

## The commands

Rules go on the track under the cursor (the primary selection's), one at a time.

| Command           | What it does                                        |
|-------------------|-----------------------------------------------------|
| `:rot N [WHEN]`   | add a rotation, N steps later (earlier if negative) |
| `:drift N [WHEN]` | add a drift, N steps more each time it applies      |
| `:rev [WHEN]`     | add a reverse                                       |
| `:tr N [WHEN]`    | add a transpose, N semitones                        |
| `:pop`            | remove the track's last rule                        |
| `:clear`          | remove all the track's rules                        |
| `:len N`          | make the track N steps long (1 to 256)              |

WHEN is `every N [OFFSET]`, `pass K`, `/N`, `/N+OFFSET` or `@K`, or left out for every pass.

A few things to know:

- New rules go at the end, so they apply after the others. There's no command to reorder or
  remove one from the middle: `:pop` the later ones and add them again.
- `:pop` with no rules says `no rules to remove`. A value out of range is refused as you type,
  with what was expected on the right (`steps is -256 to 256`, `every is 1 or more`).
- Each command is one undo, so `u` takes a rule back off and `U` puts it back.
- While the command line is open, it suggests amounts in words (`12` an octave up, `4` a beat
  later), then `every` or `pass`, then which pass of how many (`the 3rd pass of every 4`). `tab`
  fills a suggestion in.
- The engine picks up a rule while the song plays; it applies from the track's next pass.

## Rules on screen

Under each track's level and pan, its header lists its rules, one to a line, in the order they
apply, written short: `rot+2`, `drift-1/2+1`, `rev@1`, `tr+12/4+3`. A track with none says
`no rules`, faintly. A folded group's narrow columns show an `r` for each rule instead.

```
    BASS
    8
       -3dB       ▋
         C
    rot+2
    tr+12/4+3
    rev@1
    drift-1/2+1
```

The grid shows the notes as written. `tab` switches it to the notes as they're playing this
pass, after the rules, and back; the header says `pattern as written` or `pattern as playing`.
In the playing view, steps the rules have changed are drawn bold and italic, so you can watch a
drift walk round. Stopped, it shows pass 0. You edit the written notes either way.

| Key   | Action        | What it does                                              |
|-------|---------------|-----------------------------------------------------------|
| `tab` | `toggle_view` | show the pattern as written, or as it's playing this pass |

## Polymeter: tracks of different lengths

Each track has its own length (`:len`, 1 to 256 steps), and a track shorter than the pattern
goes round again. A pattern is as long as its longest track, rounded up to a whole bar, unless
`:bars` says otherwise (see [Patterns, the song and the conductor](06-song.md)).

Tracks of different lengths drift against each other. Put five steps on a tom against sixteen of
hats, and the tom's first step lands somewhere new in each bar:

```
 bar 1   hat  x . x . x . x . x . x . x . x .
         tom  1 . . . . 1 . . . . 1 . . . . 1
 bar 2   tom  . . . . 1 . . . . 1 . . . . 1 .
 bar 3   tom  . . . 1 . . . . 1 . . . . 1 . .
```

They line up again after the lowest common multiple of the two lengths: 16 and 5 make 80 steps,
five bars. A 7 against 16 takes seven bars, and adding a 5 makes it 560 steps, 35 bars, more
than a minute at most tempos. Musicians call this **polymeter**: parts in different-length
cycles over the same pulse (where polyrhythm, strictly, divides the same span into different
numbers of beats, three against two). It's common in West African drumming, and a step
sequencer's favourite trick, as each part of an Elektron or a modular sequencer can have its own
length.

Because a pattern's tracks keep running when it loops, a polymetric pattern goes on drifting for
as long as it plays, and only a change of pattern brings them all back to their first steps
together. (Lattice first made a pattern last until every track and rule lined up again; a
polymetric pattern then ran for a minute or more before the song could move on, so now its
length is its longest track.)

Rules and lengths combine: a 7-step track's passes come every 7 steps, so `:drift 1` on it turns
it once every 7 steps, and `every 4` means every 28 steps. In the demo song, the five-step tom
and the seven-step sitar (`drift-1/2` on the pluck) drift against the bar, and the bar against
them.
