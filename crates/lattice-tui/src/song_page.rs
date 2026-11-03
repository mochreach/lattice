//! Drawing the song page: the arrangement as a strip across the top, each entry as wide as it
//! plays for, then the song a bar to a row, time going down. On the left are each bar's time and
//! number, then the entries, each a block of rows in its pattern's colour, then a column for each
//! track showing how busy it is in each part of the bar. Under it all, the patterns there are to
//! choose from.

use lattice_core::Lane;
use lattice_editor::{Editor, Mode};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;
use crate::ui::fit;

/// Width of the entries' column.
const ENTRY: usize = 16;
/// Width of the time and bar number on the left.
const GUTTER: usize = 12;
/// How busy part of a bar is, from a note on every step down to one now and then.
const BUSY: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, status: &Status, theme: &Theme) {
    let arrangement = &editor.arrangement;
    let tracks = &editor.grid.tracks;
    let steps_per_beat = u64::from(editor.grid.steps_per_beat.max(1));
    let pattern_steps = arrangement.all_steps(&editor.grid);
    let entry_bars = editor.entry_bars();
    // Where each bar of the song starts, in steps, and the song's end: patterns in a meter of
    // their own have bars of their own length.
    let mut bar_starts = vec![];
    let mut total = 0;
    for (entry, &bars) in arrangement.entries.iter().zip(&entry_bars) {
        let bar = arrangement.bar(&editor.grid, entry.pattern);
        for _ in 0..bars {
            bar_starts.push(total);
            total += bar;
        }
    }
    let bar_start = |at: usize| bar_starts.get(at).copied().unwrap_or(total);
    let time = |steps: u64| {
        let seconds = steps as f64 / steps_per_beat as f64 * 60.0 / editor.bpm;
        let seconds = seconds.round() as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    let length = |bars: u64| format!("{bars} bar{}", if bars == 1 { "" } else { "s" });
    let playing = editor.song_position;
    let width = area.width as usize;
    // How far into the song the playhead is, while it plays from the arrangement.
    let playhead = playing
        .filter(|_| status.playing() && status.count_in() == 0)
        .and_then(|(entry, repeat)| {
            let steps = *pattern_steps.get(arrangement.entries.get(entry)?.pattern)?;
            let start = bar_start(editor.entry_start(entry));
            let step = status.pattern_step().min(steps.saturating_sub(1));
            Some(start + u64::from(repeat) * steps + step)
        });
    let elapsed = playhead.map_or(String::new(), |at| format!("{} / ", time(at)));
    let loop_bars = editor.song_loop_bars();
    // Looping some bars of an entry, as steps of its pattern.
    let some_bars = |entry: usize| {
        (arrangement.entries.get(entry)).is_some_and(|e| editor.step_loop(e.pattern).is_some())
    };
    let looping = match (editor.song_loop(), &loop_bars) {
        (Some((start, end)), Some(bars)) if start == end && some_bars(start) => {
            match (bars.start() + 1, bars.end() + 1) {
                (a, b) if a == b => format!(" · looping bar {a}"),
                (a, b) => format!(" · looping bars {a} to {b}"),
            }
        }
        (Some((start, end)), _) if start == end => format!(" · looping entry {}", start + 1),
        (Some((start, end)), _) => format!(" · looping entries {} to {}", start + 1, end + 1),
        (None, _) => String::new(),
    };
    let lengths: Vec<u64> = (arrangement.entries.iter().zip(&entry_bars))
        .map(|(e, &b)| b as u64 * arrangement.bar(&editor.grid, e.pattern))
        .collect();

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                " {} entries · {} · {elapsed}{}{looping}",
                arrangement.entries.len(),
                length(bar_starts.len() as u64),
                time(total)
            ),
            theme.dim,
        )),
        strip(editor, theme, &lengths, playhead, width.saturating_sub(2)),
        Line::raw(""),
    ];

    // Each track's column: a character for each beat, or each half beat where there's room,
    // scrolled sideways to keep the cursor's track in view.
    let room = width.saturating_sub(GUTTER + ENTRY);
    // As many beats as the longest bar has, the song's or a pattern's own.
    let beats = (arrangement.patterns.iter())
        .filter_map(|p| p.meter.as_ref().map(|m| m.beats))
        .fold(editor.grid.beats_per_bar, u32::max)
        .max(1) as usize;
    let wide = steps_per_beat.is_multiple_of(2) && room / tracks.len().max(1) > beats * 2;
    let per_beat = if wide { 2 } else { 1 };
    let cells = beats * per_beat;
    let shown = (room / (cells + 2)).clamp(1, tracks.len().max(1));
    let column = editor.song_column();
    let first_track = column
        .saturating_sub(1)
        .saturating_sub(shown - 1)
        .min(tracks.len().saturating_sub(shown));
    let shown_tracks = first_track..(first_track + shown).min(tracks.len());

    let mut header = vec![
        Span::raw(" ".repeat(GUTTER)),
        Span::styled(fit("entries", ENTRY), theme.dim),
    ];
    for (t, track) in tracks.iter().enumerate().skip(first_track).take(shown) {
        // Cut short to leave a space, so narrow columns' names don't run together.
        header.push(Span::styled(
            format!("{} ", fit(&track.name, cells + 1)),
            Style::new().fg(theme.track(t)),
        ));
    }
    lines.push(Line::from(header));

    // The bars, scrolled to keep the cursor in view, leaving room for the patterns below.
    let all_bars: usize = entry_bars.iter().sum();
    let rows = (area.height as usize)
        .saturating_sub(lines.len() + 2)
        .max(1);
    let cursor = editor.song_bar();
    let first = cursor
        .saturating_sub(rows / 2)
        .min(all_bars.saturating_sub(rows));
    if arrangement.entries.is_empty() {
        lines.push(Line::styled(
            "   nothing to play yet: o adds an entry",
            theme.dim,
        ));
    }
    let playing_bar = playhead.map(|at| bar_starts.partition_point(|&s| s <= at).saturating_sub(1));
    // The selection shaded, and in select mode the cursor's cell picked out within it.
    let (selected_bars, selected_columns) = editor.song_selection();
    let selecting = editor.mode == Mode::Select;
    let picked = |style: Style, at: usize, col: usize| {
        let style = match selected_bars.contains(&at) && selected_columns.contains(&col) {
            true => style.bg(theme.selection),
            false => style,
        };
        match selecting && at == cursor && col == column {
            true => style.add_modifier(Modifier::REVERSED),
            false => style,
        }
    };
    let mut all_lanes: Vec<Option<Vec<Lane>>> = vec![None; arrangement.patterns.len()];
    let no_lanes = vec![];
    let mut start = 0;
    for (entry, &bars) in arrangement.entries.iter().zip(&entry_bars) {
        let (from, to) = (start.max(first), (start + bars).min(first + rows));
        let pattern_bars = (bars / entry.repeats.max(1) as usize).max(1);
        let bar = arrangement.bar(&editor.grid, entry.pattern);
        // A cell a beat, or half beat, of its own bars, leaving the rest of the column blank.
        let beats = arrangement.meter(&editor.grid, entry.pattern).beats.max(1) as usize;
        let own_cells = (beats * per_beat).min(cells);
        let blank = " ".repeat(cells - own_cells);
        let colour = theme.track(entry.pattern);
        // An entry of a pattern that isn't there draws empty and unnamed.
        let pattern = arrangement.patterns.get(entry.pattern);
        let name = pattern.map_or("", |p| p.name.as_str());
        let lanes: &Vec<Lane> = match all_lanes.get_mut(entry.pattern) {
            Some(lanes) => lanes.get_or_insert_with(|| {
                (arrangement.lanes(&editor.grid, entry.pattern)).unwrap_or_default()
            }),
            None => &no_lanes,
        };
        for at in from..to {
            let row = at - start;
            let repeat = row / pattern_bars;
            let in_pattern = row % pattern_bars;
            let here = at == cursor;

            // The time and bar number, the playhead marked.
            let is_playing = playing_bar == Some(at);
            let (marker, number_style) = if is_playing {
                ("▶", Style::new().fg(theme.playing))
            } else if here {
                (" ", Style::new().fg(theme.accent))
            } else {
                (" ", theme.dim)
            };
            // The loop as a bar down the bars it covers, an arrow at the top going back to.
            let in_loop = loop_bars.as_ref().filter(|bars| bars.contains(&at));
            let loop_mark = match in_loop {
                Some(bars) if at == *bars.start() => "↻",
                Some(_) => "┃",
                None => " ",
            };
            let mut spans = vec![
                Span::styled(marker, Style::new().fg(theme.playing)),
                Span::styled(format!("{:>5} ", time(bar_start(at))), theme.dim),
                Span::styled(format!("{:>4}", at + 1), number_style),
                Span::styled(loop_mark, Style::new().fg(theme.accent)),
                Span::raw(" "),
            ];

            // The entry, as a bracket down its rows, named on its first.
            let edge = match (row, bars) {
                (_, 1) => '╶',
                (0, _) => '┌',
                (r, b) if r == b - 1 => '└',
                _ if in_pattern == 0 => '├',
                _ => '│',
            };
            // Named on its first row, or the first in view.
            let label = match row {
                _ if at == from && entry.repeats > 1 => format!("{name} ×{}", entry.repeats),
                _ if at == from => name.to_string(),
                _ if in_pattern == 0 => format!("{}/{}", repeat + 1, entry.repeats),
                _ => String::new(),
            };
            let style = picked(Style::new().fg(colour), at, 0);
            spans.push(Span::styled(
                format!("{edge} {}", fit(&label, ENTRY - 3)),
                style,
            ));
            spans.push(Span::raw(" "));

            // How busy each track is in this bar.
            for t in shown_tracks.clone() {
                let busy = match lanes.get(t) {
                    _ if entry.muted(t) => "─".repeat(own_cells),
                    Some(lane) => busy_bar(lane, in_pattern as u64 * bar, bar, own_cells),
                    None => String::new(),
                } + &blank;
                for c in busy.chars() {
                    let style = match c {
                        '·' | '─' => theme.faint,
                        _ => Style::new().fg(theme.track(t)),
                    };
                    spans.push(Span::styled(c.to_string(), picked(style, at, t + 1)));
                }
                spans.push(Span::raw("  "));
            }
            if is_playing {
                for span in spans.iter_mut().skip(1) {
                    span.style = span.style.add_modifier(Modifier::BOLD);
                }
            }
            lines.push(Line::from(spans));
        }
        start += bars;
    }

    // The patterns to choose from, the one being edited underlined.
    let padding = (area.height as usize).saturating_sub(lines.len() + 1);
    lines.extend(std::iter::repeat_n(Line::raw(""), padding.min(1)));
    let mut patterns = vec![Span::styled(" patterns  ", theme.dim)];
    for (p, (pattern, &steps)) in arrangement.patterns.iter().zip(&pattern_steps).enumerate() {
        let mut style = Style::new().fg(theme.track(p));
        if p == arrangement.current {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        patterns.push(Span::styled(pattern.name.clone(), style));
        let bars = steps / arrangement.bar(&editor.grid, p);
        patterns.push(Span::styled(format!(" {}   ", length(bars)), theme.dim));
    }
    lines.push(Line::from(patterns));
    frame.render_widget(Paragraph::new(lines), area);
}

/// How busy `lane` is in the bar starting at step `from`, `bar` steps long, in `cells` characters:
/// a block as tall as the share of steps with a note, a dot where there are none, and a line
/// across if the lane's muted. A lane shorter than the pattern plays round again.
fn busy_bar(lane: &Lane, from: u64, bar: u64, cells: usize) -> String {
    if lane.mute {
        return "─".repeat(cells);
    }
    let len = lane.steps.len() as u64;
    let cells = cells as u64;
    (0..cells)
        .map(|c| {
            if len == 0 {
                return '·';
            }
            // Each cell its share of the bar's steps, which needn't divide evenly.
            let steps = c * bar / cells..(c + 1) * bar / cells;
            let per = steps.end - steps.start;
            let notes = steps
                .filter(|s| {
                    let step = lane.steps.get(((from + s) % len) as usize);
                    step.is_some_and(|step| step.note.is_some())
                })
                .count();
            match notes {
                0 => '·',
                // Clamped, so it's always there.
                n => {
                    let height = (n * BUSY.len()).div_ceil(per as usize).clamp(1, BUSY.len());
                    BUSY.get(height - 1).copied().unwrap_or('█')
                }
            }
        })
        .collect()
}

/// The song in one line, each entry as wide as it plays for, coloured by its pattern and named
/// where there's room, with the playhead marked.
fn strip<'a>(
    editor: &'a Editor,
    theme: &Theme,
    lengths: &[u64],
    playhead: Option<u64>,
    width: usize,
) -> Line<'a> {
    let total: u64 = lengths.iter().sum();
    let mut cells: Vec<(char, Style)> = vec![];
    if total > 0 {
        let mut start = 0;
        for (entry, length) in editor.arrangement.entries.iter().zip(lengths) {
            start += length;
            // Rounded at the ends, so the strip fills the width exactly.
            let end = (start * width as u64).div_ceil(total) as usize;
            let room = end.saturating_sub(cells.len());
            if room == 0 {
                continue;
            }
            // A gap between entries, where there's room for one.
            let block = if room > 1 { room - 1 } else { 1 };
            // An entry of a pattern that isn't there is unnamed.
            let pattern = editor.arrangement.patterns.get(entry.pattern);
            let name = pattern.map_or("", |p| p.name.as_str());
            let style = theme.block(theme.track(entry.pattern));
            let text = format!(" {name}");
            let text = text.chars().chain(std::iter::repeat(' ')).take(block);
            cells.extend(text.map(|c| (c, style)));
            if room > 1 {
                cells.push((' ', Style::new()));
            }
        }
    }
    if let Some(at) = playhead {
        let cell = (at * width as u64 / total.max(1)) as usize;
        if let Some(c) = cells.get_mut(cell) {
            *c = (' ', theme.block(theme.playing));
        }
    }
    let mut spans = vec![Span::raw(" ")];
    spans.extend(
        cells
            .into_iter()
            .map(|(c, style)| Span::styled(c.to_string(), style)),
    );
    Line::from(spans)
}
