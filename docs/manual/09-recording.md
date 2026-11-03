# Recording, the looper and MIDI

Most of a pattern is written a step at a time, but some parts are better played. This chapter
covers the ways of getting a performance into Lattice: recording notes live as the song plays,
the numpad's performance pads, the looper that records audio from your interface, the audio
devices it all goes through, and what there is (and isn't yet) for MIDI and foot pedals.

## Recording notes live

### Arming record

`R` (`toggle_record`), or `:rec`, arms recording and starts insert mode, ready to play: a red
`● REC` appears in the header and the status line says
`record armed: play notes while the song plays`. `R` again, or `:rec`, turns it off
(`record off`). `R` works in insert mode too, so you
can turn record on and off without leaving it.

Then start the song with `space` (`toggle_play`), `^space` (`play_song`) or `A-space`
(`play_from_here`). While the song plays and you're in insert mode, every note you play, on the
piano keys or the pads, goes in at the **playhead** rather than at the cursor:

- on the cursor's track, or on each selection's track if there are several selections, so one
  performance can be entered on two tracks at once;
- on the step nearest to when you played it;
- over whatever note was on that step; empty steps you play nothing on are left as they are.

With record on but the song stopped, insert mode works as usual, entering notes at the cursor
and moving on. With the song playing but record off, the notes you play are heard and not
entered.

```
 LATTICE  PATTERN   ▶ PLAY  ● REC    mountain-king.lattice   pat cave   bpm 138
```

### The count-in

With record armed, playing starts with a **count-in**: a bar of clicks, accenting its first beat,
before the song's first step, so you can come in on the one. The header counts down the beats
left, `◆ IN 4` … `◆ IN 1`. The clicks sound whether or not the metronome (`:click`) is on.

`:count BARS` sets how many bars to count in, from 0 (none) to 4; it starts at 1. The count-in
follows the song's meter, so in 3/4 it's three clicks a bar.

A note played in the last half-step of the count-in goes on the song's first step and is played
there, since people tend to come in a fraction early; one played earlier than that is heard but
not entered.

### Where a note lands

A recorded note goes on the **nearest** step, which is what other sequencers call quantising:
rounding each note to the grid. A note played just after a step starts rounds back onto it; one
played just before the next step rounds on to that one.

Because a step that has already played can't be played again, Lattice treats the two cases
differently:

- rounded **back**, onto the step that's sounding, the note is heard the moment you play it, as
  you'd expect;
- rounded **on**, to the next step, it isn't sounded now; the sequencer plays it on that step,
  in time, as though it had always been there.

So an early note sounds tidied up, and a late one sounds as you played it, but both go in on the
grid. Lattice works out how far into a step you played from the moment it saw the engine
move on to that step, timed against the step's length.

The grid you record onto is the track's steps, so for finer timing make the steps shorter:
`:meter 4 8` gives eight steps a beat (see [the song](06-song.md)). Tracks of different lengths
each take the step from their own playhead, so a seven-step track records correctly against a
sixteen-step one.

### How long notes are

In a terminal that reports keys being let go (kitty, WezTerm, foot, Ghostty and recent
Alacritty, through the kitty keyboard protocol), a recorded note lasts as long as you held the
key, rounded to whole steps: at least one, at most the track's length. Held keys don't repeat
while recording, so a long note goes in once. Elsewhere, including inside tmux, Lattice can't
tell when a key comes up, and every recorded note is one step long. A MIDI keyboard would avoid
this altogether; see [MIDI and controllers](#midi-and-controllers).

### Takes and undo

Each pass round the track is a **take**, and each take is one undo. If the third pass goes
wrong, `u` takes out just that pass's notes, and the passes before it stay. Stopping the song, or
turning record off and on again, starts a new take too. Recording over a pattern again and
again, keeping what's good and undoing what isn't, is how a part is built up: an overdub, in
studio terms.

### Keys while recording

| Key        | Action                 | While recording                        |
|------------|------------------------|----------------------------------------|
| `R`        | `toggle_record`        | record on / off                        |
| `space`    | `toggle_play`          | play / stop                            |
| `^space`   | `play_song`            | play the song from its start           |
| `A-space`  | `play_from_here`       | play the song from this pattern        |
| `{`        | `keyboard_octave_down` | the keyboard an octave down            |
| `}`        | `keyboard_octave_up`   | the keyboard an octave up              |
| `⌫`, `del` | `clear_advance`        | (stopped) clear a step and move on     |
| `esc`      | `escape`               | back to normal mode: keys stop playing |
| `^k`       | `hush`                 | stop everything at once                |
| piano keys |                        | play and record notes                  |
| numpad     |                        | play and record the pads               |

`:vel N` sets how hard the notes are played and entered (1 to 127), and the numpad's `/` `*` `-`
pick soft, medium or hard; the header shows it as `vel`. `:layout` changes how the notes lie on
the keyboard; see [the pattern chapter](02-patterns.md).

### Recording settings

On the mixer page, `R` arms record without starting insert mode, and each setting you change
while the song plays is locked on the step that's playing; on the instrument page, `i` plays the
track live and `R` records both its notes and the settings you turn. Both are described with
locks, in [the mixer chapter](08-mixer-and-effects.md#recording-locks-as-you-play).

## The pads

The **numpad** is a set of performance pads, in any mode, as on a drum machine. It needs a
terminal that tells the numpad's keys apart from the number row (kitty, WezTerm, foot, Ghostty,
recent Alacritty).

| Pad         | What it does                                                     |
|-------------|------------------------------------------------------------------|
| `1`–`9`     | in a group, play its drums; elsewhere, the notes of a scale      |
| `0` (held)  | note repeat: pads played while it's down repeat in time          |
| `.`         | how often pads repeat: every 16th, 8th or quarter                |
| `/` `*` `-` | play and enter notes soft, medium or hard: velocity 60, 100, 127 |
| `+`         | mute mode: pads `1`–`9` mute and unmute the first nine tracks    |

### Drums and scales

With the cursor on a track in a group, pad `1` plays the group's first track at C-4, `2` its
second, and so on, as many as the group has tracks. Since a drum plays at its own tuning at C-4,
that's a drum to a pad, the way a folded drum group is laid out (see [the tracks
chapter](03-tracks.md)), so `1` is the kick and `2` the snare in a kit laid out in that order.

On any other track, pads `1`–`9` play the notes of a **scale** up from the keyboard octave, `1`
the scale's root. `:scale NAME [ROOT]` chooses it:

| Scale              | Steps, in semitones from the root |
|--------------------|-----------------------------------|
| `major`            | 0 2 4 5 7 9 11                    |
| `minor`            | 0 2 3 5 7 8 10                    |
| `dorian`           | 0 2 3 5 7 9 10                    |
| `mixolydian`       | 0 2 4 5 7 9 10                    |
| `pentatonic`       | 0 2 4 7 9                         |
| `minor-pentatonic` | 0 3 5 7 10                        |
| `blues`            | 0 3 5 6 7 10                      |
| `chromatic`        | every semitone                    |

The root is a note name, `C`, `F#` or `Bb`, in either case (`:scale minor A`); left out, the
root stays as it was. It starts at C major. Past the scale's last note the pads go on into the
next octave, so in C major pad `8` is the C above pad `1`. The status line confirms:
`pads play A minor-pentatonic`. A scale is a set of notes that sound right together; with the pads
on one,
there are no wrong notes, which makes them good for finding a melody. `:layout scale` lays the
computer keyboard out on the same scale.

In insert mode a pad also enters its note on the cursor's step, without moving on, so one step
can take several drums: `1` then `3` puts a kick and a hat on the same row. With record armed
and the song playing, pads record at the playhead as piano keys do.

### Note repeat

Hold pad `0` and the pads you play repeat in time, for as long as you hold them, as on Akai's
MPC drum machines: hold `0` and `3` for a run of hats, and with record armed each hit is
recorded. The header shows `REPEAT 1/16`. `.` cycles how often: every step, every two steps or
every four, shown as the note value they make (with four steps a beat, a 16th, an 8th or a
quarter). Note repeat needs a terminal that reports keys coming up; elsewhere `0` says so.

### Mute mode

`+` switches the pads to **mute mode** (`MUTE PADS` in the header, `pads mute tracks 1–9`):
pads `1`–`9` mute and unmute the first nine tracks, for dropping parts in and out live. A mute
made this way takes over from one the conductor made (see [the song](06-song.md)), and is one
undo. `+` again and the pads play (`pads play`).

### Without a numpad

Inside tmux, and in terminals that don't tell the numpad apart, its keys arrive as the number
row's. There, while the cursor is in a **folded** group in insert mode, the number row's `1`–`9`
stand in for the pads; anywhere else they're piano keys. `:vel` sets the velocity in place of
`/` `*` `-`.

## The looper

A **looper** records what you play into a loop, plays it back straight away, and lets you play
over it, layer on layer. It goes back to tape: in the 1970s Robert Fripp and Brian Eno ran one
tape between two tape recorders, so whatever was played came round again seconds later, to be
played over. Guitarists now stand on loop pedals that do the same. Lattice's looper records your
audio interface's input, a guitar, a voice, a synth, into a chop track, in time with the song.

### Arming a track

`:arm` arms the cursor's track to record. Then turn record on with `R` and play the song. The
header shows `ARMED TROLL` (the track's name) while it waits, and the track's header a `○`:

```
 LATTICE  PATTERN   ■ STOP  ARMED TROLL    mountain-king.lattice   pat cave   bpm 138
```

| Command        | What it does                                                         |
|----------------|----------------------------------------------------------------------|
| `:arm`         | arms the track, to record pass after pass until you stop             |
| `:arm PASSES`  | arms it for that many passes (1 to 99), then disarms it              |
| `:arm!`        | as `:arm`, but starts a new loop rather than recording over this one |
| `:arm! PASSES` | a new loop, for that many passes                                     |
| `:disarm`      | disarms the track (`troll disarmed`), or says `nothing's armed`      |

Only one track is armed at a time; arming another moves the arm there. Arming the same track
again doesn't disarm it, since that would be easy to do by mistake: it says what it's armed for.
`:arm` says what will happen:
`troll armed, a new loop for 2 passes: record on (R) and play to record`.

### Recording passes

With a track armed, record on and the song playing, the looper records the input **a pass of
the track at a time**. A pass starts when the armed track's playhead comes round to its first
step, `00`, and ends when it comes round again: so a 16-step track at four steps a beat records
a bar a pass, and a 64-step one four bars. While it records, the header shows `● LOOP TROLL`
and the track's `○` fills in to `●`.

Each pass, as it ends:

- is saved as a new WAV file in the song's `samples` folder, named after the song,
  `mountain-king-take-1.wav`, `-take-2.wav` and so on, taking the first number that's free
  (before the song is saved, in a scratch folder, copied beside the song by `:w`);
- goes onto the track: the first pass makes the track a `chop` track playing the file, cut
  into a piece for each step, so it plays back exactly as recorded (see
  [chop](07-instruments.md#chop-chop));
- plays back on the very next pass, while the next pass records.

The passes record back to back, with no gap, so nothing you play across the loop point is lost.
Since the recording is a pass of the track, measured in the track's own steps, the loop is
always exactly in time with the song, and goes on following the tempo, as any chop track does.
Room is kept for a pass and a tenth more, in case the tempo slows while it records.

### Overdubbing

Each pass after the first is recorded **over** the last: the new pass is mixed onto the loop
the track was playing and saved as a new file, so the loop builds up layer by layer, as on a
loop pedal. Each layer is one undo: `u` takes the last layer off, putting the track back on the
file before it, and the files stay in `samples`.

Recording goes on, a layer a pass, until you stop the song or turn record off, or for as many
passes as `:arm PASSES` said (`troll: pass overdubbed, and disarmed`). `:arm 1` records exactly
one pass, which is the way to record a loop once and then play over it without adding to it.

A pass records over the loop when the armed track is a chop track with notes on it. It starts a
**new loop** instead, filling the track's steps again, when:

- the track isn't a chop track yet;
- it has no notes left: delete them all (`tt` then `d`) and the next pass starts afresh;
- it was armed with `:arm!`, for its first pass.

Because the loop is a chop track, everything chop can do works on it: rules such as `:rot` and
`:rev` rearrange it each pass, `~` reverses steps, and gestures stutter and turn pieces round.

### Stopping

Turning record off (`R`) lets the pass being recorded finish, and then stops recording; the
song plays on. **Stopping** the song ends the pass where it is, and an unfinished pass is thrown
away, so a pass is always whole. `^k` (hush) stops everything at once, the same way.

### Hearing what you play

Lattice doesn't play the input back through its own output: doing so would add the whole
journey into the computer and out again to what you hear, late enough to put you off. Listen to
your instrument through your audio interface's own monitoring (its "direct monitor"), and to
Lattice through the same headphones.

## Audio devices

### :input and :output

`:input` chooses the device the looper records, and `:output` the device you hear Lattice
through. Type `:input ` (with its space) and the devices are listed in the corner, numbered, the
one in use marked `●`, and kept up to date as devices come and go:

```
:input 2              the second in the list
:input scarlett       the one whose name has "scarlett" in it, in any case
:input 0              the system's default (or :input default)
:input                says which is in use: "input: the default (Built-in Audio)"
```

A part of a name that matches several devices is refused
(`3 inputs match "hd"; type more, or its number`), unless it's the whole of one's name. While the
list is still being made, a number
says `still looking for inputs`.

`:output` switches devices while the song plays, without stopping: the new device starts at the
same sample rate, and if it fails, the old one is put back. Lattice opens the input when a track is
armed, so a new `:input` is opened the next time one is; if it can't be, the status line says
`can't record:` and why.

The choices belong to the computer, not the song, so they're remembered between runs in
`~/.config/lattice/audio.toml`, beside the config file, which Lattice writes for you:

```toml
# Written by Lattice: :input, :output and :latency.
input = "pulseaudio:alsa_input.usb-Focusrite_Scarlett-00.analog-stereo"
output = "pulseaudio:alsa_output.usb-Focusrite_Scarlett-00.analog-stereo"

[latency]
"pulseaudio:alsa_input.usb-Focusrite_Scarlett-00.analog-stereo" = -12
```

On Linux, Lattice goes through PulseAudio, which PipeWire also serves, so the devices have the
names your desktop gives them, falling back to ALSA where there's no PulseAudio; ALSA's own list
is dozens of cards, plug-ins and raw devices under names that don't say what they are. Elsewhere
it uses the system's own audio (CoreAudio on a Mac, WASAPI on Windows). A stereo input is
recorded in stereo; a mono one is heard in both sides.

### Latency

**Latency** is delay: the time between Lattice making a sound and you hearing it, and between
you playing a note and it reaching Lattice. Both come from buffers, small batches of sound the
computer fills ahead of time so that it never runs out. Lattice asks for small ones, 512
samples, wherever the device allows: through PulseAudio, about 28 ms on the way out and 16 ms on
the way in.

When you play along with a loop, what you hear is already late, and what you play reaches
Lattice later still. So the looper starts a pass not when the first step is played, but when it
was heard plus the input's own delay: the output's latency and the input's, as the audio system
reports them, plus any input waiting to be taken in. It measures this as the first pass starts
and keeps it while the passes follow on, since measured afresh each pass it wavers slightly, and
the passes would differ in length.

Audio systems don't always report their delays truthfully. If what you record sits late or early
against what you heard, `:latency MS` moves it: negative records it earlier, positive later,
from -500 to 500 milliseconds (`latency adjusted by -20 ms`). It's remembered for each input, in
`audio.toml`, and switching inputs says if it's adjusted (`(latency adjusted by -12 ms)`).

To find the right amount, record something sharp, such as a clap or a rim click on each beat
with the metronome, then compare the recorded hits with the steps they should land on; adjust,
and record again.

### When the audio can't keep up

Lattice renders sound in blocks, and each must be ready before the device needs it. If the
computer is too busy, a block is late and you hear a click or a gap. Lattice counts these
troubles and, once a second, writes any to its log, `~/.local/state/lattice/lattice.log`:
blocks that were late, blocks played as silence because the engine was busy, input that was
dropped because the engine wasn't taking it in time, and stream errors. At the end of a run it
writes how close the audio came to falling behind. Three late blocks in a second, or a device
failing, also go on the status line (`audio is struggling: 4 blocks late in the last second`).

The input waits in a buffer of about a second on its way to the engine; if that ever fills, the
frames that don't fit are dropped rather than holding the audio up, and the log counts them.

## MIDI and controllers

### Not yet

Lattice doesn't yet take MIDI input, from keyboards, pad controllers or foot pedals, or send
MIDI out. Both are planned. A MIDI keyboard would solve the terminal's problem with key
releases, giving every note its true length and velocity; and MIDI controllers and **foot
pedals** are to be able to start and stop the transport, turn record on and off, and start and
stop the looper, so that a player with their hands on an instrument can record a loop with
their foot, as on a loop pedal.

### Foot switches today

Many USB foot switches don't speak MIDI at all: they pretend to be a keyboard and type a key.
Those work with Lattice now. Set the pedal to type a key Lattice already knows, such as `R` for
record or `space` for play, or bind a key it can type to a command in the config file (see
[files and configuration](10-files-and-config.md)):

```toml
[keys.normal]
C-l = ":arm 1"        # a pedal sending Ctrl-L arms the track for one pass
```

`R` and `space` work in insert mode as well as normal mode, so a pedal on them works while you
play.
