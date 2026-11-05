//! Knobs: a MIDI controller's knob or fader bound to one setting of a track, in this song. The
//! knob turns the setting through its range wherever the cursor is, so several can be played at
//! once. They're kept with the track, so they go where it goes, and with the song, as what a
//! song's tracks are called means nothing to another; the controls that work in every song,
//! actions and macros, are the frontend's own.

use serde::{Deserialize, Serialize};

use crate::macros::Macro;

/// What marks a setting a knob turns, where a macro's number would.
pub const KNOB_MARK: char = '◎';

/// A knob, by the controller number it sends, and the setting it turns: of the instrument, an
/// effect by its place in the chain, or a macro's own value (`fx` [`MACRO`](crate::MACRO)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knob {
    /// 0 to 127.
    pub cc: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx: Option<u8>,
    pub param: u8,
}

impl Knob {
    pub fn is(&self, fx: Option<u8>, param: u8) -> bool {
        self.fx == fx && self.param == param
    }
}

/// What marks the setting `param` of `fx`: [`KNOB_MARK`] if a knob turns it, or else a macro's
/// number, as [`crate::macros::mark`] gives it. `None` when neither does.
pub fn mark(knobs: &[Knob], macros: &[Macro], fx: Option<u8>, param: u8) -> Option<char> {
    match knobs.iter().any(|k| k.is(fx, param)) {
        true => Some(KNOB_MARK),
        false => crate::macros::mark(macros, fx, param),
    }
}
