# Recording, the looper and MIDI

Most of a pattern is written a step at a time, but some parts are better played. This chapter
covers the ways of getting a performance into Lattice: recording notes live as the song plays,
the numpad's performance pads, the looper that records audio from your interface, the audio
devices it all goes through, and MIDI keyboards and foot pedals.

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

With record armed, or the loop pedal starting the song, playing starts with a **count-in**: a
bar of clicks, accenting its first beat, before the song's first step, so you can come in on the
one. The header counts down the beats
left, `◆ IN 4` … `◆ IN 1`. The clicks sound whether or not the metronome (`:click`) is on.

`:count BARS` sets how many bars to count in, from 0 (none) to 4; it starts at 1. The count-in
follows the song's meter, so in 3/4 it's three clicks a bar.

A note played in the last half-step of the count-in goes on the song's first step and is played
there, since people tend to come in a fraction early; one played earlier than that is heard but
not entered.

### Where a note lands

A recorded note goes on the **nearest** step, which is what other sequencers call quantising:
rounding each note to the grid. A note played just after a step starts rounds back onto it; one
played just before the next step rounds on to that one. How long a step lasts is taken from the
tempo playing, so under a conductor's `T` or `~` notes still land where they were played.

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
Alacritty, through the kitty keyboard protocol, and any terminal on Windows, whose console
reports them without being asked), a recorded note lasts as long as you held the
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

`:vel N` sets how hard the notes are played and entered (`00` to `99`), and the numpad's `/` `*` `-`
pick soft, medium or hard; the header shows it as `vel`. `:layout` changes how the notes lie on
the keyboard; see [the pattern chapter](02-patterns.md).

### Recording settings

On the mixer and fx pages, `R` arms record without starting insert mode, and each setting you change
while the song plays is locked on the step that's playing; on the instrument page, `i` plays the
track live and `R` records both its notes and the settings you turn. Both are described with
locks, in [the mixer chapter](08-mixer-and-effects.md#recording-locks-as-you-play).

## The pads

The **numpad** is a set of performance pads, in any mode, as on a drum machine. It needs a
terminal that tells the numpad's keys apart from the number row (kitty, WezTerm, foot, Ghostty,
recent Alacritty). On Windows it isn't told apart yet, so there are no pads there.

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

### The loop pedal

A loop pedal has one switch, and that's the point: your hands are on the instrument, so
recording, playing back and overdubbing all hang off a single press. Lattice's loop pedal is
**right Alt**, tapped on its own: pressed and let go with nothing else in between (`loop_pedal`).
On a UK keyboard, and many others, that key is AltGr; it counts too. Holding it for a chord or an
AltGr letter doesn't, so it never goes off by accident, and it sits under your right thumb, out
of the way of anything you type. Only terminals that report every key through the kitty keyboard
protocol send a lone Alt (kitty, WezTerm, foot, Ghostty and recent Alacritty; not through tmux
or on Windows): `!` does the same everywhere.

Tap it, and the cursor's track records its next pass:

1. **Record.** The track is armed, and records from the next time its playhead comes round to
   step `00`. If the song's stopped, it starts, with the count-in (`:count`, above), so you can
   come in on the first beat. Record (`R`) stays off, and you stay in whatever mode you were in.
2. **Play.** At the end of the pass it plays back, and the pedal disarms:
   `loop: pass recorded; ! overdubs another`.
3. **Overdub.** Tap again, and the next pass records over the loop, a layer. Each tap adds one,
   and `u` takes the last off.

Tap while it's recording and the pass under way finishes, then the loop plays on: a tap never
cuts a pass short.

That's a pass a tap, so the loop doesn't fill up with layers you didn't mean to play. Some
loops want building up without stopping, a layer each time round while you try things over it:
`:overdub` sets the track to overdub **pass after pass**, from the first tap until the next,
and `:overdub` again sets it back (or `:overdub on`, `:overdub off`).

With the song already playing, it waits for `00`, however far off that is: a long loop may keep
you waiting, but the loop always lines up with the song. For less of a wait, make the track
shorter, or start the song from somewhere else.

On a track that has notes and isn't a loop already, a synth or a drum, recording over it would
lose them, so the pedal adds a new track beside it, `loop` (then `loop 2` and so on), as many
steps long, moves the cursor onto it and records there. A track with no notes, or a chop track
playing a loop, records where it is.

```
 LATTICE  PATTERN   ▶ PLAY  ● LOOP LOOP    mountain-king.lattice   pat cave   bpm 138
 loop: recording a new loop from its next 00, one pass; ! again stops
```

### Loop tracks

A track the looper has recorded, or been set up for, is a **loop track**, and says so in its
header, under its length: how it records, so that it stands apart from the tracks of notes
beside it. The first word is where it starts, `at 00` or the threshold (below) it waits for,
`-30dB`; the second is `once` for a pass a tap, or `dub` for overdubbing pass after pass:

```
    BASS        LOOP        PAD
    16          16          16
                at 00 once
```

`:threshold` and `:overdub` set the cursor's track, and make it a loop track if it isn't one;
each is an undo. They're saved with the song, so a loop track keeps how it records.

### Coming in when you're ready: `:threshold`

Waiting for `00` keeps the loop in time, but it's a wait, and with the song stopped there's a
count-in to sit through as well. A **threshold** lets the looper listen instead, as many loop
pedals do: nothing is recorded until what comes in is louder than the threshold, and recording
starts the moment it is.

| Command          | What it does                                                       |
|------------------|--------------------------------------------------------------------|
| `:threshold -30` | waits for the input to reach -30 dB (from -60, the quietest, to 0) |
| `:threshold off` | no threshold: the looper waits for `00`, as before                 |
| `:threshold`     | says how the track records                                         |

With a threshold:

- **With the song stopped**, the loop pedal (or `:arm` with record on) doesn't start the song: the
  header shows `◌ LISTEN TROLL`, and the status line
  `troll: listening; play and the song starts with you`. The moment you play, the song starts
  from `00`, there and then, with no count-in, and the first pass is recorded from your first
  note.
- **With the song playing**, recording starts in the step where you come in, rather than waiting
  for the next `00`. The pass then runs round the track to that step again, a whole pass, and is
  turned round before it's saved, so that it plays from step `00` like any other loop: whichever
  step you came in on, the loop is in time.

Either way, what you played just before the step the pass starts on, the note that crossed the
threshold among it, isn't lost: it goes on the end of the loop, where you played it, just before
that step. The passes after the first follow on from it, back to back, as always.

Set it above the noise of what you're recording, the hiss of an amp, the hum of a cable, so it
isn't set off by them: `-40` suits a quiet line input, `-20` a noisy room through a microphone.

`:midi-learn loop_pedal` puts it on a MIDI foot switch (see
[`:midi-learn`](#pedals-buttons-pads-and-knobs-midi-learn)), so the whole loop can be played
from the floor. Stopping the song disarms it, so playing again
doesn't record by surprise.

### Arming a track

For more say over the passes, `:arm` arms the cursor's track to record with record on. Then turn
record on with `R` and play the song. The header shows `ARMED TROLL` (the track's name) while it
waits, and the track's header a `○`:

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
Room is kept for a pass at the slowest tempo the conductor could take it to, and a tenth more,
so a conductor's `T60` in a faster song doesn't cut the take short.

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

Turning record off (`R`), or tapping the loop pedal, lets the pass being recorded finish, and
then stops recording; the song plays on. **Stopping** the song ends the pass where it is, and an
unfinished pass is thrown away, so a pass is always whole. `^k` (hush) stops everything at once,
the same way.

### Hearing what you play

Lattice doesn't play the input back through its own output: doing so would add the whole
journey into the computer and out again to what you hear, late enough to put you off. Listen to
your instrument through your audio interface's own monitoring (its "direct monitor"), and to
Lattice through the same headphones.

## Audio devices

### :input and :output

`:input` chooses the device the looper records, or the app (below), and `:output` the device
you hear Lattice through. Type `:input ` (with its space) and the devices are suggested above
it, each with its number, kept up to date as devices come and go; `tab` fills one in. Enter
`:input` on its own and it says which is in use, and leaves them all listed in the corner,
numbered, the one in use marked `●`, until the next key. I keep the two apart, so that the list
doesn't sit under the suggestions while you type:

```
:input 2              the second in the list
:input scarlett       the one whose name has "scarlett" in it, in any case
:input 0              the system's default (or :input default)
:input                says which is in use, "input: the default (Built-in Audio)",
                      and lists them all until the next key
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

### Recording an app

The looper can also record another program's sound: a browser playing a record you want to
sample, a soft synth, a video. Apps playing now are listed after the devices, as
`app: Firefox`, and suggested first as you type, so `:input firefox` picks it. Lattice then
records that app and nothing else: not the other apps, and not Lattice itself.

That's the difference from the `Monitor of …` inputs, which PulseAudio and PipeWire give every
output. A monitor hears everything going to that output, so recording your headphones' monitor
while a loop plays records Lattice playing the loop too, and each overdub doubles what came before.
Recording the app gets only the app's stream. It's the trick pavucontrol uses to give each app its
own level meter: listen to the output's monitor, but ask the server for one stream only.

An app has a stream only while it plays, and browsers close theirs a few seconds after they go
quiet, so an app shows up in the list once it's playing. Lattice knows it by name, not by stream.
Once chosen, it stays listed while quiet, as `app: Firefox (not playing)`, and it's remembered in
`audio.toml` like a device (`input = "app:Firefox"`). While the input's open, Lattice looks every
quarter of a second for the app's stream, and records whichever it has. So pausing a video and
playing it again, or a new tab, carries on recording. Arming a track while the app is quiet says
`Firefox isn't playing: it's recorded once it does`, and the pass is silent until it plays.

Other programs that play through cpal, the audio library Lattice uses, are named by their program
and process number, so a second Lattice shows as `app: lattice 935040`. That's a way to record one
song's sound into another, with each still playing on its own.

Apps can only be recorded on Linux, through PulseAudio or PipeWire. An app's sound is lined up as
an instrument's is: what you play along on a soft synth lands on the beat you played it to. If it
lands early or late, `:latency` moves it, for that app alone.

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

### MIDI keyboards

Plug in a MIDI keyboard and play: there's nothing to set up. Lattice listens to every port that
sends MIDI, as it appears, and lets go of one that's unplugged, so a keyboard plugged in while
the song plays is heard at once. The status line says so as it happens:

```
 midi in: KeyStep 37
```

and `midi in: KeyStep 37 gone` when it's unplugged. `:midi` says which keyboards and
controllers Lattice is listening to, or that there are none. It listens on every channel, so a
keyboard plays whatever channel it's set to. Ports with **Through** in their name are left
alone: a MIDI Through port hands back whatever is sent to it, and listening to one is the first
step towards a feedback loop.

A keyboard's notes do what the piano keys' do, with two differences. A note plays **at its own
pitch**, as the keyboard has its own octave buttons, so `{` `}` and `:oct` don't move it; and it
plays **at the velocity it was played**, rather than `:vel`'s:

- **In insert mode**, a note goes in at the cursor and the cursor moves on, as a piano key's
  does, with the velocity it was played at in its `vel` field. On a lock or gesture field, where
  a note would type nothing, it's only heard.
- **Recording**, with record armed and the song playing, a note goes in at the playhead, as long
  as it was held and as hard as it was played.
- **Anywhere else**, in normal mode and on every page, a note is only heard, so you can play
  along without changing anything. It plays the track the piano keys would: the cursor's on the
  pattern page, the cursor's column on the instrument page, and the cursor's strip on the mixer
  and fx pages.

Every note sounds for as long as its key is down, whatever the terminal; a note held longer than
15 seconds is let go, as a held piano key's is, in case its key-up never comes. A track sounds
eight notes at once at most, so a ninth in a chord takes the place of the first.

This is what a MIDI keyboard was always going to fix. A terminal says when a key goes down, and
only some say when it comes up (see [Auditioning](02-patterns.md#auditioning)); none says how
hard it was struck. A MIDI keyboard says both, for every note.

**Timing.** Lattice draws the screen about sixty times a second and, between frames, waits for
the computer's keyboard. While it's listening to a MIDI port it also looks for MIDI every
millisecond, so a note isn't held up until the next frame: it reaches the engine within about a
millisecond, and is heard after the audio device's own latency, as everything is.

### Pedals, buttons, pads and knobs: `:midi-learn`

A MIDI controller's controls can do anything a key can, which is what makes a **foot pedal** a
loop pedal: with your hands on an instrument, a foot starts the song, arms the looper and turns
record on and off. Bind a control by learning it:

```
:midi-learn toggle_record
```

and then move the control, press the pedal, say. The status line says what's been bound:

```
 cc 64 does toggle_record
```

What a control can be bound to is what a key can, and one thing more:

- **an action**, by its name, as in the config file (see
  [every action, by name](10-files-and-config.md#every-action-by-name)): `toggle_play`,
  `toggle_record`, `play_fill`, `hush`;
- **a command**, after a `:`: `:midi-learn :arm 1` makes the next control moved arm the
  cursor's track to record one pass of the looper, and `:midi-learn :disarm` another disarm it;
- **a macro**, `macro 1` to `macro 8` and on: a knob or a fader turns that macro, by its number,
  on the track the piano keys play. Turned with record armed and the song playing, it's locked
  on the steps as they play, as turning one on the mixer page is (see
  [Macros](08-mixer-and-effects.md#macros)).

`:midi-learn none` unbinds the next control moved, and `:midi-learn` on its own stops learning
without binding anything.

A pedal, button or pad acts **once, as it goes down**: a pedal sends a stream of messages as
it's pressed, and Lattice acts on the first one past halfway (64 of 127) and nothing more until
it's let go. A fill bound to a pedal (`play_fill`) plays for as long as the pedal's down, as it
does on its key. A **latching** pedal, one that sends "down" on one press and "up" on the next,
acts on every other press, so set it to momentary if it has a switch for it, as most do.

A note bound to a control, a pad's say, does what it's bound to rather than play. Controls are
told apart by their number, not their channel, so a control does the same whatever channel the
controller sends it on.

The bindings are kept in `~/.config/lattice/midi.toml`, which `:midi-learn` writes and which can
be edited by hand. A control is `cc` and its number, for a pedal, button or knob, or `note` and
its number, for a pad:

```toml
[controls]
"cc 64" = "toggle_record"     # the sustain pedal records
"cc 67" = ":arm 1"            # the soft pedal arms the looper for a pass
"note 36" = "toggle_play"     # the first pad plays and stops
"cc 21" = "macro 1"           # the first knob turns the track's first macro
```

`:midi` says how many controls are bound, and the help page (`:help`) lists them under
**midi controls**.

### Knobs on settings: `:midi-learn this`

A knob bound to `macro 1` turns the first macro of whichever track you're on, which is right for
moving about the song with one knob. When I'm playing, though, I want a row of knobs that each
do one thing, the lead's cutoff, the pad's reverb, the bass's drive, whatever the cursor's on.
For that, bind a knob to the setting itself. Put the cursor on it, on the mixer, fx or
instrument page, and:

```
:midi-learn this      then turn the knob
```

The message box says what it's bound to:

```
 cc 21 turns lead's filter.cutoff
```

From then on the knob turns that setting through its whole range, from any page, wherever the
cursor is. A whole turn is one undo step. With record armed and the song playing, it locks the
setting on the steps as they play, as turning it on the mixer does. On the instrument page,
`this` on a macro binds the knob to that track's macro, rather than to macro 1 of whichever
track the cursor's on.

A setting a knob turns is marked `◎`, where a macro's number would go:

```
 ◎ cutoff      ▊▊▊▊▊▊▊▊▊▊▊▊▊    1 resonance   ▊▊▊▊
```

These bindings belong to the song, not to `midi.toml`: what the tracks are called and which
effects they have means nothing to another song, so they're saved with the track and go where it
goes. Taking out the effect a knob turns unbinds it, and moving effects about moves it with them.
In this song, a knob bound to a setting comes before what `midi.toml` binds it to, and the
message says so (`cc 21 turns lead's filter.cutoff in this song, over macro 1`). Each knob turns
one setting, and each setting has one knob: binding a knob again moves it. `:midi-learn none`,
then the knob, unbinds it, from the song's setting and `midi.toml` alike. A copied track keeps
its knobs, so a knob then turns the setting on both, which can be what you want. A pad can't
turn a setting, so `:midi-learn this` waits for a knob or fader.

`:midi` counts them, and the help page lists them under **knobs in this song**, by number.

### Foot switches that type keys

Many USB foot switches don't speak MIDI at all: they pretend to be a keyboard and type a key.
Those work with Lattice too. Set the pedal to type a key Lattice already knows, such as `R` for
record or `space` for play, or bind a key it can type to a command in the config file (see
[files and configuration](10-files-and-config.md)):

```toml
[keys.normal]
C-l = ":arm 1"        # a pedal sending Ctrl-L arms the track for one pass
```

`R`, `space` and `!` (the loop pedal) work in insert mode as well as normal mode, so a pedal on
them works while you play.

### MIDI out

Lattice plays other gear too: a track whose instrument is `midi` sends its notes, locks and
program changes to a synth, a drum machine or another program, in time with the song. `:port`
chooses where; see [MIDI tracks](07-instruments.md#midi-tracks-playing-other-gear-midi).

### Clock: `:clock`

Some gear plays patterns of its own: a drum machine, an arpeggiator, a sequencer, a delay timed
to the beat. MIDI clock keeps it in time with Lattice, and starts and stops it with the song.

```
:clock digitakt       send clock to the Digitakt
:clock 2              to port 2, as :port lists them
:clock lattice        to Lattice's own port, for a DAW or another program
:clock                say where it goes
:clock digitakt       again: stop sending it there
:clock none           stop sending it anywhere
```

It can go to as many ports as you like at once. As with `:port`, a port is named by any part of
its name, and gear plugged in later is found by it; `:clock ` lists the ports, marking the ones
with clock on. The ports are kept in `midi.toml`, beside the controls, as they belong to the
studio rather than the song:

```toml
[clock]
ports = ["Digitakt", "lattice"]
```

**What's sent.** MIDI clock is 24 pulses a beat, which the gear counts to find the tempo and
where the beat is. Lattice sends them all the time, playing or not, so the gear's tempo follows
the song's even while it's stopped; with them it sends **start** as the song's first step plays,
after any count-in, so the gear's first beat is the song's, and **stop** as it stops. A beat
is the meter's (see [the meter](06-song.md#the-meter)), so in 6/8, with three steps a beat,
each step is eight pulses.

**Tempo.** The pulses follow the tempo playing, a step at a time: `:bpm`, and the conductor's
`T140` and its slides, `~140` (see [the conductor](06-song.md#the-conductor)), so gear
following a slide speeds up with Lattice.

**Timing.** Each pulse is timed as a MIDI track's notes are, for the moment Lattice's own sound
is heard, so the gear's beat lands with Lattice's. A pulse is worked out from the step it falls
in, rather than counted on from the last, so they never drift from the steps, however long the
song plays.

I don't send song position, so gear started by Lattice starts from its own beginning.
