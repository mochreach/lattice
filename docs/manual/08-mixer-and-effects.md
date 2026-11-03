# The mixer and effects

Every sound Lattice makes goes the same way: out of a track's instrument, through that track's
effects, through its channel strip, and on into the buses that sum everything into what you
hear. This chapter follows that path. It describes the mixer page (`gm`), where the strips are;
the fx page (`gf`), where a strip's effects branch into lanes; each of the eight effects, setting
by setting; and the two ways of making settings move, locks and macros.

## How the sound flows

### From instrument to speakers

Each track's sound goes through four stages, then joins the rest:

```
 instrument → track effects → strip: level, pan, mute, solo ─┬─→ group bus ─┐
                                                            │  (if in one)  │
                                                            ├───────────────┴─→ master
                                                            └─→ send ─→ send bus ─→ master

 master bus → master effects → master level → + metronome click → soft clip → out
```

- The **instrument** turns notes into sound (see [the instruments](07-instruments.md)).
- The **track effects** are the track's own chain: up to eight effects, run in order, or in
  branching lanes (see [the fx page](#the-fx-page)).
- The **strip** sets the track's level and its place left to right, and mutes or solos it.
- From the strip the sound goes into its **group's bus**, if the track is in a group (`tg`), or
  straight to the **master** if not. A group bus has its own effects and level, then joins the
  master too.
- At the same time a copy, at the track's **send** level, goes into the **send bus**: a bus
  every track can feed, with its own effects, which also ends in the master.
- The **master** runs its own effects and level, the metronome's click is added, and last of
  all a gentle soft clipper (a `tanh` curve) rounds off anything that would go past full scale,
  so a mix pushed too hot thickens rather than crackling.

The mixer has a fixed shape on purpose. A tracker's signal flow is always the same, so Lattice
needs no general audio graph to wire up; the shape is fixed, the code is simple to make
real-time safe, and you never have to wonder where a sound has gone. Branching happens inside a
chain, on the fx page, where it's useful.

### Inserts and sends

An effect on a track's own chain is an **insert**: all of the track's sound goes through it.
Drive, a filter or a compressor are almost always inserts, since they change the sound itself.

A **send** is the other way of using an effect. Rather than putting a reverb on every track,
a studio has one reverb on a bus, and each channel sends a copy of itself into it, as much as
it wants. One reverb then makes all the instruments sound as if they're in the same room,
and one knob on each channel says how far back in that room it stands. Lattice works the same
way: each track has a `send` level into the **send bus**, and whatever effects are on the
send bus are shared by every track that sends to it.

Because the send bus carries only what's sent to it, its effects should usually be fully wet
(`mix` at 1): the dry sound is already reaching the master from the tracks themselves. A new
song starts with a send bus of a dotted-eighth echo (`delay`, mixed at 1) going into a plate
(`reverb`, mixed at 0.35), and a light compressor on the master; `:n` starts an empty song
with no effects anywhere.

The send is **post-fader**: it's taken after the track's level, pan and mute. Turn a track
down and its echoes go down with it; mute it and they stop too. Sends start off (at -60 dB).
A send is taken from the track itself, before any group bus, so a track in a group can still
send to the send bus.

### Decibels

Levels in Lattice are in **decibels** (dB), the unit audio uses for loudness ratios, because
our ears hear ratios rather than differences: going from one violin to two sounds about as
much louder as going from two to four. The scale is logarithmic:

- 0 dB leaves a level as it is (unity gain);
- every -6 dB about halves the signal's amplitude, and +6 dB about doubles it;
- -20 dB is a tenth of the amplitude, -40 dB a hundredth.

A strip goes from **-60 dB**, which Lattice treats as silence and shows as `off`, up to
**+6 dB**. Nudges move a level by 0.5 dB.

### Pan

Pan places a track from hard left (`L100`) through the centre (`C`) to hard right (`R100`).
Lattice's pan is a **balance** control: in the centre both sides are at full level, and
panning one way turns the other side down, along a quarter of a cosine, leaving the near side
alone. Hard left is the left channel only.

Many mixers use a "constant-power" pan law instead, which turns a centred mono sound down by
3 dB on each side so that it seems as loud wherever it's panned. That makes sense for mono
sources, but Lattice's instruments are stereo (a steel string's pair, Switched's spread saws,
the ensemble's three voices), and for those a pan law would only make every centred track
quieter. Balance keeps a centred track exactly as it was made.

Level, pan and send glide to new settings over about 20 milliseconds, so turning a knob, muting
or soloing never clicks.

### Mute and solo

A muted track is silent, and so are its sends. While any track is **soloed**, only soloed
tracks are heard, as on a mixing desk: solo two tracks to hear just those two. Muting and
soloing use the same gliding gain as the level, so they fade in and out rather than click.
Effects on the buses keep running, so a reverb tail rings out after its track is muted.

`m` and `s`, or `tm` and `ts` (or `:mute` and `:solo`), mute and solo the strip's track;
the [tracks chapter](03-tracks.md) covers them, with pattern mutes and the conductor's. Here
`:name`, `:len` and `:track-add`, `:track-remove` and `:track-move` go to the strip's track.

### Meters

Every strip has a meter: a bar that lights up (`━`) as loud as the strip's peaks, falling back
at 30 dB a second between them. It reads from -48 dB at its left end to full scale (0 dB) at
its right; at full scale or over it turns the error colour, a warning that the next stage will
have to round the peak off.

Each meter shows the sound after its strip: a track's after its level and pan, a group's or a
bus's after its effects and level, and the master's after its effects and level, before the
click and the soft clipper. On the pattern page, each track's header carries the same meter in
place of its underline, with its level and pan as small bars above it.

## The mixer page

`gm` opens the mixer, on the pattern cursor's track; `gp` goes back to the pattern. The page is
in two halves. Above is the **overview**, a column for every strip, so balancing a mix is a
matter of moving sideways. Below is the **detail** of the strip under the cursor: where its
sound goes, its mix, its chain of devices and the settings of whichever device is chosen.

Here is the mixer on [`examples/mountain-king`](../../examples/mountain-king), on its first
track, while it plays:

```
          TROLL       BASSOON     ›│TROLLS      KIT         SEND        MASTER
 level       -5dB  ▏     -3dB  ▌  ›│    0dB         0dB         0dB         0dB
 pan           C          L20     ›│
          ─────────── ━━━━━━───── ›│━━━━━━━──── ━━━━─────── ━━━━━────── ━━━━━━━━───

 ── TROLL  ─ → trolls → master, and → send ───────────────────────────────────────────────
 mix      level    -5dB  ▏    pan      C        send    -18dB      choke  –
 chain     synth → drive → delay → pitch

 wave           square        attack     ▎   0.002s        decay          0.16s
 sustain    ▋    0.15         release        0.08s        1 cutoff        1400Hz▎
 env         ▎  2600Hz        1 resonance     0.4         gain       ▎     0.3
```

### The overview

The overview has a column for each **track**, then each **group's bus** (named after the
group), then the **SEND** and **MASTER** buses. Each column shows the strip's name, its level
with a small bar of where it sits between -60 and +6 dB, its pan (tracks only) and its meter.
When there are more tracks than fit, the tracks scroll sideways (`‹` `›` mark that there are
more), while the group, send and master columns stay in view to the right of the `│`.

On the overview, `h` and `l` move from strip to strip, and `^a` `^x` raise and lower the
level of the one under the cursor, half a decibel at a time, or more with a count (`6^x` takes
off 3 dB).

### The detail

Under the overview, a rule names the strip and says **where its sound goes**:

-  a track: `→ master`, or `→ trolls → master` if it's in the group `trolls`, with `, and → send`
  when its send is on;
- a group's bus: the tracks that feed it, then `→ master`;
- the send bus: every track that sends to it, then `→ master`;
- the master: `→ out`.

Then come three kinds of row:

- **mix**: the track's `level`, `pan`, `send` and `choke` group; a bus has only a `level`. The
  choke group (0, shown `–`, for none, up to 8) lets drums in a group cut each other off, as a
  closed hi-hat stops an open one ringing; the [tracks chapter](03-tracks.md) explains it, with
  `:choke`.
- **chain**: the devices the sound passes through, in order: the track's instrument, then its
  effects (`synth → drive → delay → pitch`). On a bus, it's the effects alone. A chain that
  branches is listed in lane order, A first; the fx page shows its shape.
- **settings**: the settings of the device chosen on the chain, always shown, in a grid three
  columns wide, filled a row at a time. Each has its name, a small bar of where it sits in its
  range, and its value in its unit. A digit before the name (`1cutoff`) means a macro turns it
  (see [macros](#macros)); while a lock holds it during playback, its value is shown in the
  accent colour and marked `◆`.

### Keys

The mixer reuses the pattern page's keys for what they mean here. Each key's action is named as
the config file binds it (see [files and configuration](10-files-and-config.md)):

| Key              | Action                              | On the mixer page                       |
|------------------|-------------------------------------|-----------------------------------------|
| `h` `l`, `←` `→` | `left`, `right`                     | along the row: strips, fields, devices  |
| `j` `k`, `↓` `↑` | `down`, `up`                        | overview, mix, chain, settings: a row   |
| `(` `)`          | `rotate_backward`, `rotate_forward` | previous / next strip, keeping the row  |
| `gh` `gl`        | `first_track`, `last_track`         | first / last strip (the master)         |
| `gg`, `home`     | `first_step`                        | up to the overview                      |
| `ge`, `end`      | `last_step`                         | down to the last setting                |
| `tab`            | `toggle_view`                       | between the overview and the chain      |
| `^a`, `+` `=`    | `note_up`, `longer`                 | nudge the value under the cursor up     |
| `^x`, `-`        | `note_down`, `shorter`              | nudge it down                           |
| `c`              | `change`                            | type a value: `:set `, or `:inst `      |
| `d`, `A-d` `del` | `delete`, `clear`                   | remove the effect under the cursor      |
| `L`              | `lock_setting`                      | lock changed settings on selected steps |
| `esc`            | `escape`                            | cancel `L`'s locking                    |
| `m` `s`          | `toggle_mute`, `toggle_solo`        | mute / solo the cursor's track          |
| `R`              | `toggle_record`                     | arm record: changed settings are locked |
| `gf` `gi`        | `goto_effects`, `goto_instrument`   | the fx, instrument pages, on the strip  |
| `gm` `gp`        | `goto_mixer`, `goto_pattern`        | the mixer, the pattern                  |

`(` and `)` cycle selections in Helix, which the mixer doesn't have, so here they change strip
from wherever the cursor is, keeping it on the same row: on `cutoff` on one track, `)` lands on
the next track's settings. `gi` and `gf` open the instrument and fx pages on the strip's track,
and from the pattern page they open on the cursor's track. Undo, play, the command line and
the rest work as everywhere. Pattern editing keys do nothing here.

A nudge moves:

- a level or send by 0.5 dB;
- pan by 5 (of 100), kept to whole numbers so repeated nudges don't drift;
- the choke group by 1;
- a setting by a hundredth of its range, measured along its curve. Settings measured in hertz
  or seconds are on a logarithmic curve, so a nudge of a cutoff moves it by the same musical
  interval wherever it is, rather than by the same number of hertz, which would crawl at the
  top and leap at the bottom;
- a setting that picks from a list (a filter's `type`, a phaser's `stages`) by one choice; one
  in whole numbers (the pitch shifter's semitones) by one.

### Setting exact values

`c` opens the command line with `set ` typed, ready for a number; `:set VALUE` sets whatever
is under the cursor, in the units shown:

```
:set -4.5        a level or send, in dB (-60 to 6)
:set -30         pan, from -100 (left) to 100 (right): here L30
:set 3           the choke group, 0 to 8
:set 800         a setting, in its unit: a cutoff of 800 Hz
:set high        a setting with a list of choices, by name: a filter's high-pass
:set 2           or by its place from 0: the same
```

A value outside the range is refused, saying what the range is (`this goes from -60 to 6`, or
for a setting `cutoff goes from 20Hz to 16000Hz`).
On the chain row, `c` on the instrument opens `:inst ` instead, to choose another instrument;
on an effect there's nothing to type, and `:set` says to move onto a level or setting.

`:set` works on the mixer, fx and instrument pages; on the fx page it also sets a send's level,
a lane's out, a macro or a macro's depth.

### Mix commands

These work on the pattern page too, on every track the selections reach, so you can set a
whole group's sends at once:

| Command     | Range          | What it sets                                                 |
|-------------|----------------|--------------------------------------------------------------|
| `:level DB` | -60 to 6       | the track's level; on the mixer or fx page, a bus's too      |
| `:pan P`    | -100 to 100    | the track's pan, left to right                               |
| `:send DB`  | -60 (off) to 6 | the track's send to the send bus                             |
| `:choke N`  | 0 to 8         | its choke group; 0 for none                                  |
| `:mute`     |                | mutes, or unmutes, the tracks                                |
| `:solo`     |                | solos, or unsolos, the tracks                                |

On the mixer page they act on the strip under the cursor. A bus has only a level and effects,
so `:pan` or `:send` on one says so. `:mute` and `:solo` toggle all the tracks together: if any
of them is unmuted, they're all muted. Every change is one undo.

### Adding and removing effects here

`:fx-add EFFECT` adds an effect, at its default settings, and moves the cursor onto it:

- with the cursor on the chain or the settings, just after the device under it (on the
  instrument, first in the chain);
- anywhere else on the page, at the end of the chain;
- on the pattern page or the instrument page, at the end of the cursor's track's chain.

The effects are `delay`, `reverb`, `filter`, `ensemble`, `phaser`, `drive`, `comp` and
`pitch`; `tab` on the command line lists them with their settings. A chain holds at most eight
(`at most 8 effects in a chain`).

`d` on an effect on the chain row removes it (on anything else it says
`move onto an effect on the chain to remove it`). Its locks go with it, and the locks and macro
targets of the effects
after it move along, so they stay on the effects they were made for. `u` brings it back.

There's no key to move an effect along a chain; remove it and add it where you want it, or
build the chain on the fx page, where a new effect goes after the cursor.

### Group buses

`tg` on the pattern page gathers the tracks the selections reach into a **group** (see the
[tracks chapter](03-tracks.md)). Each group has a **bus**: the group's tracks mix into it
rather than into the master, and the bus has a level and up to eight effects of its own, then
goes on into the master. A song can have up to eight groups.

A group bus is where to treat several tracks as one instrument. Mixing engineers call it
"glue": a little drive and a compressor across a drum kit make the drums push and breathe
together, which they never do when each is compressed alone. In the demo song, the folded
`drums` group's bus has a drive mixed in at half, then a compressor with a 10 ms attack, so
each hit's front gets through before the compressor clamps down, mixed in at 60%. In
`mountain-king`, the four melodic tracks share a compressor on the `trolls` bus, and the kit a
drive and compressor on `kit`:

```
 ── KIT  ─ kick snare hat open clap tom → master ──────────────────────────────────────
 mix      level     0dB
 chain     drive → comp

 drive        ▊     0.2         tone          9000Hz       mix           ▋0.4
 level              0dB      ▎
```

A group bus has no pan and no send: each track's send is taken before the bus, from the track.

### The send and master buses

The **send bus** (`SEND`) takes every track's send and runs its effects on the sum. The
**master** (`MASTER`) takes everything: the tracks outside groups, the group buses and the send
bus. Each has a level and a chain of up to eight effects, set as any other strip's, and both
are saved with the song.

On the master, a gentle compressor holds a mix together; a filter there sweeps the whole song,
for a breakdown. Keep the master's level at or under 0 dB and watch its meter: the soft clipper
after it is a safety net, not a sound.

## The fx page

`gf` opens one strip's effects as a page of their own, drawn the way the pattern draws notes:
the sound goes down the page through the effects, each effect's settings on the lines under its
name, and the chain can branch into **lanes** side by side. It opens on the pattern cursor's
track from the pattern page, on the mixer's strip from the mixer, and on the instrument page's
track from there. Here is `mountain-king`'s first track:

```
 ── TROLL  ─ effects → trolls → master, and → send ───────────────────────────────────────

      macros                            A ← in                   B ← A
      1 fury                 0          drive ─────────────────  delay ─────────────────
       cutoff              +0.35▌       1 drive        0.25       time         0.326s
       resonance           +0.20         tone         7000Hz▏    1 feedback     0.35
       drive.drive         +0.60  ▎      mix           0.6▎       tone         3000Hz
       delay.feedback      +0.30         level         0dB  ▋     mix            1
                                        → B  -9.0 dB             pitch ─────────────────
                                        │                         pitch         12st
                                        │                         fine          0ct
                                        │                         window       0.08s
                                        │                         mix            1
 out                                             0dB        ▉             -8dB
```

Read it from the top. Lane A takes the track's sound (`A ← in`) and drives it. After it, a
**send** row, `→ B -9.0 dB`, copies the sound at that point into lane B, 9 dB down; lane B
(`B ← A`) puts it through a delay and then shifts it up an octave, so the echoes come back an
octave higher. Along the bottom each lane's **out** is its level into the strip: lane A at
0 dB, lane B at -8 dB. To the left is the track's macro, `fury`, and the four settings it turns.

### Lanes

A chain has one lane, A, until a send adds another, and up to four: A, B, C and D. Lane A
starts from the strip's input; the others start from silence and hear only what's sent into
them. Each lane runs its rows top to bottom, and the lanes' outs are summed into what leaves the
chain. A chain of one lane with no sends and its out at 0 dB is a plain list of effects, which
is how most chains stay, and how the song file writes them.

The heading over each lane says what feeds it: `A ← in` for the strip's input, `B ← A` for a
lane that lane A sends to, and a lane fed back into lists that too (`A ← in B`).

### Sends between lanes

`:fx-send LANE [DB]` adds a send row after the cursor's row, into lane `LANE` (a letter),
at `DB` decibels. Naming the letter after the last lane makes a new lane: with only lane A,
`:fx-send B` starts lane B. A chain has at most eight sends, over all its lanes.

Which way a send goes matters:

- **Into a lane to its right**, a send feeds that lane at once, since that lane hasn't run yet.
  Its level can go from -60 to +6 dB, and is 0 dB unless you give one. This is parallel
  processing: the same sound treated two ways and mixed back, as the `troll` track's echoes are.
- **Into its own lane, or one to its left**, a send is **feedback**, drawn `↺`: the sound goes
  back up to the top of that lane and round again. It's heard 256 samples later (about 5 ms),
  whatever the audio's buffer size, and its level is kept to 0 dB or below (-6 dB unless you
  give one), so each trip round is quieter than the last and the loop dies away. Even so, a
  feedback loop through a resonant filter or a long reverb can build up; past a certain point
  Lattice clips it, so it can't run away to infinity, but it can still get loud. Start low.

Feedback is how echoes and resonances are made; with an effect in the loop, each trip round
changes the sound again. The `shimmer` chain below is the classic example.

### Keys

The fx page's cursor is on a lane and a line, as the pattern's is on a track and a step:

| Key           | Action                              | On the fx page                             |
|---------------|-------------------------------------|--------------------------------------------|
| `j` `k`       | `down`, `up`                        | line by line: every setting, send and out  |
| `h` `l`       | `left`, `right`                     | lane to lane; left of lane A, the macros   |
| `w` `b`       | `next_note`, `prev_note`            | effects, sends and the out, past settings  |
| `gg` `ge`     | `first_step`, `last_step`           | the top line, the outs                     |
| `(` `)`       | `rotate_backward`, `rotate_forward` | the previous / next strip                  |
| `^a`, `+` `=` | `note_up`, `longer`                 | turn what's under the cursor up            |
| `^x`, `-`     | `note_down`, `shorter`              | turn it down                               |
| `c`           | `change`                            | `:set ` on a value; else `:fx-add `        |
| `o`           | `open_below`                        | `:fx-add `, for an effect after the cursor |
| `d`           | `delete`                            | remove an effect (on its name), send, lane |
| `L`           | `lock_setting`                      | lock settings, as on the mixer             |
| `gm` `gp`     | `goto_mixer`, `goto_pattern`        | the mixer, the pattern                     |

What the cursor's on can be a setting, a send, an out, a macro or a macro's depth. `d` removes
an effect when it's on the effect's name, a send, a macro or a macro's target; on an out, it
removes the whole lane.

Settings go through the mixer's cursor, which the fx page keeps on the effect and setting
under its own, so `:set`, `:map`, nudges, `L` and recorded locks all work just as on the mixer.
Sends and outs nudge by 0.5 dB.

`d` on a setting says `d on the effect's name removes it`, as a reminder that the whole effect
would go. `d` on an out removes the lane, with its effects and every send into it; lane A takes
the chain's input, so it stays (`lane A takes the chain's input, so it stays`).

### Building a branching chain

Starting on a track with nothing on it, open its effects and add a chorus, then branch off an
echo that's heard alongside it, and feed the echoes back round the chorus:

```
gf                  the fx page, on the cursor's track
:fx-add ensemble    lane A: the ensemble
:fx-send B -6       a send after it, starting lane B: "A sends into B at -6 dB"
l                   over into lane B, on the same line
:fx-add delay       lane B: a delay, fed by the send
jjjj                down to the delay's mix
:set 1              all echo, since the dry sound is in lane A
ge                  down to the outs
:fx-send A          at the end of lane B: "B feeds back into A at -6 dB"
```

`:fx-add` puts the new effect after the cursor's row, or at the end of the lane if the cursor is
below the lane's last row, and `:fx-send` the same. `o` starts typing one, as it opens a step
below the cursor on the grid. The result:

```
      macros                            A ← in B                 B ← A
                                        ensemble ──────────────  delay ─────────────────
                                         rate         0.6Hz       time         0.375s
                                         depth         0.5        feedback      0.45
                                         mix           0.6▎       tone         4000Hz
                                        → B  -6.0 dB              mix            1
                                        │                        ↺ A  -6.0 dB
 out                                             0dB        ▉             0dB        ▉
```

Each echo now goes round the chorus again, wavering a little more each time, and is heard both
through lane A's out and lane B's.

### Saved chains

A chain worth keeping can be saved on its own, to put on any strip of any song:

- `:chain-save NAME` saves the strip's chain (on the pattern page, the cursor's track's) as
  `~/.config/lattice/chains/NAME.toml`, with its lanes and sends;
- `:chain-load NAME` puts a saved chain on the strip, in place of its own. The old effects'
  locks and macro targets go with them; the instrument's stay. It's one undo.

A name is letters, digits, `-` and `_`. `tab` after `:chain-load ` lists the chains there are.
An empty chain has nothing to save, and a chain Lattice comes with can't be saved over: save it
under another name.

Lattice comes with one chain, `shimmer`, the "shimmer reverb" Brian Eno and Daniel Lanois made
famous on their ambient records of the 1980s: a long reverb whose sound is sent round again an
octave up, so each trip adds a higher octave, rising and fading like a choir. Lane A is the
reverb; it sends everything into lane B, which takes off the lows with a high-pass filter at
400 Hz, shifts the rest up an octave with `pitch`, and feeds it back into the reverb at -4 dB,
with a little of lane B heard as it is:

```
      macros                            A ← in B                 B ← A
                                        reverb ────────────────  filter ────────────────
                                         size          0.85  ▎    type          high
                                         decay         0.85  ▍    cutoff       400Hz
 ...
                                        → B  +0.0 dB             pitch ─────────────────
                                        │                         pitch         12st
 ...
                                        │                        ↺ A  -4.0 dB
 out                                             0dB        ▉             -9dB    ▌
```

Put it on a slow pad and try the feedback send's level, the reverb's `decay`, or `pitch` at 7
for rising fifths.

## The effects

Every effect works on any track or bus, in stereo. Each setting has a range, a default and a
unit; the tables below give them. Most effects end with a `mix`, from 0 (only the sound going
in) to 1 (only the effect's): in between, the two are crossfaded. On the send bus, set it to 1.

### Delay, an echo (`delay`)

An echo: the sound is written into a memory and played back a moment later, and some of what
comes back is fed in again, so the repeats go on, each quieter than the last. Each side has its
own line, so a stereo sound echoes in stereo.

Echoes were first made with tape: a loop of tape passing a record head and then one or more
play heads, as in the Echoplex of the early 1960s and Roland's RE-201 Space Echo (1974), the
sound of dub reggae. Each pass through tape lost some treble, so the repeats grew darker as
they faded.
Lattice's delay does the same, with a low-pass filter in the feedback, so the echoes recede
rather than ringing on bright.

| Setting    | Range           | Default | What it does                                         |
|------------|-----------------|---------|------------------------------------------------------|
| `time`     | 0.01 to 2 s     | 0.375 s | how long until each repeat                           |
| `feedback` | 0 to 0.95       | 0.45    | how much of each repeat comes round again            |
| `tone`     | 200 to 16000 Hz | 4000 Hz | the low-pass in the feedback: lower, darker repeats  |
| `mix`      | 0 to 1          | 0.3     | how much of the echo you hear, against the dry sound |

`time` is in seconds, not beats, so work it out from the tempo: a beat lasts 60 ÷ BPM seconds.
At 120 BPM a beat is 0.5 s, an eighth note 0.25 s, and a **dotted eighth** (three sixteenths)
0.375 s, the default: the echo of U2's guitar and of many a dub stab, landing between the
beats so the repeats make a rhythm of their own. At 130 BPM a dotted eighth is 0.346 s.
Very short times (10 to 30 ms) with a little feedback give a metallic, comb-like ring; around
80 to 120 ms with no feedback, a "slapback", the single quick echo of 1950s rock and roll
records.

`feedback` is the share of each repeat fed back: at 0.5 each is half as loud as the one before
(6 dB down), at 0.95 they go on for a long time. It stops short of 1, where the echoes would
never fade.

### Reverb, a plate (`reverb`)

A reverb makes the sound of a space: the thousands of reflections, blurring into a wash, that
follow any sound in a room or hall. Lattice's is a **plate**. In 1957 the German company EMT
made the EMT 140: a large, thin sheet of steel, about one metre by two, hung on springs in a
wooden frame, with a driver like a loudspeaker's to set it vibrating and pickups to hear it. Its
sound is bright, dense and smooth, with no echoes you can pick out, and it became the reverb on
countless records of the 1960s and 70s.

The digital plate here follows Jon Dattorro's design ("Effect Design, Part 1", 1997): the sound
is spread out by four all-pass filters, which smear it in time without colouring it, then fed
into a "tank" of two halves that feed each other round a loop, each losing some treble as it
goes. The output is tapped from seven points in the tank, differently for each side, which is
what makes it wide.

| Setting    | Range           | Default | What it does                                           |
|------------|-----------------|---------|--------------------------------------------------------|
| `size`     | 0 to 1          | 0.5     | how big the plate is: from 0.4 to 1.6 times Dattorro's |
| `decay`    | 0 to 0.98       | 0.6     | how long it rings: how much survives each trip round   |
| `tone`     | 500 to 16000 Hz | 6000 Hz | how much treble survives each trip: lower is darker    |
| `predelay` | 0 to 0.25 s     | 0.02 s  | a gap before the reverb starts                         |
| `mix`      | 0 to 1          | 0.3     | how much reverb, against the dry sound                 |
| `lowcut`   | 20 to 1000 Hz   | 20 Hz   | cuts the lows going in, so they don't boom             |

- `size` spaces the reflections out: small is a tight, close room; large spreads them into a
  hall. Changing it never clicks or stops the sound.
- `decay` stops at 0.98, short of where the tank would ring for ever.
- `tone` is the damping: real rooms soak up treble faster than bass, so a darker `tone` sounds
  bigger and further away, and sits behind the music rather than over it.
- `predelay` keeps the front of each note clear of the reverb, so a vocal or a snare stays
  crisp and close while the space opens out behind it. 20 to 40 ms is a natural room.
- `lowcut` high-passes only what goes into the plate, leaving the dry sound alone. A kick or a
  timpani sent into a long plate piles up a rumbling boom; 150 to 300 Hz keeps the tail clean.

On the send bus, set `mix` to 1 and set each track's `send` for how far back it stands.

### Filter, a sweeping filter (`filter`)

A filter takes away part of the sound's spectrum. A **low-pass** lets the lows through and cuts
the highs above its **cutoff**, from bright to muffled; a **high-pass** does the opposite,
thinning a sound from below; a **band-pass** keeps only a band around the cutoff, like a
telephone or a small radio. **Resonance** boosts the frequencies right at the cutoff, so the
filter rings with a whistling, vocal edge: the "squelch" of an acid bassline.

This one is a resonant state-variable filter, the same as the synth's (a "TPT" filter, after
Vadim Zavalishin's topology-preserving designs), with its own **LFO**, a low-frequency
oscillator: a slow sine wave that sweeps the cutoff up and down by itself.

| Setting     | Range                 | Default | What it does                                    |
|-------------|-----------------------|---------|-------------------------------------------------|
| `type`      | `low`, `band`, `high` | `low`   | low-pass, band-pass or high-pass                |
| `cutoff`    | 20 to 20000 Hz        | 2000 Hz | where the filter starts cutting                 |
| `resonance` | 0 to 0.95             | 0.2     | how much it rings at the cutoff                 |
| `rate`      | 0.02 to 20 Hz         | 0.25 Hz | how fast the LFO sweeps                         |
| `sweep`     | 0 to 4 oct            | 0 oct   | octaves it sweeps either side of `cutoff`       |
| `mix`       | 0 to 1                | 1       | how much of the filtered sound, against the dry |

With `sweep` at 0 the LFO does nothing, and the filter holds still. The cutoff glides evenly in
octaves, so a sweep sounds even all the way. The band-pass is kept at full level at its centre
whatever the resonance.

The LFO isn't tied to the tempo, so set `rate` from it: one sweep every four bars of 4/4 at 120
BPM (8 seconds) is 0.125 Hz, a slow acid rise and fall. Fast rates, from 4 Hz up, make a
wobble; a band-pass swept fast is an auto-wah. For a sweep that follows the song exactly, lock
`cutoff` on the steps instead (see [locking settings](#locking-settings-on-steps)).

### Ensemble, the string machine's chorus (`ensemble`)

In the 1970s, string machines such as the ARP Solina String Ensemble made a whole string
section from one simple oscillator per key. The secret was the **ensemble** circuit: three
copies of the sound, each delayed a few milliseconds by an analogue "bucket brigade" chip, and
each delay wavered by two LFOs, a slow one and a fast one, a third of a cycle apart. The copies
drift in and out of tune with each other as real players do, and one thin tone becomes a lush,
shimmering section. It's a chorus, but a richer one.

Lattice's reads the sound back with three heads about 7 ms behind it, each swung by a slow LFO
(up to 3 ms) and a fast one nine times quicker (up to 0.4 ms), the "shimmer". The right side's
heads sit a sixth of a cycle from the left's, so the two sides waver differently and even a mono
sound comes out wide.

| Setting | Range       | Default | What it does                                                |
|---------|-------------|---------|-------------------------------------------------------------|
| `rate`  | 0.1 to 3 Hz | 0.6 Hz  | how fast the slow LFO goes; the fast one is nine times this |
| `depth` | 0 to 1      | 0.5     | how far the copies waver                                    |
| `mix`   | 0 to 1      | 0.6     | how much of the copies, against the dry sound               |

Around the default it's a string machine. Slower and deeper, it seasicks; faster, it shivers.
Put it on Krell or a pad from the synth, or on a monochord's plucked strings to make a twelve-
string of them. It needs no send: it's an insert.

### Phaser, a sweep of notches (`phaser`)

A phaser runs the sound through a chain of **all-pass** stages, which leave every frequency at
the same level but delay some more than others, turning their phase. Mixed back with the dry
sound, the frequencies the chain has turned exactly half a cycle cancel out, cutting a notch,
one for each pair of stages. An LFO sweeps the stages, and the notches sweep up and down through
the sound: the swooshing "jet" of 1970s guitars, electric pianos and string machines.

The Uni-Vibe (Shin-ei, 1960s), built to imitate a rotating Leslie speaker, swept four stages
with a flickering light bulb and light-sensitive cells; Jimi Hendrix played through one at
Woodstock. Pedals like MXR's Phase 90 followed. Lattice's sweeps its stages around 630 Hz, by
up to three octaves either way, and its right side's LFO runs a quarter of a cycle ahead of
the left's, so the sweep moves across the stereo field too.

| Setting    | Range               | Default | What it does                                      |
|------------|---------------------|---------|---------------------------------------------------|
| `stages`   | `4`, `6`, `8`, `12` | `4`     | how many all-pass stages: each pair makes a notch |
| `rate`     | 0.02 to 10 Hz       | 0.3 Hz  | how fast the LFO sweeps                           |
| `depth`    | 0 to 1              | 0.7     | how far it sweeps, up to three octaves either way |
| `feedback` | 0 to 0.9            | 0.5     | how much goes back round, sharpening the notches  |
| `mix`      | 0 to 1              | 0.5     | the blend; at 0.5 the notches are deepest         |

Four stages, as in the Uni-Vibe and Phase 90, give two gentle notches; twelve give six, a
denser, more metallic sweep. Raising `feedback` turns the notches into ringing peaks between
them, more vocal and more obvious. At `mix` 1 you hear only the phase-shifted sound, which on
its own loses most of the effect: the notches need the dry sound to cancel against.

### Drive, a saturator (`drive`)

Push a tape machine, a valve amplifier or a mixing desk harder than it's built for and it stops
growing louder and starts rounding off the tops of the wave instead. That rounding adds
harmonics, new frequencies above the notes, which make a sound thicker, warmer and louder-
seeming, up to a fuzz when pushed hard. Valves add even harmonics as well as odd ones, as
their curve isn't quite symmetrical, which is part of why they sound "warm".

Lattice's drive is a **soft clipper**: the sound is turned up by up to 36 dB into a `tanh`
curve, which rounds peaks off smoothly, pushed a little off centre so it makes even harmonics
too. Then a low-pass, `tone`, takes off the fizz that clipping adds at the top. It's scaled so
that driving harder changes the sound without making it much louder: a full-scale input
still peaks at full scale.

| Setting | Range           | Default | What it does                                    |
|---------|-----------------|---------|-------------------------------------------------|
| `drive` | 0 to 1          | 0.3     | how hard it's pushed, from clean to crushed     |
| `tone`  | 500 to 16000 Hz | 8000 Hz | a low-pass after the clipping, to tame the fizz |
| `mix`   | 0 to 1          | 1       | how much of the driven sound, against the clean |
| `level` | -24 to 6 dB     | 0 dB    | the level out, after the mix                    |

`drive` turns the gain up exponentially, so its travel feels even. A little (0.1 to 0.3)
thickens a bass or a drum bus; most of the way is a distorted lead. Mixing it in under the
clean sound (`mix` around 0.4 to 0.6) keeps the attack of drums while adding weight, which is
what the demo's drum bus does. It isn't oversampled yet, so pushed hard on bright sounds it
adds some harsh aliasing; `tone` helps.

### Comp, a compressor (`comp`)

A compressor turns a sound down when it's loud, so the loud parts and the quiet parts come
closer together. Above its **threshold**, every few decibels more going in gives only one more
coming out: at a **ratio** of 4, a peak 8 dB over the threshold comes out only 2 dB over. The
**attack** is how quickly it clamps down when the sound gets loud, and the **release** how
quickly it lets go. **Makeup** gain then brings the whole, now steadier, sound back up.

Compressors are among the most characterful tools in a studio. The Teletronix LA-2A, a valve
design of the 1960s, reads the level with a glowing panel and a light-sensitive cell, so it
reacts slowly and smoothly, a favourite on voices and bass. Bill Putnam's Universal Audio 1176
(1967) used transistors to react in well under a millisecond, catching and flattening drums.
Lattice's is a clean, modern one that can do either, from a 0.1 ms attack to a slow 100 ms.

| Setting     | Range           | Default | What it does                                        |
|-------------|-----------------|---------|-----------------------------------------------------|
| `threshold` | -48 to 0 dB     | -18 dB  | the level above which it starts turning down        |
| `ratio`     | 1 to 20         | 4       | how hard: dB in over the threshold for each dB out  |
| `attack`    | 0.0001 to 0.1 s | 0.005 s | how fast it clamps down on a loud sound             |
| `release`   | 0.01 to 1 s     | 0.15 s  | how fast it lets go when the sound drops            |
| `makeup`    | 0 to 24 dB      | 0 dB    | gain after compressing, to make up what it took off |
| `mix`       | 0 to 1          | 1       | below 1, blends the uncompressed sound back in      |

It eases in over 6 dB around the threshold (a "soft knee"), so it never switches on with a
jolt. Both sides share one gain, worked out from whichever is louder at each moment, so the
stereo image stays put. A ratio of 2 to 4 is gentle levelling; 10 and up is limiting, a ceiling.

- A slower `attack` (10 to 30 ms) lets each drum hit's front through before it clamps, making
  the drums punchier; a very fast one flattens them.
- A short `release` makes it pump, breathing with the beat; a long one levels smoothly.
- `mix` below 1 is **parallel compression**, the "New York" trick: crush a copy hard and blend
  it under the original, so the quiet details come up while the hits keep their snap.

### Pitch, a pitch shifter (`pitch`)

The pitch shifter plays the sound back higher or lower, without changing its speed. It's the
classic delay-line kind, as in the first studio "harmonizers" of the 1970s: the sound is written
into a short memory and read back by two heads, each sweeping across a short window faster
(for higher) or slower (for lower) than it's written. When a head reaches the end of its window
it jumps back, and so that the jump isn't heard each head fades out as it gets there, while the
other, half a window behind, fades in.

| Setting  | Range         | Default | What it does                                       |
|----------|---------------|---------|----------------------------------------------------|
| `pitch`  | -24 to 24 st  | 12 st   | how far, in whole semitones: 12 is an octave up    |
| `fine`   | -50 to 50 ct  | 0 ct    | and in cents (hundredths of a semitone), to detune |
| `window` | 0.02 to 0.2 s | 0.08 s  | each head's sweep: shorter is tighter but buzzier  |
| `mix`    | 0 to 1        | 1       | how much of the shifted sound, against the dry     |

Being made of overlapping slices, it smears sharp attacks a little, which suits pads, reverb
tails and the octave in a shimmer's feedback better than drums. A short `window` follows the
sound closely but buzzes on low notes; a long one is smoother but smeared. `pitch` 7 adds a
fifth above, and -12 an octave below; a `fine` of 5 to 10 cents at `mix` 0.5, with `pitch` at 0,
is a thickening detune.

## Locking settings on steps

Any setting of a track's instrument or effects, and any of its macros, can be **locked** on a
step: from that step the setting takes the locked value, until the setting's next lock. This is
how settings change in time with the music, as on Elektron's drum machines, whose "parameter
locks" this follows, and as in a tracker's effect columns. A locked filter can open on the
fourth hit of every bar; a locked reverb `mix` can swallow one snare and leave the rest dry.

Locks live on the pattern, in up to four lock columns for each track, and the [pattern
chapter](02-patterns.md) shows how to read and edit them there. This section is about making
them from the mixer, by ear.

### L: lock what you change

To lock settings by ear, without typing values:

1. On the pattern page, select the steps the settings should change on (`v` and a motion, or
   `x`).
2. `gm` (or `gf`) for the mixer, on the track. Press `L`. The status line shows
   `◆ locking troll: change settings · L lock them on the selected steps · esc cancel`.
3. Change any of the track's settings, instrument, effects or macros, and listen.
4. Press `L` again. Each setting you changed is locked, at its new value, onto every selected
   step, and the track's own settings go back to what they were. The status line says what was
   locked: `◆ drive.level 0.9dB on troll steps 0–3`.

The whole thing is one undo. Between the two presses you can move between the mixer, fx and
instrument pages; `esc`, or going to any other page, cancels: the settings go back and nothing
is locked. If nothing changed, `L` says `nothing changed, so nothing locked`; if the
track's instrument or chain changed in between, the settings no longer line up and nothing is
locked. A bus has no steps, so `L` there says `locks are on tracks' steps; buses have none`.

Each setting locked needs a column: a track has up to four, and `L` says if there's no room.

### Recording locks as you play

To perform settings instead: on the mixer or fx page, arm record with `R`
(`record armed: settings changed while the song plays are locked`), and play. Each setting you
change is locked on the step that's playing, and heard at once. Holding `^a` on a cutoff sweeps
it up a step at a time, each nudge going on from the last value recorded rather than the
setting's own. Each pass round the track is one undo, as with recorded notes, and the setting
itself is left alone, so `u` takes the whole sweep away. A change during the count-in does nothing.

On the mixer and fx pages, `R` doesn't start insert mode, as it does on the pattern page, since
there are no notes to play here. The instrument page records settings the same way, and notes too.

### :lock and :unlock

`:lock PARAM VALUE` locks a setting on every step the selections reach, typed exactly:

```
:lock cutoff 800            the instrument's cutoff, or the first effect with one
:lock filter.mix 0.5        an effect's setting, by the effect's name
:lock 2.mix 0.5             the second effect's mix, by its number from 1
:lock inst.decay 0.3        the instrument's, by name
:lock type high             a choice, by its name
:lock cutoff 200..4000      a sweep across each selection, evenly along the curve
:lock cutoff ~4000          a slide, arriving at 4000 on the step
:lock cutoff --             back to the mixer's value from here
:lock cutoff                no value: just give the track a column for it
:lock bloom 1               a macro, by its name (or macro.bloom)
```

A plain name is looked for on the instrument first, then on each effect in order, then among the
macros. `:unlock PARAM` clears that setting's locks from the selected steps, and `:unlock` alone
clears all of them; done along a whole track, the setting's column goes too.

While a lock holds a setting during playback, the mixer and fx pages show the locked value, in
the accent colour and marked `◆`, in place of the track's own, so the knobs move with the
pattern. Turning the setting by hand meanwhile changes the track's own value, which comes back
when the lock ends.

Locks follow their effects: remove an effect and its locks go; add one before it and they move
along with it. Loading a chain with `:chain-load` replaces the effects, and their locks go with
them.

## Macros

A **macro** is one knob on a track that turns several of its settings at once, each by its own
amount, up or down. A macro called `dawn` might open a filter's cutoff, lengthen a reverb's
decay and turn the drive down, all with one turn, so a sound can move from one character to
another as a whole. Ableton Live's racks have macro knobs that work the same way; in Lattice
they're also what a preset puts forward as its controls (see
[presets](07-instruments.md)).

A track has up to eight macros, and each turns up to eight settings, of its instrument or its
effects. A macro goes from 0 to 1. Each setting it turns, a **target**, has a **depth**, from -1
to 1: how far the macro moves it, as a share of the setting's range, when the macro is all the
way up. A depth of 0.5 moves the setting half its range; -1 moves it all the way down.

A setting keeps its own value, and the macros move it from there, along its curve, stopping at
the ends of its range. Turning the setting by hand still works, and a macro back at 0 puts the
setting back exactly. Where several macros turn one setting, their moves add up.

### Making one

- `:macro NAME` adds a macro with nothing to turn yet. A name is letters, digits, `-` and `_`,
  not starting with a digit (`a macro's name is letters, digits, - and _: :macro bloom`), and
  each name is used once on a track.
- `:map NAME [DEPTH]`, on a setting on the mixer, fx or instrument page, has macro `NAME` turn
  it, by `DEPTH` (0.5 if left out). If there's no macro of that name yet it's added. If it already
  turns the setting, the depth changes. A macro can also be named by its number from 1 (`:map 1`).
  The status line confirms: `dawn turns cutoff by +0.50`.
- `:unmap NAME`, on a setting, stops the macro turning it.

For example, on a synth track:

```
gm                  the mixer, on the track
jj                  down past the mix to the chain
hh                  along it to the synth (the instrument comes first)
j jll               onto its first setting, wave; down a row and along to cutoff
:map dawn 0.6       "dawn turns cutoff by +0.60": dawn opens it by 0.6 of its range
jh                  down and back to resonance
:map dawn -0.2      and takes some resonance away as it opens
```

With `dawn` at 0 the sound is as it was; at 0.5 the cutoff has opened 0.3 of its range; at 1,
0.6 of it, and the resonance is down 0.2. Both are marked as `dawn`'s, macro 1:

```
 wave            saw          attack         0.005s        decay           0.2s
 sustain         0.5          release         0.1s        1 cutoff        1000Hz
 env            3000Hz       1 resonance     ▍0.3          gain         ▊  0.2
```

Settings a macro turns are marked with its number on the mixer and fx pages (`1cutoff`), or
`*` if more than one turns them.

### Macros on the fx page

On a track's fx page the macros are a column left of lane A: each macro with its value as a bar,
then each setting it turns with its depth. `h` from lane A goes there and `l` back. On a macro,
`^a` `^x` turn it, by a hundredth at a time; on a target, they change its depth. `c` types a
value, and `d` removes the macro or the target. While the cursor is on a target, the setting it
turns is lit in its lane. Here `50^a` on `dawn` has turned it half way:

```
      macros                            A ← in B                 B ← A
      1 dawn                0.5         ensemble ──────────────  delay ─────────────────
       cutoff              +0.60  ▎      rate         0.6Hz       time         0.375s
       resonance           -0.20         depth         0.5        feedback      0.45
```

With no macros, the page says
`no macros yet: :map NAME on a setting has macro NAME turn it`.

A macro locks like any setting, by its name (`:lock dawn 1`), with `L`, or by turning it with
record armed, so a sound can change character on a step. Macros are saved with the track and in
presets; removing an effect removes its targets, and changing the instrument removes the
instrument's.
