# Switched-On Scarlatti

`lattice examples/scarlatti/scarlatti.lattice`, then `C-space` to play the song: Domenico
Scarlatti's Sonata in G major, K. 455, written for harpsichord, played here as Wendy Carlos played
Bach on *Switched-On Bach* (1968). Each hand's lines go to their own synthesisers, panned apart
(the right hand on the right, the left hand on the left), and every note is Scarlatti's. It's here
to show off gestures, the conductor and an arrangement that orchestrates the repeats differently.

A step is an eighth note, two to a beat, so a bar of the sonata's 2/2 is a bar of Lattice's eight
steps. Each half is cut into patterns of up to 16 bars, `A1` to `A3` and `B1` to `B4`, and `gs`
shows the arrangement playing each half twice, as Scarlatti's repeats ask, with different tracks
muted each time:

1. **A**, first time: the right hand on the `reed`, a square wave through a low filter. Its
   repeated notes are detached, cut a little over half way through their steps (`X14`).
2. **A** again, on the `brass`, a saw against a copy of itself a few cents up (a Pitch effect on
   `gf`), leaning on the long notes that start its bars (`!`) and gliding into its octave leaps
   (`G6`). From bar 24 to 30 its filter opens, a lock on `cutoff` sliding (`~`) in its column:
   open the brass (`te`) to see it.
3. **B**, first time, on the `harpsi`, a plucked steel string, the harpsichord it was written for.
4. **B** again, on all three at once. The reed's filter opens over the chromatic climb from bar
   52 to 55. The last pattern, `fine`, is B4 again, a bar longer so the last chord can ring.

The left hand is split across the `bass`, the `tenor` (what's held over the bass, through a
slowly swept band-pass filter) and the `alto` (the odd third note of a chord), each note going to
the lowest of them that's free.

The rest:

- **Trills.** Where Scarlatti writes a trill, `A10` goes round the note and the semitone above
  it: on the long F♯s of bars 19 and 21, and on the leading notes before each half's last chord.
- **The runs.** The only sixteenths, falling runs in bars 46 and 107, are split between two
  tracks: the right hand plays every other note, cut half way through its step (`X12`), and the
  `echo`, panned hard the other way, plays the notes between, half a step late (`D12`).
- **Fills.** Hold backtick while the song plays (or press it once, and again to stop, where the
  terminal doesn't report keys coming up): the `echo` plays the right hand an octave up, a
  sixteenth behind, in canon, and the `timp` rolls (`R4`, faded in with `r4`) at the end of
  every other bar. Their steps are marked `F`, so they only play in a fill.
- **Chance.** Where the right hand hammers one note, as it does for bars on end, the echo rings an
  octave over it a quarter of the time (`?25`), the same notes each time the song plays.
- **The conductor**, left of the first track, sets 200 BPM at the start of each half and slows
  into each half's last chord (`~168`, `~176`), the second time more (`~144`). Then `X` stops the
  song once the last chord has rung.
- **Timpani** strike where the bass holds a whole bar.

The notes come from the [Mutopia Project](https://www.mutopiaproject.org/)'s edition of K. 455
(Mutopia-2016/01/25-2104), typeset by Knute Snortum from a source in the Italian Digital Library,
and shared under the [Creative Commons Attribution-ShareAlike 4.0 licence](https://creativecommons.org/licenses/by-sa/4.0/).
`scarlatti.lattice`, made from its MIDI file, is shared under the same licence.
