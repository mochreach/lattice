//! The numpad as a performance surface. Pads `1`–`9` play the drums of the cursor's group or,
//! on any other track, the notes of a scale (`:scale`) up from the keyboard octave. In mute mode
//! (`+`) they mute and unmute the first nine tracks instead. `/` `*` `-` choose how hard notes
//! are played and entered, soft, medium or hard (`:vel` sets any velocity). Holding `0` repeats
//! the pads played while it's down, in time, every 16th, 8th or quarter (`.` chooses), as on a
//! drum machine; this needs key releases.

use lattice_core::{DRUM_ROOT, Note};

use crate::{Editor, Effect, Key, Mode, NoteLayout};

/// What `/` `*` `-` set the velocity to.
pub const SOFT: u8 = 60;
pub const MEDIUM: u8 = 100;
pub const HARD: u8 = 127;

/// A scale the pads play, as semitones above its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    pub name: &'static str,
    pub steps: &'static [u8],
}

const MAJOR: Scale = Scale {
    name: "major",
    steps: &[0, 2, 4, 5, 7, 9, 11],
};

pub const SCALES: [Scale; 8] = [
    MAJOR,
    Scale {
        name: "minor",
        steps: &[0, 2, 3, 5, 7, 8, 10],
    },
    Scale {
        name: "dorian",
        steps: &[0, 2, 3, 5, 7, 9, 10],
    },
    Scale {
        name: "mixolydian",
        steps: &[0, 2, 4, 5, 7, 9, 10],
    },
    Scale {
        name: "pentatonic",
        steps: &[0, 2, 4, 7, 9],
    },
    Scale {
        name: "minor-pentatonic",
        steps: &[0, 3, 5, 7, 10],
    },
    Scale {
        name: "blues",
        steps: &[0, 3, 5, 6, 7, 10],
    },
    Scale {
        name: "chromatic",
        steps: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    },
];

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A root note's semitone above C: `C`, `F#`, `Bb`, any case.
fn parse_root(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let letter = b"CDEFGAB".iter().position(|&c| c == letter as u8)?;
    let natural = *MAJOR.steps.get(letter)?;
    let semitone = match chars.as_str() {
        "" => natural + 12,
        "#" => natural + 13,
        "b" => natural + 11,
        _ => return None,
    };
    Some(semitone % 12)
}

/// A root note's name, from its semitone above C.
fn note_name(root: u8) -> &'static str {
    // Always there: a root is a semitone, below 12.
    NOTE_NAMES
        .get(usize::from(root % 12))
        .copied()
        .unwrap_or("C")
}

/// What the pads do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadMode {
    /// Play drums or a scale.
    #[default]
    Play,
    /// Mute and unmute the first nine tracks.
    Mute,
}

impl Editor {
    /// Pad `n` (from 1).
    pub(crate) fn pad(&mut self, n: u8, key: Key) -> Vec<Effect> {
        if n == 0 {
            if self.releases {
                self.repeat = true;
            } else {
                self.message = Some(
                    "note repeat needs a terminal that reports key releases, such as kitty".into(),
                );
            }
            return vec![];
        }
        let n = usize::from(n);
        if self.pad_mode == PadMode::Mute {
            return self.mute_pad(n - 1);
        }
        let (track, pitch) = match self.grid.group_of(self.played_track()) {
            // In a group, its `n`th track at C-4, which plays a drum at its tuning.
            Some(g) => {
                let Some(group) = self.grid.groups.get(g) else {
                    return vec![];
                };
                if n > group.len {
                    return vec![];
                }
                (group.first + n - 1, DRUM_ROOT)
            }
            // Elsewhere, the scale's `n`th note on the cursor's track.
            None => match self.scale_pitch(n - 1) {
                Some(pitch) => (self.played_track(), pitch),
                None => return vec![],
            },
        };
        let every = if self.repeat { self.repeat_every } else { 0 };
        if let Some(mut effects) = self.record_note(key, &[track], pitch, every) {
            if every > 0 {
                // Heard now if it rounded back, as a plain note would be; then on the grid.
                let now = effects.iter().any(|e| matches!(e, Effect::Preview { .. }));
                effects.retain(|e| !matches!(e, Effect::Preview { .. }));
                effects.push(Effect::Repeat {
                    track,
                    pitch,
                    every,
                    now,
                });
            }
            return effects;
        }
        // In insert mode it's entered on the cursor's step, without moving on, so several drums
        // can go on one step.
        let mut effects = vec![];
        if self.mode == Mode::Insert {
            let step = self.cursor().step;
            let there = (self.grid.tracks.get(track)).is_some_and(|t| step < t.steps.len());
            if there {
                self.checkpoint();
                let velocity = self.velocity;
                if let Some(s) =
                    (self.grid.tracks.get_mut(track)).and_then(|t| t.steps.get_mut(step))
                {
                    s.note = Some(Note {
                        pitch,
                        velocity,
                        length: 1,
                    });
                }
                effects.push(Effect::TrackChanged(track));
            }
        }
        effects.push(if every > 0 {
            Effect::Repeat {
                track,
                pitch,
                every,
                now: true,
            }
        } else {
            Effect::Preview { track, pitch }
        });
        effects
    }

    /// How often pads repeat, as a note value: `1/16`.
    pub fn repeat_name(&self) -> String {
        let beat = self.grid.steps_per_beat.max(1) as usize;
        format!("1/{}", 4 * beat / self.repeat_every.max(1))
    }

    /// Whether the numpad's `0` is down, so pads repeat.
    pub fn repeating(&self) -> bool {
        self.repeat
    }

    /// The scale's `degree`th note (from 0), up from its root in the keyboard octave.
    fn scale_pitch(&self, degree: usize) -> Option<u8> {
        let steps = self.scale().steps;
        let octave = u16::from(self.octave) + 1 + degree.checked_div(steps.len())? as u16;
        let step = *steps.get(degree.checked_rem(steps.len())?)?;
        let pitch = octave * 12 + u16::from(self.root) + u16::from(step);
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn mute_pad(&mut self, track: usize) -> Vec<Effect> {
        if track >= self.grid.tracks.len() {
            return vec![];
        }
        self.checkpoint();
        if let Some(t) = self.grid.tracks.get_mut(track) {
            t.mix.mute = !t.mix.mute;
        }
        vec![Effect::TrackChanged(track)]
    }

    /// The scale the pads play.
    fn scale(&self) -> Scale {
        // Only `:scale` sets it, to one of these; major is where it starts.
        SCALES.get(self.scale).copied().unwrap_or(MAJOR)
    }

    /// The numpad's other keys: velocity, and the pads' mode.
    pub(crate) fn pad_op(&mut self, c: char) -> Vec<Effect> {
        let velocity = |name| format!("velocity {name}");
        match c {
            '/' => {
                self.velocity = SOFT;
                self.message = Some(velocity("soft"));
            }
            '*' => {
                self.velocity = MEDIUM;
                self.message = Some(velocity("medium"));
            }
            '-' => {
                self.velocity = HARD;
                self.message = Some(velocity("hard"));
            }
            '.' => {
                self.repeat_every = match self.repeat_every {
                    1 => 2,
                    2 => 4,
                    _ => 1,
                };
                self.message = Some(format!("pads repeat every {}", self.repeat_name()));
            }
            '+' => {
                self.pad_mode = match self.pad_mode {
                    PadMode::Play => PadMode::Mute,
                    PadMode::Mute => PadMode::Play,
                };
                self.message = Some(
                    match self.pad_mode {
                        PadMode::Play => "pads play",
                        PadMode::Mute => "pads mute tracks 1–9",
                    }
                    .into(),
                );
            }
            _ => {}
        }
        vec![]
    }

    /// `:scale NAME [ROOT]`.
    pub(crate) fn set_scale(&mut self, text: &str) -> Result<(), String> {
        let mut words = text.split_whitespace();
        let name = words.next().unwrap_or_default();
        let scale = SCALES.iter().position(|s| s.name == name).ok_or_else(|| {
            let names: Vec<_> = SCALES.iter().map(|s| s.name).collect();
            format!("no scale {name}: try {}", names.join(", "))
        })?;
        let root = match words.next() {
            Some(root) => {
                parse_root(root).ok_or_else(|| format!("no note {root}: try C, F#, Bb"))?
            }
            None => self.root,
        };
        self.scale = scale;
        self.root = root;
        if self.keymap.layout == NoteLayout::Scale {
            self.lay_out_keys(NoteLayout::Scale);
        }
        self.message = Some(format!("pads play {} {name}", note_name(root)));
        Ok(())
    }

    /// Lays the notes out on the keyboard for insert mode.
    pub(crate) fn lay_out_keys(&mut self, layout: NoteLayout) {
        self.keymap.piano = layout.keys(&self.scale(), self.root);
        self.keymap.layout = layout;
    }

    /// The scale the pads play, as `C major`.
    pub fn scale_name(&self) -> String {
        format!("{} {}", note_name(self.root), self.scale().name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let track = |len| Track {
            face: None,
            pattern_mute: false,
            name: "t".into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
            rules: vec![],
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track(16), track(16)],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn command(e: &mut Editor, line: &str) {
        e.handle(Key::Esc);
        e.handle(Key::Char(':'));
        for c in line.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
    }

    fn pad(e: &mut Editor, n: u8) -> Option<u8> {
        e.handle(Key::Pad(n))
            .iter()
            .find_map(|effect| match effect {
                Effect::Preview { pitch, .. } => Some(*pitch),
                _ => None,
            })
    }

    #[test]
    fn pads_play_a_scale_outside_a_group() {
        let mut e = editor();
        // C major from C-4, on into the next octave.
        let pitches: Vec<_> = (1..=9).map(|n| pad(&mut e, n)).collect();
        assert_eq!(pitches, [60, 62, 64, 65, 67, 69, 71, 72, 74].map(Some));

        command(&mut e, "scale minor-pentatonic A");
        assert_eq!(e.scale_name(), "A minor-pentatonic");
        assert_eq!(pad(&mut e, 1), Some(69));
        assert_eq!(pad(&mut e, 6), Some(81));
        command(&mut e, "scale blues bb");
        assert_eq!(pad(&mut e, 1), Some(70));
        command(&mut e, "scale major");
        assert_eq!(e.scale_name(), "A# major", "the root stays");

        command(&mut e, "scale lydian");
        assert!(e.message.as_deref().unwrap().contains("no scale lydian"));
        command(&mut e, "scale major H");
        assert!(e.message.as_deref().unwrap().contains("no note H"));
    }

    #[test]
    fn velocity_keys_set_how_hard_notes_go_in() {
        let mut e = editor();
        e.handle(Key::PadOp('/'));
        e.handle(Key::Char('i'));
        e.handle(Key::Char('z'));
        e.handle(Key::PadOp('-'));
        e.handle(Key::Pad(1));
        e.handle(Key::Char('x'));
        command(&mut e, "vel 30");
        let velocities: Vec<_> = e.grid.tracks[0].steps[..2]
            .iter()
            .map(|s| s.note.unwrap().velocity)
            .collect();
        assert_eq!(velocities, [SOFT, HARD]);
        assert_eq!(e.velocity, 30);
    }

    #[test]
    fn in_mute_mode_pads_mute_tracks() {
        let mut e = editor();
        e.handle(Key::PadOp('+'));
        assert_eq!(e.handle(Key::Pad(2)), [Effect::TrackChanged(1)]);
        assert!(e.grid.tracks[1].mix.mute);
        assert!(e.handle(Key::Pad(3)).is_empty(), "no third track");
        e.handle(Key::Pad(2));
        assert!(!e.grid.tracks[1].mix.mute);
        e.handle(Key::PadOp('+'));
        assert_eq!(pad(&mut e, 2), Some(62));
    }

    #[test]
    fn holding_zero_repeats_the_pads_until_it_or_they_come_up() {
        let mut e = editor();
        e.handle(Key::Pad(0));
        assert!(!e.repeating(), "no releases, no repeat");
        assert!(e.message.as_deref().unwrap().contains("key releases"));

        e.releases = true;
        e.handle(Key::PadOp('.'));
        assert_eq!(e.repeat_name(), "1/8");
        e.handle(Key::Pad(0));
        let repeat = |pitch| Effect::Repeat {
            track: 0,
            pitch,
            every: 2,
            now: true,
        };
        let release = |pitch| Effect::Release { track: 0, pitch };
        assert_eq!(e.handle(Key::Pad(1)), [repeat(60)]);
        assert_eq!(e.handle(Key::Pad(3)), [repeat(64)]);
        // Letting go of a pad ends its repeat; letting go of 0 ends them all.
        assert_eq!(e.release(Key::Pad(1)), [release(60)]);
        assert_eq!(e.release(Key::Pad(0)), [release(64)]);
        assert!(!e.repeating());
        assert_eq!(
            e.handle(Key::Pad(1)),
            [Effect::Hold {
                track: 0,
                pitch: 60
            }]
        );
    }

    #[test]
    fn repeats_are_recorded_on_their_grid() {
        use crate::Transport;
        let mut e = editor();
        e.releases = true;
        e.handle(Key::Char('R'));
        let at = |e: &mut Editor, step, into| {
            e.set_transport(Some(Transport {
                positions: vec![Some((step, 0)); 2],
                into_step: into,
                count_in: 0,
            }))
        };
        // Every 8th from late in step 2 (so on 3, played on the grid) to early in step 9.
        e.handle(Key::PadOp('.'));
        e.handle(Key::Pad(0));
        at(&mut e, 2, 0.6);
        assert_eq!(
            e.handle(Key::Pad(2)),
            [
                Effect::TrackChanged(0),
                Effect::Repeat {
                    track: 0,
                    pitch: 62,
                    every: 2,
                    now: false
                }
            ]
        );
        at(&mut e, 9, 0.2);
        e.release(Key::Pad(2));
        let hits: Vec<_> = (0..16)
            .filter(|&s| e.grid.tracks[0].steps[s].note.is_some())
            .collect();
        assert_eq!(hits, [3, 4, 6, 8]);
    }

    #[test]
    fn the_scale_layout_follows_the_scale() {
        let mut e = editor();
        command(&mut e, "layout scale");
        command(&mut e, "scale minor A");
        assert_eq!(e.keymap.layout, NoteLayout::Scale);
        e.handle(Key::Char('i'));
        for c in "zxc".chars() {
            e.handle(Key::Char(c));
        }
        let pitches: Vec<_> = e.grid.tracks[0].steps[..3]
            .iter()
            .map(|s| s.note.unwrap().pitch)
            .collect();
        assert_eq!(pitches, [69, 71, 72]);
        // [ and ] still change octave.
        assert!(e.handle(Key::Char('[')).is_empty());
        assert_eq!(e.octave, 3);
    }
}
