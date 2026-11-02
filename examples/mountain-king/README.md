# In the Hall of the Mountain King

`lattice examples/mountain-king/mountain-king.lattice`, then `C-space` to play the song: Grieg's
march from *Peer Gynt* (1875), in B minor, getting louder, higher and faster as the trolls close
in, as it does in the original: the conductor, left of the first track, takes it from 112 BPM to
176. It's here to show as much of Lattice as fits in one song. `gs` shows the arrangement:

1. **cave**, twice: the `bassoon` creeps through the theme, staccato, over a bowed sitar `drone`
   whose `bow` lock slides up (`~`) over the four bars. The `pizz` strings answer a step behind
   (a `rot+1` rule), but only the second time: the first entry mutes them. The conductor starts
   at `T112` and slides (`~126`) to 126 BPM by the end of the second time, its 128 steps running
   on through both.
2. **march**, twice: the theme on the `troll`, a square-wave lead, with the bass walking octaves
   and the drums in. The second time it's an octave up, by a rule, `tr+12/2+1`. In its last
   bar the `swell` plays a bar of `hall.wav` backwards, rising into… The tempo creeps on up
   to 138 BPM, the song's own, by the end.
3. **hall**, twice, at the song's own tempo (`T--`), which its render needs to line up: the
   march, rendered and chopped into 64 pieces on a `chop` track, a step
   each, then played out of order. Bar 2's E stutters up a fourth, a fifth and an octave (locks
   on `tune`), bar 3 opens backwards (pieces 7 down to 0, each turned round by a `<`
   gesture), and bar 4 rolls on the long A while `tune` slides down an octave, put back
   at the top. The second time round, a rule, `rot+32/2+1`, swaps the edit's halves.
4. **frenzy**, three times: the theme twice as fast, the bassoon an octave under it and the pizz
   two over, getting louder. The troll's `cutoff` and its macro, `fury`, start low and slide
   open over the pattern: open the troll (`te`) to see them in its lock columns. The first time
   the arrangement mutes the kit; the last, a rule (`tr+12@1`) takes the troll up an octave.
   The conductor, 192 steps long to run through all three, races from 144 BPM to 176, a slide
   arriving at the end of each time round.
5. **end**: two bars, the last chords, broadened to 120 BPM, cut off by the conductor's `X` once
   the last has rung.

Round the rest of the song:

- **Groups.** The four melodic tracks are `trolls`, compressed together on their bus; the six
  drums are `kit`, folded to a drum machine's columns, with drive and a compressor on theirs.
  The open hat and the closed one share a choke group, so each cuts the other off.
- **Effects.** On the fx page (`gf`, on the troll), the troll's chain branches: lane A drives
  it and sends a copy to lane B, which delays it and pitches it up an octave, quietly back in.
  `fury`, in the column left of the lanes, opens the synth's filter and resonance, the drive
  and the delay's feedback at once. The bass has its own drive, and the send bus a reverb.
- **Faces.** The troll and the bassoon each put forward their own few settings on the
  instrument page (`gi`).

`hall.wav` is Lattice's own rendering of the march. To make it again, go to the march
(`:pattern march`), select from the troll to the toms (the first eleven tracks), all 64 steps,
and `:export-loop hall.wav`: that renders exactly four bars, with the tail mixed back into the
start, so it loops cleanly. (The file here is that, mixed to mono, to keep it small.) The
`hall` and `swell` tracks are left out because they play it.
