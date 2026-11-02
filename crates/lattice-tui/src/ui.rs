//! Drawing the tracker.

use std::time::Instant;

use lattice_core::instrument::chop;
use lattice_core::{Group, InstKind, MAX_GROUPS, MAX_TRACKS, Step, Track, realise};
use lattice_editor::{
    BEATS_PER_BAR, Completion, Cursor, Editor, Infobox, Mode, NoteLayout, PadMode, Page,
    PromptKind, Selection, TutorView, View, word_start,
};
use lattice_engine::{Chain, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use crate::bars;
use crate::key_log::KeyLog;
use crate::theme::Theme;
use crate::wave::{Peaks, Waves};
use crate::{fx_page, help_page, inst_page, mixer_page, song_page};

/// Width of the step-number column.
pub(crate) const GUTTER: usize = 4;
/// Width of a track's cells, not counting the gap after them.
pub(crate) const CELL: usize = 11;

/// How fast a meter falls after a peak.
const METER_FALL_DB_PER_SECOND: f32 = 30.0;

#[derive(Debug, Default)]
pub struct ViewState {
    scroll: usize,
    /// The leftmost track shown, when they don't all fit.
    first_track: usize,
    /// Each track's meter, as a linear level, falling between peaks.
    pub(crate) meters: [f32; MAX_TRACKS],
    /// The send and master buses' meters.
    pub(crate) bus_meters: [f32; 2],
    /// Each group bus's meter, by its index in the grid's groups.
    pub(crate) group_meters: [f32; MAX_GROUPS],
    metered: Option<Instant>,
    /// The leftmost track strip on the mixer page, when they don't all fit.
    pub(crate) first_strip: usize,
    /// How many track strips fit on the mixer page.
    pub(crate) strips_shown: usize,
    /// The first line shown on the fx page, when they don't all fit.
    pub(crate) fx_top: usize,
    /// The instrument page's first line and leftmost track shown, when they don't all fit.
    pub(crate) inst_top: usize,
    pub(crate) inst_left: usize,
    /// Where faces' art takes its time from.
    pub(crate) face_clock: crate::face::Clock,
    /// Each track's count of notes started, as last seen, and when it last changed.
    pub(crate) notes: [Option<(u16, Instant)>; MAX_TRACKS],
    /// The keys just pressed, shown when the editor's `show_keys` is on.
    pub keys: KeyLog,
    /// The waveforms of the files chop tracks play.
    waves: Waves,
}

impl ViewState {
    /// Takes the peaks since the last frame, and lets the meters fall.
    fn update_meters(&mut self, status: &Status) {
        let now = Instant::now();
        let elapsed = self
            .metered
            .map_or(0.0, |then| now.duration_since(then).as_secs_f32());
        self.metered = Some(now);
        let fall = 10f32.powf(-METER_FALL_DB_PER_SECOND * elapsed / 20.0);
        for (track, meter) in self.meters.iter_mut().enumerate() {
            *meter = (*meter * fall).max(status.take_peak(track));
        }
        for (bus, meter) in [Chain::Send, Chain::Master]
            .into_iter()
            .zip(&mut self.bus_meters)
        {
            *meter = (*meter * fall).max(status.take_bus_peak(bus));
        }
        for (g, meter) in self.group_meters.iter_mut().enumerate() {
            *meter = (*meter * fall).max(status.take_bus_peak(Chain::Group(g)));
        }
    }
}

pub fn draw(
    frame: &mut Frame,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let [top, grid, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    // The tutor's lesson sits under the page.
    let lesson = editor
        .tutor_view()
        .map(|view| tutor_lines(&view, grid.width, theme));
    let (grid, panel) = match &lesson {
        Some(lines) => {
            let height = (lines.len() as u16 + 2).min(grid.height / 2);
            let [grid, panel] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(grid);
            (grid, Some(panel))
        }
        None => (grid, None),
    };
    if let (Some(lines), Some(panel)) = (lesson, panel) {
        let title = editor.tutor_view().map_or_else(String::new, |v| {
            format!(
                " TUTOR {}/{} · {} · {} ",
                v.number, v.total, v.section, v.title
            )
        });
        let block = Block::bordered()
            .border_style(Style::new().fg(theme.accent))
            .title(Span::styled(
                title,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ));
        frame.render_widget(Paragraph::new(lines).block(block), panel);
    }
    frame.render_widget(Paragraph::new(header(editor, status, theme)), top);
    view.update_meters(status);
    match editor.page {
        Page::Pattern => draw_grid(frame, grid, editor, status, theme, view),
        Page::Mixer => mixer_page::draw(frame, grid, editor, status, theme, view),
        Page::Fx => fx_page::draw(frame, grid, editor, status, theme, view),
        Page::Instrument => inst_page::draw(frame, grid, editor, status, theme, view),
        Page::Song => song_page::draw(frame, grid, editor, status, theme),
        Page::Help => help_page::draw(frame, grid, editor, theme),
    }
    frame.render_widget(
        Paragraph::new(status_line(editor, theme, bottom.width)),
        bottom,
    );
    if editor.show_keys {
        view.keys.draw(frame, grid, theme);
    }
    if let Some(view) = editor.tutor_view()
        && let Some(next) = view.complete
    {
        draw_lesson_done(frame, grid, &view, next, theme);
    }
    if let Some(infobox) = editor.infobox() {
        draw_infobox(frame, grid, &infobox, theme);
    }
    if let Mode::Command(prompt) = &editor.mode {
        let completions = editor.completions();
        if !completions.is_empty() {
            // Above the word they're for, which starts after the prompt's label.
            let line = &prompt.line;
            let column =
                prompt.kind.label().chars().count() + line[..word_start(line)].chars().count();
            draw_completions(frame, grid, column, &completions, prompt.selected, theme);
        }
    }
}

/// Most completions shown at once.
const COMPLETIONS: usize = 10;

/// Suggestions for the word being typed, above it on the command line (it starts `column` along),
/// with the matched letters picked out.
fn draw_completions(
    frame: &mut Frame,
    area: Rect,
    column: usize,
    completions: &[Completion],
    selected: Option<usize>,
    theme: &Theme,
) {
    let name_width = (completions.iter())
        .map(|c| c.text.chars().count())
        .max()
        .unwrap_or(0);
    let signature_width = (completions.iter())
        .map(|c| c.signature.chars().count())
        .max()
        .unwrap_or(0);
    // Keep the selected one in view while cycling.
    let first = selected.map_or(0, |s| (s + 1).saturating_sub(COMPLETIONS));
    let lines: Vec<Line> = completions
        .iter()
        .enumerate()
        .skip(first)
        .take(COMPLETIONS)
        .map(|(i, c)| {
            let mut spans: Vec<Span> = c
                .text
                .char_indices()
                .map(|(j, ch)| {
                    let style = if c.any {
                        theme.faint
                    } else if c.matched.contains(&j) {
                        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        theme.text
                    };
                    Span::styled(ch.to_string(), style)
                })
                .collect();
            spans.push(Span::raw(
                " ".repeat(name_width - c.text.chars().count() + 2),
            ));
            if signature_width > 0 {
                spans.push(Span::styled(
                    format!("{:<signature_width$}  ", c.signature),
                    theme.faint,
                ));
            }
            spans.push(Span::styled(c.describe.clone(), theme.dim));
            let line = Line::from(spans);
            if selected == Some(i) {
                line.style(Style::new().bg(theme.selection))
            } else {
                line
            }
        })
        .collect();
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    // The suggestions' text lines up with the word, inside the border and padding.
    let x = (area.x + column as u16).saturating_sub(2);
    let corner = Rect {
        x: x.min(area.right().saturating_sub(width)),
        y: area.bottom().saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, corner);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(theme.dim)
                .padding(Padding::horizontal(1)),
        ),
        corner,
    );
}

/// Like Helix's: the keys that can follow a prefix key, in the bottom-right corner above the
/// status line.
fn draw_infobox(frame: &mut Frame, area: Rect, infobox: &Infobox, theme: &Theme) {
    let key_width = infobox
        .items
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = infobox
        .items
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:<key_width$}"),
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {description}"), theme.text),
            ])
        })
        .collect();
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = (content.max(infobox.title.len()) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let corner = Rect {
        x: area.right().saturating_sub(width + 1),
        y: area.bottom().saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, corner);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(format!(" {} ", infobox.title))
                .border_style(theme.dim)
                .padding(Padding::horizontal(1)),
        ),
        corner,
    );
}

/// The pattern cued to follow, or where the song is in the arrangement while it plays.
fn where_in_song(editor: &Editor) -> String {
    let a = &editor.arrangement;
    match (editor.song_position, editor.cue) {
        (Some((entry, repeat)), _) => {
            let repeats = a.entries[entry].repeats;
            let times = if repeats > 1 {
                format!(" {}/{repeats}", repeat + 1)
            } else {
                String::new()
            };
            format!("{times} · song {}/{}", entry + 1, a.entries.len())
        }
        (None, Some(cue)) => format!(" → {}", a.patterns[cue].name),
        (None, None) => String::new(),
    }
}

fn header(editor: &Editor, status: &Status, theme: &Theme) -> Line<'static> {
    let label = |s: &'static str| Span::styled(s, theme.dim);
    let value = |s: String| Span::styled(s, theme.text.add_modifier(Modifier::BOLD));
    let on_off = |on: bool| if on { "on" } else { "off" };
    let counting = status.count_in() as usize;
    let transport = if counting > 0 {
        // The beats still to count, as a band leader counts them.
        let beats = counting.div_ceil(editor.grid.steps_per_beat.max(1) as usize);
        Span::styled(format!(" ◆ IN {beats} "), theme.block(theme.error))
    } else if status.playing() {
        Span::styled(" ▶ PLAY ", theme.block(theme.playing))
    } else {
        Span::styled(" ■ STOP ", theme.block(Color::DarkGray))
    };
    let view = match editor.view {
        View::Source => "as written",
        View::Live => "as playing",
    };
    // Named like Helix's status line: `[scratch]` before the song has a file, `[+]` if modified.
    let file = editor
        .path
        .as_deref()
        .and_then(|p| p.file_name())
        .map_or_else(|| "[scratch]".into(), |n| n.to_string_lossy().into_owned());
    let modified = if editor.modified() { " [+]" } else { "" };
    let page = match editor.page {
        Page::Pattern => " PATTERN ",
        Page::Mixer => " MIXER ",
        Page::Fx => " FX ",
        Page::Instrument => " INST ",
        Page::Song => " SONG ",
        Page::Help => " HELP ",
    };
    Line::from(vec![
        Span::styled(" LATTICE ", theme.block(theme.accent)),
        Span::styled(page, theme.block(theme.visual)),
        // Padded to the longest page's name, so the transport stays put.
        Span::raw(" ".repeat(1 + " PATTERN ".len() - page.len())),
        transport,
        if editor.record {
            Span::styled(" ● REC ", theme.block(theme.error))
        } else {
            Span::raw("")
        },
        match editor.armed() {
            Some(t) if status.recording() => Span::styled(
                format!(" ● LOOP {} ", editor.grid.tracks[t].name.to_uppercase()),
                theme.block(theme.error),
            ),
            Some(t) => Span::styled(
                format!(" ARMED {} ", editor.grid.tracks[t].name.to_uppercase()),
                theme.block(theme.visual),
            ),
            None => Span::raw(""),
        },
        if editor.repeating() {
            Span::styled(
                format!(" REPEAT {} ", editor.repeat_name()),
                theme.block(theme.playing),
            )
        } else {
            Span::raw("")
        },
        if editor.pad_mode == PadMode::Mute {
            Span::styled(" MUTE PADS ", theme.block(theme.visual))
        } else {
            Span::raw("")
        },
        Span::raw("   "),
        value(file),
        Span::styled(modified, theme.text),
        label("   pat "),
        value(editor.pattern_name().into()),
        Span::styled(where_in_song(editor), theme.text),
        label("   bpm "),
        value(bpm(editor.bpm)),
        label("   oct "),
        value(editor.octave.to_string()),
        label("   vel "),
        value(editor.velocity.to_string()),
        label("   step "),
        value(editor.auto_step.to_string()),
        label("   pattern "),
        value(view.into()),
        label("   follow "),
        value(on_off(editor.follow).into()),
        label("   click "),
        value(on_off(editor.click).into()),
    ])
}

/// Width of a track's column in a folded group: a hit's glyph.
const FOLDED: usize = 2;

/// Width of a chop track's column: its piece, and a waveform.
const CHOP: usize = 15;

/// The width of track `index`'s column.
fn column_width(editor: &Editor, index: usize) -> usize {
    let folded = editor
        .grid
        .group_of(index)
        .is_some_and(|g| editor.grid.groups[g].folded);
    match () {
        _ if folded => FOLDED,
        _ if editor.grid.tracks[index].instrument.kind == InstKind::Chop => CHOP,
        _ => CELL,
    }
}

/// How many of these columns fit in `room`, each with its gap; at least one.
fn fitting(widths: &[usize], room: usize) -> usize {
    let mut used = 0;
    let n = widths
        .iter()
        .take_while(|&&w| {
            used += w + 1;
            used <= room
        })
        .count();
    n.max(1)
}

/// What one track column shows this frame.
struct Column<'a> {
    index: usize,
    track: &'a Track,
    /// [`CELL`], or [`FOLDED`] in a folded group.
    width: usize,
    /// Written or realised, depending on the view.
    shown: Vec<Step>,
    /// Rows covered by an earlier note's length.
    tails: Vec<bool>,
    /// Rows an earlier step's locks still hold over: until the next note or lock.
    held: Vec<bool>,
    playhead: Option<usize>,
    pass: Option<u32>,
    colour: Color,
    /// Not muted, and not silenced by another track's solo.
    audible: bool,
    meter: f32,
    /// On a chop track, what each row plays.
    pieces: Option<Vec<Option<Piece>>>,
}

/// What a row of a chop track plays.
struct Piece {
    /// Which piece its note plays, from 0.
    piece: usize,
    /// Whether the row has the note, rather than being further on in it.
    first: bool,
    reverse: bool,
    /// The waveform of the audio it plays.
    wave: String,
}

/// What each row of a chop track plays: a note's piece, carrying on through the pieces after it
/// for as long as the note lasts, drawn `width` cells wide.
fn pieces(
    track: &Track,
    shown: &[Step],
    peaks: Option<&Peaks>,
    width: usize,
) -> Vec<Option<Piece>> {
    let instrument = &track.instrument;
    let (mut start, mut end) = (
        f64::from(instrument.get(chop::START)),
        f64::from(instrument.get(chop::END)),
    );
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    let count = instrument.get(chop::PIECES).round().max(1.0) as usize;
    let size = (end - start) / count as f64;
    let mut rows: Vec<Option<Piece>> = (0..shown.len()).map(|_| None).collect();
    for (i, step) in shown.iter().enumerate() {
        let Some(note) = step.note else {
            continue;
        };
        let piece = chop::piece(note.pitch, count);
        // A step's lock turns its piece round, or back.
        let direction = (step.locks.iter())
            .find(|l| l.fx.is_none() && u32::from(l.param) == chop::DIRECTION)
            .map_or(instrument.get(chop::DIRECTION), |l| l.value);
        let reverse = direction >= 0.5;
        for j in 0..usize::from(note.length).min(shown.len() - i) {
            let (from, to) = if reverse {
                let to = start + size * (piece + 1) as f64 - size * j as f64;
                (to - size, to)
            } else {
                let from = start + size * (piece + j) as f64;
                (from, from + size)
            };
            // Nothing plays outside the region.
            let inside = from >= start - 1e-9 && to <= end + 1e-9;
            let wave = match peaks {
                Some(peaks) if inside => peaks.row(from, to, reverse, width),
                _ => " ".repeat(width),
            };
            rows[i + j] = Some(Piece {
                piece,
                first: j == 0,
                reverse,
                wave,
            });
        }
    }
    rows
}

impl<'a> Column<'a> {
    /// On a chop track, works out what each row plays.
    fn chop(&mut self, waves: &mut Waves) {
        let instrument = &self.track.instrument;
        if instrument.kind != InstKind::Chop || self.width != CHOP {
            return;
        }
        let peaks = instrument.file.as_deref().and_then(|f| waves.get(f));
        // Room for the piece's number and direction, and a lock's mark.
        let width = self.width - 5;
        self.pieces = Some(pieces(self.track, &self.shown, peaks.as_deref(), width));
    }

    /// The track's colour, dimmed when it can't be heard.
    fn style(&self) -> Style {
        let style = Style::new().fg(self.colour);
        if self.audible {
            style
        } else {
            style.add_modifier(Modifier::DIM)
        }
    }

    fn new(
        index: usize,
        track: &'a Track,
        width: usize,
        editor: &Editor,
        status: &Status,
        theme: &Theme,
        meter: f32,
    ) -> Self {
        let position = status.position(index);
        let any_solo = editor.grid.tracks.iter().any(|t| t.mix.solo);
        let shown = match editor.view {
            View::Source => track.steps.clone(),
            View::Live => realise(track, position.map_or(0, |(_, pass)| pass)),
        };
        let mut tails = vec![false; shown.len()];
        for (i, step) in shown.iter().enumerate() {
            if let Some(note) = step.note {
                for tail in tails
                    .iter_mut()
                    .skip(i + 1)
                    .take(usize::from(note.length) - 1)
                {
                    *tail = true;
                }
            }
        }
        Self {
            index,
            track,
            width,
            pieces: None,
            held: held(&shown),
            shown,
            tails,
            playhead: position.map(|(step, _)| step),
            pass: position.map(|(_, pass)| pass),
            colour: theme.track(index),
            audible: !track.mix.mute && !track.pattern_mute && (track.mix.solo || !any_solo),
            meter,
        }
    }
}

fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let tracks = editor.grid.tracks.len();
    let widths: Vec<usize> = (0..tracks).map(|i| column_width(editor, i)).collect();
    // One column spare for the arrow that shows there are more tracks.
    let room = (area.width as usize).saturating_sub(GUTTER + 1);
    scroll_tracks(view, editor.cursor().track, &widths, room);
    let fits = fitting(&widths[view.first_track..], room);
    let columns: Vec<Column> = editor
        .grid
        .tracks
        .iter()
        .enumerate()
        .skip(view.first_track)
        .take(fits)
        .map(|(i, t)| {
            let mut column = Column::new(i, t, widths[i], editor, status, theme, view.meters[i]);
            column.chop(&mut view.waves);
            column
        })
        .collect();
    let hidden = (view.first_track > 0, view.first_track + fits < tracks);
    let mut lines: Vec<_> = group_line(&columns, &editor.grid.groups, theme)
        .into_iter()
        .collect();
    let armed = editor.armed().map(|t| (t, status.recording()));
    lines.extend(header_lines(&columns, hidden, armed, theme));
    let rows = (area.height as usize).saturating_sub(lines.len());
    let longest = columns.iter().map(|c| c.shown.len()).max().unwrap_or(0);
    scroll_to(view, editor, &columns, rows, longest, status.playing());

    let steps_per_beat = editor.grid.steps_per_beat.max(1) as usize;
    for row in (view.scroll..longest).take(rows) {
        lines.push(grid_row(row, steps_per_beat, &columns, editor, theme));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Keeps the cursor's track on screen when there are more tracks than fit in `room`, given each
/// column's width, and fills the screen when there's space to the right.
fn scroll_tracks(view: &mut ViewState, cursor: usize, widths: &[usize], room: usize) {
    let first = &mut view.first_track;
    *first = (*first).min(cursor);
    while *first < cursor && fitting(&widths[*first..], room) <= cursor - *first {
        *first += 1;
    }
    while *first > 0 && fitting(&widths[*first - 1..], room) == widths.len() - (*first - 1) {
        *first -= 1;
    }
}

/// Keeps the cursor on screen, or the playhead when following playback.
fn scroll_to(
    view: &mut ViewState,
    editor: &Editor,
    columns: &[Column],
    rows: usize,
    longest: usize,
    playing: bool,
) {
    let cursor = editor.cursor();
    let followed = columns
        .iter()
        .find(|c| c.index == cursor.track)
        .and_then(|c| c.playhead)
        .filter(|_| editor.follow && playing);
    let target = followed.unwrap_or(cursor.step);
    if target < view.scroll {
        view.scroll = target;
    } else if rows > 0 && target >= view.scroll + rows {
        view.scroll = target + 1 - rows;
    }
    view.scroll = view.scroll.min(longest.saturating_sub(rows));
}

/// A bracket over each group's columns, with its name, if the grid has groups.
fn group_line(columns: &[Column], groups: &[Group], theme: &Theme) -> Option<Line<'static>> {
    if groups.is_empty() {
        return None;
    }
    let mut spans = vec![Span::raw(" ".repeat(GUTTER))];
    let mut k = 0;
    while k < columns.len() {
        let c = &columns[k];
        let Some(group) = groups.iter().find(|g| g.contains(c.index)) else {
            spans.push(Span::raw(" ".repeat(c.width + 1)));
            k += 1;
            continue;
        };
        // The group's columns on screen, drawn as one bracket so its name fits however narrow
        // they are.
        let span: Vec<&Column> = columns[k..]
            .iter()
            .take_while(|c| group.contains(c.index))
            .collect();
        let width: usize = span.iter().map(|c| c.width + 1).sum::<usize>() - 1;
        let opens = c.index == group.first;
        let closes = span
            .last()
            .is_some_and(|c| c.index + 1 == group.first + group.len);
        let start = if opens { "╭─" } else { "──" };
        let end = if closes { "╮" } else { "─" };
        let fold = if group.folded { " ▸" } else { "" };
        let name = format!(" {}{fold} ", group.name.to_uppercase());
        let name = fit(&name, width.saturating_sub(3)).trim_end().to_string() + " ";
        let used = 2 + name.chars().count() + 1;
        spans.push(Span::styled(start, theme.dim));
        spans.push(Span::styled(name, theme.text.add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(
            "─".repeat(width.saturating_sub(used)) + end,
            theme.dim,
        ));
        spans.push(Span::raw(if closes { " " } else { "─" }));
        k += span.len();
    }
    Some(Line::from(spans))
}

/// Track names with mute (`M`, or `m` in this pattern only) and solo flags, lengths and passes, level and pan, then the rules one
/// per line, then a meter underlining it all. Arrows by the names show there are more tracks
/// off to the (left, right).
/// `armed` is the track armed to record the input, and whether it's recording.
fn header_lines(
    columns: &[Column],
    (left, right): (bool, bool),
    armed: Option<(usize, bool)>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let cell = |c: &Column, text: String, style: Style| {
        Span::styled(format!("{} ", fit(&text, c.width)), style)
    };
    let blank = |c: &Column| vec![Span::raw(" ".repeat(c.width + 1))];
    let line = |cells: Vec<Span<'static>>| {
        let mut spans = vec![Span::raw(" ".repeat(GUTTER))];
        spans.extend(cells);
        Line::from(spans)
    };

    let mut names = line(
        columns
            .iter()
            .flat_map(|c| {
                let mut style = Style::new().fg(c.colour).add_modifier(Modifier::BOLD);
                if !c.audible {
                    style = style.add_modifier(Modifier::DIM);
                }
                let mix = c.track.mix;
                let (flag, flag_style) = match (mix.mute, c.track.pattern_mute, mix.solo) {
                    (true, ..) => ("M", Style::new().fg(theme.error)),
                    (_, true, _) => ("m", Style::new().fg(theme.error)),
                    (.., true) => ("S", Style::new().fg(theme.accent)),
                    // Armed to record the input: filled while it records.
                    _ => match armed {
                        Some((t, true)) if t == c.index => ("●", Style::new().fg(theme.error)),
                        Some((t, false)) if t == c.index => ("○", Style::new().fg(theme.error)),
                        _ => (" ", Style::new()),
                    },
                };
                [
                    Span::styled(fit(&c.track.name.to_uppercase(), c.width - 1), style),
                    Span::styled(format!("{flag} "), flag_style.add_modifier(Modifier::BOLD)),
                ]
            })
            .collect(),
    );
    if left {
        names.spans[0] = Span::styled(fit(" ‹", GUTTER), theme.dim);
    }
    if right {
        names.spans.push(Span::styled("›", theme.dim));
    }
    let mut lines = vec![
        names,
        line(
            columns
                .iter()
                .map(|c| {
                    let pass = c.pass.map(|p| format!(" · pass {p}")).unwrap_or_default();
                    cell(c, format!("{}{pass}", c.track.steps.len()), theme.dim)
                })
                .collect(),
        ),
        // Folded, there's no room for the level and pan; the mixer page has them.
        line(
            columns
                .iter()
                .flat_map(|c| match c.width {
                    FOLDED => blank(c),
                    w => spaced(bars::level(c.track.mix.level, w, c.style(), theme)),
                })
                .collect(),
        ),
        line(
            columns
                .iter()
                .flat_map(|c| match c.width {
                    FOLDED => blank(c),
                    w => spaced(bars::pan(c.track.mix.pan, w, c.style(), theme)),
                })
                .collect(),
        ),
    ];
    let rule_rows = columns
        .iter()
        .map(|c| c.track.rules.len())
        .max()
        .unwrap_or(0)
        .max(1);
    for k in 0..rule_rows {
        lines.push(line(
            columns
                .iter()
                .map(|c| match c.track.rules.get(k) {
                    // Folded, a mark for each rule.
                    Some(_) if c.width == FOLDED => cell(c, "r".into(), Style::new().fg(c.colour)),
                    Some(rule) => cell(c, rule.to_string(), Style::new().fg(c.colour)),
                    None if k == 0 && c.width != FOLDED => cell(c, "no rules".into(), theme.faint),
                    None => cell(c, String::new(), Style::new()),
                })
                .collect(),
        ));
    }
    lines.push(line(
        columns
            .iter()
            .flat_map(|c| spaced(bars::meter(c.meter, c.width, c.colour, theme)))
            .collect(),
    ));
    lines
}

/// The rows a lock holds over after its own step, up to the next step with a note or lock.
fn held(steps: &[Step]) -> Vec<bool> {
    let mut holding = false;
    (steps.iter())
        .map(|step| {
            let held = holding && !step.triggers();
            if step.triggers() {
                holding = !step.locks.is_empty();
            }
            held
        })
        .collect()
}

/// The box saying a lesson's done, in the middle of the page, until space goes on.
fn draw_lesson_done(
    frame: &mut Frame,
    area: Rect,
    view: &TutorView,
    next: Option<&str>,
    theme: &Theme,
) {
    let accent = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
    let key = |k: &str| Span::styled(k.to_string(), accent);
    let lines = match next {
        Some(next) => vec![
            Line::from(Span::styled(format!("✓ {}", view.title), accent)),
            Line::default(),
            Line::from(vec![
                Span::styled("Next: ", theme.dim),
                Span::raw(next.to_string()),
            ]),
            Line::from(vec![key("space"), Span::styled(" to go on", theme.dim)]),
        ],
        None => vec![
            Line::from(Span::styled(format!("✓ {}", view.title), accent)),
            Line::default(),
            Line::from("That's every lesson."),
            Line::from(vec![key("space"), Span::styled(" to finish", theme.dim)]),
        ],
    };
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 6).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let block = Block::bordered()
        .border_style(Style::new().fg(theme.accent))
        .padding(Padding::horizontal(2))
        .title(Span::styled(" LESSON DONE ", accent));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// A tutor lesson's paragraphs, wrapped to fit inside its box, with keys and commands (in
/// `backticks`) picked out.
fn tutor_lines(view: &TutorView, width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let room = usize::from(width.saturating_sub(4)).max(20);
    let code = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
    let mut lines = vec![];
    for (i, paragraph) in view.text.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        let mut in_code = false;
        for row in crate::help_page::wrap(paragraph, room) {
            let mut spans = vec![Span::raw(" ")];
            for (k, part) in row.split('`').enumerate() {
                if k > 0 {
                    in_code = !in_code;
                }
                let style = if in_code { code } else { theme.text };
                spans.push(Span::styled(part.to_string(), style));
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// Spans followed by the gap between columns.
fn spaced(spans: impl IntoIterator<Item = Span<'static>>) -> Vec<Span<'static>> {
    spans.into_iter().chain([Span::raw(" ")]).collect()
}

fn grid_row(
    row: usize,
    steps_per_beat: usize,
    columns: &[Column],
    editor: &Editor,
    theme: &Theme,
) -> Line<'static> {
    let on_bar = row.is_multiple_of(steps_per_beat * BEATS_PER_BAR);
    let on_beat = row.is_multiple_of(steps_per_beat);
    let gutter_style = if on_bar {
        theme.text.add_modifier(Modifier::BOLD)
    } else if on_beat {
        theme.text
    } else {
        theme.faint
    };
    // Shade the whole row on the beat, gutter and gaps included, so it reads as one band.
    let band = match theme.beat_row {
        Some(bg) if on_beat => Style::new().bg(bg),
        _ => Style::new(),
    };
    let mut spans = vec![Span::styled(
        format!(" {row:02X} "),
        gutter_style.patch(band),
    )];

    // The selections crossing this row, so each cell only checks those.
    let here: Vec<&Selection> = editor
        .selections()
        .iter()
        .filter(|s| (s.region().first_step..=s.region().last_step).contains(&row))
        .collect();
    let primary = editor.cursor();
    for c in columns {
        let i = c.index;
        let Some(step) = c.shown.get(row) else {
            spans.push(Span::styled(" ".repeat(c.width + 1), band));
            continue;
        };

        let piece = c.pieces.as_ref().map(|rows| match &rows[row] {
            Some(p) => {
                let text = if p.first {
                    let direction = if p.reverse { '◂' } else { ' ' };
                    format!(" {:02X}{direction}{}", p.piece, p.wave)
                } else {
                    format!("    {}", p.wave)
                };
                (text, c.style())
            }
            None => (" ···".into(), theme.faint),
        });
        let (text, mut style) = match step.note {
            _ if piece.is_some() => piece.expect("just checked"),
            // Folded: a hit, by how hard.
            Some(note) if c.width == FOLDED => {
                let glyph = match note.velocity {
                    110.. => "●",
                    80.. => "•",
                    _ => "∙",
                };
                let mut style = Style::new().fg(c.colour);
                if !c.audible {
                    style = style.add_modifier(Modifier::DIM);
                }
                (format!(" {glyph}"), style)
            }
            None if c.width == FOLDED && !c.tails[row] => (" ·".into(), theme.faint),
            Some(note) => {
                let length = if note.length > 1 {
                    note.length.to_string()
                } else {
                    String::new()
                };
                let mut style = Style::new().fg(c.colour);
                if !c.audible {
                    style = style.add_modifier(Modifier::DIM);
                }
                (format!(" {} {length:>4}  ", note_name(note.pitch)), style)
            }
            None if c.tails[row] => (
                " │".into(),
                Style::new().fg(c.colour).add_modifier(Modifier::DIM),
            ),
            None => (" ···".into(), theme.faint),
        };
        // A step that locks settings is marked at its end, and the steps it holds over too.
        let mark = match () {
            _ if c.width == FOLDED => None,
            _ if !step.locks.is_empty() => Some("◆"),
            _ if c.held[row] => Some("╎"),
            _ => None,
        };
        // In the live view, highlight what the rules have changed.
        if editor.view == View::Live && c.track.steps.get(row) != Some(step) {
            style = style.add_modifier(Modifier::BOLD | Modifier::ITALIC);
        }
        style = style.patch(band);

        let at = |cursor: Cursor| cursor.track == i && cursor.step == row;
        let mode_colour = match editor.mode {
            Mode::Insert => theme.accent,
            Mode::Select => theme.visual,
            _ => Color::Reset,
        };
        let selected = here.iter().any(|s| s.region().contains(i, row));
        let plain = style;
        if at(primary) {
            style = theme.block(mode_colour);
        } else if here.iter().any(|s| at(s.cursor)) {
            // The other selections' cursors: marked, but not as a block like the primary's.
            style = style.bg(theme.selection).patch(theme.secondary_cursor);
            if mode_colour != Color::Reset {
                style = style.fg(mode_colour);
            }
        } else if c.playhead == Some(row) {
            style = theme.block(c.colour);
        } else if selected {
            style = style.bg(theme.selection);
        }

        match mark {
            Some(mark) => {
                spans.push(Span::styled(fit(&text, c.width - 1), style));
                // In the track's colour, unless the cell is highlighted.
                let mark_style = if style == plain {
                    c.style().patch(band)
                } else {
                    style
                };
                spans.push(Span::styled(mark, mark_style));
            }
            None => spans.push(Span::styled(fit(&text, c.width), style)),
        }
        spans.push(Span::styled(" ", band));
    }
    Line::from(spans)
}

fn status_line(editor: &Editor, theme: &Theme, width: u16) -> Line<'static> {
    let mut left = match &editor.mode {
        Mode::Normal => vec![Span::styled(" NORMAL ", theme.block(Color::DarkGray))],
        // On the instrument page, the keys play rather than enter notes.
        Mode::Insert if editor.page == Page::Instrument => {
            vec![Span::styled(" PLAY ", theme.block(theme.accent))]
        }
        Mode::Insert => vec![Span::styled(" INSERT ", theme.block(theme.accent))],
        Mode::Select => vec![Span::styled(" SELECT ", theme.block(theme.visual))],
        Mode::Command(prompt) => {
            // The parameters still to type sit faintly after the cursor, to be typed over.
            let hint = editor.command_hint().unwrap_or_default();
            let mut hint = hint.chars();
            let under_cursor = hint.next().unwrap_or(' ');
            // A mistake is underlined in the error colour.
            let range = editor.command_problem().map_or(0..0, |p| p.range);
            let chars: Vec<char> = prompt.line.chars().collect();
            let part = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
            vec![
                Span::styled(
                    format!("{}{}", prompt.kind.label(), part(0, range.start)),
                    theme.text,
                ),
                Span::styled(
                    part(range.start, range.end),
                    theme
                        .text
                        .fg(theme.error)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(part(range.end, chars.len()), theme.text),
                Span::styled(
                    under_cursor.to_string(),
                    theme.faint.add_modifier(Modifier::REVERSED),
                ),
                Span::styled(hint.collect::<String>(), theme.faint),
            ]
        }
    };
    // Like Helix: which selection is primary, and how many there are.
    let selections = editor.selections().len();
    if selections > 1 {
        left.push(Span::styled(
            format!(" {}/{selections} sels", editor.primary() + 1),
            theme.text,
        ));
    }
    if let Some(message) = &editor.message {
        left.push(Span::styled(
            format!(" {message}"),
            Style::new().fg(theme.accent),
        ));
    } else if let Some(count) = editor.count() {
        left.push(Span::styled(format!(" {count}"), theme.text));
    }

    let notes = match editor.keymap.layout {
        NoteLayout::Piano => "z–m q–u notes".to_string(),
        NoteLayout::Scale => format!("{} notes", editor.scale_name()),
        layout => format!("{} notes", layout.name()),
    };
    let (hints, style) = match &editor.mode {
        Mode::Normal
            if matches!(editor.page, Page::Mixer | Page::Fx | Page::Instrument)
                && editor.locking().is_some() =>
        {
            (
            format!(
                "◆ locking {}: change settings · L lock them on the selected steps · esc cancel",
                editor.grid.tracks[editor.locking().unwrap_or(0)].name
            ),
            Style::new().fg(theme.accent),
        )
        }
        Mode::Normal if editor.page == Page::Fx => (
            "hl lanes · jk lines · w b effects · C-a C-x adjust · c set · :fx-add · :fx-send B · :map macro · d remove · L lock · ( ) strip · gp pattern".into(),
            theme.dim,
        ),
        Mode::Normal if editor.page == Page::Instrument => (
            "hl tracks · jk lines · w b parts · C-a C-x adjust · c set · i play · :map macro · L lock · ret face, effects · gp pattern".into(),
            theme.dim,
        ),
        Mode::Normal if editor.page == Page::Mixer => (
            "hl strips · jk detail · ( ) strip · tab chain · C-a C-x adjust · c set · L lock · :fx-add · d remove · gp pattern".into(),
            theme.dim,
        ),
        Mode::Normal if editor.page == Page::Help => (
            "jk scroll · C-d C-u half a page · gg ge top and end · esc back".into(),
            theme.dim,
        ),
        Mode::Normal if editor.page == Page::Song => (
            "hl pattern · + - repeats · o add · d delete · ret edit · space play from here · gp pattern".into(),
            theme.dim,
        ),
        Mode::Normal => {
            let cursor = editor.cursor();
            let locks = editor.describe_locks(cursor.track, cursor.step);
            if locks.is_empty() {
                (
                    "v select · x rows · g goto · ] next · i insert · d delete · : command".into(),
                    theme.dim,
                )
            } else {
                (format!("◆ {}", locks.join(" · ")), theme.text)
            }
        }
        Mode::Insert if editor.page == Page::Instrument && editor.record => (
            format!(
                "recording: {notes} and turned settings at the playhead, while playing · ←→ tracks · ↑↓ lines · C-a C-x adjust · R off · esc stop"
            ),
            theme.dim,
        ),
        Mode::Insert if editor.page == Page::Instrument => (
            format!(
                "{notes} play · [ ] octave · ←→ tracks · ↑↓ lines · C-a C-x adjust · R record · esc stop"
            ),
            theme.dim,
        ),
        Mode::Insert if editor.record && editor.in_folded_group() => (
            format!("recording: 1–9 pads · {notes} at the playhead, while playing · R off · esc done"),
            theme.dim,
        ),
        Mode::Insert if editor.record => (
            format!(
                "recording: {notes} at the playhead, while playing · [ ] octave · R off · esc done"
            ),
            theme.dim,
        ),
        Mode::Insert if editor.in_folded_group() => (
            format!("1–9 pads · {notes} · [ ] octave · ⌫ clear · esc done"),
            theme.dim,
        ),
        Mode::Insert => (
            format!(
                "{notes} · pads {} · [ ] octave · ⌫ clear · esc done",
                editor.scale_name()
            ),
            theme.dim,
        ),
        Mode::Select => (
            "hjkl extend · ; collapse · y copy · d delete · r fill · C-a C-x transpose".into(),
            theme.dim,
        ),
        Mode::Command(prompt) if prompt.kind != PromptKind::Command => {
            ("C C# Eb4 C-4 * · ret select · esc cancel".into(), theme.dim)
        }
        // What's wrong with the line, if anything, in place of what the command does.
        Mode::Command(_) => match editor.command_problem() {
            Some(problem) => (problem.message, Style::new().fg(theme.error)),
            None => (
                editor
                    .command_spec()
                    .map_or("tab complete · ret run · esc cancel".into(), |spec| {
                        spec.description()
                    }),
                theme.dim,
            ),
        },
    };
    let used: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let room = (width as usize).saturating_sub(used + 1);
    let hints: String = hints.chars().take(room).collect();
    left.push(Span::raw(
        " ".repeat(room - hints.chars().count().min(room)),
    ));
    left.push(Span::styled(hints, style));
    Line::from(left)
}

/// The tempo without a fractional part unless it has one: `120`, `122.5`.
fn bpm(bpm: f64) -> String {
    let s = format!("{bpm:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Pads or truncates to exactly `width` characters.
pub(crate) fn fit(text: &str, width: usize) -> String {
    let mut s: String = text.chars().take(width).collect();
    let len = s.chars().count();
    s.extend(std::iter::repeat_n(' ', width - len));
    s
}

/// Tracker-style note names: `C-4`, `F#2`.
fn note_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
    ];
    let octave = i32::from(pitch / 12) - 1;
    let octave = if octave < 0 {
        "-".to_string()
    } else {
        octave.to_string()
    };
    format!("{}{octave}", NAMES[usize::from(pitch % 12)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_hold_until_the_next_note_or_lock() {
        let lock = lattice_core::Lock {
            fx: None,
            param: 0,
            value: 1.0,
        };
        let mut locked = Step::note(60);
        locked.locks.set(lock);
        let mut trigless = Step::default();
        trigless.locks.set(lock);
        let steps = [
            locked,
            Step::default(),
            Step::default(),
            Step::note(62),
            Step::default(),
            trigless,
            Step::default(),
        ];
        assert_eq!(held(&steps), [false, true, true, false, false, false, true]);
    }

    #[test]
    fn note_names() {
        assert_eq!(note_name(60), "C-4");
        assert_eq!(note_name(70), "A#4");
        assert_eq!(note_name(1), "C#-");
        assert_eq!(note_name(127), "G-9");
    }

    #[test]
    fn tracks_scroll_to_keep_the_cursor_in_view() {
        let mut view = ViewState::default();
        let room = 3 * (CELL + 1);
        let widths = [CELL; 8];
        scroll_tracks(&mut view, 5, &widths, room);
        assert_eq!(view.first_track, 3);
        scroll_tracks(&mut view, 4, &widths, room);
        assert_eq!(view.first_track, 3, "still in view");
        scroll_tracks(&mut view, 1, &widths, room);
        assert_eq!(view.first_track, 1);
        scroll_tracks(&mut view, 1, &widths[..2], room);
        assert_eq!(view.first_track, 0, "everything fits again");

        // Folded columns take less room, so more fit.
        let mut widths = [CELL; 8];
        widths[1..5].fill(FOLDED);
        assert_eq!(fitting(&widths, room), 6);
        scroll_tracks(&mut view, 6, &widths, room);
        assert_eq!(view.first_track, 1);
    }

    #[test]
    fn tempo_shows_fractions_only_when_there_are_some() {
        assert_eq!(bpm(120.0), "120");
        assert_eq!(bpm(122.5), "122.5");
        assert_eq!(bpm(100.25), "100.25");
    }

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abcd");
        assert_eq!(fit("—·", 3), "—· ");
    }
}
