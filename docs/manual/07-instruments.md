# Instruments

Every track plays one instrument. It turns the track's notes into sound: a kick drum, a stack
of saws through a screaming filter, a plucked string, an alien harpsichord, an electric piano,
the bells and basses of FM, a violin made of glass, a sound file. This chapter covers choosing
an instrument, the instrument page where you shape it, each of Lattice's fifteen instruments and
every one of their settings, the sampler and chop for playing sound files, and presets and
faces, which make an instrument into one of its own.

The effects that come after the instrument, the mixer, and macros in full are in
[chapter 8](08-mixer-and-effects.md). Gestures that change how a single note plays (`<`, `O`,
`G`) are in [chapter 5](05-gestures.md), and locks, which change a setting step by step, in
chapter 8.

## The instruments at a glance

Lattice has fifteen instruments. All are built in: the engine is written by hand, in Rust, for
Lattice alone. Fourteen are below; a fifteenth, `midi`, makes no sound of its own: it plays
gear outside Lattice (see [MIDI tracks](#midi-tracks-playing-other-gear-midi)).

| Name        | Engine     | What it is                                                   | Voices |
|-------------|------------|--------------------------------------------------------------|--------|
| `synth`     |            | a plain test synth: one oscillator, a filter, an envelope    | 8      |
| `switched`  | Switched   | a modular: a stack of saws through a screaming ladder filter | 8      |
| `monochord` | Monochord  | a plucked or bowed string, modelled as a waveguide           | 8      |
| `mosaic`    | Mosaic     | a bowed string on a body of wood, glass, metal or a voice    | 8      |
| `krell`     | Krell      | an alien harpsichord: choirs of quill-plucked strings        | 8      |
| `tuss`      | Tuss       | an electric piano: struck tines, barking pickups, tunings    | 8      |
| `bessel`    | Bessel     | FM: four operators, for bells, basses, brass and drums       | 8      |
| `sampler`   |            | plays a WAV file, pitched by the note or cut into slices     | 16     |
| `chop`      |            | a WAV file cut into pieces, a step playing each              | 16     |
| `kick`      | Rhythmicon | a bass drum                                                  | 1      |
| `snare`     | Rhythmicon | a snare drum                                                 | 1      |
| `hat`       | Rhythmicon | a hi-hat or cymbal                                           | 1      |
| `clap`      | Rhythmicon | a handclap                                                   | 1      |
| `tom`       | Rhythmicon | a tom-tom                                                    | 1      |

**Voices** are how many notes an instrument can sound at once. When every voice is busy, a new
note takes one: a free voice if there is one, else the oldest that's already fading out, else
the oldest of all. A drum is one voice, as a real drum is one skin: each hit restarts it.

A new track plays `synth`. The song file keeps each track's instrument and only the settings
that differ from their defaults, by name:

```
instrument: (
    kind: Kick,
    params: {"decay": 0.3, "drive": 0.2, "model": 0.0, "punch": 0.6, "tune": 55.0},
),
```

## Choosing an instrument

`:inst NAME` puts an instrument on a track. On the pattern page it changes every track the
selections reach; on the mixer, instrument and fx pages, the track under the cursor.

```
:inst krell          the cursor's track plays the alien harpsichord
:inst kick           and now a bass drum
```

The names are `synth`, `switched`, `monochord`, `mosaic`, `krell`, `tuss`, `bessel`, `sampler`,
`chop`, `kick`, `snare`, `hat`, `clap` and `tom`, and `midi` for gear outside Lattice. Type
`:inst ` and the command line lists them; a name it doesn't know is refused with the list:
`no instrument called "string"; there's synth, switched, …`.

What changing it does:

- The new instrument starts at its default settings. A track that already plays that kind is
  left alone, settings and all, so `:inst kick` on a tuned kick doesn't reset it.
- The track's face, if it has one, goes (see [faces](#faces)): it belonged to the old
  instrument.
- Locks on the old instrument's settings go, and so do any macro's hold on them, since those
  settings mean nothing to the new one. The track's effects, their locks, and the macros
  themselves stay, so a macro that turned a filter after the old instrument still turns it.
- A track that hasn't been given a name of its own takes the instrument's: one still called
  `track 3`, as `:track-add` left it, or named after the instrument it played before, so a new
  track becomes `krell` and then `kick` as you try instruments on it. If another track has the
  name, it's numbered, `kick 2`. A name you chose (`:name bass`) stays.
- It's one step of undo, so `u` brings back the old instrument as it was.

On the instrument page and the mixer, `c` on the instrument's name opens the command line with
`inst ` ready, which is the same command. `:sample` and `:chop` also change the instrument, to a
sampler or chop playing a file; see [playing sound files](#playing-sound-files).

### Why the engines have names

Lattice's own engines are named, not called "drum machine", "polysynth" or "physical model". Each
name is a nod to a pioneer of electronic music, and each engine has a character to match rather
than a wall of generic controls:

- **Rhythmicon**, the drums, after the first drum machine, which Leon Theremin built for Henry
  Cowell in 1931.
- **Monochord**, the string, after the one-stringed instrument with which Pythagoras worked out
  the ratios of musical intervals.
- **Switched**, the modular, after Wendy Carlos's Switched-On Bach, the 1968 record that put
  the Moog synthesiser in front of everyone.
- **Krell**, the harpsichord, after the vanished civilisation of the 1956 film *Forbidden
  Planet*, whose score, by Louis and Bebe Barron, was the first made all electronically.
- **Tuss**, the electric piano, after The Tuss, a name Richard D. James, Aphex Twin, put records
  out under in 2007.
- **Bessel**, the FM synthesiser, after Friedrich Bessel, the astronomer whose functions say how
  loud each partial of an FM sound is.
- **Mosaic**, the bowed string, after IRCAM's program for modal synthesis, MOSAIC, now Modalys,
  made in Paris around 1990 for building instruments that can't exist.

Each engine's section below tells the story behind its name. The design notes put it this way:
Lattice is a personal instrument first, and its engines should be "interesting and quirky,
powerful enough, but not a wall of options". So each has a small set of controls, each doing
something characterful, named for what it does to that instrument: a harpsichord has `quill`
and `damper`, not "filter envelope amount". The plain `synth` is the exception, there to test the
engine, and the sampler and chop are named for what they do.

The drums keep plain names (`:inst kick`) because a track plays one drum: you'd never want
`:inst rhythmicon` and then a menu. The others are engines you choose by name, so they go by
it: `:inst monochord` used to be `:inst string`, `:inst switched` was `:inst modular` and
`:inst krell` was `:inst harpsichord`, until the engines took their own names.

## The instrument page

The instrument page shows every track's instrument side by side, a column each, read from top
to bottom. `gi` opens it, from any page, on the cursor's track (from the mixer or fx page, on
the strip under the cursor), with the cursor on the instrument's first setting. `gp` goes back
to the pattern, `gm` to the mixer and `gf` to the cursor's track's effects.

It's for shaping sounds: every setting of every instrument is a line you can move to and turn,
and you can play the track on the computer keyboard while you do.

### Layout

Here is part of the page, with a string, a harpsichord and a track wearing the `night-bus`
preset's face:

```
 ─‹ instruments › ────────────────────────────────────────────────────────────────────────

      PERC                        RIDE                        BASS
                                          ⣶⡆  ⣄⣖⣖⣖⣟⣻⣲⣲⣲⣠          ⢀    ⠈⠉⣿⣿⣿⠉⠁    ⡀
          ⡀⠄⠄⠂⠂⠂⠄⠄⡀                 ⣴⣶⡄⠿⠇   ⡄⡇⡖⠭⣉⣱⣀⣀⣎⣉⠭⢲⢸⢠      ⣠⣶⣿   ⣀⣸⣿⣿⣿⣿⣿⣇⣀   ⣿⣶⣄
      ⣀⡄⠂⠁         ⠁⠂⣄⡤⠦⠥⠥⠥⠦⢄⣀⣀    ⠂⠙⠛⠁    ⡇⡇⡇⡗⠒⣒⡒⣒⣒⢒⣒⠒⢺⢸⢸⢸   ⢀⣾⣿⣿⣿ ⢠⣤⣿⣿⣿⣿⣿⣿⣿⣿⣿⣤⡄ ⣿⣿⣿⣷⡀
      ⣿⠉⠋⠕⡒⠢⠤⠤⠤⠤⠤⠒⡒⠍⠋⡏⡇⠂⠄⠄⠄⠂⠁⣿        ⡀    ⡇⡇⡇⡇⢀⢛⡃⣛⣛⢘⡛⡀⢸⢸⢸⢸   ⣾⣿⣿⣿⣿⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣶⣿⣿⣿⣿⣷
      ⠛⠒⠒⠒⠒⠓⠓⣒⣒⡒⠓⠓⡖⠒⠚⠲⡚⢲⠒⠒⠒⠒⠒⠛⠒   ⡓⢄⠅ ⡂     ⠃⡇⠣⢌⡈⠁⠉⠉⠈⢁⡡⠜⢸⠘    ⢿⣿⣿⣿⣿⠿⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠿⣿⣿⣿⣿⡿
            ⠯⣀⣀⡨⠇ ⠁   ⠁ ⠁         ⡂ ⠕⡄⡂         ⠈⢱  ⡎⠁        ⠈⢿⣿⣿⣿ ⠘⠛⣿⣿⣿⣿⣿⣿⣿⣿⣿⠛⠃ ⣿⣿⣿⡿⠁
      ⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒   ⠂ ⠁⠈⠢⡀      ⡸⠉⠉⡏⠉⠉⢹⠉⠉⢇        ⠙⠿⣿   ⠉⢹⣿⣿⣿⣿⣿⡏⠉   ⣿⠿⠋
      ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉        ⠈⠢⡀⠠⠨⠩⠩⠠⠠⠠⠠⠠⠄⠄⠄⠄⠄⠍⠍⠅       ⠈    ⢀⣀⣿⣿⣿⣀⡀    ⠁
      monochord ────────────────  krell ────────────────────  Night Bus synth ──────────
       model         nylon         stops         8' 8'        1 dawn           0
       pluck          0.5          drift          0.15        2 haze          0.3
       position       0.15         quill          0.7          attack         0.6s
       decay           3s          decay           4s          release         2s
       tone           0.6          damper         0.8         → reverb filter pitch
       bow             0           body           0.3
      → no effects                 hull          196Hz
                                   glass          0.5
                                   gain           0.5
                                  → no effects
```

From the top:

- **The rule**, `instruments`, with `‹` and `›` when there are more tracks off to the left or
  right, or above and below. Each column is 24 to 34 characters wide, as many as fit across.
  In a tall window, the tracks that don't fit across go in rows under the first, each with its
  names, art and lines, so a tall, narrow terminal shows every instrument at once rather than a
  row of four with the rest of the screen empty. It shows as many rows as fit wherever the
  cursor is, the same number on every screen, shrinking the art to fit another row rather
  than showing fewer, and scrolls a row at a time to keep the cursor's track in view. When
  even one whole row won't fit, there's just the one, scrolled sideways to the cursor's track
  and down to its line.
- **The tracks' names**, in each track's colour, with `M` or `S` when it's muted or soloed. A
  track the song has muted in the entry playing is marked `─` and dimmed, as on every page.
- **The art**, up to eight rows of braille: the track's face, or else its engine's own drawing
  (the monochord's string over its bridge, the Krell's Great Machine). It moves with the beat
  and lights up as the track plays; see [the art](#the-art).
- **The lines**, with `▸` down the left at the cursor's. They aren't numbered as a pattern's
  rows are, since they're a list, not time. Each column is in
  **parts**: the instrument's name first, then its settings a line each, then the track's
  macros, then its chain of effects.

A **setting's line** has its name, its value with its unit (`0.6s`, `196Hz`, `0st` for
semitones, or a choice such as `nylon`), and a bar behind the value, shaded from the left as
far as the value is through its range. Values are shown to a sensible precision: three
decimals below 10, one below 100, none above. A setting a macro turns is marked with the
macro's number before its name (`1cutoff`), or `*` if more than one turns it. While the song
plays, a setting a step has locked shows the lock's value instead, marked `◆` in the accent
colour, so you see what's actually playing.

A **macro's line** has its number, its name and its value from 0 to 1: `1 dawn 0`.

The **chain** is the last line: `→ reverb filter pitch`, the effects the sound goes through
after the instrument, or `→ no effects`.

A track with a **face** shows less: its title with the instrument faint beside it
(`Night Bus synth`), its macros first, then only the settings the face puts forward, then the
chain. `enter` on the title opens all its settings, as on the KRELL column, and again goes back
to the face. See [faces](#faces).

### Moving and adjusting

These are the instrument page's keys. Each is an ordinary key of the keymap, doing its page's
job here, so the names in the middle column are what a config file binds (see
[chapter 10](10-files-and-config.md)).

| Key         | Action                      | On the instrument page                          |
|-------------|-----------------------------|-------------------------------------------------|
| `gi`        | `goto_instrument`           | open the page, on the cursor's track            |
| `h` `l`     | `left` `right`              | the track before or after, on the same line     |
| `(` `)`     | `rotate_backward` `rotate_forward` | the same, as on the mixer                |
| `gh` `gl`   | `first_track` `last_track`  | the first or last track                         |
| `j` `k`     | `down` `up`                 | down or up the column                           |
| `w` `b`     | `next_note` `prev_note`     | to the next or previous part                    |
| `gg` `ge`   | `first_step` `last_step`    | to the name, or the chain                       |
| `C-a` `C-x` | `note_up` `note_down`       | turn the setting or macro up or down            |
| `+` `=` `-` | `longer` `shorter`          | the same as `C-a` and `C-x`                     |
| `c`         | `change`                    | type a value, an instrument or an effect        |
| `enter`     | `open`                      | open or close a face; on the chain, the fx page |
| `d` `A-d`   | `delete` `clear`            | on a macro, remove it                           |
| `i` `a`     | `insert` `append`           | play the track on the computer keyboard         |
| `L`         | `lock_setting`              | lock settings onto the pattern's selection      |
| `m` `s`     | `toggle_mute` `toggle_solo` | mute or solo the cursor's track (or `tm` `ts`)  |
| `gf`        | `goto_effects`              | the track's effects                             |
| `gm`        | `goto_mixer`                | the mixer                                       |
| `gp`        | `goto_pattern`              | the pattern                                     |

Counts work as everywhere: `3l` moves three tracks right, `5j` five lines down, and `10C-a` turns
a setting ten times as far. `h` and `l` keep to the same line as they cross, so you can
run along the `decay` line of a row of drums; on a shorter column the cursor stops at its last
line.

`c` opens the command line ready for what's under the cursor: `set ` on a setting or macro (see
below), `inst ` on the instrument's name, and `fx-add ` on the chain, to add an effect with
`:fx-add` (chapter 8).

Moving the cursor also moves the mixer's onto the same track and setting, which is why
`:set`, `:map`, `:face-show`, `L` and recorded locks all work here as they do on the mixer.

**How far a turn goes.** Each `C-a` or `C-x` moves a setting a hundredth of its range, along its
curve:

- **Linear** settings (most from 0 to 1) move by a hundredth: 0.5 to 0.51.
- **Logarithmic** settings, frequencies and times, move by a hundredth of their range as a
  ratio, so a step is the same musical distance wherever you are: `cutoff` from 1000 Hz goes to
  about 1072 Hz, and from 100 Hz to about 107 Hz. That's how you hear pitch and time.
- **Choices** (`model`, `wave`, `stops`) move one choice at a time, stopping at the ends.
- **Counts** (chop's `pieces`) move by one.

A macro moves by 0.01. The ends of a range stop a turn rather than wrapping. Turning the
instrument's name or the chain does nothing but say what does: `c changes the instrument`, or
`enter opens the effects (gf)`.

**Typing a value.** `c` on a setting or macro opens the command line with `set ` ready, and
`:set VALUE` sets the value under the cursor exactly, in the setting's own units: hertz for a
frequency, seconds for a time, semitones for a `tune` marked `st`; the unit can be written too
(`:set 800Hz`). A choice is set by its name (`:set swarm` on Switched's `stack`) or by its number
from 0, in the order the choices are listed in its section (`:set 2` is `swarm` too); a wrong name
is answered with the choices (`stack is one of one, pair, swarm, stops, fifths`). A value outside
the range is refused with the range, as `cutoff goes from 20Hz to 16000Hz`; a macro takes 0 to
1. `:set` works on the mixer, fx and instrument pages
(`set works on the mixer (gm), fx (gf) and instrument (gi) pages`, it says elsewhere), and the
chapter on the mixer covers it there.

Every change is a step of undo, and with record armed and the song playing, a turn is recorded
as a lock on the step playing instead (see chapter 8).

### Auditioning: playing the track live

`i` (or `a`) plays the cursor's track on the computer keyboard, laid out as in insert mode on the
pattern, so you can try a sound out without entering any notes. The status line says `PLAY`,
and the hints change to the keys that work while playing:

```
 PLAY  playing bass on the keyboard; esc stops       z–] notes play · { } octave · ←→ tracks
```

| Key         | While playing                                                    |
|-------------|------------------------------------------------------------------|
| `z` … `]`   | the notes, in the keyboard layout (`:layout`)                    |
| `{` `}`     | octave down or up (`keyboard_octave_down`, `keyboard_octave_up`) |
| numpad      | the pads play the track too                                      |
| `←` `→`     | to the track before or after                                     |
| `↑` `↓`     | up or down the column                                            |
| `C-a` `C-x` | turn the setting or macro, as in normal mode                     |
| `R`         | arm record                                                       |
| `esc`       | stop playing, back to normal mode                                |

On the piano layout, `z` is C, the row above it the black keys, and `q` starts the octave
above. The letters all play notes, so `h` `j` `k` `l` don't move here; the arrows do. A `C-` key
works as it does in normal mode. With the keyboard at octave 4, `z` plays C-4, which is the
note that plays a drum at its tuning and a sample as recorded.

**Recording.** Nothing goes into the pattern unless record is armed (`R`, `toggle_record`,
which on this page also starts playing the keyboard) and the song is playing: then the notes
you play go in at the playhead, on the cursor's track, and the settings you turn are locked as
you go, as when recording on the pattern page (see
[chapter 9](09-recording.md)). Without the song playing, you only hear them.

### The art

Every column on the page has art above its lines, eight rows of braille at most, fewer if the
terminal is short. A track with no face shows its engine's own drawing, each nodding to what the
engine is named after:

| Instrument  | Its art                                                                      |
|-------------|------------------------------------------------------------------------------|
| `synth`     | an oscilloscope, a wave scrolling across it                                  |
| `switched`  | a Moog's wall of modules, its patch cables swaying                           |
| `monochord` | Pythagoras's monochord, its string over a bridge set at a fifth              |
| `krell`     | the Great Machine of the Krell, from Forbidden Planet, and its halls         |
| `tuss`      | a tine and its tone bar, struck by the hammer, swinging before the pickup    |
| `bessel`    | a sine bent by FM, over its sidebands, Bessel's functions, as it bends more  |
| `mosaic`    | a bow on a string over a body of tiles, which light up as it sounds          |
| `sampler`   | a reel-to-reel, as at the Radiophonic Workshop, its reels turning            |
| `chop`      | a break's waveform sliced up, a slice and its pad lit in turn                |
| the drums   | the Rhythmicon's lamp through its spinning disc of holes onto its photocells |
| `midi`      | a MIDI socket, its five pins, and a note going down the wire, bit by bit     |

They were drawn by hand, dot by dot, rather than traced, as pictures don't survive being made so
small. A face can have art of its own, drawn by hand too, or else a fractal its title picks
(see [faces](#faces)).

Whichever it is, the art plays along with the track:

- **At rest** it's a faint outline. Playing lights it from the middle out, as far as the track
  is loud.
- **Each note** the track starts sends a ring out from the middle, and the note's pitch picks
  its colour: a step round the colour wheel for each step round the circle of fifths, so notes
  a fifth apart are neighbouring colours and a key has its own family of them. The colours are
  the terminal's own six (see [chapter 10](10-files-and-config.md)).
- **Frames**: hand-drawn art can have up to eight frames, and steps on one with each new beat
  and with each note the track starts, once for a note that lands on the beat, so a drum on
  every beat doesn't skip a two-frame loop's other frame. When the song's stopped it steps
  slowly, every three quarters of a second.
- **A fractal** face is shaped by the track's macros: the first zooms in on it and the second
  hurries its flow.

Where a column is narrower than the art, the art is cut down around where it's busiest, all its
frames together, rather than about its middle, so the subject stays in.

## How the engines make sound

The sections that follow describe each engine. This one is a short primer on the ideas they
use, for readers who haven't met them. Skip it if you have.

**Pitch and C-4.** Notes are written as in trackers: `C-4` is middle C, about 261.6 Hz, and
`A-4` is 440 Hz. Each octave doubles the frequency. Lattice's drums and sound files treat C-4
as home: C-4 plays a drum at its `tune` and a sample at the pitch it was recorded at, and each
semitone away retunes it by a twelfth of an octave.

**Oscillators** make a tone by repeating a shape. A **saw** wave ramps up and drops, and has
every harmonic, so it's bright and buzzy, the raw material of strings and brass. A **square**
wave has only the odd harmonics, which sounds hollow and woody, like a clarinet. Lattice's are
**band-limited** (with polyBLEP corrections at each jump), which keeps high notes from folding
back into harsh, out-of-tune whistles.

**Filters** take harmonics away. A **low-pass** filter lets through what's below its **cutoff**
and cuts above it, darkening the sound; sweeping the cutoff up is the classic "opening" of a
synth. **Resonance** feeds the filter's output back to its input, boosting the frequencies near
the cutoff into a peak, a nasal "wow", and at its extreme a whistle of its own. A filter's
slope says how steeply it cuts: a **two-pole** filter cuts 12 dB an octave, a **four-pole**
24 dB.

**Envelopes** shape a note over time. The classic one is **ADSR**: **attack**, how long the note
takes to rise; **decay**, how long it then takes to fall to the **sustain** level, where it holds
while the note is held; and **release**, how long it takes to die away once the note ends. An
envelope can drive the level, the filter's cutoff, or both.

**Velocity** is how hard a note is played, from the step's velocity field (1 to 127). Every
instrument plays harder notes louder, and some brighter too.

**Decay times** in Lattice's physical engines and drums are times to fall by **60 dB**, to a
thousandth of the level: about when a sound stops being heard in a mix. It's the convention
for reverberation (RT60), and it means a `decay` of 2 s sounds about two seconds long.

**Physical modelling** makes sound by simulating something that vibrates, rather than by
starting from a wave and filtering it. Lattice uses two kinds:

- **Waveguides** model a string as a wave travelling up and down it. A delay line as long as
  one trip along the string holds the wave; each time round, a gentle low-pass filter and a
  small loss stand in for the string's stiffness and friction, so it rings, then dulls, then
  dies, as a real one does. The idea began with the **Karplus-Strong** algorithm (Kevin Karplus
  and Alex Strong, 1983), which fills a delay line with noise and averages it as it goes round,
  and was developed into digital waveguides by Julius O. Smith at Stanford. Monochord, Krell
  and Mosaic's strings are waveguides.
- **Modal synthesis** models a resonant body (a drumhead, a bell, a plate of glass) as a set
  of **modes**, each a frequency at which it rings and a time it rings for. Hit it, and each mode
  rings at its own pitch and dies at its own rate. The drums' shells, Krell's body, Tuss's
  tines and Mosaic's bodies are modal.

**FM synthesis** makes rich sounds from plain sine waves, by one bending another's pitch so fast
that the bending becomes new partials. Bessel is FM, and its section explains it in full.

**Choke.** A choke group (set on the mixer, see chapter 8) silences one track when another in its
group plays, as an open hi-hat is cut off by the closed one. What a choke does depends on the
instrument: the drums, strings and sound files fade out over a few milliseconds (quick, but not
a click), and the synths release their notes.

## The engines

### Synth, the plain test synth (`synth`)

`synth` is a plain subtractive synthesiser: one oscillator per note, a low-pass filter and an
envelope. It's here to exercise the engine and to set the pattern for the others (all eight
voices are computed together in SIMD lanes, and anything expensive is worked out every 16
samples rather than every sample), not as a flagship. But it's a perfectly good bass, lead or
pad, and many of the presets are built on it.

**How it sounds.** Each note's oscillator, a `saw` or `square`, goes through a two-pole (12 dB
an octave) state-variable low-pass filter, then an amplifier. One ADSR envelope drives both
the amplifier and the filter: at the envelope's peak the cutoff rises by `env` hertz, and it
falls back with the envelope, so notes open and close as they're played. It's mono, the same
in both sides. A `G` gesture glides a note in from the last one's pitch.

| Setting     | Range           | Default | What it does                                        |
|-------------|-----------------|---------|-----------------------------------------------------|
| `wave`      | `saw` `square`  | `saw`   | the oscillator: bright and buzzy, or hollow         |
| `attack`    | 0.001 to 2 s    | 0.005 s | how long a note takes to reach full level           |
| `decay`     | 0.01 to 4 s     | 0.2 s   | how quickly it then falls towards the sustain level |
| `sustain`   | 0 to 1          | 0.5     | the level it holds at while the note is held        |
| `release`   | 0.005 to 4 s    | 0.1 s   | how quickly it dies once the note ends              |
| `cutoff`    | 20 Hz to 16 kHz | 1000 Hz | the filter's cutoff with the envelope closed        |
| `env`       | 0 to 10000 Hz   | 3000 Hz | how far the envelope opens the filter, at its peak  |
| `resonance` | 0 to 0.95       | 0.3     | the filter's peak at the cutoff, up to a whistle    |
| `gain`      | 0 to 1          | 0.2     | the output level                                    |

The attack rises as an analogue envelope does, curving towards a target above full level and
stopping when it gets there, so `attack` is the time to full level. `decay` and `release` are
time constants: in that time the level falls about two thirds of the way to where it's going.

Some starting points:

- **Pluck**: `sustain` 0, `decay` 0.2 s, `cutoff` low and `env` high, so each note snaps open
  and closes again.
- **Acid bass**: `wave` `square` or `saw`, `cutoff` near 300 Hz, `resonance` 0.8 or more,
  `decay` short. Lock `cutoff` on some steps (chapter 8) for the squelch of a Roland TB-303.
- **Pad**: `attack` and `release` over a second, `sustain` high, `cutoff` moderate.

### Switched, the modular (`switched`)

Wendy Carlos's Switched-On Bach (1968) played Bach on a Moog modular synthesiser, a wall of
modules connected by patch cables, one line at a time onto multitrack tape, as the Moog could
play only one note at once. It became one of the best-selling classical records of its day, won
Grammy awards, and did more than any record before it to show what a synthesiser could do.
Switched is named after it: a big, bright stereo synth in the manner of those modular sounds,
and the engine behind the Switched-On Scarlatti example (`examples/scarlatti`).

**How it sounds.** Each note is a **stack** of up to seven saw waves, a little out of tune with
each other by `spread` and spread across the stereo field by `width`, which is what makes a
synth sound wide and lush rather than thin. Each saw can be **hard-synced** by `grit`: restarted
every time a hidden "master" oscillator at the note's pitch comes round, while the saw itself
runs faster, so its shape tears and its upper harmonics shift as it plays. That's the sync sweep,
a classic of synth leads. Then the stack goes through a **ladder filter** modelled on Moog's: four
one-pole low-passes in a row, 24 dB an octave, with the last fed back to the first for
resonance. Robert Moog's transistor ladder is the filter of the Minimoog and his modulars, and its
warm, heavy resonance is much of their sound. Here the feedback goes through a soft clipper,
which lets `scream` push the resonance past the point where the filter rings on its own without
running away: it growls, then screams, then sings. Each side has its own ladder, so the stereo
survives it.

One envelope, shaped by `snap`, drives both the level and the ladder. It's one knob in place of
two ADSRs: a pluck at the bottom, an organ's held note at the middle, a slow swell at the top.

| Setting   | Range             | Default | What it does                                          |
|-----------|-------------------|---------|-------------------------------------------------------|
| `stack`   | `one` to `fifths` | `swarm` | what's stacked: see below                             |
| `spread`  | 0 to 1            | 0.3     | detune between the saws, up to 50 cents either side   |
| `width`   | 0 to 1            | 0.6     | how far the stack spreads left and right              |
| `grit`    | 0 to 1            | 0       | hard sync: saws up to four times their master's speed |
| `ladder`  | 20 Hz to 16 kHz   | 900 Hz  | the ladder's cutoff at middle C, before the envelope  |
| `scream`  | 0 to 1            | 0.3     | resonance and drive, until it rings on its own        |
| `contour` | 0 to 1            | 0.45    | how far the envelope opens the ladder, to 7 octaves   |
| `snap`    | 0 to 1            | 0.35    | the envelope: pluck, organ, swell                     |
| `gain`    | 0 to 1            | 0.25    | the output level                                      |

The **stacks**, in order (for `:set`, 0 to 4):

- `one`: a single saw, in the middle.
- `pair`: two saws, either side, detuned apart.
- `swarm`: seven saws detuned evenly and panned alternately, so each side gets the whole spread.
- `stops`: an organ's stops: a saw an octave down (16') in the middle, pairs at pitch (8') and
  an octave up (4') spread out.
- `fifths`: the note and its fifth in pairs, and the octave on top: a ready-made power chord.

More about each control:

- **`spread`** at 0 puts every saw at the same pitch, which is louder but plain; a little
  (0.1 to 0.3) is the chorused shimmer of a string machine; the top is wild, beating detune.
- **`grit`** rises with the envelope too: the master's restarts get faster as the envelope opens,
  so a note sweeps as it plays, and more so with `contour` up.
- **`ladder`** follows the keyboard half way: an octave up the keys raises the cutoff half an
  octave, so high notes aren't muffled and low ones aren't harsh.
- **`scream`** below about 0.5 is a classic resonant sweep; past 0.8 the filter sings a note of
  its own at the cutoff, which you can play by sweeping `ladder`.
- **`contour`** opens the ladder further for harder notes.
- **`snap`** from 0 to 0.5 goes from an instant pluck that dies in about a tenth of a second,
  through ever longer decays and higher sustains, to an organ: instant, held at full level. From
  0.5 to 1 the attack slows to a swell of a second and a half, and the release lengthens. The
  ladder's own envelope decays faster than the level's and holds lower, so a pluck closes as it
  fades and an organ note keeps some edge.

A voice starting from silence starts its saws at random points in their cycles, as free-running
analogue oscillators would be, so a big stack doesn't open with every saw jumping at once. A `G`
gesture glides its notes. It costs about five times the test synth's work, still hundreds of
times faster than real time.

### Monochord, the string (`monochord`)

The monochord is one string stretched over a sound box, with a bridge you can move along it.
The Greeks credited Pythagoras with using it to discover that intervals are ratios: stop the
string at half its length and it sounds an octave higher (2:1); at two thirds, a fifth (3:2);
at three quarters, a fourth (4:3). It's where the mathematics of tuning begins, so it names
Lattice's second engine, a string you can pluck or bow.

**How it sounds.** Each of the eight voices is a string, modelled as a **waveguide**: two delay
lines, from the bow to the bridge and from the bow to the nut, the wave travelling round them
and inverted at each end, as on the Synthesis ToolKit's bowed string (Perry Cook and Gary
Scavone's library of instrument models). At the bridge a gentle low-pass and a loss take a
little away each trip, so high notes and high harmonics die sooner than low ones, as on a real
string. The loop is read with fine interpolation, so every note is in tune (to within 3 cents,
which the tests check).

- **A pluck** fills the string with a burst of noise, softened from a thumb (nearly a sine) to a
  hard pick (bright, and with a click of its own), and notched where the string is plucked: a
  string can't move at the point you hold it, so the harmonics with a node there go missing.
  That's why plucking near the bridge sounds thin and nasal and plucking in the middle hollow.
  Harder notes pluck brighter. Plucking a string that's still ringing keeps 30% of what was there.
- **A bow** is modelled as friction: where the bow meets the string, the bow drags the string
  along until it slips and snaps back, again and again. That stick-slip motion, which Hermann von
  Helmholtz described in the nineteenth century, is what gives a bowed string its sawtooth tone.
  Bowing keeps the string sounding while the note is held, and its pressure wavers a little, as
  a hand's does.
- **When a note ends**, only the bow lifts: a plucked string rings on by its `decay`, as a
  harp's or a guitar's open string does. Choking the track fades every string out.

| Setting    | Range              | Default | What it does                                       |
|------------|--------------------|---------|----------------------------------------------------|
| `model`    | `nylon` to `sitar` | `nylon` | the kind of string: see below                      |
| `pluck`    | 0 to 1             | 0.5     | what plucks it, from a soft thumb to a hard pick   |
| `position` | 0.02 to 0.5        | 0.15    | where it's plucked, by the bridge (thin) or middle |
| `decay`    | 0.2 to 20 s        | 3 s     | time for low notes to fade by 60 dB                |
| `tone`     | 0 to 1             | 0.6     | how much high end survives each trip along it      |
| `bow`      | 0 to 1             | 0       | bowing while held: 0 only plucks, 1 only bows      |

The **models**, in order (0 to 2):

- `nylon`: warm, like a classical guitar.
- `steel`: each note on a pair of bright strings 7 cents apart, spread a little left and right so
  they beat, as on a twelve-string guitar, with a trace of the slight fall in pitch of a hard
  pluck.
- `sitar`: the string buzzes against a curved bridge, as a sitar's does against its jawari,
  shortening the string while it swings wide: a buzz that brightens the attack and settles as
  the note fades.

More about the controls:

- **`position`** is a fraction of the string's length: 0.5 is the middle, which notches out every
  even harmonic.
- **`decay`** is for the lowest notes; the higher a note, the sooner it dies, as on a real string,
  and `tone` takes more away still.
- **`bow`** takes over from the pluck as it's turned up: a little bow under a pluck sustains it
  like an EBow; the middle is a cello section; at 1 there's no pluck at all. It raises the bow's
  speed and grip together, so light bowing skates over the string and heavy bowing grips and
  grits. Bowed notes swell for a second or two on low, long-ringing strings before they hold,
  so long notes suit it.

`decay`, `tone` and `bow` change the strings already sounding; the others apply from the next
note. A `G` gesture glides a note in from the last one's pitch: the string is plucked there and
slid along.

### Krell, the alien harpsichord (`krell`)

In Forbidden Planet (1956), explorers find on the planet Altair IV the works of the Krell, a
civilisation that vanished overnight, leaving behind a vast machine beneath the planet's surface,
still running. The film's score, Louis and Bebe Barron's "electronic tonalities", was made from
circuits they built and drove until they burned out: the first film score made entirely
electronically. Krell is the harpsichord such a civilisation might have built. It's the solo
instrument of the Switched-On Scarlatti example.

**A harpsichord** plucks its strings rather than striking them: each key lifts a jack carrying a
plectrum, traditionally a sliver of bird's quill, which plucks the string as it passes and
slides back past it as the key falls, while a felt damper on the jack stops the string. Big
harpsichords have several sets of strings, **choirs**, which the player chooses between with
**stops**, named in feet after organ pipes: **8'** sounds at the written pitch, **4'** an octave
above, **16'** an octave below and **2'** two octaves above.

**How it sounds.** Krell's strings are Monochord's waveguides, plucked by a quill rather than
bowed. Each key plucks every choir its `stops` choose (up to five strings a note, eight notes,
so forty strings), each choir plucked at its own point (8% to 20% of the way along), at its own
level and somewhere different in the stereo field. The quill plucks near the end of the string,
bright and thin, with a click as it snaps off; a harpsichord's quills pluck much the same however
hard the key is struck, so velocity changes the level by only up to 30%. `drift` pulls the choirs
out of tune with each other and sets each wavering slowly, so they beat and their beating moves.

Under the strings is the **body**, a modal resonator of eight modes, which every string plays
into. Its lowest mode is at `hull`, and `glass` moves the rest from wood (in tune with each
other, the hull's harmonics, and soon still) to glass and metal plates (out of tune, ringing on
for seconds). Each quill's click sets it ringing faintly, and notes in tune with its modes ring it
up, loud, and on after them: tune `hull` to the song's key and it rings with the notes in it.
Even modes ring on the left and odd ones on the right.

| Setting  | Range               | Default | What it does                                  |
|----------|---------------------|---------|-----------------------------------------------|
| `stops`  | `8'` to `16' to 2'` | `8' 8'` | the choirs each key plucks                    |
| `drift`  | 0 to 1              | 0.15    | how far the choirs drift apart and waver      |
| `quill`  | 0 to 1              | 0.7     | the plectrum, from soft leather to hard metal |
| `decay`  | 0.2 to 20 s         | 4 s     | time for low notes to fade by 60 dB, key held |
| `damper` | 0 to 1              | 0.8     | how fast strings stop when the key comes up   |
| `body`   | 0 to 1              | 0.3     | how loudly the body rings in sympathy         |
| `hull`   | 40 Hz to 2 kHz      | 196 Hz  | the body's lowest mode                        |
| `glass`  | 0 to 1              | 0.5     | the body, from wood to glass and metal plates |
| `gain`   | 0 to 1              | 0.5     | the output level                              |

The **stops**, in order (0 to 4):

- `8'`: one string at pitch.
- `8' 8'`: two at pitch, the second plucked further from its end, so rounder.
- `8' 4'`: one at pitch and one an octave up.
- `16' 8' 4'`: an octave down too.
- `16' to 2'`: all five, from an octave down to two octaves up. More stops are louder, but not
  by as much as they add up to.

More about the controls:

- **`drift`** at a little (0.1 to 0.2) is a real harpsichord's shimmer, its choirs never quite in
  tune; at the top, about 30 cents, it's something stranger. Each choir goes its own way: the
  second 8' sharp, the 16' and 2' flat, the 4' a little sharp. Turning it doesn't make ringing
  strings jump; they bend.
- **`quill`** from leather (soft, darker, the damping in the string's loop higher) to metal
  (bright, ringing, with a louder click). Leather plectra were used on some late
  eighteenth-century French harpsichords for a softer tone.
- **`damper`** at 0 leaves the strings ringing when the key comes up, as a harp's do; at 1 they
  stop in about 40 ms, dulling as they go, with a quieter click as the quill slides back.
- **`hull`** defaults to 196 Hz, G-3. Set it to the song's key note, or a fifth, and the body
  blooms on the notes of the key.
- **`glass`** at 0 is wood, ringing for under half a second; at 1 plates of glass and metal that
  ring for six, inharmonic, like a bell. At high `body` and `glass`, the harpsichord sits in a
  halo of its own.

Its notes don't glide. Choking the track fades the body with the strings. The `great-machine`
preset is Krell at its most alien: all five choirs drifting apart over a glass body, down
echoing halls, with the Great Machine of the Krell drawn on its face.

### Tuss, the electric piano (`tuss`)

In 2007 two records came out on Rephlex, *Confederation Trough* and *Rushup Edge*, by The Tuss:
Brian and Karen Tregaskin, a brother and sister from Cornwall, said the sleeves. Nobody had heard
of them, and they sounded a great deal like Richard D. James, Aphex Twin, who grew up in
Cornwall and co-founded Rephlex. It's widely taken to have been him. (The name is a word of
Cornish dialect best not translated here.) I named Tuss in his honour, and built it for a song
in the spirit of his "Flim" (1997), whose soft, glassy keys float over drums that never sit
still. He's known, too, for retuning his synthesisers away from equal temperament, and helped
Korg put microtuning into the monologue; so Tuss can play in other tunings.

**An electric piano**, as the Fender Rhodes of the 1960s and 70s, has no strings. Each key
throws a felt-tipped hammer at a **tine**, a thin steel rod clamped at one end, which rings with
a heavier **tone bar** clamped beside it, the two a tuning fork. Facing the tine's free end is a
**pickup**, a coil round a magnet: the tine swinging through its field makes a current, as a
guitar string does over its pickup. The field isn't even, so what comes out isn't quite the
tine's swing. Struck hard, the tine swings far into the field's curve and the note **barks**,
the growl of a Rhodes played hard. Where the pickup sits across the tine changes the tone too,
and technicians set it by hand, note by note: in line with the tine, it hears it twice a swing,
hollow, an octave up; off to one side, once, round and full.

**How it sounds.** Tuss's tine is modal (see
[how the engines make sound](#how-the-engines-make-sound)): its fundamental, which rings on for
seconds, and two of a clamped bar's upper modes, 6.27 and 17.55 times the note, out of tune with it.
They clang at the start of the note and are gone in under half a second: the bell. How much of them
the hammer rings depends on how long it touches the tine, shorter for a harder hammer and a harder
note, so soft notes are round and hard ones bright. Hard notes are louder, by much more than on
Krell, and swing the tine further into the pickup's field, so they bark too. Each note's pickup
hears its tine through a field that falls away either side of the pole, `1 / (1 + u²)`, where `u` is
how far across the tine is.

`drift` sends each note out of tune its own way and sets them all wavering together, as on a
stretched, worn tape, and sits each in stereo the way it's drifted. `tuning` and `key` play the
notes in other tunings than equal temperament.

| Setting  | Range                 | Default | What it does                                    |
|----------|-----------------------|---------|-------------------------------------------------|
| `strike` | 0 to 1                | 0.5     | the hammer, from soft felt to hard              |
| `bell`   | 0 to 1                | 0.4     | how loud the tine's clanging upper modes are    |
| `pickup` | 0 to 1                | 0.7     | where the pickup sits, from in line to one side |
| `bark`   | 0 to 1                | 0.3     | how hard the pickup's driven: clean to growling |
| `decay`  | 0.2 to 20 s           | 6 s     | time for low notes to fade by 60 dB, key held   |
| `damper` | 0 to 1                | 0.7     | how fast tines stop when the key comes up       |
| `drift`  | 0 to 1                | 0.1     | how far the notes drift out of tune and waver   |
| `tuning` | `equal` to `harmonic` | `equal` | the tuning the notes play in                    |
| `key`    | `C` to `B`            | `C`     | the key note the tuning's built on              |
| `gain`   | 0 to 1                | 0.5     | the output level                                |

More about the controls:

- **`strike`** at 0 is a soft felt hammer: round and dark, the bell barely there whatever
  `bell` says. At 1 it's hard, and the bell rings out. Velocity always counts: a hard note
  touches the tine for less time than a soft one, so it's brighter.
- **`bell`** sets how loud the clang can be. At 0 there's none, only the tine's fundamental
  and what the pickup makes of it; at 1, with a hard `strike`, the start of each note is a bell.
- **`pickup`** at 0 sits in line with the tine, so it hears the note an octave up, hollow and
  quiet when played softly; at 1, off to the side, the note itself, round and full.
- **`bark`** at 0 is clean however hard a note is played; turned up, hard notes growl and soft
  ones stay sweet, so playing harder and softer changes the tone as much as the level.
- **`decay`** is for C-3; higher notes ring for less, an octave up three quarters as long, as on
  a real electric piano.
- **`damper`** at 0 leaves the tines ringing when the key comes up; at 1 they stop in about
  50 ms.
- **`drift`** at a little (0.1 to 0.3) is an old tape: each note up to 20 cents off its own way
  at the top, all wavering 8 cents together, about three times every ten seconds. Turned up,
  the chords smear and spread across the stereo field.

**Tunings.** Equal temperament, every piano's, splits the octave into twelve equal semitones,
so every key sounds alike and none is quite in tune: its major third is 14 cents wider than the
pure 5/4 of two strings ringing together. The other tunings make some intervals pure at the cost
of others, so they're built on a `key`, the note they're in tune from. In `just` on A, an A
major chord rings without beating, while chords far from A sound strange. The key note itself
stays where equal temperament has it, so Tuss stays in tune with the rest of the song on it.

- `equal`: twelve equal semitones, as the rest of Lattice plays.
- `just`: five-limit just intonation, every interval from the key a ratio of small whole
  numbers: 1/1, 16/15, 9/8, 6/5, 5/4, 4/3, 45/32, 3/2, 8/5, 5/3, 9/5, 15/8. Pure major and minor
  thirds, the sweetest chords on the key and those near it.
- `pythagorean`: pure 3/2 fifths stacked up, as medieval music was tuned. Bright, wide thirds.
- `meantone`: quarter-comma meantone, the Renaissance's tuning: fifths narrowed so their thirds
  are pure, with one "wolf" fifth, from G♯ to E♭ when the key is C, that howls.
- `werckmeister`: Werckmeister III (1691), a well temperament, every key playable but each
  with its own colour, as Bach's keyboards may have been tuned.
- `harmonic`: the harmonic series, folded into an octave: its seventh (7/4) and eleventh (11/8)
  are far from anything equal temperament has, bluesy and strange.

Its notes don't glide. The `tregaskin` preset is a warm electric piano on a worn tape, through
an ensemble into a small room, with the tine on its face; its `worn` macro turns up the drift
and the chorus, and `growl` the bark. Six more go round the piano's corners: `flim` is soft and
glassy, a felt hammer close up, for a quiet phrase over a busy beat; `suitcase` the seventies
stage piano, barking through a phaser; `music-box` short, bright tines into a long echo;
`hollow-tine` has the pickup in line, an octave up and hollow; `monologue` is tuned to the
harmonic series and drifts, after the microtunings Aphex Twin made for Korg's monologue
synthesiser; and `pentagon` is a barking bass piano, held level by a compressor.

### Bessel, the FM synthesiser (`bessel`)

Friedrich Bessel (1784–1846) was an astronomer at Königsberg, the first to measure how far away
a star is: in 1838, by watching 61 Cygni shift against the stars behind it as the Earth went
round the Sun. Working out the planets' orbits, he set down a family of functions that turn up
wherever something swings round and round, and they're the arithmetic of this engine: how loud
each partial is when one sine wave bends another. I named it for him because his functions are
what you hear.

The bending was found by ear. In 1967 John Chowning, a composer at Stanford, was trying out
vibrato on a computer and sped it up past anything a singer could do: at some point it stopped
being vibrato and became a new tone, bright or metallic or bell-like, from just two sine waves.
He published it in 1973, Stanford licensed it to Yamaha, and in 1983 Yamaha's DX7 put it in
every studio, the first digital synthesiser most people heard and, for years, the best selling
of any kind. Its electric piano is on half the ballads of the 1980s, its basses, bells and brass
on most of the rest. The cheaper four-operator Yamahas that followed, like the TX81Z (1987),
went into house and techno, whose bass lines are full of them. Bessel is one of those: four
operators, eight algorithms, and the extra waveforms the TX81Z added.

**Frequency modulation.** Vibrato moves a note's pitch up and down a few times a second. Move it
hundreds of times a second, as fast as a note, and you no longer hear the pitch moving: you hear
new partials, **sidebands**, either side of the note, spaced by the modulating frequency. The
wave being bent is the **carrier**, the one bending it the **modulator**, and how far it bends
is the **index**. Bessel's functions `Jₙ(index)` say how loud the `n`th pair of sidebands is;
the carrier itself is `J₀`. Here's a carrier `c` and its sidebands, `c ± n` times the
modulator's frequency, bent three ways:

```
index 0.5 (level 0.2)          index 2 (level 0.4)          index 5 (level 0.63)
               ▆▆
               ██                         ▂▂    ▂▂
               ██                      ▃▃ ██    ██ ▃▃           ▅▅ ▄▄    ▂▂    ▂▂    ▄▄ ▅▅
         ▁▁ ██ ██ ██ ▁▁          ▁▁ ▄▄ ██ ██ ▇▇ ██ ██ ▄▄ ▁▁   ▄▄ ██ ██ ██ ▁▁ ██ ▆▆ ██ ▁▁ ██ ██ ██ ▄▄
         -2 -1  c +1 +2          -4 -3 -2 -1  c +1 +2 +3 +4   -6 -5 -4 -3 -2 -1  c +1 +2 +3 +4 +5 +6
```

A little bending adds a pair or two of sidebands, warm, like a filter opening a little. More
spreads the sound wider, about `index + 1` pairs each side, and the carrier fades, even to
nothing: at an index of 2.405, the first zero of `J₀`, it's gone, and only the sidebands are
left. So a modulator's level is like a filter's cutoff, except that it doesn't only let
harmonics through but makes them, and moving it through the note, by the modulator's own
envelope, is what makes FM come alive: a piano bright as it's struck and mellowing as it rings,
brass that brightens as it swells.

**Ratios** say where the sidebands land. With the modulator at a whole number times the carrier
(1, 2, 3), they fall on the note's harmonics, so it sounds pitched, like a string or a horn: a
ratio of 1 makes every harmonic, like a saw, and 2 only the odd ones, hollow, like a clarinet.
At a ratio that isn't whole (1.4, 3.5) they fall between the harmonics, as a bell's partials do,
and it clangs.

**Operators** are what Yamaha called the sine waves: each a sine oscillator with its own envelope,
a carrier if it's heard, a modulator if it bends another. **Feedback** sends operator 4 back into
itself: a little brightens its sine towards a saw, a lot turns it to noise, which is how FM makes
a snare's rattle and a hat's hiss. Strictly, Bessel bends the phase, not the frequency, as
Yamaha's did: the same sidebands, and the pitch stays put however hard it's bent.

**The algorithms** wire the operators together. Each operator modulates only ones numbered lower
than itself; `▼` is heard:

```
  stack        branch        crown         pairs
    4            4                         2     4
    │            │           2  3  4       │     │
    3            3   2       └──┼──┘       1     3
    │            └─┬─┘          1          ▼     ▼
    2              1            ▼
    │              ▼
    1
    ▼

  fan          chain         trio          organ
    4            4                 4
 ┌──┼──┐         │                 │
 1  2  3         3   1       1  2  3       1  2  3  4
 ▼  ▼  ▼         │   ▼       ▼  ▼  ▼       ▼  ▼  ▼  ▼
                 2
                 ▼
```

`stack` is the brightest, each operator making the next richer; `pairs` is two two-operator
voices at once, as the DX7's electric piano was built; `fan` puts one modulator on three
carriers tuned apart; and `organ` bends nothing, four sines added, as a drawbar organ adds pipes.
Two or more carriers share the output equally, so changing the algorithm doesn't jump the level.

**Drums.** Set an operator's `track` to 0 and its pitch stops following the note: it plays its
`ratio` times C-4 (261.6 Hz) whatever note is played. With the `pitch` envelope, which starts
every operator up to four octaves away and sweeps it back over `sweep`, that makes drums: a sine
swept down from high is a kick, a body under a carrier bent by feedback noise is a snare, high
operators at ratios nothing like each other are a hat. The presets below have one of each.

Its settings are the voice's own, then ten for each operator, numbered 1 to 4: `ratio1` is
operator 1's ratio, `level4` operator 4's level. On the instrument page they're in that order.

| Setting     | Range                 | Default | What it does                                 |
|-------------|-----------------------|---------|----------------------------------------------|
| `algorithm` | `stack` to `organ`    | `pairs` | how the operators are wired, as drawn above  |
| `feedback`  | 0 to 1                | 0       | operator 4 bending itself: sine, saw, noise  |
| `pitch`     | -48 to 48 st          | 0       | where every operator's pitch starts          |
| `sweep`     | 0.002 to 2 s          | 0.05 s  | how fast it falls back, to 1/e               |
| `keyscale`  | 0 to 1                | 0.3     | how much higher notes are shorter and duller |
| `tuning`    | `equal` to `harmonic` | `equal` | the tuning notes play in, as on Tuss         |
| `key`       | `C` to `B`            | `C`     | the key note the tuning's built on           |
| `gain`      | 0 to 1                | 0.5     | the output level                             |

And for each operator, `1` to `4`:

| Settings                                        | Range          | What it does                  |
|-------------------------------------------------|----------------|-------------------------------|
| `ratio1` `ratio2` `ratio3` `ratio4`             | 0.125–32       | its pitch, times the note's   |
| `detune1` `detune2` `detune3` `detune4`         | ±100 c         | its pitch nudged              |
| `level1` `level2` `level3` `level4`             | 0–1            | how loud, or how far it bends |
| `attack1` `attack2` `attack3` `attack4`         | 0.5 ms–10 s    | how long it takes to rise     |
| `decay1` `decay2` `decay3` `decay4`             | 5 ms–20 s      | how long it takes to fall     |
| `sustain1` `sustain2` `sustain3` `sustain4`     | 0–1            | what it falls to, key held    |
| `release1` `release2` `release3` `release4`     | 5 ms–20 s      | how long it fades, key up     |
| `velocity1` `velocity2` `velocity3` `velocity4` | 0–1            | how much soft notes soften it |
| `track1` `track2` `track3` `track4`             | 0–1            | how far it follows the note   |
| `wave1` `wave2` `wave3` `wave4`                 | `sine`–`pulse` | its waveform                  |

The defaults are an electric piano, on `pairs`: operators 1 and 3 heard, 2 bending 1 at the same
pitch for the body, and 4 bending 3 at fourteen times it, gone in a moment, for the tine's ping.

More about the controls:

- **`level`** is heard as loudness on a carrier. On a modulator it's the index, squared so the
  bottom of its travel is finely graded: 0.2 bends gently (an index of 0.5), 0.4 brightly (2),
  and 1 as far as a DX7 can (4π, about 12.6), a harsh, glittering wall of sidebands.
- **`ratio`** below 1 puts a modulator under the note, which makes sidebands below it too, for
  growling basses; on a carrier, it plays the note an octave or more down.
- **The envelope**: the attack rises in a straight line, the decay falls towards `sustain` and the
  release towards nothing, each by 60 dB in its time, as Lattice's other decays do. Most FM
  sounds give the modulators shorter decays than the carriers, so notes start bright and
  mellow.
- **`velocity`** at 0 plays every note the same; at 1, a soft note is all but silent from a
  carrier and barely bends from a modulator, so harder notes are louder and brighter at once.
- **`keyscale`** shortens every envelope for notes above C-4 and lengthens it below: at 1, an
  octave up lasts half as long. It also bends the modulators of high notes less, as a real
  piano's high strings are shorter-lived and purer.
- **`wave`**: `sine`; `half`, the top half of a sine with the bottom flattened, brighter and
  rounder; `abs`, both halves turned up, an octave higher and buzzy; and `pulse`, a whole sine
  squeezed into half the cycle, then nothing, nasal and bright. Bent, each makes its own family
  of sidebands.
- **`feedback`** brightens the sine a little at 0.2, makes a saw's harmonics by 0.6, and noise
  past 0.7.
- **`tuning`** and **`key`** are Tuss's (see [Tuss](#tuss-the-electric-piano-tuss)).

Its notes don't glide; the pitch envelope sweeps instead. Choking the track fades it in a few
milliseconds.

**The presets** are in `bessel/`. Its keys and bells: `ballad`, the eighties electric piano,
with an ensemble; `chowning`, the bell in Chowning's 1973 paper, a modulator at 1.4 times the
carrier bent hard at the strike and less as it rings; `campanile`, tubular bells, a modulator
at 3.5 times; and `rosewood`, a marimba. Its basses: `lately`, the plucked bass of house
records, after a famous preset of the TX81Z's; `slap`, a slapped bass's pop over a round body;
and `sawmill`, the four in a stack, feeding back into a growl. Then `fanfare`, a brass section
swelling brighter as it's blown. And its drums: `boom`, a kick; `rimshot`, a snare; `tinsel`, a
hat; and `more-cowbell`, the drum machine's, at 540 and 800 Hz.

### Mosaic, the bowed string (`mosaic`)

Around 1990, at IRCAM, the institute for music and acoustics Pierre Boulez founded under the
Pompidou Centre in Paris, Jean-Marie Adrien and Joseph Morrison built a program called MOSAIC.
It made sound by describing things that vibrate, strings and plates and tubes and membranes, as
their modes, and joining them up with what sets them going: a bow, a hammer, a reed, a finger.
Because it only needed the modes, it could join anything to anything. A violin's string could
be bowed on a steel plate the size of a room, or a drum played with a bow. It was renamed
Modalys, and composers at IRCAM still use it to build instruments that can't exist.

That's what this engine is for, and why it has the name. I wanted a bowed string that plays
like a violinist plays it, attack, slur, vibrato, so it can carry a real melody, but whose body
can be anything: a violin's own wood, a glass bowl, a plate of iron, a singer's open mouth, or
something halfway between, as small as a toy or as big as a church. It began with Tchaikovsky's
violin concerto: arranging it, I didn't want an orchestra imitated, I wanted the sounds an
orchestra couldn't make.

**The string.** Each of the eight voices is a waveguide string bowed by friction, as
Monochord's is (see [Monochord](#monochord-the-string-monochord)): the bow drags the string
until it slips back, again and again, the motion Hermann von Helmholtz first watched through a
microscope on a violin string in the 1860s. It only works in a window. Press too lightly for
the bow's speed and it skates, airy and whistling; too hard and it crunches. The acoustician
John Schelleng drew that window in 1973, and violinists live in it. A model can fall out of it
in a way a player never would, settling into a weaker, rougher motion depending on how the note
began, so Mosaic ties the bow's grip to its speed and keeps every note inside the window. Low
strings ring longer than high ones once the bow lifts, as on a violin.

**Playing it as a player does.** Mosaic listens to how the notes join:

- **A slur.** A note that starts at the very moment the last one ends, on the same track, is
  slurred: the bow stays on the string and only the finger moves, so a legato line is one long
  stroke, with no new attack on each note. The pitch slides there in `glide`, from a finger's
  change at the default to a singer's portamento turned up. To write a slur, give each note the
  length that reaches the next.
- **A new bow.** A note after a gap, even a sixteenth's, starts a new stroke, with `bite`'s
  attack. So shorter notes than the gaps between them play detached, as a violinist plays
  separate bows.
- **The same note again**, joined, changes the bow's direction: it slows for a moment and
  speeds up again, with a little rosin, as the bow turns, so repeated notes are heard as
  repeated without stopping.
- **Vibrato** starts a moment into each note, an eighth of a second, and comes in over the
  next half second, as a player's does: long notes sing, and fast passages, whose notes are
  over before it starts, stay clean. Its speed wanders a little, as a hand's does.

Arpeggios (`A`, chapter 5) are slurred, the bow staying on through them. A `G` gesture glides a
new bow in from the last note played, as a shift of the hand.

**The body.** Every string sounds through one body, shared, as a violin's four strings share
theirs. It's 24 modes, each a resonance with a pitch, a level, a ring time and a place in the
stereo field, so a chord spreads out as the different strings excite different modes. There are
four sets of them, and `material` slides between them, each mode moving to the same mode of the
next, so a body can be halfway between two, and turning it while a note plays morphs the body
under the note:

- `0`, **wood**: a violin's resonances, as measured on real ones. The lowest, about 275 Hz, is
  the air inside, breathing through the f-holes; the strong pair just above, about 460 and
  530 Hz, are the top and back first bending together; then a dense crowd of modes through the
  middle, and a hill of them at 2 to 3 kHz, the bridge's own rocking, which gives a violin the
  bright edge that carries over an orchestra.
- `1`, **glass**: the modes of a glass bowl, few and pure and ringing for seconds. Each is a
  pair a few cents apart, as a wine glass's are when it isn't perfectly round, so they beat
  slowly against each other. A note that lands on one of them makes the body sing it back.
- `2`, **metal**: a plate's partials, out of tune with each other and crowding closer as they
  rise, as a gong's or a bell's do. A melody played on it has a halo of clang that isn't in any
  key.
- `3`, **voice**: the formants of a sung "ah", the resonances of a singer's throat and mouth,
  and a little chest below them. The string sings through them, a vowel with a bow's attack.

`size` scales the body: half the size is an octave higher, twice the size an octave lower,
and a bigger body rings longer, by the square root of its size. `ring` lengthens or shortens every
mode's ring, from dry and woody to a glass that keeps sounding after the bow has gone. A
quarter of the string is heard as it is, before the body, so the note is always clear.

**The halo.** Behind the body are five sympathetic strings, never bowed, tuned to the root and
fifth of `key`: the key's note above middle C, the octave below it, the fifth above it, the
octave above, and the twelfth. When the body plays one of their notes, or a harmonic of one,
they ring in sympathy, on and on, panned across the field. It's the viola d'amore's trick, and
the Hardanger fiddle's and the sitar's: notes in the key glow, notes outside it don't.

| Setting    | Range           | Default | What it does                                         |
|------------|-----------------|---------|------------------------------------------------------|
| `material` | 0 to 3          | 0       | the body: wood, glass, metal, voice, and between     |
| `size`     | 0.25 to 8       | 1       | how big the body is: bigger is lower and rings on    |
| `bow`      | 0 to 1          | 0.5     | from skating lightly and airily to pressing hard     |
| `bite`     | 0 to 1          | 0.6     | the attack, from a slow swell to an instant bite     |
| `vibrato`  | 0 to 1          | 0.3     | how wide and fast, coming in a moment into the note  |
| `halo`     | 0 to 1          | 0.2     | how loudly the sympathetic strings ring              |
| `key`      | `C` to `B`      | `D`     | what they're tuned to                                |
| `glide`    | 0 to 0.5 s      | 0.015 s | how long a slurred note takes to reach its pitch     |
| `ring`     | 0 to 1          | 0.3     | how long the body rings                              |
| `gain`     | 0 to 1          | 0.5     | the level                                            |

More about the controls:

- **`bow`** raises the bow's speed, and so its loudness, with its pressure. Light, it breathes:
  a hiss of bow hair goes into the body with the note. Heavy, it's loud and gritty.
- **`bite`** is how fast the bow takes hold, from a quarter of a second's swell to three
  thousandths, and at the hard end the bow starts faster than it carries on, as a player's
  does for an accent, with a scratch of rosin, so the note speaks at once.
- **`vibrato`** at 0.3 is a violinist's, about 16 cents either way, five times a second. At 1
  it's a hundred cents, a semitone, nearly nine times a second: no one's hand, a siren.
- **`key`** defaults to D, the key of Tchaikovsky's concerto, where this engine started.

All of them change the notes already sounding: the body and halo at once, the bow, bite and
vibrato on the strings, and `glide` from the next slur. Choking the track fades every string
in a few milliseconds; the body and halo ring out after the strings have stopped, and then
Mosaic does nothing at all until the next note.

**The presets** are in `mosaic/`. `fiddle` is the nearest Mosaic comes to a violin, in a hall,
and where the others start; its `sing` macro widens the vibrato and leans on the bow, `tasto`
lightens both, as bowing over the fingerboard does. `glass-violin` is the same string on a
glass bowl, the halo up; `iron-cello` a metal plate a cello's size, bowed hard; `choir-string`
the string through a singer's "ah"; `nave` a body the size of a building, halfway from wood to
glass, every note leaving a place behind it; and `kit`, after the pocket fiddle dancing masters
carried, tiny and quick and bright, for fast passages.

## Rhythmicon, the drum engine

In 1930 the composer Henry Cowell asked Leon Theremin, inventor of the instrument that bears his
name, to build a machine that could play rhythms too intricate for any drummer. The result, the
**Rhythmicon** (1931), is called the first drum machine. Each key played a note of the harmonic
series together with a rhythm in the same ratio, the second harmonic twice as fast as the first,
the third three times, so it could play many rhythms against each other at once. Inside, discs
with rings of holes spun in front of a lamp, chopping its light onto photocells that made the
tones. Lattice's first engine, the drums, is named after it, and the drums' art is its lamp
shining through its spinning disc.

Rhythmicon has five drums, each an instrument of its own, so a track plays one drum and keeps its
own rules, length, mixer strip and effects: `kick`, `snare`, `hat`, `clap` and `tom`. A kit is a
group of tracks (see [chapter 3](03-tracks.md)).

What every drum shares:

- **One voice.** Every note restarts the drum; it ignores note-offs and rings out by its decay,
  so a note's length doesn't matter. A hit that lands while the drum still rings starts from
  where the wave was, smoothing the jump over 2 ms, so a fast roll doesn't click on every hit.
- **Tuning by note.** C-4 plays the drum at its `tune`; other notes tune it up or down from
  there, a semitone at a time. So a tom track can play a melody, and C-3 is a kick an octave
  down.
- **Velocity** scales the level.
- **Settings** apply from the next hit, except `decay`, which also shortens or lengthens the hit
  ringing. Every `decay` is the time to fall by 60 dB.
- **Models.** Each drum's first setting, `model`, is a choice that changes the drum's insides.
  The other settings keep their meaning within it, so a kick's `punch` is its punch in every
  model. The middle model is the default, and each is levelled to sit at much the same loudness.
  Models are named for how they sound rather than after the machines they recall, though they
  nod to them: Roland's TR-808 (1980), whose long, booming kick and analogue clap are all over
  hip-hop and electronic music, and the TR-909 (1983), harder and punchier, the sound of house
  and techno.
- **Choke.** Choked, a drum fades out over about 3 ms.
- **Mono.** A drum is the same in both sides; pan it on the mixer.

### The kick (`kick`)

A bass drum: a sine wave whose pitch drops fast from up to 12 times its tuning onto it, the
"punch" of a drum's skin settling after the beater, with a 2 ms click of noise for the beater
itself. It's held at full level for part of its decay before it falls, which is what gives an
808 its long boom, and goes through drive: saturation pushed off centre, so it adds even
harmonics, the warmth of a valve, fattening the tail and squaring it off.

| Setting | Range                 | Default | What it does                                      |
|---------|-----------------------|---------|---------------------------------------------------|
| `model` | `tight` `boom` `hard` | `boom`  | the kind of kick, which the others work within    |
| `tune`  | 30 to 120 Hz          | 50 Hz   | its pitch once the drop is over, at C-4           |
| `punch` | 0 to 1                | 0.5     | the pitch drop and click: a thud to a sharp click |
| `decay` | 0.05 to 2 s           | 0.45 s  | time to fade by 60 dB                             |
| `drive` | 0 to 1                | 0.35    | saturation: fattens the tail and squares it off   |

The **models** (0 to 2):

- `tight`: no hold, a shorter decay, a faster, shallower drop and more click: the small, clicky
  kick of a pocket drum machine.
- `boom`: an 808's, holding for a fifth of its decay before it falls.
- `hard`: a 909's, a deeper, faster drop, a loud click and extra drive.

Tune a kick to the song's key, or a note under the bass: at 50 Hz it's about G-1, and 55 Hz is A.
For a long 808 that plays bass lines, take `model` `boom`, `decay` over a second, and play notes
on it.

### The snare (`snare`)

A snare drum is two sounds: the shell and its skins, a pitched thump, and the **snares**, wires
stretched across the bottom skin that buzz against it. Here the shell is two modes of a membrane
(at 1 and 1.59 times its tuning, a drumhead's first two), with a small pitch drop, driven; the
wires are a burst of noise, high-passed, so it hisses.

| Setting | Range                 | Default | What it does                                   |
|---------|-----------------------|---------|------------------------------------------------|
| `model` | `tight` `fat` `crack` | `fat`   | the kind of snare                              |
| `tune`  | 100 to 400 Hz         | 180 Hz  | the shell's lower mode, at C-4                 |
| `decay` | 0.05 to 1 s           | 0.22 s  | time to fade by 60 dB                          |
| `snap`  | 0 to 1                | 0.6     | how much of the wires' noise against the shell |
| `tone`  | 1 to 12 kHz           | 5 kHz   | the brightness of the noise                    |

The **models** (0 to 2):

- `tight`: a clean, short shell and noise high-passed at 900 Hz: thin and short.
- `fat`: a driven shell that rings nearly as long as the wires, with noise kept down to 180 Hz:
  full and weighty.
- `crack`: a 909's, a bigger pitch drop, a hard-driven shell and louder, brighter wires.

`snap` low is mostly shell, a tom-like thud; high is mostly wires, a hiss. A low `snap`, low
`tone` and long `decay` make something like brushes; the `brushfire` preset starts there.

### The hat (`hat`)

A hi-hat, or a cymbal. The 808 made its cymbals and hats from six square-wave oscillators at
unrelated pitches, a cluster dense enough to sound like metal, and so does this, with noise
mixed in, all through a high-pass filter so only the sizzle is left. Short, it's a closed hat;
long, an open hat or a ride or crash.

| Setting | Range                   | Default | What it does                                 |
|---------|-------------------------|---------|----------------------------------------------|
| `model` | `metal` `ring` `sizzle` | `ring`  | the kind of hat                              |
| `tune`  | 100 to 800 Hz           | 205 Hz  | the lowest of the six squares, at C-4        |
| `decay` | 0.01 to 2 s             | 0.06 s  | short for a closed hat, long for an open one |
| `tone`  | 2 to 14 kHz             | 7 kHz   | the high-pass: lower lets more metal through |
| `noise` | 0 to 1                  | 0.3     | noise against the metal                      |

The **models** (0 to 2):

- `metal`: the squares multiplied in pairs, for a clangier metal. Multiplying two square waves
  is ring modulation, and for squares it's a logic gate's XOR; the 808 and 606 sum theirs, and
  the 909's hats are samples, so this is a sound of Lattice's own.
- `ring`: the 808's, the six summed.
- `sizzle`: mostly noise, through a steeper (18 dB an octave) high-pass: bright and airy.

An open hat cut short by the closed one is a choke group: put the open and closed hat tracks in
the same group (chapter 8). The Amen example's ride and crash are both hats: a ring at 320 Hz
with half a second's decay, and a sizzle ringing for a second and a half.

### The clap (`clap`)

A handclap is a handful of hands not quite together, then the room. The clap is bursts of noise
through a band-pass filter, a few milliseconds apart, then a tail fading in under them: the
808's recipe.

| Setting  | Range                  | Default | What it does                            |
|----------|------------------------|---------|-----------------------------------------|
| `model`  | `tight` `room` `crowd` | `room`  | the kind of clap                        |
| `tune`   | 500 to 3000 Hz         | 1100 Hz | where the noise's band sits, at C-4     |
| `decay`  | 0.05 to 1.5 s          | 0.3 s   | how long the tail rings                 |
| `spread` | 0.003 to 0.03 s        | 0.011 s | the time between the bursts, 3 to 30 ms |
| `room`   | 0 to 1                 | 0.5     | the tail against the bursts             |

The **models** (0 to 2):

- `tight`: two quick bursts and little tail: one tight snap.
- `room`: an 808's four, slightly uneven, and its tail.
- `crowd`: nine bursts at random gaps through a wider band: a room clapping out of time.

A wide `spread` sounds like a flam, several people; a narrow one like a single snap. Layered on
a snare (two tracks on the same steps), a clap is the backbeat of a great deal of dance music.

### The tom (`tom`)

A tom-tom: a tuned membrane. A sine that starts above its pitch and bends down onto it, through a
gentle saturation, with a short burst of softened noise for the stick.

| Setting | Range                  | Default | What it does                            |
|---------|------------------------|---------|-----------------------------------------|
| `model` | `tight` `round` `hard` | `round` | the kind of tom                         |
| `tune`  | 50 to 400 Hz           | 120 Hz  | its pitch once the bend is over, at C-4 |
| `decay` | 0.05 to 2 s            | 0.4 s   | time to fade by 60 dB                   |
| `bend`  | 0 to 1                 | 0.4     | how far above its pitch it starts       |
| `noise` | 0 to 1                 | 0.2     | the stick's noise against the shell     |

The **models** (0 to 2):

- `tight`: short, a small bend and more stick: short and dry.
- `round`: an 808's, a long, gentle bend: a round boom.
- `hard`: a 909's, with a second shell mode, a deeper, faster bend and drive: a harder, ringing
  shell.

Toms are pitched, so one track plays a whole fill: a run down from E-4 to C-4 to G-3. A long
`decay` and a big `bend` give the "pew" of a disco syndrum. Low and long, with a reverb, it's a
timpani: the Scarlatti example's timps are toms.

## Playing sound files

The sampler and chop play WAV files. They split by use: the **sampler** plays a sound across the
keys, pitched by the note or cut into slices, while **chop** rearranges a loop a step at a time.
(A kit of one-shot samples is a group of tracks, a sampler on each.) Both share the same voices:
sixteen of them, each reading the file with smooth (cubic) interpolation at the speed the note
and `tune` ask for, corrected for the file's own sample rate, and fading in and out over 2 ms at
the ends of what they play, so a cut never clicks.

**The files** they read are WAV: 8, 16, 24 or 32-bit, or 32 or 64-bit floating point, mono or
stereo, at any sample rate. A file is read once, off the audio thread, and shared by every track
playing it; a file changed on disk is read again. A file that can't be read leaves the
instrument silent, and the status line says why (`not a WAV file`,
`can't read 12-bit audio of format 1`, `no audio in it`).

**Where they live.** The song keeps each file's path in the instrument (`file`). `:sample` and
`:chop` take a path relative to where you started Lattice and store it whole. Samples Lattice
makes itself (`:export-sel`, `:export-loop` and the looper's takes) belong to the song: they go in
a `samples` folder beside it and are saved relative to it, so the song and its samples move
together; until the song has a file, they wait in a scratch folder, and `:w` copies the ones its
tracks play into `samples`. Sound files from elsewhere, such as a sample library, are left where
they are. Chapter 10 has more on files.

### The sampler (`sampler`)

`:sample PATH` puts a sampler on the track playing the WAV file at `PATH`:

```
:sample kick.wav
:sample ~/samples/choir-aah.wav
```

On the pattern page it changes every track the selections reach; elsewhere, the cursor's track.
A track already on a sampler keeps its settings and changes only its file; any other track gets a
new sampler at its defaults, and loses its face. The status line says `pad plays choir-aah.wav`.
`:sample` with no path plays the selection last exported with `:export-sel` or `:export-loop`,
even while it's still rendering, so

```
%            select the whole pattern
:export-sel  render it to a sample
:track-add VOX
:sample      and play it on the new track
```

turns a bar into a sample to play. A file that isn't there is refused at once
(`no file kick.wav`), and with nothing exported, `which file? nothing's been exported yet`.

**Playing it.** C-4 plays the file at the pitch it was recorded at; each semitone up or down
plays it faster or slower, higher or lower, as a tape does. With `slices` above 1, the region is
cut into that many equal slices instead, C-4 playing the first and each note up the next, at the
file's own pitch: a drum loop cut into sixteen slices plays its hits from sixteen notes. The
notes wrap round, so with 4 slices E-4 is the first again and B-3 the last.

| Setting     | Range                    | Default    | What it does                               |
|-------------|--------------------------|------------|--------------------------------------------|
| `mode`      | `one-shot` `gate` `loop` | `one-shot` | how notes play it: see below               |
| `start`     | 0 to 1                   | 0          | where the region played starts in the file |
| `end`       | 0 to 1                   | 1          | where it ends                              |
| `slices`    | 1 to 16                  | 1          | slices the region is cut into, one a note  |
| `tune`      | -24 to 24 st             | 0 st       | semitones up or down                       |
| `attack`    | 0.0005 to 2 s            | 0.0005 s   | a fade in at the start of each note        |
| `release`   | 0.005 to 4 s             | 0.03 s     | a fade out when the note ends              |
| `direction` | `forward` `reverse`      | `forward`  | which way it plays                         |
| `gain`      | 0 to 1                   | 0.7        | the output level                           |

The **modes** (0 to 2):

- `one-shot`: each note plays to the end of the region, however long the note is. Drum hits.
- `gate`: plays until the note ends, then fades out by `release`. Sustained sounds cut to length.
- `loop`: plays round the region, back to `start` each time it reaches `end`, until the note
  ends. Pads, drones, sustained instruments.

More about the controls:

- **`start`** and **`end`** trim the file to a region: `start` 0.5 plays only its second half. If
  `end` is before `start` they're swapped. Slices cut up the region, not the whole file.
- **`slices`** is a choice, 1 to 16, named by its count, so `:set 16` is 16 slices.
- **`tune`** applies to slices too, so a sliced loop can be pitched as a whole. Each `C-a` moves
  it about half a semitone (a hundredth of 48 semitones); `:set 7` puts it up a fifth exactly.
- **`attack`** is `0.0005s` at its smallest, half a millisecond: a tiny fade, there only to stop
  a click.
- **`direction`** `reverse` plays each note backwards, from the end of its region or slice.

Two gestures work on a sampler's notes (chapter 5): `<` plays one note the other way round from
`direction`, and `O8` starts a note 8/16 of the way into its sound (or its slice), `O0` to `O15`.

### Chop (`chop`)

**Chopping** a break is an old trick: hip-hop producers in the 1980s cut drum breaks from funk
records into hits on samplers like E-mu's SP-1200 and Akai's MPC and played them back in new
orders, and jungle and drum and bass in the early 1990s did the same at speed, above all to the
Amen break (from The Winstons' "Amen, Brother", 1969). Chop is that, the tracker way: a sound
file cut into equal pieces, one to a step, with each step a note that plays its piece, so a loop
can be rearranged a step at a time with the editing you already know.

`:chop PATH` cuts the WAV file at `PATH` into as many equal pieces as the track has steps (up to
256), and fills each step with the note for its own piece: C-4 for piece 0, C#4 for piece 1,
and so on. So straight away it plays the loop as recorded. On the pattern page it chops every
track the selections reach; elsewhere, the cursor's track. With no path it takes the last
selection exported. It replaces every step's note on the track (its locks are kept), so use it on
an empty track; the status line says `amen plays amen.wav, in 64 pieces`.

**The steps hold references to the audio, not the audio.** The file is never changed: what you
edit is which piece each step plays. So copy, paste, delete, undo, rules and locks all work on
pieces as they do on notes, and nothing you do to the steps can damage the recording.

**On the pattern page** a chop track is drawn wider: each row shows its piece number (in decimal,
like the rows, so an untouched loop reads straight down), `◂` if the piece plays backwards, and
the waveform of the audio the row plays, in braille, four dots down a row, each the loudest of a
quarter of the step, spread out from the middle. Here is the Amen example's chopped edit, a snare
roll on pieces 28 and 12 in turn:

```
 26     26 ⠀⠉⠙⠻⢿⡿⠟⠋⠉⠀  78
 27     27 ⠀⠈⠉⠛⢿⡿⠛⠉⠁⠀  78
 28     28 ⠀⠉⠉⠛⢿⡿⠛⠉⠉⠀  63
 29     12 ⠈⠉⠉⠛⢿⡿⠛⠉⠉⠁  71
 30     28 ⠀⠉⠉⠛⢿⡿⠛⠉⠉⠀  82
 31     12 ⠈⠉⠉⠛⢿⡿⠛⠉⠉⠁  94
 32     32 ⠈⠉⠙⠻⢿⡿⠟⠋⠉⠁  78
 33     33 ⠀⢀⣀⣀⣸⣇⣀⣀⡀⠀  78
```

**Playing.** A note plays its piece from where the piece starts, at the file's own speed, until
the note ends, then fades by `release`. If the note is longer than the piece it plays on into the
next. So a chopped loop follows the song's tempo without being stretched: faster than the
recording, each piece is cut a little short, leaving small gaps; slower, each runs a little into
the next. Notes above the last piece wrap round to the first.

When a piece starts where the note before it has got to, as the next piece does in a loop left
as recorded, the note carries on into it rather than fading out under it. Otherwise the two would
play the same audio twice for the length of the release, and the loop would flutter every step.
So an untouched chop track sounds exactly like the file, and only the steps you've moved start
afresh.

| Setting     | Range               | Default   | What it does                                   |
|-------------|---------------------|-----------|------------------------------------------------|
| `pieces`    | 1 to 256            | 16        | pieces the region is cut into                  |
| `start`     | 0 to 1              | 0         | where the region cut up starts in the file     |
| `end`       | 0 to 1              | 1         | where it ends                                  |
| `tune`      | -24 to 24 st        | 0 st      | semitones up or down, playing faster or slower |
| `direction` | `forward` `reverse` | `forward` | forwards, or each piece backwards from its end |
| `release`   | 0.002 to 1 s        | 0.005 s   | the fade when a note ends                      |
| `gain`      | 0 to 1              | 0.7       | the output level                               |

More about the controls:

- **`pieces`** is a count, nudged by one. Changing it re-cuts the region without changing the
  notes, so piece 3 of 16 becomes piece 3 of 32, a different, shorter bit of audio: set it to
  match the track's steps.
- **`start`** and **`end`** cut a region out of the file before it's divided: swapped if `end` is
  before `start`.
- **`tune`** pitches the whole loop by playing it faster or slower, so it changes length too:
  12 is an octave up and twice as fast, and every piece ends sooner.
- **`direction`** `reverse` turns every piece round, playing back from its end. Lock it on a step
  to turn one piece round, or use the `<` gesture.

**Rearranging** is pattern editing (chapter 2 has the keys): on a chop track, `C-a` and `C-x` on
a step move it to the next or last piece; `y` and `p` copy pieces about; `d` silences one; `~`
reverses the selected steps and turns each piece round too, with a `<` gesture, marked `◂`, so
the audio really plays backwards; `*` repeats the first step, or the first few, through the
selection, for a stutter; `A-(` and `A-)` rotate the selection; and rules such as `:rot` and
`:rev` (chapter 4) rearrange the loop each time round. The `O` gesture starts a step part way
into its piece: `O8` half way.

### Chopping a loop, step by step

The Amen example (`examples/amen`; open a copy) has the whole workflow in three patterns:
**drums**, the Amen break programmed note by note on Rhythmicon; **sample**, the same four bars
rendered and played on a chop track; and **chopped**, an edit of it. To do it yourself:

1. **Make the loop.** On the pattern you want to sample, select what to render (`%` for all of it,
   or `x` for rows) and `:export-loop amen.wav`. It renders exactly the selection's length and
   mixes the tail (a crash ringing on, a reverb) back into the start, as the loop would sound
   going round, so it loops cleanly. `:export-sel` renders once, with the tail after, for a
   one-shot sample instead.
2. **Chop it.** Add an empty track as long as the loop (`:track-add BEAT`, then set its length),
   and `:chop` with no path: it cuts the export into a piece a step. Or `:chop break.wav` for a
   file of your own. Play it: it sounds as it did.
3. **Rearrange.** Move pieces with `C-a` `C-x`, copy and paste them, repeat one with `*` for a
   stutter, reverse a run with `~`, silence some with `d`. Add a rule (`:rot+8/2+1` in the example
   shifts the whole edit half a bar later every other time round).
4. **Shape it.** On the instrument page or mixer, `tune` the loop up or down, `release` for
   tighter or looser pieces, and effects after it as on any track.

The same works on any audio: a recording of your own, a bar of the song itself rendered and
chopped, or a loop recorded live by the looper, which records into a chop track (chapter 9).

## Presets

A preset makes an instrument into one of its own: an instrument and its settings, the effects
after it, the macros that turn them, and a face, put on a track all at once. Lattice comes with
30, and you can save your own.

### Loading a preset

`:preset NAME` puts the preset `NAME` on a track: the instrument page's track, the mixer's or
fx page's strip, or on the pattern page the cursor's track.

```
:preset synth/night-bus
```

Presets live in folders, and a preset's name is its path, as a file's is: Lattice's own are each
in the folder of their instrument's engine, `synth/`, `monochord/`, `kick/` and so on to
`tuss/`, and yours wherever you saved them. The status line says
`bass is Night Bus (synth/night-bus)`. A preset replaces the track's instrument, its whole chain
of effects and its macros, and gives the track its face. Locks on the old effects and macros go
with them, and so do locks on the old instrument's settings, unless the preset is the same kind
of instrument, when they're kept. It's one step of undo.

You needn't remember the folders. Type `:preset ` and the command line lists them, each with how
many presets it holds; `tab` fills one in, and then the list is what's inside it, each with its
title and instrument. Or type a name without a folder and the list finds it in whichever folder
it's in: `:preset fli` suggests `tuss/flim`. A name on its own loads, too, as long as only one
folder has it, so `:preset night-bus` still puts on `synth/night-bus`; if two have it, the
status line names both and asks which. I wanted the list short enough to read at a glance: with
thirty presets and more saved, a flat list ran off the bottom of the screen, while the folders
fit, and an engine's folder is where you'd look for its sounds anyway.

A name Lattice doesn't know is refused with what's at the top:
`no preset called nope; there's synth/, monochord/, …`. Lattice's folders come first, then
yours.

### The presets Lattice comes with

Each has its effects, a face, and one or two macros, named on its face. Turn them on the
instrument page. Each is in the folder of its instrument, so `flim` below is `tuss/flim`.

| Preset           | Instrument  | Sound                                     | Macros              |
|------------------|-------------|-------------------------------------------|---------------------|
| `night-bus`      | `synth`     | a dark saw pad in a shimmer               | `dawn` `haze`       |
| `paper-lanterns` | `synth`     | a slow pad through a swaying band-pass    | `drift` `air`       |
| `cathedral`      | `synth`     | a square organ in a stone nave            | `nave` `light`      |
| `sodium`         | `synth`     | a bright plucked lead trailing echoes     | `glow` `trail`      |
| `wasp`           | `synth`     | a driven square lead                      | `sting` `swarm`     |
| `acid-rain`      | `synth`     | a 303's squelch                           | `acid` `drip`       |
| `rubber`         | `synth`     | a squelchy square bass                    | `squelch` `grit`    |
| `sub-zero`       | `synth`     | a low, round sub bass                     | `growl` `thaw`      |
| `glasshouse`     | `monochord` | a bright steel string in a glassy room    | `bloom` `bow`       |
| `dust-harp`      | `monochord` | a soft, dark nylon string, far off        | `settle` `distance` |
| `cello-ghost`    | `monochord` | a bowed string with a ghost an octave up  | `breath` `haunt`    |
| `tidewater`      | `monochord` | a twangy steel string off a slapback echo | `twang` `slap`      |
| `monsoon`        | `monochord` | a sitar in an echo                        | `rain`              |
| `fiddle`         | `mosaic`    | a violin, in a hall                       | `sing` `tasto`      |
| `glass-violin`   | `mosaic`    | a violin's string on a glass bowl         | `temper` `halo`     |
| `iron-cello`     | `mosaic`    | a gong the size of a cello, bowed         | `timber` `toll`     |
| `choir-string`   | `mosaic`    | a string that sings "ah"                  | `wood` `crowd`      |
| `nave`           | `mosaic`    | a body the size of a building             | `vast` `voices`     |
| `kit`            | `mosaic`    | a pocket fiddle, quick and bright         | `spiccato`          |
| `foundry`        | `kick`      | a hard, hot kick                          | `weight` `heat`     |
| `velvet-boom`    | `kick`      | an 808's long boom                        | `length` `growl`    |
| `tin-roof`       | `snare`     | a 909 crack                               | `ring` `room`       |
| `brushfire`      | `snare`     | a snare like brushes                      | `sweep` `flare`     |
| `cicada`         | `hat`       | a ticking, swarming hat                   | `chirp` `swarm`     |
| `brass-bell`     | `hat`       | the hat's metal rung like a bell          | `toll` `tarnish`    |
| `crowd-pleaser`  | `clap`      | a clap from a crowd                       | `crowd` `hall`      |
| `thunderhead`    | `tom`       | a low, rolling tom in a storm             | `roll` `storm`      |
| `hollow-log`     | `tom`       | a knock on wood                           | `knock` `hollow`    |
| `great-machine`  | `krell`     | five drifting choirs over a glass body    | `alien` `halls`     |
| `tregaskin`      | `tuss`      | a warm electric piano on a worn tape      | `worn` `growl`      |
| `flim`           | `tuss`      | a soft, glassy piano, close up            | `glass` `hush`      |
| `suitcase`       | `tuss`      | a seventies stage piano, barking, phased  | `bark` `swirl`      |
| `music-box`      | `tuss`      | short, bright tines into a long echo      | `chime` `echo`      |
| `hollow-tine`    | `tuss`      | an octave up and hollow, far off          | `hollow` `space`    |
| `monologue`      | `tuss`      | the harmonic series, drifting, in echo    | `wonk` `tape`       |
| `pentagon`       | `tuss`      | a barking bass piano, squashed            | `snarl` `squash`    |
| `ballad`         | `bessel`    | the eighties' electric piano, chorused    | `bark` `chorus`     |
| `chowning`       | `bessel`    | Chowning's bell, clanging then humming    | `clang` `toll`      |
| `campanile`      | `bessel`    | tubular bells in a tower                  | `strike` `tower`    |
| `rosewood`       | `bessel`    | a marimba                                 | `mallet` `ring`     |
| `lately`         | `bessel`    | a plucked house bass                      | `snap` `grit`       |
| `slap`           | `bessel`    | a slapped bass                            | `pop` `thump`       |
| `fanfare`        | `bessel`    | a brass section, swelling                 | `blare` `swell`     |
| `sawmill`        | `bessel`    | a feeding-back growl                      | `tear` `dark`       |
| `boom`           | `bessel`    | an FM kick                                | `punch` `tail`      |
| `rimshot`        | `bessel`    | an FM snare, rattling with noise          | `wires` `crack`     |
| `tinsel`         | `bessel`    | an FM hat, metallic                       | `open` `hiss`       |
| `more-cowbell`   | `bessel`    | the drum machine's cowbell                | `clank` `ring`      |

### Saving a preset

`:preset-save NAME` saves the track's instrument, effects and macros, and its face, as a preset
called `NAME`, to put on any track of any song:

```
:preset-save cold-iron
:preset-save mine/cold-iron
```

A name is letters, digits, `-` and `_` (it's the file's name), with a `/` after each folder it's
in, so the second saves it in a folder called `mine`, made if it isn't there. While you type,
the command line lists the folders there are to save into. If the track has no face, it's given
one titled after the name, `Cold Iron`, which the track then wears. A name Lattice's own presets
use is refused (`synth/night-bus is one Lattice comes with: save it under another name`), as
`:preset` would always find Lattice's first; saving in an engine's folder under a name of your
own is fine, and your preset is listed there beside Lattice's. Saving under a name of yours
that's already there replaces that file.

Presets are saved as TOML files, one each, in `presets` beside the config file:
`~/.config/lattice/presets/cold-iron.toml`, or `presets/mine/cold-iron.toml` in a folder (or
under `$XDG_CONFIG_HOME` if it's set). Folders inside folders work too, four deep. The status
line says where. You can edit them by hand, copy them between machines and share them. Here is a
snare saved with a macro called `crack` that turns its `snap` and `tone`, a reverb, and a face
putting `snap` forward:

```toml
[face]
title = "Brittle"
controls = [3]

[instrument]
kind = "Snare"

[instrument.params]
decay = 0.2
model = 2.0
snap = 0.75
tone = 6000.0
tune = 200.0

[[effects]]
kind = "Reverb"

[[macros]]
name = "crack"
value = 0.0

[[macros.targets]]
param = 3
depth = 0.5

[[macros.targets]]
param = 4
depth = 0.3
```

Settings are by name, and only those that differ from their defaults are written. A face's
`controls` and a macro target's `param` are settings by number, in the order the instrument's
section lists them from 0 (`snap` is the snare's fourth, 3); a target with `fx = 0` turns the
first effect's setting rather than the instrument's. A preset is checked when it's loaded: a face
or macro naming a setting that isn't there, more than 8 macros, or art too big, and it's refused
with what's wrong and the file it's in.

## Faces

A **face** is how a track presents its instrument on the instrument page, as a hardware synth
presents a few knobs on its front panel and hides the rest. It's a title, the controls it puts
forward (the track's macros and up to six of the instrument's settings), and art. A preset gives
its track a face; so do the commands below. The track keeps its face, and the song saves it,
until its instrument changes for another kind (by `:inst` or `:sample`), when it goes, as it
belonged to the old one.

A face's column shows its title with the instrument faint beside it, then the macros, then the
settings it puts forward, then the chain. `enter` on the title opens all the instrument's
settings, and again goes back to the face; which faces are open isn't saved. `w` and `b` stop
wherever the kind of line changes.

```
      Brittle snare ────────────
      1 crack          0
      2 size           0
      1 snap          0.75
      → reverb
```

### Making a face

| Command                | What it does                                                      |
|------------------------|-------------------------------------------------------------------|
| `:face TITLE`          | titles the face, giving the track one if it has none              |
| `:face-show [SETTING]` | puts a setting on the face: the one named, or the cursor's        |
| `:face-hide [SETTING]` | takes a setting off the face                                      |
| `:face-art [PATH]`     | draws the face from a text file; with no path, takes the art away |

They work on the same track as `:preset`: the instrument page's, the mixer's or fx page's, or
the pattern cursor's. Each is a step of undo.

- `:face` needs a title: `a face needs a title: :face Night Bus`. Any text will do, spaces and
  all.
- `:face-show` gives a track with no face one titled after the track (`ride` becomes `Ride`),
  and adds settings in the order you show them, up to six (`a face has at most 6 settings on it`).
  With no setting named, the cursor must be on one of this track's instrument's settings,
  on the instrument page or the mixer; an effect's setting can't go on a face directly, but a
  macro that turns it can. A setting the instrument doesn't have is refused with those it does:
  `snare has no cutoff; there's model, tune, decay, snap, tone`. Showing one already there, or
  hiding one that isn't, only says so.
- A face with no settings put forward shows only its title, macros and chain: all the more
  reason to `enter` on the title.

**Macros on faces.** A face's first controls are the track's macros: they're what a preset is
played with, each turning several settings at once (`dawn` on `night-bus` opens its cutoff and
envelope together). Add one with `:macro NAME` and have it turn the setting under the cursor with
`:map NAME`, then turn it with `C-a` `C-x` on its line. See [macros](#macros), below.

### Drawing a face by hand

`:face-art PATH` draws the face from a text file of your own:

```
:face-art lamp.txt
```

The file is up to 8 frames, each up to 8 rows of up to 31 characters, with an empty line between
frames. Braille characters (U+2800 to U+28FF) draw best, as each holds a 2 by 4 grid of dots, but
any characters will do except tabs and other control characters. Spaces at the ends of lines,
and empty lines before the first frame and after the last, are dropped. A track with no face gets
one titled after the file (`lamp.txt` is `Lamp`). The status line says
`Lamp is drawn from lamp.txt, 2 frames`. A file too big is refused with what's too big:
`a face's art is at most 31 characters wide`, `a frame of a face's art has at most 8 rows`,
`a face's art has at most 8 frames`. `:face-art` with no path takes the art away, leaving the
fractal; if there's none to take, `the face has no art to take away; :face-art FILE draws it`.

This two-frame lamp blinks with the beat:

```
   .--.
  ( oo )
   \__/

   .--.
  ( -- )
   \__/
```

In a preset's TOML, the art is the face's `art`, a list of rows, each frame after an empty row:
`art = ["⢠⣶⣦ ⣿", …, "", …]`. The `great-machine` preset draws the Great Machine of the Krell this
way.

**Without art of its own**, a face shows a fractal, one of those from Lattice's opening
animation, picked by hashing its title, so each title has its own, and mirrored about the middle
so it looks back at you. Its first macro zooms in on it, and its second hurries its flow.

## Macros

A **macro** is a knob on a track, from 0 to 1, that turns several of its settings at once, of its
instrument or its effects, each by a depth of its own. One macro can open a filter while it
lengthens a decay and pushes a reverb, which is how presets make one control do something
musical. Macros are covered in full with the fx page in chapter 8; this is what you need for
instruments.

| Command             | What it does                                                            |
|---------------------|-------------------------------------------------------------------------|
| `:macro NAME`       | adds a macro to the track: `:macro bloom`                               |
| `:map NAME [DEPTH]` | has `NAME` (or its number) turn the cursor's setting by `DEPTH`         |
| `:unmap NAME`       | stops macro `NAME` (or its number) turning the setting under the cursor |

On the instrument page, put the cursor on a setting and `:map bloom`: the status line says
`bloom turns cutoff by +0.50`, the setting is marked `1cutoff` (the macro's number), and the
macro appears under the settings as `1 bloom 0`. Turn it with `C-a` `C-x`, or `c` and a value
from 0 to 1; `d` on it removes it, and any locks on it.

- **Depth** is the share of the setting's range the macro moves it by when it's all the way up:
  0.5 is half the range, -1 down by all of it. It moves along the setting's curve, so a depth on
  a frequency is musical, and stops at the ends. `:map bloom 0.8` on a setting it already turns
  changes the depth.
- **Settings keep their own values**, and macros move them from there. So turning a setting by
  hand still works, and a macro back at 0 puts it back exactly.
- A track has up to 8 macros, each turning up to 8 settings. A macro's name is letters, digits,
  `-` and `_`, not starting with a digit, so its number can stand for it.
- A macro locks like a setting: `:lock bloom 1`, `L`, or turning it with record armed.
- Changing the instrument (`:inst`) keeps the macros but lets go of the old instrument's
  settings; a preset brings its own macros, in place of the track's.

## MIDI tracks: playing other gear (`midi`)

A MIDI track's instrument is something outside Lattice: a synth on the desk, a drum machine, a
modular's MIDI interface, or another program. Its notes go out over MIDI, as they're played, and
its settings go out as controllers. Everything else about the track is as it is for any other:
its steps, rules, gestures, locks, macros and recording all work, so a hardware synth is
sequenced exactly as `krell` is.

I'd planned it from the start: when I designed gestures and locks (chapter 5), the test of the
design was that MIDI out should need nothing new. A track that plays gear is a track whose
instrument is a MIDI port; its lock columns are controllers; a program change is a gesture.

### Choosing the gear: `:port`

`:port` makes the track a MIDI track, if it isn't one, and says where it plays. On its own it
lists the ports there are:

```
:port                 midi out: 1 minilogue xd, 2 Digitakt; or none, for Lattice's own
:port 2               the track plays the Digitakt
:port mini            any part of the name will do: the track plays the minilogue xd
:port none            Lattice's own port
```

On the pattern page it changes every track the selections reach; on the other pages, the
cursor's track. Type `:port ` and the command line lists them too.

The track keeps the name as you wrote it (`port: "mini"` in the song file) rather than a number,
and finds the port by it, in any case. A number can change when gear is plugged in in another
order; a name doesn't. Lattice looks for ports every second, so a synth switched on after the
song is opened is played as soon as it's there, and the status line says
`midi plays prophet, once it's plugged in` when the port isn't there yet. `:midi` lists the ports
too.

**Lattice's own port.** A track on `none` plays on a port of Lattice's own, called
`Lattice:out`, which other programs connect to: a DAW, a soft synth, or `aconnect` on Linux to
wire it to whatever you like. It's there whenever Lattice is running.

### Notes and channels

A note goes out as a note on, at its pitch and velocity, and a note off when it ends, timed as
the steps, gestures and lengths say: a `R3` sends three notes, an `A37` an arpeggio, a `D8` a
late one. `channel` is the MIDI channel, 1 to 16, so several tracks can play one multitimbral
synth on a channel each. Changing it ends the notes sounding on the old one first.

**Timing.** Every message is stamped with the moment its sound would be heard if it were an
instrument of Lattice's own, and a thread of its own sends it then, within a millisecond. So
gear plays in time with the built-in instruments: a MIDI note leaves at the moment Lattice's
kick on the same step comes out of the speakers. What the gear adds itself, between hearing a
note and sounding it, is up to the gear; a few milliseconds is usual.

**Muting.** A muted track, or one silent because another's soloed, sends no notes, and ends any
it was playing. Stopping the song, `:inst` to something else, and quitting all end every note
sounding, so none is left hanging on the gear.

### Settings: controllers and bend

A MIDI track's settings are its channel, its pitch `bend`, from -1 to 1, sent as MIDI's 14-bit
pitch bend, and its 128 controllers, `cc0` to `cc127`, each 0 to 127. A controller starts unset,
`–`, and an unset one isn't sent at all: a synth's own settings stay as they are until the track
says otherwise. `:cc` sets one:

```
:cc 74 40             send controller 74 at 40: the filter's cutoff, on most synths
:cc 74 none           stop sending it
```

The instrument page shows only the controllers the track sends, or a lock column changes, as
128 lines of `–` would bury the ones that matter; the mixer shows only the channel and bend.
Once set, a controller is turned with `+` and `-` like any setting, a step at a time, and sent
as it changes. When the track starts playing, its settings are sent once, so the gear starts
where the track says.

**Locks** work on controllers as on anything: `:lock cc74 20` puts a lock column on the
track, and a lock in it sends the controller on its step, held until the next, and back to the
track's setting when the song stops. As in every lock column, `00` to `99` cover the whole
range, here 0 to 127. A macro can turn controllers too, several at once (see
[Macros](#macros)): a knob on a MIDI controller turning a macro on a MIDI track turns the
gear's settings, recorded as locks if you record.

**Program change** is a gesture, `P0` to `P127`, sent before the step's note, so the note plays
with the gear's new sound (see [sound gestures](05-gestures.md#sound-gestures)).

### The controllers

What a controller does is up to the gear, and its manual has a chart of them. Most synths follow
the conventions below for the ones they have; the unnamed ones are free for a synth to use as it
likes, and most of its knobs are among them. `cc32` to `cc63` are the fine halves of `cc0` to
`cc31`, for the few synths that take 14-bit settings.

| 0–31                 | 32–63                | 64–95                | 96–127               |
|----------------------|----------------------|----------------------|----------------------|
| `cc0` bank           | `cc32` fine cc0      | `cc64` sustain       | `cc96` data +1       |
| `cc1` mod wheel      | `cc33` fine cc1      | `cc65` glide on      | `cc97` data −1       |
| `cc2` breath         | `cc34` fine cc2      | `cc66` sostenuto     | `cc98` NRPN fine     |
| `cc3`                | `cc35` fine cc3      | `cc67` soft pedal    | `cc99` NRPN          |
| `cc4` foot           | `cc36` fine cc4      | `cc68` legato        | `cc100` RPN fine     |
| `cc5` glide time     | `cc37` fine cc5      | `cc69` hold 2        | `cc101` RPN          |
| `cc6` data entry     | `cc38` fine cc6      | `cc70` variation     | `cc102`              |
| `cc7` volume         | `cc39` fine cc7      | `cc71` resonance     | `cc103`              |
| `cc8` balance        | `cc40` fine cc8      | `cc72` release       | `cc104`              |
| `cc9`                | `cc41` fine cc9      | `cc73` attack        | `cc105`              |
| `cc10` pan           | `cc42` fine cc10     | `cc74` cutoff        | `cc106`              |
| `cc11` expression    | `cc43` fine cc11     | `cc75` decay         | `cc107`              |
| `cc12` effect 1      | `cc44` fine cc12     | `cc76` vibrato rate  | `cc108`              |
| `cc13` effect 2      | `cc45` fine cc13     | `cc77` vibrato depth | `cc109`              |
| `cc14`               | `cc46` fine cc14     | `cc78` vibrato delay | `cc110`              |
| `cc15`               | `cc47` fine cc15     | `cc79` sound 10      | `cc111`              |
| `cc16`               | `cc48` fine cc16     | `cc80`               | `cc112`              |
| `cc17`               | `cc49` fine cc17     | `cc81`               | `cc113`              |
| `cc18`               | `cc50` fine cc18     | `cc82`               | `cc114`              |
| `cc19`               | `cc51` fine cc19     | `cc83`               | `cc115`              |
| `cc20`               | `cc52` fine cc20     | `cc84` glide from    | `cc116`              |
| `cc21`               | `cc53` fine cc21     | `cc85`               | `cc117`              |
| `cc22`               | `cc54` fine cc22     | `cc86`               | `cc118`              |
| `cc23`               | `cc55` fine cc23     | `cc87`               | `cc119`              |
| `cc24`               | `cc56` fine cc24     | `cc88` velocity fine | `cc120` sound off    |
| `cc25`               | `cc57` fine cc25     | `cc89`               | `cc121` reset all    |
| `cc26`               | `cc58` fine cc26     | `cc90`               | `cc122` local        |
| `cc27`               | `cc59` fine cc27     | `cc91` reverb        | `cc123` notes off    |
| `cc28`               | `cc60` fine cc28     | `cc92` tremolo       | `cc124` omni off     |
| `cc29`               | `cc61` fine cc29     | `cc93` chorus        | `cc125` omni on      |
| `cc30`               | `cc62` fine cc30     | `cc94` detune        | `cc126` mono         |
| `cc31`               | `cc63` fine cc31     | `cc95` phaser        | `cc127` poly         |

`cc120` to `cc127` aren't settings but orders to the synth: `cc123` ends every note, which
Lattice sends itself when it needs to. Setting them on a track sends them every time it starts,
which is rarely what you want.

## Audio devices

The instruments are all built in. There's no instrument that plays the audio input live; sound
from outside comes in through the looper, which records your audio interface's input into a chop
track a pass at a time (chapter 9). `:input` and `:output` choose the devices it records from and
you hear Lattice through, also in chapter 9.

Hosting plugins is planned, in the open CLAP standard: the built-in engines already work through
the same interface CLAP instruments use, with events stamped to the sample and settings in their
own units, so a plugin will sit on a track like any engine and lock and macro the same way.
