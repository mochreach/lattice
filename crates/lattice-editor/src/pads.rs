//! The numpad as a performance surface. Pads `1`–`9` play the drums of the cursor's group or,
//! on any other track, the notes of a scale (`:scale`) up from the keyboard octave. In mute mode
//! (`+`) they mute and unmute the first nine tracks instead. `/` `*` `-` choose how hard notes
//! are played and entered, soft, medium or hard (`:vel` sets any velocity).

use lattice_core::{DRUM_ROOT, Note};

use crate::{Editor, Effect, Key, Mode};

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

pub const SCALES: [Scale; 8] = [
    Scale {
        name: "major",
        steps: &[0, 2, 4, 5, 7, 9, 11],
    },
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
    let natural = [0, 2, 4, 5, 7, 9, 11]
        [usize::from(b"CDEFGAB".iter().position(|&c| c == letter as u8)? as u8)];
    let semitone = match chars.as_str() {
        "" => natural + 12,
        "#" => natural + 13,
        "b" => natural + 11,
        _ => return None,
    };
    Some(semitone % 12)
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
            return vec![];
        }
        let n = usize::from(n);
        if self.pad_mode == PadMode::Mute {
            return self.mute_pad(n - 1);
        }
        let (track, pitch) = match self.grid.group_of(self.cursor().track) {
            // In a group, its `n`th track at C-4, which plays a drum at its tuning.
            Some(g) => {
                let group = &self.grid.groups[g];
                if n > group.len {
                    return vec![];
                }
                (group.first + n - 1, DRUM_ROOT)
            }
            // Elsewhere, the scale's `n`th note on the cursor's track.
            None => match self.scale_pitch(n - 1) {
                Some(pitch) => (self.cursor().track, pitch),
                None => return vec![],
            },
        };
        if let Some(effects) = self.record_note(key, &[track], pitch) {
            return effects;
        }
        // In insert mode it's entered on the cursor's step, without moving on, so several drums
        // can go on one step.
        let mut effects = vec![];
        if self.mode == Mode::Insert {
            let step = self.cursor().step;
            if step < self.track_len(track) {
                self.checkpoint();
                self.grid.tracks[track].steps[step].note = Some(Note {
                    pitch,
                    velocity: self.velocity,
                    length: 1,
                });
                effects.push(Effect::TrackChanged(track));
            }
        }
        effects.push(Effect::Preview { track, pitch });
        effects
    }

    /// The scale's `degree`th note (from 0), up from its root in the keyboard octave.
    fn scale_pitch(&self, degree: usize) -> Option<u8> {
        let steps = SCALES[self.scale].steps;
        let octave = u16::from(self.octave) + 1 + (degree / steps.len()) as u16;
        let pitch = octave * 12 + u16::from(self.root) + u16::from(steps[degree % steps.len()]);
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn mute_pad(&mut self, track: usize) -> Vec<Effect> {
        let Some(t) = self.grid.tracks.get(track) else {
            return vec![];
        };
        let mute = !t.mix.mute;
        self.checkpoint();
        self.grid.tracks[track].mix.mute = mute;
        vec![Effect::TrackChanged(track)]
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
        self.message = Some(format!(
            "pads play {} {name}",
            NOTE_NAMES[usize::from(root)]
        ));
        Ok(())
    }

    /// The scale the pads play, as `C major`.
    pub fn scale_name(&self) -> String {
        format!(
            "{} {}",
            NOTE_NAMES[usize::from(self.root)],
            SCALES[self.scale].name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let track = |len| Track {
            name: "t".into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: vec![],
            choke: 0,
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
}
