//! Modal editor state for Lattice, independent of any frontend.
//!
//! Planned shape (see `docs/DESIGN.md`): the frontend converts its key events into this
//! crate's own `Key` type, a `Keymap` maps `(Mode, [Key])` to an `Action`, and
//! `Editor::apply(Action)` updates the document and cursor, returning effects (send MIDI,
//! transport, save, quit) for the frontend to carry out. Edits are invertible commands, which
//! gives undo and `.` repeat.
