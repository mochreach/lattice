# Flam

`lattice examples/flam/flam.lattice`, then `C-space` to play the song: an original piece
for Tuss, Lattice's electric piano, in the spirit of Aphex Twin's "Flim", from the *Come to Daddy*
EP (1997). That track sets a soft, slow, stepwise tune and gently rocking electric-piano chords
over drums far too busy for them, and the two somehow sit together. Flam borrows the idea, not
the notes: its tune, chords and drums are its own. The name is a drummer's word as well, a
note played with a grace note just ahead of it, and the snare plays plenty of them.

It's at 152 BPM in A major, a step a sixteenth, and every Tuss track is tuned in just
intonation on A (`tuning` and `key`), so its major thirds are pure and the sevenths of its
chords shimmer against the equal-tempered bass. `gs` shows the arrangement, 105 bars:

1. **intro** (8 bars): the chords alone, then the bass and the hats creeping in, the kick's
   pickup and a snare roll into
2. **A** (16): the tune on `tune`, a falling phrase over Amaj7, C♯m7, Dmaj7 and Dm6, two bars a
   chord, the minor sixth chord on D turning back home, as a plagal cadence does. The second eight
   bars break up its long notes, an answer on the off-beat. The `glint`, a hard little hammer
   through an echo, is muted the first time.
3. **B** (16): F♯m7, Dmaj9, Bm7, Esus4 resolving to E, the tune climbing higher in long notes,
   the glint answering its first two notes a beat late.
4. **A** again, the glint doubling the tune an octave up.
5. **break** (8): the chords held whole, fragments of the tune far apart, the hat alone, then the
   kick on every beat, and the snare rolling faster and louder (`R2` to `R4`) into the low toms.
6. **B** and **A** again, everything at once.
7. **outro** (9): the tune once more, softly, over shorter chords, the drums gone but the hat,
   and a last Amaj7 with A on top; the conductor slows it from 152 to 120 BPM, then stops
   (`X`).

The chords are three tracks, `keys1` to `keys3`, a note each (a step holds one note), in 3+3+2
eighths: on the beat, on the "and" of two, and on four. The `bass`, a square on Synth, plays the
roots in the same 3+3+2, leaping up an octave or walking up to the next chord at the end of each
bar.

The drums are a two-bar groove, straight and busy, with the kick leading:

- **The snare** has its backbeat on two and four, the last of each two bars a flam (`R2`), and
  ghost notes between, some only some of the time (`?60`). Every four bars a low tom answers the
  kick and the snare rolls (`r4`, each hit quieter) into the next four.
- **The hats** play every sixteenth, accented on the beat, the off-sixteenths only three times in
  four (`?75`), with a flam of their own; the `ohat`, an open hat, is on the "and" of one, and the
  closed hat chokes it (`choke`).
- **Fills.** Hold backtick while the song plays (or press it once, and again to stop): every four
  bars, in place of the last backbeat (marked `F-`), the toms tumble down (`F`).

The effects:

- **`tune`** wears the `flim` preset's face, the tine, and its macros: `glass` turns up the
  bell, the tine's clanging upper modes, and `hush` softens the hammer and opens the room.
- **The keys** are a group with an ensemble and a slow phaser on its bus, so the three tracks
  chorus as one instrument; the drums are a group driven a little and compressed.
- **The hall.** Everything sends to a reverb, and the master is gently compressed.

To hear how much the tuning does, turn `tuning` on the tracks back to `equal`, and back.
