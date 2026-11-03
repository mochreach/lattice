# Tracks and groups

A song is played by its **tracks**, the columns of the pattern page. This chapter covers what a
track is and what it keeps, how to add, remove, move, copy and name tracks, how to mute and solo
them, and how to gather several into a **group**: a drum kit, say, that folds into a drum
machine's step view, shares a bus on the mixer, and can choke its own hi-hats.

Most of it is on the `t` menu: press `t` on the pattern page and the box in the corner lists
what can follow.

```
                                  ┌ track ───────────────────────────┐
                                  │ t  select whole tracks           │
                                  │ m  mute / unmute                 │
                                  │ M  mute / unmute in this pattern │
                                  │ s  solo / unsolo                 │
                                  │ a  add a track after this one    │
                                  │ d  remove tracks (and copy)      │
                                  │ y  copy tracks                   │
                                  │ p  paste tracks after            │
                                  │ P  paste tracks before           │
                                  │ h  previous track, on its notes  │
                                  │ l  next track, on its notes      │
                                  │ H  move track left               │
                                  │ L  move track right              │
                                  │ g  group / ungroup tracks        │
                                  │ z  fold / unfold group           │
                                  │ e  keep tracks open / close them │
                                  └──────────────────────────────────┘
```

## What a track is

A track is one line of music played by one instrument: a kick drum, a bass line, a pad. It has
its own length, so tracks of different lengths can loop against each other, and its own mixer
strip, so it can be set louder or softer, left or right, on its own.

Some of a track belongs to the whole song, and some to each pattern. Patterns are the sections
of a song ([chapter 6](06-song.md)); every pattern has the same tracks, but each can give a
track different notes.

| Belongs to   | What                                                                              |
|--------------|-----------------------------------------------------------------------------------|
| the song     | name, instrument, effects, mixer strip, choke number, macros, lock columns, group |
| each pattern | steps and so length, rules, pattern mute                                          |

So a bass line can be 16 steps in the verse and 12 in the chorus, with a `:tr 5` rule in one and
none in the other, while it's the same instrument through the same effects at the same level in
both. Changing the bass's sound changes it everywhere; changing its notes changes them only in
the pattern you're on.

A song has at least one track and at most 16. When there are more tracks than fit across the
terminal, the grid scrolls sideways to keep the cursor's track in view, with `‹` and `›` by the
names to show there are more off to that side.

### The header

Each track's header, above its steps, shows its name, its flags, its length, its level, its pan,
its rules and a meter ([chapter 2](02-patterns.md#the-header)). Here the kick is muted in this
pattern only (`m`), the hat soloed (`S`) and the bass muted (`M`). On screen, every track but
the hat is drawn dimmed, as the solo silences them:

```
    ╭─ DRUMS ───────────────────────────────────────╮
    KICK      m HAT                     S HAT2        BASS      M
    16          16                        16          16
        0dB                0dB        ▊       0dB         0dB
         C                  C                  C           C
    no rules    no rules                  no rules    no rules
    ─────────── ───────────────────────── ─────────── ───────────
```

### Colours

Each track has a colour, used for its playhead, its meter, and its strip on the mixer page.
Colours go by position, from the theme's list of track colours, round and round: by default the
terminal's blue, green, magenta, cyan, yellow and red. So the first track is blue and the
seventh blue again, and moving a track changes its colour to its new place's. Because they're
the terminal's own colours, they suit whatever theme your terminal uses; `tracks` in the config's
`[theme]` changes the list ([chapter 10](10-files-and-config.md#colours)).

## Adding and removing tracks

`ta` adds a track after the cursor's and moves the cursor onto it. It has 16 empty steps, the
default synth, and a name, `track 5` or whichever `track N` is free first. `:track-add NAME`
(or `:ta NAME`) does the same with a name of your choosing.

`td` removes the tracks the selections reach, copying them first, as `d` does steps: `td` here
and `tp` there moves tracks. It won't remove every track: "can't remove every track". The status
line says how many it copied (`copied 2 tracks`). `:track-remove` (or `:trm`) removes the
cursor's track without copying it, and won't remove the only one.

Removing a track is undone like anything else: `u` brings it back with its notes in every
pattern, its instrument and its effects. Tracks can be added and removed while the song plays;
the others play on.

| Keys | Action         | Command         | What it does                                         |
|------|----------------|-----------------|------------------------------------------------------|
| `ta` | `add_track`    | `:track-add`    | add a track after the cursor's                       |
| `td` | `remove_track` |                 | remove the tracks the selections reach, copying them |
|      |                | `:track-remove` | remove the cursor's track, without copying           |

A count doesn't repeat the track commands: `3ta` adds one track.

## Selecting, copying and pasting tracks

`tt` selects every step of the tracks the selections reach, from the top to the bottom of each.
It's the way to act on whole tracks with the pattern page's edits: `tt` then `~` reverses a
track, `tt` then `C-a` transposes it, `tt` then `y` copies its steps.

`ty` copies whole tracks rather than their steps: everything about them, their name, notes,
rules, instrument, effects and mixer settings. `tp` pastes the copies after the cursor's track,
and `tP` before it, as new tracks. A pasted track carries only the notes and rules it had in the
pattern it was copied from; in the other patterns it starts empty, at that length. Pasting won't
go past 16 tracks, and with nothing copied says "no tracks copied".

That's how to make a variation of a part to play beside the original: `ty` `tp` on the bass gives
two basses, the same sound, ready to be changed. Names needn't be unique, so the copy is called
`BASS` too until you rename it.

| Keys | Action                | What it does                                         |
|------|-----------------------|------------------------------------------------------|
| `tt` | `select_tracks`       | select every step of the tracks the selections reach |
| `ty` | `yank_tracks`         | copy whole tracks                                    |
| `tp` | `paste_tracks_after`  | paste copied tracks after the cursor's               |
| `tP` | `paste_tracks_before` | paste copied tracks before the cursor's              |

The track clipboard is apart from the steps' (`y` and `p`), so copying steps doesn't lose the
tracks you copied, or the other way round.

## Moving between and moving tracks

`th` and `tl` go to the previous and next track, landing on its notes, whichever field you were
in; `h` and `l` go through the open track's fields first ([chapter 2](02-patterns.md)). `gh` and
`gl` go to the first and last track.

`tH` and `tL` move the cursor's track one place left or right, taking its notes, sound and mix
with it; the cursor goes with it. `:track-move N` (or `:tmv N`) moves it N places, right for a
positive number and left for a negative one, as far as the end: `:track-move -3`, or
`:track-move 16` to send it to the end.

| Keys | Action             | Command          | What it does                        |
|------|--------------------|------------------|-------------------------------------|
| `th` | `prev_track`       |                  | to the previous track, on its notes |
| `tl` | `next_track`       |                  | to the next track, on its notes     |
| `tH` | `move_track_left`  | `:track-move -1` | move the track one place left       |
| `tL` | `move_track_right` | `:track-move 1`  | move the track one place right      |

## Naming and length

`:name NAME` renames the cursor's track: on the mixer, fx and instrument pages the strip's, and
on the song page the column's. The name is the song's, the same in every pattern, and the header
shows it in capitals. It's how the track is known elsewhere too: on the mixer and
instrument pages, and in the song page's mutes. Spaces are allowed: `:name open hat`.

`:len N` sets the cursor's track's length in this pattern, from 1 to 256 steps. Lengthening adds
empty steps at the end. Shortening cuts the steps past the new end off, notes and all; `u` brings
them back, but a later `:len` to the old length doesn't.

### Tracks of different lengths

Each track loops on its own length, so tracks of different lengths drift against each other and
come back together. This is **polymeter**, several meters at once, a staple of techno and of
minimal music. A 12-step bass against 16-step drums lines up again only every 48 steps, three
bars of 16: each bar, the bass starts its figure a beat earlier against the kick, so the same
two parts make a three-bar phrase.

While playing, each track's header says how many times round it is. Here the bass is 12 steps
and on its second pass while the drums are still on their first:

```
    ╭─ DRUMS ─────────────────────────╮
    KICK        HAT         HAT2        BASS
    16 · pass 1 16 · pass 1 16 · pass 1 12 · pass 2
        0dB         0dB         0dB                0dB        ▊
         C           C           C                  C
    no rules    no rules    no rules    no rules
    ━━━━━━━━━── ━━━━━━━━━── ━━━━━────── ━━━━━━━━━━━━━━━━━────────
                                                    vel g    G
 00  C-4         ···         ···         C-2        78   ··   ··
 ...
 10  ···         C-4         ···         C-2        78   ··   ··
 11  ···         ···         ···         ···             ··   ··
 12  C-4         ···         C-4
 13  ···         ···         ···
```

The shorter track's column stops at its last step. The meter under each header fills as the
track goes round, so you can see where each one is.

Lengths aren't the only way to make parts go round at different rates. There's no speed setting
for a whole track; the gestures `*2` and `/2` play a track faster or slower from a step on
([chapter 5](05-gestures.md#playhead-gestures)), and rules such as `:drift` shift a part a step
further each pass ([chapter 4](04-rules.md)).

## The mix: level, pan and send

Each track has a mixer strip. The mixer page (`gm`) is where to work on it, by ear, but these
commands set it exactly from the pattern page too, for every track the selections reach, so
`tt` across three tracks then `:level -6` turns all three down.

| Command     | Range              | What it sets                                            |
|-------------|--------------------|---------------------------------------------------------|
| `:level DB` | -60 to +6 dB       | how loud the track is; 0 dB leaves it as it is          |
| `:pan P`    | -100 to 100        | where it sits, from hard left (-100) to hard right      |
| `:send DB`  | -60 (off) to +6 dB | how much of it goes to the send bus, the shared effects |

`:level -6` halves the track's level, near enough; `:pan -30` puts it a little left; `:send -12`
sends a little of it to the send bus's reverb or delay. Decimals are allowed for decibels
(`:level -4.5`). On the mixer page the same commands set the strip under the cursor, including
a bus's level. What decibels are, how pan works and what the send bus is are in
[chapter 8](08-mixer-and-effects.md).

## Mute and solo

There are four ways to silence a track, for four different jobs.

| Flag | How                   | Lasts                   | What it's for                         |
|------|-----------------------|-------------------------|---------------------------------------|
| `M`  | `tm`, `:mute`         | the song, every pattern | taking a part out of the mix          |
| `S`  | `ts`, `:solo`         | the song, every pattern | hearing a part on its own             |
| `m`  | `tM`, `:pattern-mute` | this pattern only       | a part that sits out a section        |
| `─`  | the song page's `m`   | one entry of the song   | a part that sits out one time through |

### Mute: `tm`

`tm` mutes the tracks the selections reach, and unmutes them if they're all muted already; with
some muted and some not, it mutes them all. `:mute` does the same. A muted track's name shows
`M` and is drawn dimmed.

Muting is the mixer's: it turns the track's strip down to silence, gliding there over a few
milliseconds so it doesn't click. The track goes on playing behind it, so unmuting brings it
straight back in mid-phrase, and its effects ring out, a delay's echoes dying away rather than
stopping. Because the send is taken after the strip's level, a muted track sends nothing either.
A mute is saved with the song, and undone with `u`.

### Solo: `ts`

`ts` solos the tracks the selections reach, or unsolos them; `:solo` does the same. While any
track is soloed, only soloed tracks are heard, and the rest are dimmed: solo two tracks to hear
those two together. It's a mixing desk's solo, for listening closely to one part: to set a
bass's tone, or find which of eight tracks has the wrong note. Unsolo them all to hear the
whole again.

A soloed track that's also muted stays silent; a mute wins.

### Pattern mute: `tM`

`tM` mutes the tracks the selections reach in this pattern only, and `:pattern-mute` (or `:pm`)
does the same. The name shows `m`. It's for a part that sits out a whole section: the drums out
of the breakdown, the lead out of the intro, without making a pattern with the notes deleted.

A pattern mute works differently from `tm`'s. It stops the track's steps from playing, notes and
locks alike, while the strip stays as it is: a note already sounding rings on to its end, and
its effects too. The playhead still goes round, and still follows the track's gestures, so when
the next pattern comes in the track is where it should be. It's kept with the track's steps in
the pattern, so going to another pattern brings that pattern's own.

### Mutes in an entry, and the conductor's

The song page can mute a track in one **entry** of the song, so a pattern played twice can be
orchestrated differently each time; the name then shows `─` while that entry plays. That's in
[chapter 6](06-song.md#mutes-in-an-entry).

The conductor's gestures `M2` and `S2` mute and solo a track from a step of the song, for as long
as the song plays, or until `M-2` or `S-2` undoes it. Pressing `tm` or `ts` on that track takes
over from the conductor ([chapter 6](06-song.md#conductor-gestures)).

With several at once, the name shows one flag, the first of `M`, `m`, `─` and `S` that applies;
`○` and `●` mark a track armed for the looper ([chapter 9](09-recording.md)).

| Keys | Action                | Command         | What it does                        |
|------|-----------------------|-----------------|-------------------------------------|
| `tm` | `toggle_mute`         | `:mute`         | mute or unmute, in every pattern    |
| `tM` | `toggle_pattern_mute` | `:pattern-mute` | mute or unmute in this pattern only |
| `ts` | `toggle_solo`         | `:solo`         | solo or unsolo                      |

## Groups

A **group** is a named run of tracks next to each other: a drum kit, with a drum on each track,
or the strings of a string section. Each track in it stays a whole track, with its own notes,
length, rules, instrument and strip. The group adds three things:

- it can **fold**, narrowing its tracks to a drum machine's grid of hits, so a kit of eight
  drums takes little room;
- it has a **bus** on the mixer, so the kit can be turned up or down, compressed or driven, as
  one instrument;
- its tracks can **choke** each other, as a closed hi-hat stops an open one.

A song can have up to eight groups. They're saved with the song, and undone like everything
else.

### Making a group: `tg`

Select the tracks, then `tg`: it groups every track from the first the selections reach to the
last, and names the group `group 1` (or the next number free). `:group-name NAME` renames the
group the cursor's track is in:

```
gh v tl tl tg
:group-name drums
```

```
    ╭─ DRUMS ───────────────────────────────────────╮
    KICK        HAT         HAT2                      BASS
    16          16          16                        16
        0dB         0dB                0dB        ▊       0dB
         C           C                  C                  C
    no rules    no rules    no rules                  no rules
    ─────────── ─────────── ───────────────────────── ───────────
```

The bracket above the names spans the group. `tg` on the tracks of a group, from its first to
its last, ungroups them; the tracks stay as they were. Groups can't overlap: grouping tracks
that take in part of another group is refused ("those tracks overlap the group DRUMS").
`tg` leaves select mode on, so you can go on.

As the tracks of a group are next to each other, arrange them first: `tH` and `tL`, or
`:track-move`, bring the drums together.

### Tracks coming and going

Adding, removing and moving tracks keeps the groups whole:

- a track added or pasted **between** two of a group's tracks joins the group; one added at
  either **end** stays outside it;
- a track moved within its group stays in it; moved past either end, it leaves;
- removing a track leaves the rest of its group; when the last of a group's tracks goes, the
  group goes with it.

So to add a drum to a kit, add it from any track but the last, and it's in.

### Folding: `tz`

`tz` folds the groups the selections reach, and unfolds them. A folded group draws each track as
a column two characters wide, showing a hit on each step with a note as a glyph by how hard it's
played, as a drum machine's step view does:

| Glyph | Velocity                       |
|-------|--------------------------------|
| `●`   | 110 and up (`86` and up shown) |
| `•`   | 80 and up (`63` and up shown)  |
| `∙`   | below 80                       |
| `·`   | no note                        |

```
    ╭─ DRUMS ▸
    K  H  H  BASS
    16 16 16 16
                 0dB
                  C
             no rules
    ── ── ── ───────────

 00  ●  ·  ·  C-2
 01  ·  ·  ·  ···
 02  ·  •  ·  C-2
 03  ·  ·  ·  ···
 04  ∙  ·  •  D#2
 05  ·  ·  ·  ···
 06  ·  •  ·  ···
```

The header keeps each track's initial, its length, an `r` for each rule and its meter; the level
and pan are on the mixer page. The bracket shows the group's name with `▸` while it's folded. If the
group is too narrow for it, as here, the name runs on over the space after, where there's no other
group, in place of the bracket's end; only a group straight after cuts it short.

Folded, everything works as unfolded: the cursor moves between the narrow columns, and notes,
selections, edits and insert mode all work on them. In insert mode in a folded group, the number
row's `1` to `9` play and enter the group's drums, as pads, instead of the piano's sharps; the
numpad does the same anywhere ([chapter 9](09-recording.md#the-pads)). `tz` outside a group says
"only groups fold; tg makes one". Folding is saved with the song and undone with `u`.

### Keeping tracks open: `te`

Only the cursor's track shows its velocity, gesture and lock fields; the others are **closed**,
showing only their notes ([chapter 2](02-patterns.md#reading-a-step)). `te` keeps the tracks the
selections reach open after the cursor leaves, to watch one track's locks while editing
another's notes. If any track is open already, `te` closes them all instead. Which tracks are
open isn't saved.

### The group's bus

A group's tracks mix into a strip of its own, the group's **bus**, before the master: it has a
level and an effect chain, so a whole kit can be turned down at once, or given drive and
compression together, which is how drums are glued into one instrument in a mix. Each track's
send still goes straight to the send bus. The bus is on the mixer page after the tracks, in its
first track's colour; [chapter 8](08-mixer-and-effects.md#group-buses) covers it.

### Choke groups

On a drum machine, an open hi-hat rings for a long time, a closed one barely at all, and on a
real kit they're the same pair of cymbals: closing the pedal stops the open ring. Roland's
TR-808 and TR-909 copied this, so a closed hat cuts off an open one sounding before it, and
samplers and drum machines since have called it a **choke group**: a set of sounds where each
cuts the others off. It's what makes a hat pattern sound played rather than layered.

`:choke N` gives the tracks the selections reach a choke number, 1 to 8; `:choke 0` takes it
off. When a track plays a note, every other track in the same group with the same number is
cut off at that instant, faded over a few milliseconds so it doesn't click:

```
gh tl v tl :choke 1
```

Now the closed hat (`HAT`) on step 06 cuts off the open hat (`HAT2`) from step 04, and the open
hat's next hit will cut off the closed hat's tail. The mixer page shows the number on the
track's detail, where it can be changed too:

```
 ── HAT2  ─ → drums → master ─────────────────────────────────────────────────────
 mix      level     0dB       pan      C        send     off       choke  1
 chain     hat
```

A few points:

- The number only counts within a group: two tracks in different groups with the same number
  don't choke each other, and a track in no group never chokes. So `1` can be the hats in one
  kit and the congas in another.
- Tracks that play on the same step don't choke each other, so a hat and an open hat played
  together both sound.
- Every instrument can be choked: a drum or sample fades out, a synth releases its notes.
- Playing the tracks by hand, from the keyboard or the pads, chokes as the pattern does.

| Keys | Action         | Command       | What it does                                    |
|------|----------------|---------------|-------------------------------------------------|
| `tg` | `toggle_group` | `:group-name` | group or ungroup tracks; the command renames it |
| `tz` | `toggle_fold`  |               | fold or unfold groups                           |
| `te` | `toggle_open`  |               | keep tracks open, or close them                 |
|      |                | `:choke N`    | a choke number, 1 to 8, or 0 for none           |

## Every track command

| Command             | Short  | What it does                                          |
|---------------------|--------|-------------------------------------------------------|
| `:track-add [NAME]` | `:ta`  | add a track after the cursor's, named or `track N`    |
| `:track-remove`     | `:trm` | remove the cursor's track                             |
| `:track-move ±N`    | `:tmv` | move the cursor's track N places                      |
| `:name NAME`        |        | rename the cursor's track                             |
| `:len N`            |        | the cursor's track's length in this pattern, 1 to 256 |
| `:level DB`         |        | level, -60 to +6 dB                                   |
| `:pan P`            |        | pan, -100 to 100                                      |
| `:send DB`          |        | send, -60 (off) to +6 dB                              |
| `:mute`             |        | mute or unmute                                        |
| `:solo`             |        | solo or unsolo                                        |
| `:pattern-mute`     | `:pm`  | mute or unmute in this pattern                        |
| `:choke N`          |        | choke number, 0 to 8                                  |
| `:group-name NAME`  |        | rename the cursor's track's group                     |

The mix commands, `:mute`, `:solo`, `:pattern-mute` and `:choke` act on every track the
selections reach (on the song page, the columns its selection reaches); the others on the
cursor's track, or the strip's or column's on the other pages. In the conductor's column, `:len`
sets the conductor's length ([chapter 6](06-song.md#the-conductor)).

## Notes and oddities

- Tracks are numbered by position, so moving a track changes its colour, and which track a
  conductor's `M2` or the pads' mute mode means.
