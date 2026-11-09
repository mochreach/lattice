# Gestures and locks

A step can do more than start a note. Lattice has two ways to say what else happens there, and
one rule for which is which: **if it happens once on a step, it's a gesture; if it's a value
that holds, it's a lock.**

- A **gesture** is written in one of a step's two gesture fields: `R3` plays the note three
  times within the step, `D8` starts it late, `?50` plays the step on half the passes, `J0`
  sends the track back to its first step. It's done, and it's over.
- A **lock** sets one of the track's instrument or effect settings from its step on, until the
  setting's next lock: the filter shut on step 0, opening on step 8, sliding shut again by step
  15. Each setting a track locks gets a **lock column** of its own beside its notes.

The song-wide equivalent of both, for tempo, stopping, cueing patterns and muting tracks, is the
**conductor**, covered in [Patterns, the song and the conductor](06-song.md).

## Where they come from

### Trackers' effect columns

Trackers began on the Amiga, with Karsten Obarski's Ultimate Soundtracker (1987) and the
trackers after it, ProTracker the best known. Each row of a pattern held a note, an instrument
number and an **effect**: a hex digit naming the effect and two more for its amount. `037`
played a chord by cycling the note with the notes 3 and 7 semitones up, fast enough to blur into
one sound, the trick behind the sparkle of so much Commodore 64 and Amiga music. `3xx` slid into
the note, `9xx` started the sample part way in, `E9x` retriggered it, `ECx` cut it short, `EDx`
delayed it, `Bxx` jumped to another position and `Fxx` set the speed.

Lattice's gestures are those ideas, written so you can read them. Several map straight across:

| Tracker effect   | Lattice gesture | What it does                                            |
|------------------|-----------------|---------------------------------------------------------|
| `0xy` arpeggio   | `Axy`           | cycle the note, +x and +y semitones                     |
| `3xx` portamento | `Gn`            | slide into the note from the last pitch                 |
| `9xx` offset     | `On`            | start the sample part way in                            |
| `E9x` retrigger  | `Rn`, `rn`      | play the note again within the step                     |
| `ECx` note cut   | `Xn`            | end the note part way through the step                  |
| `EDx` note delay | `Dn`            | start the note part way through the step                |
| `Bxx` jump       | `Jn`            | carry on from another step                              |
| `Fxx` speed      | `*2` `/2`       | one track's speed (the tempo is the conductor's `T140`) |

They aren't called effects because in Lattice the effects are the delays, reverbs and filters on
the mixer. A gesture is something a player does once, with a hand or a bow: a flam, a trill, a
slide. And there's no hex: amounts are decimal, and times are in 24ths of a step.

### Elektron's parameter locks

Elektron's Machinedrum (2001) gave every step of its sequencer the power to change any of the
sound's settings for that step alone: turn a knob while holding a step's key, and that step
plays with the knob there. Elektron called these **parameter locks**, "p-locks", and every
Elektron machine since has had them, along with **trig conditions** that decide whether a step
plays: on a percentage of passes, on the first of every four, or only during a fill.

Lattice takes both. Its locks are Elektron's, laid out as a tracker's columns so you can see and
edit them, and its conditions (`?50`, `1:4`, `F`, `F-`) are Elektron's trig conditions written
as gestures. Unlike an Elektron p-lock, which lasts for its step only, a Lattice lock holds
until the setting's next lock, as a tracker's column does; `--` ends one.

## The open track

The track under the cursor **opens out** to show each step's fields: its note, its velocity
(`vel`), its two gesture fields (`g` and `G`), and one field for each lock column. Every other
track shows only its notes, marking a step that has gestures or locks with `◆`. `te` keeps the
tracks the selections reach open too, and again closes them.

```
                vel g    G    cut res wav
 00  C-4        78  R3    ··   34  ··   ╵
 01  D-4        78  D8   F-    49  ··   ╵
 02  E-4        78  A37   ··   64  ··   ╵
 03  F-4        78  ?50   ··   78  ··   ╵
 04  G-4        78  X6    ··    ╵  ··   ╵
 05  A-4        78  r4    ··    ╵  52   ╵
 06  B-4        78  !     ··    ╵  --  99
 07  C-5        78  J0    ··  ~89  ··   ╵
```

The header row names each field and underlines the letter that goes to it after `f`: `fn` the
note, `fv` the velocity, `fg` and `fG` the gesture fields, and each lock column the first letter
of its name that no other field has (`c` for cutoff, `r` for resonance, `a` for gain, as `g` is
taken), or a number if none is free. `fC` goes to the conductor's fields, from any track. When
`f` is pressed, a box in the corner lists the fields with their full names.

`h` and `l` move from field to field, then on to the next track; any other move to another
track, `th` and `tl` among them, lands on its notes. A track folded into its group (`tz`) shows
only its hits and has no fields; `fg` on it says to unfold it.

| Key         | Action                  | What it does                                             |
|-------------|-------------------------|----------------------------------------------------------|
| `f`         | `goto_field`            | then a key: go to that field of the open track           |
| `h` `l`     | `left` `right`          | the previous / next field, then on to the next track     |
| `te`        | `toggle_open`           | keep the tracks the selections reach open, or close them |
| `^a` `^x`   | `note_up` `note_down`   | in a field, its values up / down one                     |
| `&`         | `fill`                  | in a field, fill each selection evenly between its ends  |
| `d` `y` `p` | `delete` `yank` `paste` | on the field alone, as on notes                          |

The column the cursor's in decides what an edit acts on, across as many tracks as the selections
reach: a gesture column's edits go to the gestures on every track, and a lock column's to every
track locking the same setting ([chapter 2](02-patterns.md) has the whole list).

## Gestures

### Typing a gesture

Go to a gesture field (`fg` or `fG`), and in insert mode (`i`) type the gesture. It goes in as
soon as it's whole, and the cursor moves on to the next step, ready for the next:

```
iR3            R3 goes in on this step; on to the next
D8<ret>        D8 might have been D8x, so enter puts it in
A37            whole at three characters
<esc>          leaves insert mode, putting in what's typed if it's a gesture
```

In insert mode on a gesture field, a window lists the gestures you can type there, each with an
example and what it does, so you needn't remember them: those the track's instrument plays (`<`
and `O8` only on a sampler or chop, `G6` on a synth, Switched or Monochord, `P12` on a MIDI
track). Type a key and it narrows to the gestures that begin with it: after `J`, `J12` and `J?`.
It goes when you leave insert mode.

```
 ┌ gestures ────────────────────────────────┐
 │ R3   retrigger: 2 to 9 hits in the step  │
 │ r3   retrigger, each hit quieter         │
 │ D8   delay: 1 to 23 24ths late           │
 │ X6   cut: 1 to 23 24ths in               │
 │ A37  arpeggio: the note, +3, +7          │
 │ …                                        │
 └──────────────────────────────────────────┘
```

While you type, the status line shows what's typed so far, `D8_  (enter if that's all)`, and
once it's in, what it does: `R3: retrigger, 3 hits`. `⌫` takes back a character. A letter that
can't follow what's typed puts in what was there before it, if that's a gesture, and starts
anew, so `P3` followed by `X` is two gestures. Something out of range is refused with what's
allowed: `retriggers go from 2 to 9`, `24ths of a step go from 1 to 23`.

Letters may be typed in either case, but for `r`, which fades where `R` doesn't. Each gesture is
written one way only: `J0`, not `J00`.

In normal mode, the status line describes the gestures and locks under the cursor, and `^a` and
`^x` change a gesture's amount within its range (`R3` to `R4`, `J12` to `J13`). `&` doesn't fill
gestures: `y` and `p` copy them. `d` clears them.

### Time in 24ths of a step

Gestures that happen part way through a step count time in 24ths of it. At 120 BPM with four
steps a beat, a step is 125 ms and a 24th about 5 ms, close to the smallest shift a listener
feels. 24 divides into halves (12), thirds (8), quarters (6), sixths (4) and eighths (3), so the
common subdivisions are all whole numbers. Times are worked out from the step's length when it
plays, so they follow the tempo, the conductor's changes and a track's speed.

### Note gestures

Note gestures act on the step's note. On a step with no note, they act on the latest note still
sounding from an earlier step, so a long note can be retriggered or cut later on.

In short:

| Gesture | Range      | What it does                                                     |
|---------|------------|------------------------------------------------------------------|
| `R3`    | 2–9 hits   | plays the note 3 times, evenly, within the step                  |
| `r3`    | 2–9 hits   | as `R3`, each hit quieter than the last                          |
| `D8`    | 1–23 24ths | starts the note 8/24 of a step late                              |
| `X6`    | 1–23 24ths | cuts the note 6/24 of a step in                                  |
| `A37`   | 0–9, 0–9   | plays the note, then 3 and 7 semitones up, round while it's held |
| `!`     |            | plays the note at full velocity                                  |

In detail:

**Retrigger, `R2` to `R9`.** The note plays that many times, the hits spaced evenly through the
step: `R2` on the step's start and half way, `R3` at each third, and `R4` at each quarter, which
at four steps a beat are sixty-fourth notes. The step's own note is the first hit. On a step
without a note, the held note gets all the hits, the first on the step's start. A retrigger is a
drum machine's **ratchet**, a snare roll, a hat's stutter, a flam (`R2` on a kick). In the
Scarlatti example, the snare rolls faster and faster into the first note, `R2` to `R4`. Hits
stop if the note is cut or ends first.

**Fading retrigger, `r2` to `r9`.** As `R`, but each hit is quieter, falling evenly: `r4` plays
at full, three quarters, half and a quarter of the note's velocity, like a bounce or an echo.

**Delay, `D1` to `D23`.** The note starts so many 24ths into the step, late. `D12` is half a
step late; small values (`D2`, `D3`) push a note behind the beat for a lazy feel, and a delayed
snare after an undelayed one makes a flam. A retrigger on the same step spreads its hits from
the delayed start to the step's end.

**Cut, `X1` to `X23`.** Every note the track is holding ends so many 24ths into the step,
counted from the step's start. It's how to play staccato on an instrument whose notes ring for
their whole length, or a gated stab: the Scarlatti's repeated notes are cut a little over half
way (`X14`). A cut also stops a retrigger's or an arpeggio's later hits.

**Arpeggio, `A` and two digits.** The note plays, then the note so many semitones up, then the
other, in turn, a sixth of a step each, round and round for as long as the note is held, across
the steps after too: `A37` goes root, minor third, fifth (a minor chord), `A47` a major chord,
`A57` a suspended one, and `A05` or `A07` swing between the note and a fourth or a fifth. Each
digit goes from 0 to 9, so an octave is out of reach. `A10` goes note, a semitone up, note: a
trill, as the Scarlatti plays where Scarlatti wrote one. This is the tracker's chord trick,
described above; on a monophonic part it gives the impression of a chord.

As on a tracker, it's one note changing pitch, not a run of new ones. On the synths (the synth,
Switched and Bessel) the note's envelope and filter carry on through the arpeggio, so at a sixth
of a step the notes fuse into a buzzing chord rather than a smear of fresh attacks, each ringing
over the next. Instruments whose notes can't change pitch once started play each as a new note:
a string on Krell or Monochord is plucked again, a tine on Tuss struck again, a drum hit again,
a sample started again, and MIDI gear sent a new note.

**Accent, `!`.** The note plays at full velocity, 127, whatever its own: a quick way to lean on
the first note of a bar without changing what's written.

### Sound gestures

Sound gestures change how one note sounds, for that note alone. They reach the instrument just
before the note starts, and it forgets them after, so the next note plays as usual. Only some
instruments can do each; on the rest, the field shows the gesture dimmed, the status line adds
`(nothing on a synth)`, and it does nothing.

| Gesture | Range      | On                               | What it does                  |
|---------|------------|----------------------------------|-------------------------------|
| `<`     |            | `sampler`, `chop`                | plays the sound the other way |
| `O8`    | 0–15 16ths | `sampler`, `chop`                | starts 8/16 of the way in     |
| `G6`    | 1–23 24ths | `synth`, `switched`, `monochord` | slides in from the last pitch |
| `P12`   | 0–127      | `midi`                           | changes the gear's program    |

**Reverse, `<`.** Plays the note's sound, or its piece of a chop, the other way round from the
instrument's `direction`: backwards usually, forwards if `direction` is set to reverse. `~` on a
chop track's selected steps reverses their order and puts `<` on each, so the slices play
backwards too (shown `◂`).

**Offset, `O0` to `O15`.** Starts the note part way into its sound, in sixteenths of it: of the
sampler's region, or of the chop's piece. `O8` starts half way, `O0` at the start (the same as
no offset, unless a lock moved `start`). A tracker's `9xx`.

**Glide, `G1` to `G23`.** Slides the note's pitch in from the previous note's, over so many
24ths of a step: a portamento. On the synth and Switched, each voice's pitch moves in even
steps; on Monochord the string is plucked at the last pitch and retuned as it goes, as a finger
slides up a fretboard. The Scarlatti's lead glides into its octave leaps (`G6`), and the demo's
acid line slides down to its E♭ (`G8`). Krell, being a harpsichord, doesn't glide.

**Program change, `P0` to `P127`.** On a MIDI track (see
[MIDI tracks](07-instruments.md#midi-tracks-playing-other-gear-midi)), sends a program change
before the step's note, so the gear switches sound and the note plays with the new one: a bass
line that swaps patches for its chorus, or a drum machine that changes kit. It's sent on its
step whether or not the step has a note, so a step with only `P5` changes the sound for the
notes after it. Most gear numbers its programs from 1, so `P0` is its first.

A held note carries its shape to its later hits: `<` with `R3` plays three hits backwards, and
`G3` with `A37` slides round the chord.

### Conditions

Conditions decide whether the step plays at all. When they don't hold, the step plays as an
empty one: no note, no locks, and no other gestures, including the playhead's. So a step with
`F` and `J12` jumps only during a fill, and a lock on a `?50` step changes the setting only on
the passes the step plays.

| Gesture | Range      | Plays the step                             |
|---------|------------|--------------------------------------------|
| `?50`   | 1–99 %     | on 50% of passes, the same ones every time |
| `1:4`   | 1–9 of 2–9 | on the first of every 4 passes             |
| `F`     |            | only during a fill                         |
| `F-`    |            | only when there's no fill                  |

**Chance, `?1` to `?99`.** The step plays on that share of passes. Which passes is decided by
the pass, the step and the track together, scrambled, so it sounds random but is the same every
time the song plays, and differs from track to track and step to step. A song never comes out
differently on the night, and an export sounds as you heard it. In the Scarlatti, the echo rings
an octave over the right hand's hammered notes a quarter of the time (`?25`); in the demo, the
ride drops its last hit on half the passes.

**Every, `k:n`.** The step plays on pass k of every n, counting from 1: `1:4` on the first of
four, `2:4` the second, `4:4` the last, `3:3` the third of three. n goes from 2 to 9, and k from
1 to n. The passes are the track's own, from 0, so `1:4` plays on passes 0, 4, 8. It's how a
crash comes once every four bars, or a fill's pickup once every eight. Note that a rule's
`every 4 1` counts its offset from 0, while `2:4` counts from 1, as Elektron's do: they mean the
same passes.

**Fill, `F`, and no fill, `F-`.** See below.

Both fields can hold conditions; then all of them must hold.

### Fills

A **fill** is something played instead of the usual part for a moment, as a drummer rolls round
the toms going into a chorus. In Lattice, hold the backtick key while the song plays, and the
header shows `FILL`: every step marked `F` plays, and every step marked `F-` doesn't, until you
let go. It starts and ends at once, on whatever step it's on, not at the next bar, so it can be
a quick stab or as long as you like.

So write the fill's notes on steps marked `F`, a snare roll, a tom run or a jump elsewhere (`F`
with `J12`), and mark `F-` the steps they take over from, so the usual beat steps aside. In the
demo song's peak, hold backtick for a snare roll while the second clap gives way to it. In the
Scarlatti, a fill brings in the echo playing the right hand an octave up in canon, timpani rolls
and a snare roll in place of the backbeat.

Knowing when a key comes up needs a terminal that reports key releases (Lattice asks for the
kitty keyboard protocol: kitty, WezTerm, foot, Ghostty and recent Alacritty have it; tmux
doesn't pass it on). Elsewhere, backtick starts a fill and pressing it again ends it. Stopping
ends a fill too.

`F` might become `F-`, so it goes in with enter.

| Key      | Action      | What it does                                        |
|----------|-------------|-----------------------------------------------------|
| backtick | `play_fill` | hold for a fill: steps marked `F` play, `F-` don't  |

### Playhead gestures

Playhead gestures move the track's own playhead, so each track can jump, wait, turn round or
change speed on its own. They're drawn in the accent colour.

| Gesture | Range | What it does                                                   |
|---------|-------|----------------------------------------------------------------|
| `J12`   | 0–255 | after this step, carries on from step 12                       |
| `J?`    |       | after this step, carries on from any step, the same each time  |
| `W2`    | 1–99  | plays this step 2 more times before going on                   |
| `B`     |       | plays the track backwards from here, or forwards again         |
| `*2`    | 1–4   | plays the track twice as fast, until the next speed or its top |
| `/2`    | 1–4   | plays the track at half speed, likewise                        |

**Jump, `J0` to `J255`.** After the step plays, the playhead goes to that step instead of the
next. A step past the track's end wraps round its length. `J0` loops the track early: on step 7
of a 16-step track, `J0` makes it an 8-step loop, and a pass starts each time it jumps back. A
jump forwards skips steps; a jump back repeats them. With `F`, a jump only happens in a fill.

**Wander, `J?`.** As a jump, to a step picked from the pass, the step and the track, so the song
plays the same each time but the track seems to wander. A few on a melody make a part that's
never quite the same twice round.

**Wait, `W1` to `W99`.** The playhead stays on this step for so many more steps' time, playing
it again each time: `W1` plays it twice in all, `W3` four times. The track falls behind the
others by that many steps, as a jump would put it ahead.

**Backwards, `B`.** From here, the playhead goes backwards, a step at a time, until another `B`
turns it forwards again. Going back past step 0, it comes round to the last step, starting a new
pass. A `B` at each end of a phrase makes it go back and forth, as the demo's pluck does on its
G.

**Faster, `*2` to `*4`, and slower, `/2` to `/4`.** The track plays at that many times the
song's speed, or that share of it, from this step until the next speed gesture or the top of its
loop, where it goes back to the song's speed. `*1` (or `/1`) puts it back sooner. At `*2`, a
track plays two of its steps in each of the song's; at `/2`, each of its steps lasts two of the
song's. Note lengths count the track's own steps, so a note at half speed lasts twice as long,
and gestures' 24ths are of the track's step. Double-time hats, or a half-time bass for a
breakdown, without writing them out.

A track that waits, jumps or changes speed drifts against the others, as tracks of different
lengths do (see [Rules](04-rules.md)). A new pattern and stopping put every playhead back at the
top, going forwards at the song's speed.

### Gestures with the rest

- **Two fields.** Each step has two gesture fields, so a step can retrigger and accent (`R3`,
  `!`), play in a fill and jump (`F`, `J12`), or delay and fade (`D6`, `r3`).
- **Rules** move gestures with their steps: rotate a track and its gestures turn with it.
- **Muted tracks** don't play their notes or note gestures, but their playhead gestures still
  move them, so a muted track is in the right place when it comes back.
- **The live view** (`tab`) shows gestures where the rules have put them this pass.

## Locks

### What a lock is

Every instrument and effect has settings: a filter's cutoff, a delay's feedback, Krell's
`drift`. The mixer page sets them for the whole song. A **lock** sets one from a particular step
of a track: the cutoff to 800 Hz from step 4. It holds from its step until that setting's next
lock on the track, going round the end of the track to the first if need be, as a tracker's
columns do. Other settings' locks don't touch it, and notes don't end it. A lock on a step with
no note changes the setting on its own, so a few locks down an empty stretch can move an effect
along under a held note.

A lock can be one of three kinds:

| Written | Kind  | What it does                                                       |
|---------|-------|--------------------------------------------------------------------|
| `45`    | set   | sets the setting, at 45 of the way from 00 to 99 across its range  |
| `~90`   | slide | slides from the setting's last lock to here, arriving on this step |
| `--`    | reset | puts the setting back to the mixer's                               |

A **slide** moves gradually: when a lock plays, Lattice looks ahead along the track for that
setting's next lock, and if it's a slide, the setting moves there over the steps between,
arriving exactly on the slide's step. A filter sweep is two locks: `00` on step 0 and `~99` on
step 15. The setting moves along its own curve, so a cutoff rises by the same musical interval
each step, rather than racing through the low notes.

Stopping, and a change of pattern, end every lock and slide: the settings go back to the
mixer's.

While a lock holds, the mixer page shows the locked value in place of the setting's own, in the
accent colour and marked `◆`, so its knobs move with the pattern. Turning the setting on the
mixer while a lock holds changes the mixer's value, which you hear once the lock ends.

### Lock columns

A track keeps the settings it locks as its **lock columns**, up to four, each a field beside its
gesture fields. A column shows each step's lock as two digits from `00` to `99` across the
setting's range (its own curve: a cutoff's 50 is about the middle of the audible range, not half
its top frequency), `~` before a slide, `--` for a reset, and `╵` on the steps a lock carries on
over. The `╵` marks count round from the track's last lock, as the loop plays them.

The column is headed with the setting's name, shortened to three letters (`cut`, `res`). Where
two columns' settings share a name, an instrument's cutoff and a filter effect's, say, the
effect's column puts the effect's number first (`2cu`), as `:lock 2.cutoff` would name it.

A step holds up to four locks, one for each column, so it always has room. A fifth setting is
refused: `bass has its 4 lock columns already: :unlock one along the whole track`.

The status line lists the locks under the cursor: `cutoff 4000Hz`, `cutoff ~8000Hz`,
`resonance --`.

### `:lock`

`:lock PARAM VALUE` locks a setting on every step the selections reach, on each track they
reach:

```
:lock cutoff 800              the cutoff at 800 Hz from each selected step
:lock cutoff 800Hz            the same: the setting's unit may follow the number
:lock cutoff 200..4000        a sweep: 200 Hz on each selection's first step to 4000 on its last
:lock cutoff ~4000            a slide, arriving at 4000 on each selected step
:lock cutoff --               back to the mixer's cutoff from here
:lock wave square             a choice, by its name
:lock cutoff                  no value: give the track a column for it, to fill in
```

**Naming the setting.** A plain name is looked for on the instrument first, then on each effect
in the track's chain in order, then among the track's macros. To pick one, put where it is
first:

| Name            | Finds                                                        |
|-----------------|--------------------------------------------------------------|
| `cutoff`        | the instrument's cutoff, or else the first effect's with one |
| `inst.cutoff`   | the instrument's, only                                       |
| `filter.cutoff` | the first `filter` effect's                                  |
| `2.mix`         | the second effect's mix, counting from 1                     |
| `macro.bloom`   | the macro called `bloom` (or `bloom` alone)                  |

A setting the track doesn't have is refused with the ones it has:
`bass has no reso: try wave, attack, decay, sustain, release, cutoff, env, resonance, gain`. On
the command line, typing `:lock ` lists the track's settings with their values now, and after a
setting, its choices.

**Values** are numbers in the setting's own units and range, or for a setting with choices, the
choice's name. A value out of range is refused, with the setting's range.

**A sweep**, `FROM..TO`, spreads the values evenly along the setting's curve from each
selection's first step to its last, as `set` locks on every step. **A slide**, `~VALUE`, puts
the slide on the selected steps; the setting slides there from its lock before. Every track has
to have the setting, and room for its column, or nothing is locked.

A macro locks like a setting: `:lock bloom 1` turns the macro all the way up from that step, and
every setting it turns with it (see [The mixer and effects](08-mixer-and-effects.md)).

### `:unlock`

`:unlock PARAM` clears that setting's locks from every step the selections reach; `:unlock`
alone clears all their locks. When a selection covers a whole track, from its first step to its
last, the columns go too, freeing room for others (`%` then `:unlock gain`).

### Surround: `ms`, `mr` and `md`

To change a setting over a passage, a filter dip over two bars or a burst of reverb on a fill,
takes two locks: one at the start, and one after the end that puts the setting back to what it
was. The second is the one that's easy to get wrong, as what it was depends on the locks
before. Vim's surround plugin, which Helix took into its match menu as `ms`, `mr` and `md`,
puts a pair of brackets round a word, changes them, or takes them away. Here the pair is
locks, and they go round the selected steps.

`ms` opens the command line with `:surround ` typed, for a setting and a value, as `:lock`
takes them: `ms` `cutoff 500` `ret`. On each selection, on each track it reaches, the setting is
locked at the value on the first step, and on the step after the last, put back: to the
setting's lock before the selection if it has one, or the mixer's own value (`--`) if not. A
`~` before the value slides there instead. Over steps 08 to 15 of the bass, with nothing locked
before:

```
 07  ···
 08  C-2   cutoff 500   ┐  the first lock, from ms
 09  ···                │
 ..                     │  the selected steps
 15  ···                ┘
 16  C-2   cutoff --       the second, back to the mixer's
```

The message says what went in: `cutoff 500Hz from 08 to 15, then cutoff --`. A lock of the
setting already on the step after is left, as it already says what comes next; that's also what
lets `mr` work. A selection that runs to the end of the track gets no second lock, as playing
stops there or goes round to the top.

`mr` changes a pair: with the cursor inside, it selects where the lock holds (`mil`) and opens
the command line with `:surround cutoff ` typed, for the new value. `md` takes the pair away:
the lock holding at each cursor, and the setting's next lock too if it puts the setting back to
what it was, so the steps play as they did before. If the next lock sets something else, it
stays, and only the first goes.

Which setting? The lock column the cursor's in, or, on the note field, the one locked nearest
above the cursor on its track.

| Keys | Action             | Command     | What it does                                        |
|------|--------------------|-------------|-----------------------------------------------------|
| `ms` | `surround_add`     | `:surround` | lock a setting over the selection, back after it    |
| `mr` | `surround_replace` |             | select where the lock holds, to give it a new value |
| `md` | `surround_delete`  |             | take away the lock, and the one putting it back     |

### Typing in a lock column

In a lock column, insert mode's keys type values rather than play notes:

- two digits set the value, `00` to `99`, on each cursor's step, and move on;
- `~` first, then two digits, makes it a slide;
- `-` puts the setting back (`--`).

In normal mode, `^a` and `^x` nudge the values the selections reach by one (with a count, by
more: `10^a`), `&` fills each selection evenly between the values at its ends (both ends need
one), and `d`, `y` and `p` work on the column alone. Copying a field copies values, not
settings, so `y` on the cutoff column and `p` on the resonance column gives the resonance the
same shape.

### Locking by ear: `L`

Choosing values by number is precise but not musical. Instead, select the steps, go to the mixer
(`gm`), and press `L`: Lattice remembers the track's settings. Turn whatever you like,
listening, and press `L` again: each setting you changed is locked, at its new value, onto the
selected steps, and the track's own settings go back as they were. It's one undo. The status
line says what it did: `◆ cutoff 1200Hz on bass steps 4–7`.

`esc`, or leaving the page, cancels and puts the settings back. If the track's instrument or
chain changed in between, nothing is locked. `L` works the same way on the fx page (`gf`) and
the instrument page (`gi`). Buses have no steps, so they can't be locked.

| Key | Action         | What it does                                                       |
|-----|----------------|--------------------------------------------------------------------|
| `L` | `lock_setting` | mixer, fx or instrument page: `L`, change settings, `L` locks them |

On the song page, the same key loops the song (see [Patterns, the song and the
conductor](06-song.md)).

### Recording locks

To perform locks, arm record (`R`) on the mixer page and play the song: each setting you change
is locked on the step that's playing, and heard at once, rather than changing the mixer's value.
Holding `^a` on the cutoff records a sweep step by step: each nudge goes on from the last lock
recorded. Each pass round the track is one undo, as with recorded notes, so `u` takes back a bad
pass and keeps the ones before it. On the mixer page, arming record doesn't start insert mode,
and a change during the count-in does nothing. See [Recording](09-recording.md).

### Locks with the rest

- **Rules** move locks with their steps; a transpose leaves them alone.
- **Conditions** gate locks: a step that doesn't play this pass doesn't lock.
- **Effects.** Adding or removing an effect in the track's chain moves the later effects' locks
  along with them, in every pattern, and removing an effect drops its locks. Changing the
  instrument drops the instrument's locks.
- **Pasting** steps with locks onto a track gives it columns for their settings, if it has room.

### Locks and the conductor

Tempo is a value that holds, so it might have been a lock. But it's the whole song's, not a
track's, and it's the only such value, so it lives in the conductor as a gesture that behaves
like a lock: `T140` sets it from its step until the next tempo, `~140` slides there, arriving on
its step, and `T--` puts back the song's `:bpm`, as `--` puts back the mixer's value. Stopping
puts `:bpm` back, as it puts every lock back.

Lock slides are counted in steps, so the conductor's tempo changes how long they take: a filter
sweep over a bar takes a bar however fast the bar is. The conductor's step plays before the
tracks' steps, so a tempo set on a step already times that step's locks. The conductor's mutes
and solos (`M2`, `S2`) are an overlay on the mix, like a lock on the mixer's mute, dropped when
the song stops. See [the conductor](06-song.md).
