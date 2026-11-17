//! How the keyboard's set up to play live, kept with the song: the layout, the scale, each
//! track's octave, the tracks the duet's hands play and the keys recorded into F keys. A song
//! played over live needs them back as they were, so opening it puts them back.

use lattice_core::{MAX_OCTAVE, Playing};

use crate::keymap::{Hand, Key, NoteLayout};
use crate::pads::SCALES;
use crate::{Editor, Page, TrackId};

/// The keyboard octave a track plays at until it's given its own: `z` is middle C.
pub(crate) const DEFAULT_OCTAVE: u8 = 4;

impl Editor {
    /// Track `t`'s keyboard octave.
    pub fn octave_of(&self, t: usize) -> u8 {
        (self.ids.get(t))
            .and_then(|id| self.octaves.get(id))
            .copied()
            .unwrap_or(DEFAULT_OCTAVE)
    }

    /// The keyboard octave of the track the keyboard plays outside the duet's hands.
    pub fn octave(&self) -> u8 {
        self.octave_of(self.keyboard_track())
    }

    /// The octave as the status line shows it: with the duet's hands on tracks of their own,
    /// each hand's, `3 5`, left then right.
    pub fn octave_label(&self) -> String {
        if !self.hands_set() {
            return self.octave().to_string();
        }
        let hand = |hand: Hand| {
            (self.hand_tracks_of(hand).first())
                .map_or_else(|| self.octave(), |&t| self.octave_of(t))
        };
        format!("{} {}", hand(Hand::Left), hand(Hand::Right))
    }

    /// The track the keyboard plays outside the duet's hands: the instrument page's, or the
    /// cursor's.
    pub(crate) fn keyboard_track(&self) -> usize {
        match self.page {
            Page::Instrument => self.inst_cursor().track,
            _ => self.cursor().track,
        }
    }

    /// The octave `key` plays at: its hand's first track's in the duet, or the keyboard's
    /// track's.
    pub(crate) fn key_octave(&self, key: Key) -> u8 {
        match self.hand_tracks(key).first() {
            Some(&t) => self.octave_of(t),
            None => self.octave(),
        }
    }

    /// Whether either hand of the duet has tracks of its own.
    fn hands_set(&self) -> bool {
        self.keymap.layout == NoteLayout::Duet && self.hands.iter().any(|h| !h.is_empty())
    }

    /// The tracks `hand` plays, by index, those still there.
    fn hand_tracks_of(&self, hand: Hand) -> Vec<usize> {
        (self.hands.get(hand as usize).into_iter().flatten())
            .filter_map(|id| self.ids.iter().position(|i| i == id))
            .collect()
    }

    /// The tracks the keyboard plays: both hands' in the duet, if they have their own, so the
    /// keyboard moves as one, or the keyboard's track.
    fn keyboard_tracks(&self) -> Vec<usize> {
        if !self.hands_set() {
            return vec![self.keyboard_track()];
        }
        let mut tracks = self.hand_tracks_of(Hand::Left);
        tracks.extend(self.hand_tracks_of(Hand::Right));
        // A hand with none of its own plays the keyboard's track.
        if self.hands.iter().any(|h| h.is_empty()) {
            tracks.push(self.keyboard_track());
        }
        tracks.sort_unstable();
        tracks.dedup();
        tracks
    }

    /// Sets `tracks`' octaves to what `to` makes of each.
    fn change_octaves(&mut self, tracks: &[usize], to: impl Fn(u8) -> u8) {
        for &t in tracks {
            let octave = to(self.octave_of(t)).min(MAX_OCTAVE);
            let Some(&id) = self.ids.get(t) else {
                continue;
            };
            if octave == DEFAULT_OCTAVE {
                self.octaves.remove(&id);
            } else {
                self.octaves.insert(id, octave);
            }
        }
    }

    /// `{` and `}`: the keyboard's tracks an octave down or up, each from its own.
    pub(crate) fn shift_octave(&mut self, by: i8) {
        let tracks = self.keyboard_tracks();
        self.change_octaves(&tracks, |o| o.saturating_add_signed(by));
        self.message = Some(format!("octave {}", self.octave_label()));
    }

    /// `:oct OCTAVE [left|right|TRACKS]`: the keyboard's tracks' octave, or one hand's, or
    /// the tracks named, numbers or names between commas.
    pub(crate) fn set_octave(&mut self, octave: u8, which: &str) -> Result<(), String> {
        if octave > MAX_OCTAVE {
            return Err(format!("octaves go from 0 to {MAX_OCTAVE}, not {octave}"));
        }
        let tracks = match which.trim() {
            "" => self.keyboard_tracks(),
            hand @ ("left" | "right") => {
                let tracks = self.hand_tracks_of(match hand {
                    "left" => Hand::Left,
                    _ => Hand::Right,
                });
                if tracks.is_empty() {
                    return Err(format!(
                        "the {hand} hand has no tracks: :hand {hand} gives it some"
                    ));
                }
                tracks
            }
            named => (named.split(','))
                .map(|word| self.track_called(word.trim()))
                .collect::<Result<_, _>>()?,
        };
        self.change_octaves(&tracks, |_| octave);
        let names: Vec<_> = (tracks.iter())
            .filter_map(|&t| self.grid.tracks.get(t))
            .map(|t| t.name.as_str())
            .collect();
        self.message = Some(format!("{} at octave {octave}", names.join(", ")));
        Ok(())
    }

    /// The keyboard's setup, to save with the song.
    pub(crate) fn playing(&self) -> Playing {
        let tracks = |ids: &[TrackId]| {
            (ids.iter())
                .filter_map(|id| self.ids.iter().position(|i| i == id))
                .collect()
        };
        let [left, right] = &self.hands;
        Playing {
            layout: self.keymap.layout.name().into(),
            scale: self.scale().name.into(),
            root: self.root,
            octaves: (self.ids.iter().enumerate())
                .filter_map(|(t, id)| Some((t, *self.octaves.get(id)?)))
                .collect(),
            left: tracks(left),
            right: tracks(right),
            keys: (self.fkeys.iter())
                .map(|(&n, keys)| (n, Key::write_sequence(keys)))
                .collect(),
        }
    }

    /// Sets the keyboard up as `playing` has it, for a song just opened, its tracks given their
    /// ids. What isn't a layout, scale or key Lattice knows is left as it starts, and said why.
    pub(crate) fn set_playing(&mut self, playing: &Playing) -> Option<String> {
        let mut problems = vec![];
        self.scale = match SCALES.iter().position(|s| s.name == playing.scale) {
            Some(scale) => scale,
            None => {
                problems.push(format!("there's no scale {}", playing.scale));
                0
            }
        };
        self.root = playing.root;
        self.octaves = (playing.octaves.iter())
            .filter(|&(_, &octave)| octave != DEFAULT_OCTAVE)
            .filter_map(|(&t, &octave)| Some((*self.ids.get(t)?, octave)))
            .collect();
        let layout = match NoteLayout::ALL
            .into_iter()
            .find(|l| l.name() == playing.layout)
        {
            Some(layout) => layout,
            None => {
                problems.push(format!("there's no layout {}", playing.layout));
                NoteLayout::default()
            }
        };
        self.lay_out_keys(layout);
        let ids = |tracks: &[usize]| {
            (tracks.iter())
                .filter_map(|&t| self.ids.get(t).copied())
                .collect()
        };
        self.hands = [ids(&playing.left), ids(&playing.right)];
        self.voices.clear();
        self.fkeys.clear();
        for (&n, keys) in &playing.keys {
            match Key::sequence(keys) {
                Ok(keys) => {
                    self.fkeys.insert(n, keys);
                }
                Err(e) => problems.push(format!("F{n}'s keys: {e}")),
            }
        }
        (!problems.is_empty()).then(|| problems.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Song, Step, Track};

    /// Presses keys as the tutor writes them.
    fn press(e: &mut Editor, keys: &str) {
        for key in Key::sequence(keys).unwrap() {
            e.handle(key);
        }
    }

    fn song() -> Song {
        let track = |name: &str| Track {
            name: name.into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        Song {
            name: "live".into(),
            bpm: 120.0,
            grid: Grid {
                name: "live".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: ["lead", "pad", "bass"].map(track).to_vec(),
                groups: vec![],
            },
            arrangement: Default::default(),
            buses: Default::default(),
            click: true,
            playing: Default::default(),
        }
    }

    #[test]
    fn the_keyboard_s_setup_is_saved_and_opened_with_the_song() {
        let mut e = Editor::new(song().grid, 120.0);
        e.load(song(), "live.lattice".into());
        e.run_command("scale dorian D");
        e.run_command("oct 3");
        e.run_command("hand right lead");
        e.run_command("hand left pad,bass");
        press(&mut e, "Q<F1>:set voices mono 1<ret>QQ<F2><lt>xQ");
        assert!(e.modified(), "the setup is part of the song");
        let saved = e.song();
        assert_eq!(
            saved.playing,
            Playing {
                layout: "duet".into(),
                scale: "dorian".into(),
                root: 2,
                octaves: [(0, 3)].into(),
                left: vec![1, 2],
                right: vec![0],
                keys: [(1, ":set voices mono 1<ret>".into()), (2, "<lt>x".into())].into(),
            }
        );
        let text = lattice_core::to_ron(&saved).unwrap();
        let opened = lattice_core::from_ron(&text).unwrap();

        let mut e = Editor::new(song().grid, 120.0);
        e.load(opened, "live.lattice".into());
        assert_eq!(e.song().playing, saved.playing);
        assert_eq!(e.keymap.layout, NoteLayout::Duet);
        assert_eq!(e.scale_name(), "D dorian");
        assert!(!e.modified());
        assert_eq!(e.fkeys.get(&2), Some(&vec![Key::Char('<'), Key::Char('x')]));
        // F1 sets track 1's voices from anywhere.
        e.run_command("inst switched");
        press(&mut e, "tl<F1>");
        assert_eq!(e.message.as_deref(), Some("lead voices mono"));
    }

    #[test]
    fn a_song_s_setup_lattice_doesn_t_know_is_said_and_left_as_it_starts() {
        let mut song = song();
        song.playing.layout = "harp".into();
        song.playing.keys.insert(3, "<nokey>".into());
        let mut e = Editor::new(song.grid.clone(), 120.0);
        e.load(song, "live.lattice".into());
        assert_eq!(e.keymap.layout, NoteLayout::default());
        assert!(e.fkeys.is_empty());
        let message = e.message.clone().unwrap_or_default();
        assert!(message.contains("no layout harp"), "{message}");
        assert!(
            message.contains("F3's keys: no key called <nokey>"),
            "{message}"
        );
    }

    #[test]
    fn every_key_written_reads_back_as_itself() {
        let all = [
            Key::Char('a'),
            Key::Char(' '),
            Key::Char('<'),
            Key::Char('>'),
            Key::Char('`'),
            Key::Ctrl('a'),
            Key::Alt(';'),
            Key::Alt(' '),
            Key::Esc,
            Key::Enter,
            Key::Backspace,
            Key::Delete,
            Key::Tab,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::PageUp,
            Key::PageDown,
            Key::Home,
            Key::End,
            Key::Pad(3),
            Key::PadOp('+'),
            Key::Note(60),
            Key::F(12),
            Key::RightAlt,
        ];
        let written = Key::write_sequence(&all);
        assert_eq!(Key::sequence(&written), Ok(all.to_vec()), "{written}");
    }
}
