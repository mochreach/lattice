//! The tutor (`:tutor`, or `lattice --tutor`): lessons worked through on a song of its own, as
//! Helix's tutor is. Each lesson says what to do, and moves on by itself once it's been done,
//! judged by the song and editor rather than the keys pressed, so any way of doing it counts.
//!
//! Keys in a lesson's text are written as `{action_name}` and shown as whatever they're bound to,
//! so a changed keymap is taught as it is. The tests keep the tutor up to date: every action and
//! command is either taught by a lesson or listed in [`NOT_TAUGHT`] or [`COMMANDS_NOT_TAUGHT`],
//! and every lesson's solution is played through to check it still works.

use lattice_core::{
    Arrangement, Buses, Condition, Entry, FxKind, Gesture, Grid, InstKind, Instrument, Note, Song,
    Step, Track,
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
    "new",
    "new!",
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
/// None if it's been renamed or removed, and then its lesson can't be done.
fn track<'a>(e: &'a Editor, name: &str) -> Option<&'a Track> {
    e.grid.tracks.get(index(e, name)?)
}

fn index(e: &Editor, name: &str) -> Option<usize> {
    e.grid.tracks.iter().position(|t| t.name == name)
}

/// The note on `step` of `track`, if there's such a step.
fn note(track: &Track, step: usize) -> Option<Option<Note>> {
    track.steps.get(step).map(|s| s.note)
}

fn has_note(track: &Track, step: usize) -> bool {
    track.steps.get(step).is_some_and(|s| s.note.is_some())
}

/// The run of bass notes the selection lessons work on.
fn run() -> std::ops::RangeInclusive<usize> {
    4..=7
}

fn fill_run(e: &mut Editor) {
    let Some(bass) = index(e, "BASS").and_then(|t| e.grid.tracks.get_mut(t)) else {
        return;
    };
    for (i, step) in run().enumerate() {
        if let Some(slot) = bass.steps.get_mut(step)
            && slot.note.is_none()
        {
            *slot = Step::note(36 + 3 * i as u8);
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
        .find(|(_, first)| (LESSONS.iter().take(lesson + 1)).any(|l| l.title == *first))
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
               down four. The track under the cursor opens out, and {left} {right} go through \
               its fields, the note, its velocity and more, before the next track; \
               {prev_track} {next_track} jump straight to the previous and next track's notes. \
               Move to the BASS track's step 08.",
        teaches: &[
            Action::Left,
            Action::Down,
            Action::Up,
            Action::Right,
            Action::PrevTrack,
            Action::NextTrack,
        ],
        commands: &["tutor"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let c = e.cursor();
            index(e, "BASS") == Some(c.track) && c.step == 8
        },
        solution: "tltl8j",
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
            c.track + 1 == e.grid.tracks.len() && c.step + 1 == e.track_len(c.track)
        },
        solution: "gegl",
    },
    Lesson {
        title: "Entering notes",
        text: "{insert} starts insert mode at the selection, and {append} after it. There, the \
               keyboard's bottom two rows are a piano: `z` is C, `s` C#, `x` D and so on, \
               with the B below on `\\`, and the octave above from `q`, on into the next \
               from `i`. Each note moves the cursor down a step, or more: `C-4` makes it four \
               (a note a beat), `C-1` one again and `C-0` none, in any mode. {clear_advance} \
               clears a step instead. With every letter a note, the arrow keys move without entering one, \
               so `down` skips a step and leaves it as it was; `space` still plays and stops. \
               {keyboard_octave_up} and {keyboard_octave_down} change the keyboard's \
               octave, in any mode and on any page, and {escape} goes back to normal mode.\n\nGo to the top of the LEAD track, put four notes in it, then go \
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
            let notes = (track(e, "LEAD").into_iter())
                .flat_map(|t| &t.steps)
                .filter(|s| s.note.is_some())
                .count();
            notes >= 4 && e.mode == Mode::Normal
        },
        solution: "glggizxcv<esc>",
    },
    Lesson {
        title: "Playing",
        text: "{toggle_play} plays the pattern, looping, and stops it again. {play_song} plays \
               the whole song from its start, and {play_from_here} from the pattern you're \
               on. Stopping lets the notes ring out and the echoes fade; {hush} stops \
               everything at once, every note, echo and tail.\n\nPlay the pattern, listen to \
               your notes, then stop it.",
        teaches: &[
            Action::TogglePlay,
            Action::PlaySong,
            Action::PlayFromHere,
            Action::Hush,
        ],
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
               count-in. A MIDI keyboard plays too, once it's plugged in: its notes go in and \
               are recorded as the piano keys' are, at the velocity they're played, and \
               elsewhere they're only heard. `:midi` says which keyboards Lattice hears, and \
               `:midi-learn toggle_record` binds the next pedal, button or knob you move to \
               an action (or a `:command`, or `macro 1`). On a setting on the mixer, fx or \
               instrument page, `:midi-learn this` binds the next knob you turn to that \
               setting, in this song: it's marked ◎, and turns it wherever the cursor is.\n\nArm \
               recording, go back to normal mode, and disarm it.",
        teaches: &[Action::ToggleRecord],
        commands: &["midi", "midi-learn"],
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
            track(e, "BASS").is_some_and(|bass| {
                run().all(|s| !has_note(bass, s)) && has_note(bass, 0) && has_note(bass, 8)
            })
        },
        solution: "ghtltlgg4jv3jd",
    },
    Lesson {
        title: "Undo",
        text: "{undo} undoes and {redo} redoes. Every change can be undone.\n\nBring back the \
               notes you deleted.",
        teaches: &[Action::Undo, Action::Redo],
        commands: &[],
        // Reached some other way than by deleting them, delete them, to undo.
        setup: |e| {
            let Some(t) = index(e, "BASS") else {
                return;
            };
            let deleted = (e.grid.tracks.get(t)).is_some_and(|b| run().all(|s| !has_note(b, s)));
            if !e.undo.is_empty() && deleted {
                return;
            }
            fill_run(e);
            e.checkpoint();
            if let Some(bass) = e.grid.tracks.get_mut(t) {
                for s in run() {
                    if let Some(step) = bass.steps.get_mut(s) {
                        *step = Step::default();
                    }
                }
            }
        },
        watch: never,
        done: |e, _| track(e, "BASS").is_some_and(|bass| run().all(|s| has_note(bass, s))),
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
            let now = track(e, "BASS");
            let was = index(e, "BASS").and_then(|b| t.start.tracks.get(b));
            let (Some(now), Some(was)) = (now, was) else {
                return false;
            };
            run().all(|s| match (note(now, s), note(was, s)) {
                (Some(Some(now)), Some(Some(was))) => now.pitch == was.pitch + 12,
                _ => false,
            })
        },
        solution: "ghtltlgg4jv3j>",
    },
    Lesson {
        title: "Copy and paste",
        text: "{yank} copies the selection, and {paste} pastes it at the cursor, over what's \
               there, then moves on past it, as a note does, so pasting again carries on down \
               the track; {paste_insert} pushes the rest down instead.\n\nCopy the KICK track's \
               first beat, steps 00 to 03, onto the HAT track at step 08.",
        teaches: &[Action::Yank, Action::Paste, Action::PasteInsert],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let (Some(kick), Some(hat)) = (track(e, "KICK"), track(e, "HAT")) else {
                return false;
            };
            (0..4).all(|s| note(kick, s).is_some_and(|n| note(hat, 8 + s) == Some(n)))
        },
        solution: "ghggv3jytl5jp",
    },
    Lesson {
        title: "Registers",
        text: "Copying again replaces what you copied, unless you put it somewhere of its own. \
               {select_register} then a letter or digit picks a register for the next copy or \
               paste, so `\"ay` copies into register a and `\"ap` pastes from it, while plain \
               {yank} and {paste} carry on with their own. {select_register} alone shows what \
               each register holds.\n\nCopy the KICK track's first beat into register a, then \
               copy anything else the usual way, and paste from register a onto the HAT track \
               at step 12.",
        teaches: &[Action::SelectRegister],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let (Some(kick), Some(hat)) = (track(e, "KICK"), track(e, "HAT")) else {
                return false;
            };
            let holds = |c| (e.registers.get(&c)).is_some_and(|r| !r.steps.is_empty());
            holds('a')
                && holds(crate::registers::DEFAULT)
                && (0..4).all(|s| note(kick, s).is_some_and(|n| note(hat, 12 + s) == Some(n)))
        },
        solution: "ghggv3j\"aytl;y9j\"ap",
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
            let Some((t, kick)) = index(e, "KICK").and_then(|t| Some((t, e.grid.tracks.get(t)?)))
            else {
                return;
            };
            let points: Vec<Selection> = (0..kick.steps.len())
                .filter(|&s| has_note(kick, s))
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
               `tab` takes one, then the values that fit each word, such as the effects after \
               `:fx-add `. `↑` brings back the commands you've run. `:w` saves the song, `:q` quits ({quit} too, asking first if there are unsaved changes), and `:bpm` \
               sets the tempo.\n\nSet the tempo to 128.",
        teaches: &[Action::CommandLine, Action::Quit],
        commands: &["write", "quit", "bpm"],
        setup: nothing,
        watch: never,
        done: |e, _| e.bpm == 128.0,
        solution: ":bpm 128<ret>",
    },
    Lesson {
        title: "Recording keys",
        text: "{record_keys} then a letter records the keys you press into that register, \
               until {record_keys} again; {replay_keys} then the letter presses them all over, \
               and a count before it repeats them. Moves, edits and whole command lines all go \
               in, so you can record a change once and make it everywhere. A replay is one \
               undo, and stops at a move that can't go any further. Record into an F key \
               instead, `QF4`, and F4 alone presses them.\n\nPut the cursor on step \
               04 of the BASS track. Record into register a: raise that note an octave, then \
               move down a step. Stop, and replay it three times for the other notes of the run.",
        teaches: &[Action::RecordKeys, Action::ReplayKeys],
        commands: &[],
        setup: fill_run,
        watch: never,
        done: |e, t| {
            let now = track(e, "BASS");
            let was = index(e, "BASS").and_then(|b| t.start.tracks.get(b));
            let (Some(now), Some(was)) = (now, was) else {
                return false;
            };
            let recorded = (e.registers.get(&'a')).is_some_and(|r| !r.keys.is_empty());
            recorded
                && run().all(|s| match (note(now, s), note(was, s)) {
                    (Some(Some(now)), Some(Some(was))) => now.pitch == was.pitch + 12,
                    _ => false,
                })
        },
        solution: "ghtltlgg4jQa>jQ3qa",
    },
    Lesson {
        title: "Mute and solo",
        text: "Some keys open a menu of what can follow, shown in the corner. {toggle_mute} \
               mutes the tracks the selections reach (`m` alone does too, on every page), and \
               {toggle_solo} solos them (as `s` alone does where there are no notes to select); \
               {toggle_pattern_mute} (or `:pattern-mute`) mutes them in this pattern only.\n\nMute the BASS \
               track, listen, and unmute it.",
        teaches: &[
            Action::ToggleMute,
            Action::ToggleSolo,
            Action::TogglePatternMute,
        ],
        commands: &["pattern-mute"],
        setup: nothing,
        watch: |e| track(e, "BASS").is_some_and(|t| t.mix.mute),
        done: |e, t| t.seen && track(e, "BASS").is_some_and(|t| !t.mix.mute),
        solution: "ghtltltmtm",
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
            let Some(was) = index(e, "BASS").and_then(|b| t.start.tracks.get(b)) else {
                return false;
            };
            (track(e, "BASS").into_iter())
                .flat_map(|t| t.rules.iter().skip(was.rules.len()))
                .any(|r| r.when != Condition::Always)
        },
        solution: "ghtltl:tr 5 every 4<ret>",
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
        solution: "ghtltlta",
    },
    Lesson {
        title: "The mixer",
        text: "{goto_mixer} opens the mixer on the cursor's track, and {goto_pattern} comes \
               back. There, {left} {right} move between strips, {down} goes into the \
               detail, and {note_up} {note_down} turn what's under the cursor. `:level -6`, \
               `:pan 20` and `:send -12` set them by number, and `:set` the setting under the cursor.\n\n\
               Turn the BASS track's level down, then come back to the pattern.",
        teaches: &[Action::GotoMixer, Action::GotoPattern],
        commands: &["level", "pan", "send", "set"],
        setup: nothing,
        watch: never,
        done: |e, t| {
            let Some(was) = index(e, "BASS").and_then(|b| t.start.tracks.get(b)) else {
                return false;
            };
            track(e, "BASS").is_some_and(|t| t.mix.level < was.mix.level) && e.page == Page::Pattern
        },
        solution: "ghtltlgm<C-x>gp",
    },
    Lesson {
        title: "Instruments and effects",
        text: "{goto_instrument} opens the instrument page: every track's instrument side by \
               side, a column each, with its settings, the track's macros and its effects. \
               {left} {right} move across the tracks and {up} {down} down a column, and \
               {note_up} {note_down} turn a setting; {change} on the instrument's name changes \
               it. {insert} plays the track on the piano keys, to try it out, without entering \
               notes; the arrows still move and {note_up} {note_down} still turn, so you can \
               shape it as you play, and {escape} stops. \
               `:inst switched` changes it too (a big stack of saws through a screaming ladder \
               filter), `:inst krell` gives it an alien harpsichord, and \
               `:sample kick.wav` plays a sound file on it. \
               `:port minilogue` plays a synth of your own over MIDI instead, its notes and \
               locks going out to it, `:cc 74 40` sends it a controller, and \
               `:clock digitakt` keeps a drum machine in time. \
               `:fx-add delay` adds an effect: `delay`, `reverb`, `filter`, `ensemble`, \
               `phaser`, `drive`, `comp` or `pitch`.\n\nAdd a reverb to the LEAD track.",
        teaches: &[Action::GotoInstrument],
        commands: &["inst", "sample", "port", "cc", "clock", "fx-add"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (track(e, "LEAD").into_iter())
                .flat_map(|t| t.effects.iter())
                .any(|fx| fx.kind == FxKind::Reverb)
        },
        solution: "gl:fx-add reverb<ret>",
    },
    Lesson {
        title: "Effects in lanes",
        text: "{goto_effects} opens the fx page: the track's effects drawn like the pattern, \
               the sound going down through them, each with its settings on the lines below \
               it. {note_up} {note_down} turn the setting under the cursor as you pass it, and \
               {next_note} {prev_note} jump from effect to effect. {open_below} starts typing `:fx-add` for an effect after the \
               cursor's, and {goto_pattern} comes back. They can \
               branch: `:fx-send B` adds a row after the cursor that sends the sound into lane \
               B, beside the first, starting it if it's new. A send into its own lane or one to \
               its left is feedback, heard again a moment later, and kept below 0 dB so it dies \
               away. Each lane ends in an `out`, its level into the track; {note_up} \
               {note_down} turn a send or an out, and {delete} on an out removes its lane.\n\n\
               On the LEAD track, branch into a new lane B and put a delay on it.",
        teaches: &[Action::GotoEffects],
        commands: &["fx-send"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let Some(lead) = track(e, "LEAD").map(|t| &t.effects) else {
                return false;
            };
            (lead.rows()).any(|(at, _, effect)| {
                at.lane > 0
                    && effect
                        .and_then(|i| lead.get(i))
                        .is_some_and(|fx| fx.kind == FxKind::Delay)
            })
        },
        solution: "gpglgf:fx-send B<ret>l:fx-add delay<ret>gp",
    },
    Lesson {
        title: "Shimmer",
        text: "A chain can be saved, to put on any track of any song. `:chain-save wash` saves \
               the cursor's track's chain (on the mixer and fx pages, the strip's) as `wash`, \
               and `:chain-load wash` puts it on another, in place of its own. Lattice comes \
               with `shimmer`, a reverb whose sound goes round again an octave up, through a \
               `pitch` shifter, so it rises as it fades, as on Eno's ambient records.\n\n\
               Load the shimmer onto the LEAD track, look at it on the fx page, and come back \
               to the pattern.",
        teaches: &[],
        commands: &["chain-save", "chain-load"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (track(e, "LEAD").into_iter())
                .flat_map(|t| t.effects.iter())
                .any(|fx| fx.kind == FxKind::Pitch)
                && e.page == Page::Pattern
        },
        solution: "gpgl:chain-load shimmer<ret>gfgp",
    },
    Lesson {
        title: "Locks",
        text: "A step can lock its track's settings: each changes on that step, and holds \
               until the setting's next lock. `:lock cutoff 2000` locks the selected steps, \
               `:lock cutoff ~4000` slides there, arriving on the step, `:lock cutoff --` puts \
               it back to the mixer's, `:lock cutoff 300..4000` sweeps across them, and \
               `:unlock` clears them. On the mixer, {lock_setting}, then a change, then \
               {lock_setting} again locks what you changed.\n\n\
               The track under the cursor opens out, showing each note's velocity and a \
               column for each setting it locks, up to four: 00 to 99 across the setting's \
               range, `~` sliding, `--` put back, and `╵` where a value carries on. Other \
               tracks mark a step with locks `◆`. {toggle_open} keeps the selected tracks open \
               too, and again closes them. `:lock cutoff` alone adds a column, and `:unlock` \
               along the whole track takes one away.\n\n\
               {goto_field}, then a field's letter, underlined in its header, goes straight to \
               it: `n` the note, `v` the velocity, `g` `G` the gestures. In a field, insert mode's digits type values, \
               two to a step, `~` first slides and `-` puts the setting back; {note_up} \
               {note_down} nudge them, {delete} {yank} {paste} work on them, and {fill} fills \
               a selection evenly between the values at its ends.\n\n\
               Lock the LEAD track's cutoff on step 00.",
        teaches: &[
            Action::LockSetting,
            Action::ToggleOpen,
            Action::GotoField,
            Action::Fill,
        ],
        commands: &["lock", "unlock"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let Some(lead) = track(e, "LEAD") else {
                return false;
            };
            let cutoff = (lead.instrument.params().iter()).find(|p| p.name == "cutoff");
            cutoff.is_some_and(|p| {
                (lead.steps.first().into_iter())
                    .flat_map(|s| s.locks.iter())
                    .any(|l| l.fx.is_none() && u32::from(l.param) == p.id)
            })
        },
        solution: "glgg:lock cutoff 2000<ret>",
    },
    Lesson {
        title: "Macros",
        text: "A macro is one knob that turns several settings at once. On the fx page, \
               `:map bloom` on a setting has the macro `bloom` turn it, adding the macro if \
               it's new (`:macro bloom` adds one on its own); `:map bloom -0.5` turns it the \
               other way, by half its range when `bloom` is all the way up, and `:unmap bloom` \
               lets it go. The macros are a column left of lane A: \
               {note_up} {note_down} turn a macro, or under it, how far it turns each setting, \
               and {delete} removes either. A setting a macro turns is marked with the macro's number. The settings keep their own values, and the macros \
               move them from there. A macro locks as a setting does, by its name: `:lock \
               bloom 1`.\n\n\
               On the LEAD track, add a filter, map its cutoff to `bloom`, and turn `bloom` up.",
        teaches: &[],
        commands: &["macro", "map", "unmap"],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (track(e, "LEAD").into_iter())
                .flat_map(|t| t.macros.iter())
                .any(|m| m.name == "bloom" && m.value > 0.0 && !m.targets.is_empty())
        },
        solution: "gpglgf:fx-add filter<ret>jj:map bloom<ret>hk10<C-a>gp",
    },
    Lesson {
        title: "Presets",
        text: "A preset is an instrument made into one of its own: its settings, the effects \
               after it and the macros that turn them, put on a track at once by `:preset \
               night-bus`, in place of the track's own. Lattice comes with some; \
               `:preset-save cold-iron` saves the track's as another, to put on any track of \
               any song. A preset gives its track a face: a title, `:face Cold Iron` to change \
               it, and the controls it puts forward, its macros and whichever settings \
               `:face-show cutoff` adds (`:face-hide cutoff` takes one back). Changing the \
               instrument for another kind takes the face away. The instrument page \
               ({goto_instrument}) shows a face in place of the instrument's settings, its art \
               lit by what the track plays: drawn from its title, or by hand, as \
               `great-machine`'s is, the Great Machine of the Krell, moving with each beat \
               and note; `:face-art lamp.txt` draws one from a text file of your own. A track \
               without a face shows its engine's drawing. {open} on its title opens \
               all the settings, and again goes back.\n\n\
               Put `night-bus` on the LEAD track, and show its resonance on its face.",
        teaches: &[],
        commands: &[
            "preset",
            "preset-save",
            "face",
            "face-art",
            "face-show",
            "face-hide",
        ],
        setup: nothing,
        watch: never,
        done: |e, _| {
            let Some(lead) = track(e, "LEAD") else {
                return false;
            };
            let resonance = (lead.instrument.params().iter()).position(|p| p.name == "resonance");
            (lead.face.as_ref()).is_some_and(|face| {
                face.title == "Night Bus"
                    && resonance.is_some_and(|r| face.controls.contains(&(r as u8)))
            })
        },
        solution: "gpgl:preset night-bus<ret>:face-show resonance<ret>",
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
        solution: "ghvtltt:export-loop<ret>:track-add BEAT<ret>:chop<ret>",
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
                // A chop has its pieces, so this sets them.
                let _ = instrument.set(chop::PIECES, 16.0);
                let steps = (0..16).map(|i| Step::note(chop::pitch(i))).collect();
                let beat = Track {
                    name: "BEAT".into(),
                    steps,
                    instrument,
                    ..Track::default()
                };
                // After the HAT track, or first if it's gone.
                let at = index(e, "HAT")
                    .map_or(0, |h| h + 1)
                    .min(e.grid.tracks.len());
                // At most at the end, so it goes in.
                let _ = e.insert_track(at, beat);
            }
        },
        watch: never,
        done: |e, t| {
            let Some(c) = e.grid.tracks.iter().position(chopped) else {
                return false;
            };
            let (Some(now), Some(was)) = (e.grid.tracks.get(c), t.start.tracks.get(c)) else {
                return false;
            };
            let (Some(now_end), Some(was_end), Some(now_run), Some(repeated)) = (
                now.steps.get(12..16),
                was.steps.get(12..16),
                now.steps.get(4..8),
                was.steps.get(4),
            ) else {
                return false;
            };
            let mut turned = was_end.to_vec();
            crate::rearrange::reverse(&mut turned, &was.instrument);
            now_end == turned.as_slice() && now_run.iter().all(|s| s == repeated)
        },
        solution: "ghtltlgg12jv3j~<esc>;11kv3j*",
    },
    Lesson {
        title: "Looping",
        text: "Lattice can record a loop from the audio input, as a loop pedal does. Tap \
               {loop_pedal} (right Alt, or AltGr, on its own; `!` where the terminal doesn't \
               report it) and it records the cursor's track from its next step 00, starting the \
               song with a count-in if it's stopped: a pass, from 00 round to 00 again (`●` by \
               the track's name), which then plays back, cut into a piece a step. Tap it again \
               and it overdubs a pass over it, a layer a tap, and {undo} takes one off; tapped \
               while it records, the pass finishes and the loop plays on. On a track of notes it \
               adds a track beside it, `loop`, to record into. `:midi-learn loop_pedal` puts it \
               on a foot switch.\n\n\
               A loop track shows how it records under its name: `at 00 once`. `:overdub` sets \
               it to `dub`, overdubbing pass after pass until you tap again, and back. \
               `:threshold -30` (`-30dB`) makes it listen instead: it records from where you come \
               in, the moment you play louder than -30 dB, and if the song's stopped, starts it \
               from 00 with you; `:threshold off` goes back to waiting for 00.\n\n\
               `:arm` arms the track for record ({toggle_record}) to record instead, and \
               `:arm 1` records one pass and stops, `:arm 4` four. `:arm!` records a new loop \
               over the old one, and `:disarm` disarms; delete a loop's notes and the next pass \
               starts a new one. \
               `:input` chooses what to record from: type `:input ` and the devices are \
               suggested, or enter `:input` alone to list them all, then a number or part of a \
               name picks one. Apps playing are listed too, \
               `app: Firefox`, to record just that app's sound. `:output` chooses what you \
               hear. If what you play comes back late or early, `:latency 20` moves it by \
               milliseconds, or `:latency -20`.\n\nStart a loop on the chop track.",
        teaches: &[Action::LoopPedal],
        commands: &[
            "arm",
            "arm!",
            "disarm",
            "threshold",
            "overdub",
            "input",
            "output",
            "latency",
        ],
        setup: nothing,
        watch: never,
        done: |e, _| e.armed().is_some(),
        solution: "ghtltl<ralt>",
    },
    Lesson {
        title: "Patterns",
        text: "A song is a list of patterns, which all have the same tracks, each with its own \
               notes. `:pattern-new verse` makes an empty pattern, `:pattern-dup` a copy of this \
               one, and `:pattern` goes to one by name; {next_pattern} and {prev_pattern} step \
               through them. `:pattern-name` renames this pattern and `:pattern-remove` removes \
               it. A pattern plays for as long as its longest track, or `:bars 2` sets how many \
               bars. A bar is four beats, or `:meter 3` makes it three; `:meter 2 3` makes 6/8, \
               two beats of three steps, and `:meter 2+2+3` groups a bar of seven, accenting \
               the start of each group. `:pattern-meter 5` gives just this pattern bars of \
               five, until `:pattern-meter song`.\n\nMake a copy of this pattern called `chorus`.",
        teaches: &[Action::NextPattern, Action::PrevPattern],
        commands: &[
            "pattern-new",
            "pattern-dup",
            "pattern",
            "pattern-name",
            "pattern-remove",
            "bars",
            "meter",
            "pattern-meter",
        ],
        setup: nothing,
        watch: never,
        done: |e, _| e.pattern_name() == "chorus",
        solution: ":pattern-dup chorus<ret>",
    },
    Lesson {
        title: "The song page",
        text: "{goto_song} opens the song page: the order the patterns play in, a bar to a row, \
               time going down. Each entry plays a pattern some number of times, marked down \
               the left in its pattern's colour; across are the tracks, each showing how busy \
               it is in each bar. {up} {down} move a bar, {next_note} {prev_note} from entry to \
               entry, and {left} {right} across the tracks. {note_up} {note_down} change which \
               pattern the entry plays, and {longer} {shorter} play it more or fewer \
               times.\n\nOpen the song page and make an entry play twice.",
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
               it, at the cursor's bar and on its track. {change} renames the entry's pattern, and `:bars` and `:pattern-name` here \
               change the cursor's pattern rather than the one being edited.\n\nAdd an entry, \
               then edit its pattern.",
        teaches: &[Action::OpenBelow, Action::OpenAbove, Action::Open],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| e.arrangement.entries.len() > t.entries.len() && e.page == Page::Pattern,
        solution: "gso<ret>",
    },
    Lesson {
        title: "Sitting out an entry",
        text: "A track can sit out one entry and play in the rest. On the song page, on a \
               track's column, `m` mutes it in the cursor's entry alone: its bars there show a \
               line across, and the song plays them silent. `s` solos it instead, muting the \
               others there. {select_mode} selects bars and tracks to do several at once, and \
               {select_rows} the whole entry, again for the next; `m` and `s` act on them all, \
               as do {delete}, {yank}, {paste} and {lock_setting}.\n\nMute a track in an \
               entry.",
        teaches: &[],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, t| {
            (e.arrangement.entries.iter().zip(&t.entries)).any(|(now, was)| now.mutes != was.mutes)
        },
        solution: "gslm",
    },
    Lesson {
        title: "Looping a section",
        text: "To work on one part of the song, loop it. On the song page, {loop_selection} \
               loops the song round the selected entries, or round the selected bars of one, or \
               with nothing selected, round the cursor's bar; on what's looping, it stops. A bar \
               down the left marks what it covers, and the song plays from the cursor as ever, \
               going back to the loop's start at its end. On the pattern page, {loop_selection} \
               loops the selected steps, or the cursor's beat, the same way.\n\nLoop a bar.",
        teaches: &[Action::LoopSelection],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| e.song_loop().is_some(),
        solution: "gs|",
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
        title: "Gestures",
        text: "Beside its velocity, an open track has two gesture fields on each step \
               ({goto_field} `g` `G`), for things that happen once there. In insert mode, type \
               one. Some act on the step's note, or on the one still sounding if the step has \
               none: `R3` plays it three times in the step, `r3` fading away, `D8` starts it \
               8/24 of a step late, `X6` cuts it 6/24 in, `A37` goes round it and the notes 3 \
               and 7 semitones up, and `!` accents it. Some change its sound, for that note \
               alone: `<` plays a sampler's or chop's sound the other way round, `O8` starts it \
               half way in, and `G6` slides a synth's, switched's or monochord's note in from the last one's \
               pitch; where the instrument can't, they're dimmed and do nothing. Others move \
               the track's playhead: `J12` \
               carries on from step 12, `J?` from any step, the same each time the song plays; \
               `W2` plays the step twice more before going on, `B` turns the track backwards, \
               or forwards again, and `*2` `/2` play it twice or half as fast, until the next \
               speed or the top of its loop.\n\n\
               Each goes in once it's whole, or with enter (`D2` could be `D23`), and the \
               status line says what the one under the cursor does. {note_up} {note_down} \
               change its amount, and {delete} {yank} {paste} work as in any field.\n\n\
               Put a retrigger on the first step of the KICK track, for a flam.",
        teaches: &[],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (track(e, "KICK").and_then(|t| t.steps.first())).is_some_and(|s| {
                s.gestures()
                    .any(|g| matches!(g, Gesture::Retrigger(_) | Gesture::Fade(_)))
            })
        },
        solution: "gpghggfgiR2<esc>",
    },
    Lesson {
        title: "Chance and fills",
        text: "Some gestures decide whether a step plays at all, gestures and locks with it. \
               `?50` plays it on half the passes, the same ones each time the song plays, and \
               `1:4` on the first of every four (`2:4` the second, `3:3` the third of three). \
               `F` plays it only during a fill, and `F-` only outside one: while the song \
               plays, hold {play_fill} for a fill, and the header shows FILL. A fill starts \
               at once, on any step, so the `F` steps can be a snare roll or a jump (`F` with \
               `J12`) and the `F-` ones the beat it takes over from. Where the terminal \
               doesn't say when keys come up, {play_fill} starts a fill and again ends it.\n\n\
               `F` might be `F-`, so it goes in with enter.\n\n\
               Put a chance on a step of the KICK track, in its second gesture field.",
        teaches: &[Action::PlayFill],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (track(e, "KICK")).is_some_and(|t| {
                (t.steps.iter()).any(|s| s.gestures().any(|g| matches!(g, Gesture::Chance(_))))
            })
        },
        solution: "gpghggfGi?50<esc>",
    },
    Lesson {
        title: "The conductor",
        text: "Left of the first track is the conductor: two fields on each step for what \
               belongs to the whole song, not one track. From the first track's notes \
               ({first_track}), {left} goes into them, and {goto_field} `C` straight there, from any track. In \
               insert mode, type a gesture: `T140` sets the tempo from that step, `~140` \
               slides there, arriving on the step, and `T--` goes back to the song's `:bpm`; \
               `X` stops the song at the end of the step, `P3` cues pattern 3 for the next bar, \
               `M2` mutes track 2 and `M-2` unmutes it, and `S2` `S-2` solo and unsolo it, \
               until the song stops. Each goes in once it's whole, or with enter (`P3` could \
               be `P31`). {note_up} {note_down} nudge a tempo, {delete} {yank} \
               {paste} work as in any field, and {fill} fills tempos between two. It has its \
               own length, as long as the pattern until `:len` in it changes it, so a tempo \
               change can come once a bar or once in four.\n\n\
               Set the tempo to 140 on the first step of the conductor.",
        teaches: &[],
        commands: &[],
        setup: nothing,
        watch: never,
        done: |e, _| {
            (e.conductor().into_iter())
                .flat_map(|c| c.gestures())
                .any(|(_, g)| g == lattice_core::SongGesture::Tempo(140))
        },
        solution: "gpfCggit140<esc>",
    },
    Lesson {
        title: "Finding more",
        text: "That's the tutor. `:help` lists every key and command, as they're bound; \
               `:manual` explains everything in full, and `:manual delay` goes to what it says \
               about delay; on either, `/` finds words and `n` `N` the next and the one before. \
               `:tutor` starts this again.\n\nOpen the help page to finish.",
        teaches: &[],
        commands: &["help", "manual"],
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
            if let Some(slot) = steps.get_mut(step) {
                *slot = Step::note(pitch);
            }
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
            beats_per_bar: 4,
            grouping: vec![],
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
        click: true,
    }
}

/// `{action_name}` as the first keys bound to it, in `backticks`.
fn fill(text: &str, keymap: &Keymap) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('{') {
        out.push_str(before);
        let (name, after) = after.split_once('}').unwrap_or((after, ""));
        match Action::named(name) {
            Some(action) => match keys_for(keymap, action).first() {
                Some(keys) => out.push_str(&format!("`{keys}`")),
                None => out.push_str(&format!("({name}, unbound)")),
            },
            None => out.push_str(name),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Every way to press `action`, the shortest first (`w` before `]n`), then in order: in normal
/// mode, through a menu, then in insert mode.
pub(crate) fn keys_for(keymap: &Keymap, action: Action) -> Vec<String> {
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
    keys.sort_by_key(|k| (k.chars().count(), k.clone()));
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
        let lesson = LESSONS.get(tutor.lesson)?;
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
        let Some(lesson) = LESSONS.get(at) else {
            return vec![];
        };
        let seen = (lesson.watch)(self);
        let Some(tutor) = self.tutor.as_mut() else {
            return vec![];
        };
        tutor.seen |= seen;
        let done = self.tutor.as_ref().is_some_and(|t| (lesson.done)(self, t));
        if let Some(tutor) = self.tutor.as_mut().filter(|_| done) {
            tutor.complete = true;
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
        let lesson = lesson.min(LESSONS.len().saturating_sub(1));
        let before = self.grid.clone();
        self.mode = Mode::Normal;
        self.pending = None;
        self.count = None;
        if let Some(l) = LESSONS.get(lesson) {
            (l.setup)(self);
        }
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
            beats_per_bar: 4,
            grouping: vec![],
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
            beats_per_bar: 4,
            grouping: vec![],
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
