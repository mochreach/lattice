# Switched-On Scarlatti

`lattice examples/scarlatti/scarlatti.lattice`, then `C-space` to play the song: Domenico
Scarlatti's Sonata in G major, K. 455, written for harpsichord, shot into space in a tight silver
suit and blasted out on a harpsichord built by an ancient alien civilisation, with a modular
synthesiser screaming along, as Wendy Carlos played Bach on *Switched-On Bach* (1968) and Look Mum
No Computer plays Handel. Every note is Scarlatti's. It's here to show off Lattice's own engines,
Krell and Switched, the ensemble and the phaser, gestures, locks, the conductor and an arrangement
that orchestrates the repeats differently, building from the harpsichord and a bass to a full,
booming tutti over a drum machine.

A step is an eighth note, two to a beat, so a bar of the sonata's 2/2 is a bar of Lattice's eight
steps. Each half is cut into patterns of up to 16 bars, `A1` to `A3` and `B1` to `B4`, and `gs`
shows the arrangement playing each half twice, as Scarlatti's repeats ask, with different tracks
muted each time:

1. **Launch.** Four bars on G before the sonata: the `lead`, a swarm of saws, holds a low G with
   its ladder filter shut and screaming, a lock on `ladder` sliding (`~`) it open over the four
   bars, while `krell` strikes the G under it, the timpani boom and the snare rolls faster and
   faster (`R2` to `R4`) into the first note.
2. **A**, first time: the right hand on `krell` alone, all five choirs (`16' to 2'`) drifting out
   of tune with each other over a glass body tuned to G. Its repeated notes are detached, cut a
   little over half way through their steps (`X14`). Under it, the kick and the hats.
3. **A** again, the `lead` doubling Krell, leaning on the long notes that start its bars (`!`) and
   gliding into its octave leaps (`G6`), and the snare's backbeat in. From bar 24 to 30 its
   ladder opens, a lock sliding in its column: open the lead (`te`) to see it.
4. **B**, first time, with the `arp` in place of the lead: a pair of saws two or three octaves over the
   bass, a note every eighth, rising and falling through the chord the left hand holds (its
   root, third and fifth where it has them, and the octave).
5. **B** again, everything at once. The lead's ladder opens over the chromatic climb from bar 52
   to 55. The last pattern, `fine`, is B4 again, a bar longer so the last chord can ring.

The left hand is split across the `bass`, a single saw on Switched with its ladder low and
screaming, the `tenor` (what's held over the bass, on organ stops) and the `alto` (the odd third
note of a chord, on a swarm), each note going to the lowest of them that's free. The `pedal`, one
plain saw, doubles the bass an octave down, as an organ's 16-foot stops do, its ladder open just
enough to keep the saw's lowest harmonics, so it's heard on small speakers too.

The effects, on `gf`:

- **The lead's chain branches.** Lane A drives it and echoes it a dotted eighth later, then sends
  a copy to lane B, which echoes it a half note later and shifts the echo up an octave, a
  shimmer, quietly back in. Its face, *Silver Suit*, puts its ladder, scream, grit and contour on
  one page.
- **The inner voices** go through an `ensemble`, a string machine's chorus, so each sounds like a
  section, and then a `phaser`, slowly sweeping notches through them, the alto's eight stages
  against the tenor's four.
- **Groups.** The right hand (Krell, the lead and the echo) is compressed together on its bus;
  the left hand and the drum machine are driven and compressed on theirs, so the bass, pedal and
  inner voices growl as one. The arp is in neither, going straight to the master, so Krell's
  notes don't push it down.
- **The hall.** Every track sends to a vast reverb. The master drives the whole a little, warmly,
  and compresses it.

The rest:

- **Trills.** Where Scarlatti writes a trill, `A10` goes round the note and the semitone above
  it: on the long F♯s of bars 19 and 21, and on the leading notes before each half's last chord.
- **The runs.** The only sixteenths, falling runs in bars 46 and 107, are split between two
  tracks: the right hand plays every other note, cut half way through its step (`X12`), and the
  `echo`, two of Krell's choirs through a phaser, panned hard the other way, plays the notes
  between, half a step late (`D12`).
- **Fills.** Hold backtick while the song plays (or press it once, and again to stop, where the
  terminal doesn't report keys coming up): the `echo` plays the right hand an octave up, a
  sixteenth behind, in canon; the `timp` rolls (`R4`, faded in with `r4`) at the end of every
  other bar; and the snare gives up its last backbeat of each four bars for a roll ratcheting up
  from `R2` to `R6`. Their steps are marked `F`, so they only play in a fill, and the backbeat
  they replace `F-`, so it plays only outside one.
- **Chance.** The hats ratchet into sixteenths at the end of a bar on half their passes (`?50`),
  and where the right hand hammers one note, as it does for bars on end, the echo rings an octave
  over it a quarter of the time (`?25`), the same notes each time the song plays.
- **The conductor**, left of the first track, sets 200 BPM at the start of each half and slows
  into each half's last chord (`~168`, `~176`), the second time more (`~144`). Then `X` stops the
  song once the last chord has rung.
- **Timpani** strike where the bass holds a whole bar, and before each half's last chord strike
  its two chords and roll into it (`R4`, `R6`), with the kick.

The notes come from the [Mutopia Project](https://www.mutopiaproject.org/)'s edition of K. 455
(Mutopia-2016/01/25-2104), typeset by Knute Snortum from a source in the Italian Digital Library,
and shared under the [Creative Commons Attribution-ShareAlike 4.0 licence](https://creativecommons.org/licenses/by-sa/4.0/).
`scarlatti.lattice`, made from its MIDI file, is shared under the same licence.
