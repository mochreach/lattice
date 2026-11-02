# The Amen break: programmed, sampled and chopped

`lattice examples/amen/amen.lattice`, then `C-space` to play the song. It has three patterns,
one for each stage (`gs` shows them):

1. **drums**: the Amen break, from The Winstons' *Amen, Brother* (1969), programmed note by note
   on Lattice's own drums at 136 BPM: kick, snare (with its ghost notes), ride and the crash in
   the fourth bar, in a group named `kit` with drive and a compressor on its bus.
2. **sample**: the same four bars, rendered to `amen.wav` and played back on a `chop` track cut
   into 64 pieces, each step playing its own. It sounds as programmed, but it's now audio.
3. **chopped**: an edit of the sample, made by moving pieces around as notes: a snare roll at the
   end of bar 2 (pieces 28 and 12 in turn), bar 1's opening and a reversed snare (`◂`, a lock
   on `direction`, put back with `--` on the next step) in bar 3, and a stutter cutting off the
   crash in bar 4. A rule, `rot+8/2+1`, shifts the whole edit half a bar later every other time
   round.

`amen.wav` is Lattice's own rendering, not the original record. To make it again, go to the
drums pattern (`:pattern drums`), select it all with `%`, and `:export-loop amen.wav`: that renders
exactly four bars, with the tail (the crash ringing on) mixed back into the start, so it loops
cleanly. (The file here is that, mixed to mono and normalised, to keep it small.) Then `:chop`,
with no file, on an empty track cuts up what was just exported.
