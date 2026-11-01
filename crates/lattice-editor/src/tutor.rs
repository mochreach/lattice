//! The tutor (`:tutor`, or `lattice --tutor`): lessons worked through on a song of its own, as
//! Helix's tutor is. Each lesson says what to do, and moves on by itself once it's been done,
//! judged by the song and editor rather than the keys pressed, so any way of doing it counts.
//!
//! Keys in a lesson's text are written as `{action_name}` and shown as whatever they're bound to,
//! so a changed keymap is taught as it is. The tests keep the tutor up to date: every action and
//! command is either taught by a lesson or listed in [`NOT_TAUGHT`] or [`COMMANDS_NOT_TAUGHT`],
//! and every lesson's solution is played through to check it still works.

use lattice_core::{
    Arrangement, Buses, Condition, Entry, FxKind, Grid, InstKind, Instrument, Song, Step, Track,
};

use lattice_core::instrument::chop;

use crate::keymap::{Action, Binding, Key, Keymap};
use crate::{Cursor, Editor, Effect, Mode, Page, Selection};

/// One lesson.
struct Lesson {
    title: &'static str,
    /// What to do. `{action_name}` is replaced by the keys bound to it; a blank line starts a
    /// new paragraph; `backticks` mark keys and commands.
    text: &'static str,
    /// The actions it teaches, each named in the text.
    #[cfg_attr(not(test), allow(dead_code))]
    teaches: &'static [Action],
    /// The commands it teaches, each written in the text as `:name`.
    #[cfg_attr(not(test), allow(dead_code))]
    commands: &'static [&'static str],
    /// Gets the song ready, so the lesson can be done (and isn't done already), changing as
    /// little as it can, so each lesson carries on from the last. The text says where to go,
    /// rather than the setup moving the cursor there.
    setup: fn(&mut Editor),
    /// Something to notice on the way, such as playback having started: see [`Tutor::seen`].
    watch: fn(&Editor) -> bool,
    /// Whether it's been done.
    done: fn(&Editor, &Tutor) -> bool,
    /// Keys that do it, for the tests: characters typed as they are, `<name>` a key by name
    /// (`<esc>`, `<C-a>`), and `<playing>` and `<stopped>` for the transport.
    #[cfg_attr(not(test), allow(dead_code))]
    solution: &'static str,
}

/// Where the tutor is.
#[derive(Debug, Clone)]
pub(crate) struct Tutor {
    lesson: usize,
    /// The song as the lesson started, to compare with.
    start: Grid,
    entries: Vec<Entry>,
    /// Whether the lesson's `watch` has been true since it started.
    seen: bool,
    /// Done, and waiting for space to go on.
    complete: bool,
}

/// The tutor's lesson, for drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorView {
    /// From 1.
    pub number: usize,
    pub total: usize,
    pub section: &'static str,
    pub title: &'static str,
    /// Paragraphs, with keys filled in, `backticks` marking keys and commands.
    pub text: Vec<String>,
    /// Once it's done: the next lesson's title, or `None` after the last.
    pub complete: Option<Option<&'static str>>,
}

/// Actions no lesson teaches yet, so a new action has to be put in one, or here.
#[cfg_attr(not(test), allow(dead_code))]
pub const NOT_TAUGHT: &[Action] = &[
    // Selections beyond the basics.
    Action::Flip,
    Action::CopyDown,
    Action::CopyUp,
    Action::SplitRows,
    // Groups.
    Action::ToggleGroup,
    Action::ToggleFold,
];

/// The same for commands.
#[cfg_attr(not(test), allow(dead_code))]
pub const COMMANDS_NOT_TAUGHT: &[&str] = &[
    "choke",
    "mute",
    "solo",
    "track-remove",
    "track-move",
    "group-name",
    "step",
    "oct",
    "click",
    "rec",
    "count",
    "layout",
    "vel",
    "scale",
    "keys",
    "open",
    "open!",
    "export",
    "export-sel",
    "write-quit",
    "quit!",
    "tutor!",
];

fn never(_: &Editor) -> bool {
    false
}

fn nothing(_: &mut Editor) {}

/// The track called `name`, which lessons find by name as tracks can be added and moved.
fn track<'a>(e: &'a Editor, name: &str) -> &'a Track {
    let t = index(e, name);
    &e.grid.tracks[t]
}

fn index(e: &Editor, name: &str) -> usize {
    (e.grid.tracks.iter())
        .position(|t| t.name == name)
        .unwrap_or(0)
}

fn has_note(track: &Track, step: usize) -> bool {
    track.steps.get(step).is_some_and(|s| s.note.is_some())
}

/// The run of bass notes the selection lessons work on.
fn run() -> std::ops::RangeInclusive<usize> {
    4..=7
}

fn fill_run(e: &mut Editor) {
    let t = index(e, "BASS");
    for (i, step) in run().enumerate() {
        if !has_note(&e.grid.tracks[t], step) {
            e.grid.tracks[t].steps[step] = Step::note(36 + 3 * i as u8);
        }
    }
}

/// The tutor's sections, each named with the title of its first lesson.
const SECTIONS: &[(&str, &str)] = &[
    ("Basics", "Moving around"),
    ("The pattern", "Mute and solo"),
    ("Sound", "The mixer"),
    ("The song", "Patterns"),
    ("Help", "Finding more"),
];

/// The section lesson `lesson` is in.
fn section(lesson: usize) -> &'static str {
    (SECTIONS.iter())
        .rev()
        .find(|(_, first)| LESSONS[..=lesson].iter().any(|l| l.title == *first))
        .map_or("", |(name, _)| name)
}

const LESSONS: &[Lesson] = &[
    Lesson {
        title: "Moving around",
        text: "Welcome to Lattice. The tutor teaches the keys by having you use them: each lesson \
               says what to do, and once you've done it, `space` goes on to the next. `:tutor next` skips \
               a lesson, `:tutor back` goes back one and `:tutor end` stops.\n\n\
               The grid has a column for each track and a row for each step. Move with \
               {left} {down} {up} {right}; a number first repeats the move, so 4 then {down} goes \
               down four. Move to the BASS track's step 08.",
        teaches: &[Action::Left, Action::Down, Action::Up, Action::Right],
        commands: &["tutor"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let c = e.cursor();
            c.track == index(e, "BASS") && c.step == 8
        },
        solution: "ll8j",
    },
    Lesson {
        title: "Jumping",
        text: "{first_step} and {last_step} jump to the first and last step, and {first_track} \
               and {last_track} to the first and last track. {half_page_down} {half_page_up} \
               move half a page, {page_down} {page_up} a page, {next_beat} {prev_beat} a beat, \
               {next_bar} {prev_bar} a bar, and {next_note} {prev_note} to the next and previous \
               note.\n\nGo to the last step of the last track.",
        teaches: &[
            Action::FirstStep,
            Action::LastStep,
            Action::FirstTrack,
            Action::LastTrack,
            Action::HalfPageDown,
            Action::HalfPageUp,
            Action::PageDown,
            Action::PageUp,
            Action::NextBeat,
            Action::PrevBeat,
            Action::NextBar,
            Action::PrevBar,
            Action::NextNote,
            Action::PrevNote,
        ],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let c = e.cursor();
            c.track == e.grid.tracks.len() - 1 && c.step == e.track_len(c.track) - 1
        },
        solution: "gegl",
    },
    Lesson {
        title: "Entering notes",
        text: "{insert} starts insert mode at the selection, and {append} after it. There, the \
               keyboard's bottom two rows are a piano: `z` is C, `s` C#, `x` D and so on, with \
               the octave above from `q`. Each note moves the cursor down a step; \
               {clear_advance} clears one instead. {keyboard_octave_up} and \
               {keyboard_octave_down} change the keyboard's octave, and {escape} goes back to \
               normal mode.\n\nGo to the top of the LEAD track, put four notes in it, then go \
               back to normal mode.",
        teaches: &[
            Action::Insert,
            Action::Append,
            Action::ClearAdvance,
            Action::KeyboardOctaveUp,
            Action::KeyboardOctaveDown,
            Action::Escape,
        ],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let notes = (track(e, "LEAD").steps.iter())
                .filter(|s| s.note.is_some())
                .count();
            notes >= 4 && e.mode == Mode::Normal
        },
        solution: "glggizxcv<esc>",
    },
    Lesson {
        title: "Playing",
        text: "{toggle_play} plays the pattern, looping, and stops it again. {play_song} plays \
               the whole song, from the pattern you're on.\n\nPlay the pattern, listen to your \
               notes, then stop it.",
        teaches: &[Action::TogglePlay, Action::PlaySong],
        commands: &[],
        setup: nothing,
        watch: |e| e.transport.is_some(),
        done: |e, t| t.seen && e.transport.is_none(),
        solution: "<space><playing><space><stopped>",
    },
    Lesson {
        title: "Recording",
        text: "{toggle_record} arms recording, and starts insert mode. Then notes played while \
               the song plays go in where the playhead is, rather than at the cursor, after a \
               count-in.\n\nArm recording, go back to normal mode, and disarm it.",
        teaches: &[Action::ToggleRecord],
        commands: &[],
        setup: nothing,
        watch: |e| e.record,
        done: |e, t| t.seen && !e.record,
        solution: "R<esc>R",
    },
    Lesson {
        title: "Selecting and deleting",
        text: "There's always a selection: with nothing selected, it's just the cursor. \
               {select_mode} starts select mode, where moving stretches the selection, and \
               {collapse} shrinks it back to the cursor. {select_rows} selects whole rows. \
               {delete} deletes what's selected, and copies it; {clear} only deletes.\n\nThe \
               BASS track has a run of notes on steps 04 to 07. Select them and delete them, \
               leaving the rest.",
        teaches: &[
            Action::SelectMode,
            Action::Collapse,
            Action::SelectRows,
            Action::Delete,
            Action::Clear,
        ],
        commands: &[],
        setup: fill_run,
        watch: never,
        done: |e, _| {
            let bass = track(e, "BASS");
            run().all(|s| !has_note(bass, s)) && has_note(bass, 0) && has_note(bass, 8)
        },
        solution: "ghllgg4jv3jd",
    },
    Lesson {
        title: "Undo",
        text: "{undo} undoes and {redo} redoes. Every change can be undone.\n\nBring back the \
               notes you deleted.",
        teaches: &[Action::Undo, Action::Redo],
        commands: &[],
        // Reached some other way than by deleting them, delete them, to undo.
        setup: |e| {
            let t = index(e, "BASS");
            if !e.undo.is_empty() && run().all(|s| !has_note(&e.grid.tracks[t], s)) {
                return;
            }
            fill_run(e);
            e.checkpoint();
            let t = index(e, "BASS");
            for s in run() {
                e.grid.tracks[t].steps[s] = Step::default();
            }
        },
        watch: never,
        done: |e, _| run().all(|s| has_note(track(e, "BASS"), s)),
        solution: "u",
    },
    Lesson {
        title: "Changing notes",
        text: "{note_up} and {note_down} move the selected notes a semitone, {note_octave_up} \
               and {note_octave_down} an octave, and {longer} and {shorter} make them last \
               longer or shorter. {replace} then a note fills the selection with it, and \
               {change} clears it and starts insert mode.\n\nWith the bass notes on steps 04 to \
               07 selected (undo brought the selection back too), raise them an octave.",
        teaches: &[
            Action::NoteUp,
            Action::NoteDown,
            Action::NoteOctaveUp,
            Action::NoteOctaveDown,
            Action::Longer,
            Action::Shorter,
            Action::Replace,
            Action::Change,
        ],
        commands: &[],
        setup: fill_run,
        watch: never,
        done: |e, t| {
            let (now, was) = (track(e, "BASS"), &t.start.tracks[index(e, "BASS")]);
            run().all(|s| match (now.steps[s].note, was.steps[s].note) {
                (Some(now), Some(was)) => now.pitch == was.pitch + 12,
                _ => false,
            })
        },
        solution: "ghllgg4jv3j>",
    },
    Lesson {
        title: "Copy and paste",
        text: "{yank} copies the selection, and {paste} pastes it at the cursor, over what's \
               there; {paste_insert} pushes the rest down instead.\n\nCopy the KICK track's \
               first beat, steps 00 to 03, onto the HAT track at step 08.",
        teaches: &[Action::Yank, Action::Paste, Action::PasteInsert],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let (kick, hat) = (track(e, "KICK"), track(e, "HAT"));
            (0..4).all(|s| kick.steps[s].note == hat.steps[8 + s].note)
        },
        solution: "ghggv3jyl5jp",
    },
    Lesson {
        title: "Many selections",
        text: "You can have many cursors at once: Lattice keeps several \
               selections, each with its own cursor, and every key you press acts at all of \
               them together. {select_all} selects everything; then {select_notes} splits that \
               into one selection for each note matching what you type, such as `C`, or `C E G` \
               for several. {keep_notes} and {remove_notes} keep or drop the selections with \
               matching notes.\n\nSelect every C in the pattern, so there's a cursor on each, \
               and raise them all a semitone with one key.",
        teaches: &[
            Action::SelectAll,
            Action::SelectNotes,
            Action::KeepNotes,
            Action::RemoveNotes,
        ],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| {
            let mut raised = false;
            for (now, was) in e.grid.tracks.iter().zip(&t.start.tracks) {
                for (now, was) in now.steps.iter().zip(&was.steps) {
                    match (now.note, was.note) {
                        (Some(n), Some(w)) if w.pitch % 12 == 0 => {
                            if n.pitch != w.pitch + 1 {
                                return false;
                            }
                            raised = true;
                        }
                        (n, w) if n != w => return false,
                        _ => {}
                    }
                }
            }
            raised
        },
        solution: "%sC<ret><C-a>",
    },
    Lesson {
        title: "Back to one cursor",
        text: "You now have a cursor on every C: the status line shows how \
               many selections there are, and which is the primary, the one with the solid \
               cursor that the view follows. {rotate_forward} and {rotate_backward} make the \
               next or previous one primary, {remove_primary} drops it, and {merge_all} merges \
               them all into one.\n\nTo get back to a single cursor, {keep_primary} drops \
               every selection but the primary, then {collapse} shrinks it to just its cursor \
               (it may already be one step). Do that now.",
        teaches: &[
            Action::RotateForward,
            Action::RotateBackward,
            Action::RemovePrimary,
            Action::KeepPrimary,
            Action::MergeAll,
            Action::Collapse,
        ],
        commands: &[],
        // Reached some other way, select each kick, to have several.
        setup: |e| {
            if e.selections.len() > 1 {
                return;
            }
            let t = index(e, "KICK");
            let points: Vec<Selection> = (0..e.track_len(t))
                .filter(|&s| has_note(&e.grid.tracks[t], s))
                .map(|step| {
                    let point = Cursor { track: t, step };
                    e.clamped(point, point)
                })
                .collect();
            e.set_selections(points, 0);
        },
        watch: never,
        done: |e, _| {
            let r = e.selection();
            e.selections.len() == 1 && r.first_track == r.last_track && r.first_step == r.last_step
        },
        solution: ",;",
    },
    Lesson {
        title: "The command line",
        text: "{command_line} opens the command line; it suggests commands as you type, and \
               `tab` takes one. `:w` saves the song, `:q` quits ({quit} quits too), and `:bpm` \
               sets the tempo.\n\nSet the tempo to 128.",
        teaches: &[Action::CommandLine, Action::Quit],
        commands: &["write", "quit", "bpm"],
        setup: nothing,
        watch: never,
        done: |e, _| e.bpm == 128.0,
        solution: ":bpm 128<ret>",
    },
    Lesson {
        title: "Mute and solo",
        text: "Some keys open a menu of what can follow, shown in the corner. {toggle_mute} \
               mutes the tracks the selections reach, and {toggle_solo} solos them; \
               {toggle_pattern_mute} (or `:pattern-mute`) mutes them in this pattern only.\n\nMute the BASS \
               track, listen, and unmute it.",
        teaches: &[
            Action::ToggleMute,
            Action::ToggleSolo,
            Action::TogglePatternMute,
        ],
        commands: &["pattern-mute"],
        setup: nothing,
        watch: |e| track(e, "BASS").mix.mute,
        done: |e, t| t.seen && !track(e, "BASS").mix.mute,
        solution: "ghlltmtm",
    },
    Lesson {
        title: "Rules",
        text: "Rules change how a track plays each time round (each pass), leaving its notes \
               as written. `:tr 5` transposes the track up 5 semitones, `:rot 1` turns it a \
               step later, `:drift 1` a step further each pass, and `:rev` plays it \
               backwards.\n\nAdd `every 4` and a rule applies only every fourth pass; `every \
               4 2` picks which of the four (counting from 0), and `pass 2` applies it on one \
               pass only. Rules are listed above their track, written short: `:tr 5 every 4` \
               shows as `tr+5/4`, and you can type them short too, as `:tr 5 /4`. `:pop` removes the track's last rule, and `:clear` all of \
               them. {toggle_view} switches the grid between the notes as written and as \
               they're playing.\n\n`:len 12` makes a track 12 steps long, so it lands \
               differently against the others each time, and `:name` renames it.\n\nGive the \
               BASS track a rule that applies only some passes: transpose it up 5 every fourth \
               pass. Play to hear it if you like.",
        teaches: &[Action::ToggleView],
        commands: &["tr", "rot", "drift", "rev", "pop", "clear", "len", "name"],
        setup: nothing,
        watch: never,
        done: |e, t| {
            let was = t.start.tracks[index(e, "BASS")].rules.len();
            (track(e, "BASS").rules.iter().skip(was)).any(|r| r.when != Condition::Always)
        },
        solution: "ghll:tr 5 every 4<ret>",
    },
    Lesson {
        title: "Tracks",
        text: "{add_track} adds a track after the cursor's, and {remove_track} removes the \
               selected tracks. {select_tracks} selects whole tracks; {yank_tracks} copies \
               them, and {paste_tracks_after} {paste_tracks_before} paste them. \
               {move_track_left} and {move_track_right} move them.\n\nAdd a track after BASS.",
        teaches: &[
            Action::AddTrack,
            Action::RemoveTrack,
            Action::SelectTracks,
            Action::YankTracks,
            Action::PasteTracksAfter,
            Action::PasteTracksBefore,
            Action::MoveTrackLeft,
            Action::MoveTrackRight,
        ],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| e.grid.tracks.len() > t.start.tracks.len() && e.page == Page::Pattern,
        solution: "ghllta",
    },
    Lesson {
        title: "The mixer",
        text: "{goto_mixer} opens the mixer on the cursor's track, and {goto_pattern} comes \
               back; {goto_instrument} and {goto_effects} go straight to the track's instrument \
               and effects. There, {left} {right} move between strips, {down} goes into the \
               detail, and {note_up} {note_down} turn what's under the cursor. `:level -6`, \
               `:pan 20` and `:send -12` set them by number, and `:set` the setting under the cursor.\n\n\
               Turn the BASS track's level down, then come back to the pattern.",
        teaches: &[
            Action::GotoMixer,
            Action::GotoPattern,
            Action::GotoInstrument,
            Action::GotoEffects,
        ],
        commands: &["level", "pan", "send", "set"],
        setup: nothing,
        watch: never,
        done: |e, t| {
            let was = t.start.tracks[index(e, "BASS")].mix.level;
            track(e, "BASS").mix.level < was && e.page == Page::Pattern
        },
        solution: "ghllgm<C-x>gp",
    },
    Lesson {
        title: "Instruments and effects",
        text: "`:inst string` changes the cursor's track's instrument, and `:sample kick.wav` \
               plays a sound file on it. `:fx-add delay` adds an effect to it: `delay`, `reverb`, `filter`, `drive` or `comp`. On the mixer, \
               {delete} removes the effect under the cursor.\n\nAdd a reverb to the LEAD track.",
        teaches: &[],
        commands: &["inst", "sample", "fx-add"],
        setup: nothing,
        watch: never,
        done: |e, _| (track(e, "LEAD").effects.iter()).any(|fx| fx.kind == FxKind::Reverb),
        solution: "gl:fx-add reverb<ret>",
    },
    Lesson {
        title: "Locks",
        text: "A step can lock its track's settings: they change on that step, and hold until \
               the next note. `:lock cutoff 2000` locks the selected steps, `:lock cutoff \
               300..4000` sweeps across them, and `:unlock` clears them. On the mixer, \
               {lock_setting}, then a change, then {lock_setting} again locks what you \
               changed.\n\nLock the LEAD track's cutoff on step 00.",
        teaches: &[Action::LockSetting],
        commands: &["lock", "unlock"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let lead = track(e, "LEAD");
            let cutoff = (lead.instrument.params().iter()).find(|p| p.name == "cutoff");
            cutoff.is_some_and(|p| {
                (lead.steps[0].locks.iter()).any(|l| l.fx.is_none() && u32::from(l.param) == p.id)
            })
        },
        solution: "glgg:lock cutoff 2000<ret>",
    },
    Lesson {
        title: "Chopping",
        text: "A chop track plays one sound file cut into pieces, a piece a step, drawn as a \
               waveform down the track, so you can rearrange the sound like notes. \
               `:export-loop` renders the selection to a sound file exactly as long, to cut up, \
               in a `samples` folder beside the song (until it's saved, a scratch one: saving \
               copies it in). `:track-add BEAT` adds a track after the cursor's, and `:chop` \
               cuts the file last exported, or one named, into a piece for each of the track's \
               steps.\n\nSelect the KICK and HAT tracks whole ({select_tracks}), export them as \
               a loop, add a track and chop the loop on it.",
        teaches: &[],
        commands: &["export-loop", "track-add", "chop"],
        setup: nothing,
        watch: never,
        done: |e, _| e.grid.tracks.iter().any(chopped),
        solution: "ghvltt:export-loop<ret>:track-add BEAT<ret>:chop<ret>",
    },
    Lesson {
        title: "Rearranging",
        text: "Keys rearrange the selected steps, on any track. {reverse_steps} turns them back \
               to front; on a chop track each piece plays backwards too, marked `◂`. \
               {repeat_steps} repeats the first step through the rest, or with a count, as \
               `2*`, the first two. {rotate_steps_forward} and {rotate_steps_backward} move them \
               down or up one, round and round.\n\nOn the chop track, turn the last beat round, \
               steps 0C to 0F, and repeat step 04 through the second beat, to 07.",
        teaches: &[
            Action::ReverseSteps,
            Action::RepeatSteps,
            Action::RotateStepsForward,
            Action::RotateStepsBackward,
        ],
        commands: &[],
        setup: |e| {
            if !e.grid.tracks.iter().any(chopped) {
                // Silent, without a file, but there to rearrange.
                let mut instrument = Instrument::new(InstKind::Chop);
                instrument.values[chop::PIECES as usize] = 16.0;
                let steps = (0..16).map(|i| Step::note(chop::pitch(i))).collect();
                let beat = Track {
                    name: "BEAT".into(),
                    steps,
                    instrument,
                    ..Track::default()
                };
                e.insert_track(index(e, "HAT") + 1, beat);
            }
        },
        watch: never,
        done: |e, t| {
            let Some(c) = e.grid.tracks.iter().position(chopped) else {
                return false;
            };
            let (now, was) = (&e.grid.tracks[c], &t.start.tracks[c]);
            if now.steps.len() < 16 || was.steps.len() < 16 {
                return false;
            }
            let mut turned = was.steps[12..16].to_vec();
            crate::rearrange::reverse(&mut turned, &was.instrument);
            now.steps[12..16] == turned[..] && now.steps[4..8].iter().all(|s| *s == was.steps[4])
        },
        solution: "ghllgg12jv3j~<esc>;11kv3j*",
    },
    Lesson {
        title: "Looping",
        text: "A chop track can record its loop from the audio input. `:arm` arms the cursor's \
               track (`○` by its name); then with record on ({toggle_record}) and the song \
               playing, it records a pass, from step 00 round to 00 again (`●`), and plays it, \
               cut into a piece a step. Each pass after that overdubs the last, and {undo} takes \
               one off; delete its notes and the next pass starts a new loop. `:arm 1` records \
               one pass and stops, `:arm 4` four. `:arm!` records a new loop over the old one, \
               and `:disarm` disarms. \
               `:input` chooses what to record from: type `:input ` and the devices are \
               listed, then a number or part of a name picks one; `:output` chooses what you \
               hear. If what you play comes back late or early, `:latency 20` moves it by \
               milliseconds, or `:latency -20`.\n\nArm the chop track.",
        teaches: &[],
        commands: &["arm", "arm!", "disarm", "input", "output", "latency"],
        setup: nothing,
        watch: never,
        done: |e, _| e.armed().is_some(),
        solution: "ghll:arm<ret>",
    },
    Lesson {
        title: "Patterns",
        text: "A song is a list of patterns, which all have the same tracks, each with its own \
               notes. `:pattern-new verse` makes an empty pattern, `:pattern-dup` a copy of this \
               one, and `:pattern` goes to one by name; {next_pattern} and {prev_pattern} step \
               through them. `:pattern-name` renames this pattern and `:pattern-remove` removes \
               it. A pattern plays for as long as its longest track, or `:bars 2` sets how many \
               bars.\n\nMake a copy of this pattern called `chorus`.",
        teaches: &[Action::NextPattern, Action::PrevPattern],
        commands: &[
            "pattern-new",
            "pattern-dup",
            "pattern",
            "pattern-name",
            "pattern-remove",
            "bars",
        ],
        setup: nothing,
        watch: never,
        done: |e, _| e.arrangement.patterns[e.arrangement.current].name == "chorus",
        solution: ":pattern-dup chorus<ret>",
    },
    Lesson {
        title: "The song page",
        text: "{goto_song} opens the song page: the order the patterns play in. Each entry plays \
               a pattern some number of times. {up} {down} move between entries, {left} \
               {right} change which pattern the entry plays, and {longer} {shorter} play it \
               more or fewer times.\n\nOpen the song page and make an entry play twice.",
        teaches: &[Action::GotoSong, Action::Longer, Action::Shorter],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| {
            e.page == Page::Song
                && (e.arrangement.entries.iter().zip(&t.entries))
                    .any(|(now, was)| now.repeats > was.repeats)
        },
        solution: "gs+",
    },
    Lesson {
        title: "Arranging",
        text: "On the song page, {open_below} adds an entry after the cursor's, playing the same \
               pattern, and {open_above} one before. {delete} removes an entry, {yank} copies it \
               and {paste} pastes it after. {open} goes to the entry's pattern, to edit \
               it.\n\nAdd an entry, then edit its pattern.",
        teaches: &[Action::OpenBelow, Action::OpenAbove, Action::Open],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| e.arrangement.entries.len() > t.entries.len() && e.page == Page::Pattern,
        solution: "gso<ret>",
    },
    Lesson {
        title: "Playing the song",
        text: "On the song page, {toggle_play} plays the song from the cursor's entry. On the \
               pattern page, {play_song} plays it from the pattern you're on, and the page \
               follows along as the song moves from pattern to pattern; `:follow` turns that \
               off, to stay on one pattern while the song plays.\n\nPlay the song from the song \
               page, then stop it.",
        teaches: &[],
        commands: &["follow"],
        setup: nothing,
        watch: |e| e.transport.is_some() && e.page == Page::Song,
        done: |e, t| t.seen && e.transport.is_none(),
        solution: "gs<space><playing><space><stopped>",
    },
    Lesson {
        title: "Finding more",
        text: "That's the tutor. `:help` lists every key and command, as they're bound, and \
               `:tutor` starts this again.\n\nOpen the help page to finish.",
        teaches: &[],
        commands: &["help"],
        setup: nothing,
        watch: never,
        done: |e, _| e.page == Page::Help,
        solution: ":help<ret>",
    },
];

/// A chop track, made by the Chopping lesson, or by the next if it was skipped.
fn chopped(track: &Track) -> bool {
    track.instrument.kind == InstKind::Chop
}

/// The tutor's own song: drums, a bass line, and an empty track for the lead.
fn song() -> Song {
    let track = |name: &str, kind: InstKind, notes: &[(usize, u8)]| {
        let mut steps = vec![Step::default(); 16];
        for &(step, pitch) in notes {
            steps[step] = Step::note(pitch);
        }
        Track {
            name: name.into(),
            steps,
            instrument: Instrument::new(kind),
            ..Track::default()
        }
    };
    let drum = lattice_core::DRUM_ROOT;
    Song {
        name: "tutor".into(),
        bpm: 120.0,
        grid: Grid {
            name: "tutor".into(),
            steps_per_beat: 4,
            tracks: vec![
                track(
                    "KICK",
                    InstKind::Kick,
                    &[(0, drum), (4, drum), (8, drum), (12, drum)],
                ),
                track(
                    "HAT",
                    InstKind::Hat,
                    &[(2, drum), (6, drum), (10, drum), (14, drum)],
                ),
                track(
                    "BASS",
                    InstKind::Synth,
                    &[
                        (0, 36),
                        (3, 36),
                        (4, 39),
                        (5, 41),
                        (6, 43),
                        (7, 46),
                        (8, 36),
                        (11, 31),
                        (14, 34),
                    ],
                ),
                track("LEAD", InstKind::Synth, &[]),
            ],
            groups: vec![],
        },
        arrangement: Arrangement::default(),
        buses: Buses::default(),
    }
}

/// `{action_name}` as the first keys bound to it, in `backticks`.
fn fill(text: &str, keymap: &Keymap) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}').map_or(rest.len(), |c| open + c);
        let name = &rest[open + 1..close.min(rest.len())];
        match Action::named(name) {
            Some(action) => match keys_for(keymap, action).first() {
                Some(keys) => out.push_str(&format!("`{keys}`")),
                None => out.push_str(&format!("({name}, unbound)")),
            },
            None => out.push_str(name),
        }
        rest = rest.get(close + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// Every way to press `action`, letters first: in normal mode, through a menu, then in insert
/// mode.
fn keys_for(keymap: &Keymap, action: Action) -> Vec<String> {
    let mut keys = vec![];
    for (key, binding) in &keymap.normal {
        match binding {
            Binding::Action(a) if *a == action => keys.push(key.to_string()),
            Binding::Menu(menu) => {
                for (item, binding) in &menu.items {
                    if *binding == Binding::Action(action) {
                        keys.push(format!("{key}{item}"));
                    }
                }
            }
            _ => {}
        }
    }
    for (key, a) in &keymap.insert {
        if *a == action {
            keys.push(key.to_string());
        }
    }
    keys.sort_by_key(|k| (k.chars().count() > 2, k.clone()));
    keys
}

impl Editor {
    /// Starts the tutor on its own song, at the first lesson.
    pub fn start_tutor(&mut self) -> Vec<Effect> {
        let mut effects = self.replace_song(song(), None);
        self.page = Page::Pattern;
        self.message = None;
        self.tutor = Some(Tutor {
            lesson: 0,
            start: self.grid.clone(),
            entries: self.arrangement.entries.clone(),
            seen: false,
            complete: false,
        });
        effects.extend(self.start_lesson(0));
        effects
    }

    /// `:tutor next`, `back` or `end`.
    pub(crate) fn tutor_command(&mut self, what: &str) -> Result<Vec<Effect>, String> {
        let Some(tutor) = &self.tutor else {
            return Err("the tutor isn't running: :tutor starts it".into());
        };
        let lesson = tutor.lesson;
        match what {
            "next" => Ok(self.start_lesson(lesson + 1)),
            "back" => Ok(self.start_lesson(lesson.saturating_sub(1))),
            "end" => {
                self.end_tutor("tutor ended: :tutor starts it again");
                Ok(vec![])
            }
            _ => Err("the tutor takes next, back or end".into()),
        }
    }

    /// Whether the tutor's running.
    pub fn tutoring(&self) -> bool {
        self.tutor.is_some()
    }

    /// The lesson to show, if the tutor's running.
    pub fn tutor_view(&self) -> Option<TutorView> {
        let tutor = self.tutor.as_ref()?;
        let lesson = &LESSONS[tutor.lesson];
        Some(TutorView {
            number: tutor.lesson + 1,
            total: LESSONS.len(),
            section: section(tutor.lesson),
            title: lesson.title,
            text: (lesson.text.split("\n\n"))
                .map(|p| fill(p, &self.keymap))
                .collect(),
            complete: (tutor.complete).then(|| LESSONS.get(tutor.lesson + 1).map(|l| l.title)),
        })
    }

    /// Checks the lesson, and once it's done waits for space to go on. Call it after each key,
    /// and as playback starts and stops.
    pub fn tutor_tick(&mut self) -> Vec<Effect> {
        let Some(at) = self
            .tutor
            .as_ref()
            .filter(|t| !t.complete)
            .map(|t| t.lesson)
        else {
            return vec![];
        };
        let lesson = &LESSONS[at];
        let seen = (lesson.watch)(self);
        let tutor = self.tutor.as_mut().expect("tutoring");
        tutor.seen |= seen;
        let tutor = self.tutor.as_ref().expect("tutoring");
        if (lesson.done)(self, tutor) {
            self.tutor.as_mut().expect("tutoring").complete = true;
        }
        vec![]
    }

    /// Space (or return) on a finished lesson goes on to the next. Other keys work as usual,
    /// so there's time to look at what was done.
    pub(crate) fn tutor_key(&mut self, key: Key) -> Option<Vec<Effect>> {
        let tutor = self.tutor.as_ref().filter(|t| t.complete)?;
        if !matches!(key, Key::Char(' ') | Key::Enter)
            || !matches!(self.mode, Mode::Normal | Mode::Select)
        {
            return None;
        }
        let next = tutor.lesson + 1;
        if next == LESSONS.len() {
            self.end_tutor("that's the tutor done: :tutor starts it again");
            return Some(vec![]);
        }
        Some(self.start_lesson(next))
    }

    /// Starts a lesson from wherever the last one left off: its setup changes only what it
    /// must.
    fn start_lesson(&mut self, lesson: usize) -> Vec<Effect> {
        let lesson = lesson.min(LESSONS.len() - 1);
        let before = self.grid.clone();
        self.mode = Mode::Normal;
        self.pending = None;
        self.count = None;
        (LESSONS[lesson].setup)(self);
        self.tutor = Some(Tutor {
            lesson,
            start: self.grid.clone(),
            entries: self.arrangement.entries.clone(),
            seen: false,
            complete: false,
        });
        // Setting up changes the song without undo, so send what it changed.
        (self.grid.tracks.iter().zip(&before.tracks))
            .enumerate()
            .filter(|(_, (now, was))| now != was)
            .map(|(t, _)| Effect::TrackChanged(t))
            .collect()
    }

    /// Ends the tutor. Its song stays, but isn't counted as unsaved.
    fn end_tutor(&mut self, message: &str) {
        self.tutor = None;
        self.mark_saved();
        self.message = Some(message.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Key;
    use crate::{COMMANDS, Transport};

    fn editor() -> Editor {
        let grid = Grid {
            name: "mine".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::default(); 16],
                ..Track::default()
            }],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        crate::command::run(&mut e, "tutor").unwrap();
        e
    }

    /// Plays a lesson's solution, ticking the tutor after each key as the frontend does.
    fn play(e: &mut Editor, solution: &str) {
        let mut chars = solution.chars();
        while let Some(c) = chars.next() {
            if c == '<' {
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                match name.as_str() {
                    "playing" => e.set_transport(Some(Transport {
                        positions: vec![Some((0, 0)); e.grid.tracks.len()],
                        into_step: 0.0,
                        count_in: 0,
                    })),
                    "stopped" => e.set_transport(None),
                    name => {
                        let key = Key::parse(name).unwrap_or_else(|| panic!("no key {name}"));
                        e.handle(key);
                    }
                }
            } else {
                e.handle(Key::Char(c));
            }
            e.tutor_tick();
        }
    }

    #[test]
    fn every_lesson_can_be_done_in_turn() {
        let mut e = editor();
        for (i, lesson) in LESSONS.iter().enumerate() {
            assert_eq!(
                e.tutor.as_ref().map(|t| t.lesson),
                Some(i),
                "{}",
                lesson.title
            );
            // Not done before it's begun.
            e.tutor_tick();
            assert_eq!(e.tutor_view().unwrap().complete, None, "{}", lesson.title);
            play(&mut e, lesson.solution);
            let next = LESSONS.get(i + 1).map(|l| l.title);
            assert_eq!(
                e.tutor_view().unwrap().complete,
                Some(next),
                "{}",
                lesson.title
            );
            e.handle(Key::Char(' '));
        }
        assert!(!e.tutoring());
        assert!(!e.modified(), "the tutor's song isn't left unsaved");
    }

    #[test]
    fn every_lesson_can_be_done_on_its_own() {
        for (i, lesson) in LESSONS.iter().enumerate() {
            let mut e = editor();
            e.start_lesson(i);
            play(&mut e, lesson.solution);
            assert!(
                e.tutor_view().unwrap().complete.is_some(),
                "{}",
                lesson.title
            );
        }
    }

    #[test]
    fn every_action_and_command_is_taught_or_listed() {
        let taught: Vec<Action> = LESSONS.iter().flat_map(|l| l.teaches).copied().collect();
        for action in Action::ALL {
            let listed = NOT_TAUGHT.contains(action);
            assert!(
                taught.contains(action) != listed,
                "{action:?}: teach it in a lesson in tutor.rs, or list it in NOT_TAUGHT (not \
                 both)"
            );
        }
        let commands: Vec<&str> = LESSONS.iter().flat_map(|l| l.commands).copied().collect();
        for spec in COMMANDS {
            let listed = COMMANDS_NOT_TAUGHT.contains(&spec.name);
            assert!(
                commands.contains(&spec.name) != listed,
                ":{}: teach it in a lesson in tutor.rs, or list it in COMMANDS_NOT_TAUGHT (not \
                 both)",
                spec.name
            );
        }
        for name in COMMANDS_NOT_TAUGHT.iter().chain(&commands) {
            assert!(
                COMMANDS.iter().any(|s| s.name == *name),
                ":{name} isn't a command"
            );
        }
    }

    #[test]
    fn lessons_name_what_they_teach() {
        for lesson in LESSONS {
            for action in lesson.teaches {
                assert!(
                    lesson.text.contains(&format!("{{{}}}", action.name())),
                    "{}: the text should name {{{}}}",
                    lesson.title,
                    action.name()
                );
            }
            for name in lesson.commands {
                let spec = COMMANDS.iter().find(|s| s.name == *name).unwrap();
                let written = |n: &str| lesson.text.contains(&format!("`:{n}"));
                assert!(
                    written(spec.name) || spec.alias.is_some_and(written),
                    "{}: the text should show :{name}",
                    lesson.title
                );
            }
            // Every placeholder is an action.
            let mut rest = lesson.text;
            while let Some(open) = rest.find('{') {
                let close = rest[open..].find('}').unwrap() + open;
                let name = &rest[open + 1..close];
                assert!(Action::named(name).is_some(), "{}: no {name}", lesson.title);
                rest = &rest[close + 1..];
            }
        }
    }

    #[test]
    fn sections_start_at_lessons_in_order() {
        let firsts: Vec<usize> = (SECTIONS.iter())
            .map(|(name, first)| {
                (LESSONS.iter().position(|l| l.title == *first))
                    .unwrap_or_else(|| panic!("{name}: no lesson {first}"))
            })
            .collect();
        assert_eq!(firsts[0], 0);
        assert!(firsts.is_sorted(), "{firsts:?}");
        assert_eq!(section(0), "Basics");
        assert_eq!(section(LESSONS.len() - 1), "Help");
    }

    #[test]
    fn keys_are_shown_as_bound() {
        let mut e = editor();
        let view = e.tutor_view().unwrap();
        assert_eq!(view.number, 1);
        assert!(view.text[1].contains("`h` `j` `k` `l`"), "{}", view.text[1]);
        e.keymap.bind_normal(None, Key::Char('h'), "none").unwrap();
        let view = e.tutor_view().unwrap();
        assert!(view.text[1].contains("`left` `j`"), "{}", view.text[1]);
        // Through a menu.
        assert_eq!(fill("{goto_mixer}", &e.keymap), "`gm`");
    }

    #[test]
    fn next_back_and_end() {
        let mut e = editor();
        crate::command::run(&mut e, "tutor next").unwrap();
        assert_eq!(e.tutor_view().unwrap().number, 2);
        crate::command::run(&mut e, "tutor back").unwrap();
        assert_eq!(e.tutor_view().unwrap().number, 1);
        crate::command::run(&mut e, "tutor end").unwrap();
        assert!(!e.tutoring());
        assert!(crate::command::run(&mut e, "tutor next").is_err());
    }

    #[test]
    fn starting_keeps_unsaved_work_unless_told() {
        let grid = Grid {
            name: "mine".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::default(); 16],
                ..Track::default()
            }],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        e.handle(Key::Char('i'));
        e.handle(Key::Char('z'));
        assert!(crate::command::run(&mut e, "tutor").is_err());
        assert!(crate::command::run(&mut e, "tutor!").is_ok());
        assert!(e.tutoring());
    }
}
