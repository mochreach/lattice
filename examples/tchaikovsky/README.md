# Tchaikovsky's finale

`lattice examples/tchaikovsky/tchaikovsky.lattice`, then `C-space` to play the song: the last
movement of Tchaikovsky's Violin Concerto in D, Op. 35 (1878), *Allegro vivacissimo*, all 638
bars of it, about ten minutes, from the fanfare to that ending. It isn't an orchestra imitated.
Every part is the score's, note for note where a track can hold it, but each is played by
something no orchestra has: the violin by Mosaic, a bowed string whose body turns from wood to
glass to iron as the music goes, the cellos bowed on iron, the strings a string machine, the
flutes struck glass, the bassoons Krell's alien harpsichord. It's in 2/4 (`:meter 2 4`), at
sixteenths, and its 28 patterns, the longest 32 bars, are the movement in order. `gs` shows
them:

1. **fanfare**: the orchestra's call to the dance, at 155 BPM.
2. **cadenza** and **cadenza-2**: the violin alone, its `echo` (the delay's mix, locked on the
   first step) up, its halo of sympathetic strings ringing on. Over the two the body slides from
   wood (`material` 0) to glass (1), and the conductor holds back to 128 BPM into the dance.
3. **dance**, **answer**, **tutti** and **race**: the Russian dance, the theme passed to the
   orchestra and back, then running on until the conductor slows to 120.
4. **drone**: the second theme, at 95 BPM, over the cellos' drone. The violin plays it on a body
   most of the way from iron to a voice (2.6), with the strings' filters nearly shut and the halo
   wide open.
5. **flutes**: back up to speed, the violin running on in sixteenths with the flutes, Bessel's
   struck glass, in with it, until the conductor slows to 130.
6. **dream** and **molto**: the theme slowed right down, the oboes and clarinets singing it and
   the violin answering, softer, on glass; then *molto meno mosso*, 75 BPM, the violin alone over
   the strings, its body sliding back to the voice.
7. **quickening**: a slide from 100 BPM to 150, the body going back to wood as it speeds up.
8. **dance-2** to **quickening-2**: the whole thing again, as Tchaikovsky has it, with the
   second time's **storm** in place of the race.
9. **dance-3**, **answer-3** and **race-3**: the dance a last time, racing up to 160 BPM.
10. **coda**, **coda-2**, **coda-3** and **finale**: the coda at 165 BPM and then 170, the
    violin's body iron now, the kick and snare driving it. Through coda-3 the strings' filters
    slide wide open (`ladder` to 12000 Hz), and through the finale the halo opens until the
    last chords ring with it.

Round the rest of the song:

- **Tracks.** Sixteen, Lattice's most: the `solo`, and its `double`, which takes the lowest of
  each chord the violin plays (a track plays one note at a time); `flutes`, `oboes` (Switched as
  a shawm), `clarinets` (Bessel's hollow reed), `bassoons` (Krell); `horns` and `trumpets`;
  `timpani`, and a `kick` and `snare` that aren't in the score; and the strings, `violins`,
  `seconds` and `violas` on Switched's string machine, `cellos` on Mosaic's iron and the
  `basses` on the synth, driven. A chord in one part keeps its top note; a note with no room
  goes to another track or, rarely, is left out.
- **Bowing.** The Mosaic tracks bow the way a player would: a new bow on the beat, on long notes
  and after leaps, but in the legato sections only at the bar lines. Overlapping notes slur, and
  where the bow should change on a slurred note, an `X20` gesture lifts it just before the next.
  Notes off the sixteenth grid, the grace notes and tuplets, are put there by a `D` delay.
- **Drums.** The kick doubles the timpani's hard strokes and the tutti downbeats; in the coda
  it's on both beats, with the snare on the backbeat. Before each tutti, a bar of snare roll
  (`R2`) gets louder into it.
- **Moods.** Each pattern's first step locks its mood: the solo's `material`, `vibrato`, `halo`
  and echo, the double's material and vibrato, and the strings' `ladder` and gain. Open the solo
  (`te`) to see them in its lock columns. A lock on a pattern's last step that slides (`~`)
  carries the sound across it to the next.
- **Groups.** `soloist` (the solo and double), `winds` (compressed together), `brass`, `drums`
  (folded) and `strings` (with an ensemble and compressor on their bus). The send bus is a long
  reverb, and the master bus a gentle compressor.
- **Faces.** The solo, flutes, oboes, clarinets, violins and cellos each put forward their own
  few settings on the instrument page (`gi`).

The notes come from the [Mutopia Project](https://www.mutopiaproject.org/)'s edition of the
concerto (Mutopia-2017/11/06-466), typeset by Will Oram from Eulenburg's score of 1920 and in
the public domain, as is `tchaikovsky.lattice`, made from its MIDI file.
