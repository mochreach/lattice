//! Drawing the mixer page: a strip per track, then the send and master buses, and the settings
//! of the instrument or effect under the cursor.

use lattice_core::Kind;
use lattice_editor::{Editor, Row, Shown, Strip};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::bars;
use crate::theme::Theme;
use crate::ui::{CELL, ViewState, fit};

/// Width of the row names down the left.
const LABEL: usize = 10;
/// Widest an effect setting's bar gets.
const PANEL: usize = 4 * CELL + 3;

/// One strip as drawn this frame.
struct Column {
    index: usize,
    strip: Strip,
    name: String,
    colour: Color,
    /// Dimmed when muted or silenced by another track's solo.
    style: Style,
    meter: f32,
}

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, theme: &Theme, view: &mut ViewState) {
    let cursor = editor.mixer_cursor();
    let tracks = editor.grid.tracks.len();
    let any_solo = editor.grid.tracks.iter().any(|t| t.mix.solo);

    // The buses always show, on the right; the tracks scroll if they don't all fit.
    let room = (area.width as usize).saturating_sub(LABEL + 2 * (CELL + 1) + 2);
    let fits = (room / (CELL + 1)).max(1);
    if cursor.strip < tracks {
        if cursor.strip < view.first_strip {
            view.first_strip = cursor.strip;
        } else if cursor.strip >= view.first_strip + fits {
            view.first_strip = cursor.strip + 1 - fits;
        }
    }
    view.first_strip = view.first_strip.min(tracks.saturating_sub(fits));

    let track_columns = (view.first_strip..tracks).take(fits).map(|i| {
        let track = &editor.grid.tracks[i];
        let colour = theme.track(i);
        let audible = !track.mix.mute && (track.mix.solo || !any_solo);
        let style = Style::new().fg(colour);
        Column {
            index: i,
            strip: Strip::Track(i),
            name: track.name.to_uppercase(),
            colour,
            style: if audible {
                style
            } else {
                style.add_modifier(Modifier::DIM)
            },
            meter: view.meters[i],
        }
    });
    let buses = [
        (Strip::Send, "SEND", theme.accent, view.bus_meters[0]),
        (Strip::Master, "MASTER", theme.visual, view.bus_meters[1]),
    ];
    let bus_columns = buses
        .into_iter()
        .enumerate()
        .map(|(i, (strip, name, colour, meter))| Column {
            index: tracks + i,
            strip,
            name: name.into(),
            colour,
            style: Style::new().fg(colour),
            meter,
        });
    let columns: Vec<Column> = track_columns.chain(bus_columns).collect();

    let at = |c: &Column, row: Row| cursor.strip == c.index && cursor.row == row;
    // The cursor's cell is drawn in the terminal's own colour, like the pattern's cursor.
    let style_at = |c: &Column, row: Row| {
        if at(c, row) {
            Style::new().fg(Color::Reset)
        } else {
            c.style
        }
    };
    let label = |text: &str, row: Option<Row>| {
        let here = row.is_some_and(|r| cursor.row == r)
            || matches!((row, cursor.row), (Some(Row::Effect(0)), Row::Effect(_)));
        let style = if here {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        };
        Span::styled(fit(&format!(" {text}"), LABEL), style)
    };
    let hidden = (view.first_strip > 0, view.first_strip + fits < tracks);
    let line = |label: Span<'static>, cells: Vec<Vec<Span<'static>>>| {
        let mut spans = vec![label];
        for (k, (c, cell)) in columns.iter().zip(cells).enumerate() {
            if c.strip == Strip::Send {
                let more = if hidden.1 && k > 0 { "›" } else { " " };
                spans.push(Span::styled(format!("{more}│"), theme.faint));
            }
            spans.extend(cell);
            spans.push(Span::raw(" "));
        }
        Line::from(spans)
    };
    let empty = || vec![Span::raw(" ".repeat(CELL))];

    let names = columns
        .iter()
        .map(|c| {
            let mut style = c.style.add_modifier(Modifier::BOLD);
            if cursor.strip == c.index {
                style = theme.block(c.colour);
            }
            let flag = match c.strip {
                Strip::Track(i) if editor.grid.tracks[i].mix.mute => "M",
                Strip::Track(i) if editor.grid.tracks[i].mix.solo => "S",
                _ => " ",
            };
            let flag_colour = if flag == "M" {
                theme.error
            } else {
                theme.accent
            };
            vec![
                Span::styled(fit(&c.name, CELL - 1), style),
                Span::styled(
                    flag,
                    Style::new().fg(flag_colour).add_modifier(Modifier::BOLD),
                ),
            ]
        })
        .collect();
    let first_label = if hidden.0 { " ‹" } else { "" };
    let mut lines = vec![line(
        Span::styled(fit(first_label, LABEL), theme.dim),
        names,
    )];

    let track_mix = |c: &Column| match c.strip {
        Strip::Track(i) => Some(editor.grid.tracks[i].mix),
        _ => None,
    };
    lines.push(line(
        label("level", Some(Row::Level)),
        columns
            .iter()
            .map(|c| {
                let level = match c.strip {
                    Strip::Track(i) => editor.grid.tracks[i].mix.level,
                    Strip::Send => editor.buses.send.level,
                    Strip::Master => editor.buses.master.level,
                };
                bars::level(level, CELL, style_at(c, Row::Level), theme)
            })
            .collect(),
    ));
    lines.push(line(
        label("pan", Some(Row::Pan)),
        columns
            .iter()
            .map(|c| match track_mix(c) {
                Some(mix) => bars::pan(mix.pan, CELL, style_at(c, Row::Pan), theme),
                None => empty(),
            })
            .collect(),
    ));
    lines.push(line(
        label("send", Some(Row::Send)),
        columns
            .iter()
            .map(|c| match track_mix(c) {
                Some(mix) => bars::level(mix.send, CELL, style_at(c, Row::Send), theme),
                None => empty(),
            })
            .collect(),
    ));
    lines.push(line(
        label("", None),
        columns
            .iter()
            .map(|c| bars::meter(c.meter, CELL, c.colour, theme).to_vec())
            .collect(),
    ));

    // The settings of the instrument or effect under the cursor, or the last one it was on in
    // this strip, straight after what they belong to.
    let panel = |shown: Shown<'_>| {
        let (owner, colour) = columns
            .iter()
            .find(|c| c.strip == shown.strip)
            .map_or((String::new(), theme.accent), |c| {
                (c.name.clone(), c.colour)
            });
        let title = match shown.slot {
            Some(slot) => format!("── {} · {owner} · {} ", shown.name, slot + 1),
            None => format!("── {} · {owner} ", shown.name),
        };
        let rule = (area.width as usize).saturating_sub(title.chars().count() + 1);
        let mut lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::raw(" "),
                Span::styled(title, Style::new().fg(colour).add_modifier(Modifier::BOLD)),
                Span::styled("─".repeat(rule), theme.faint),
            ]),
        ];
        let width = PANEL.min((area.width as usize).saturating_sub(LABEL + 1));
        for (p, info) in shown.params.iter().enumerate() {
            let row = Row::Param(p);
            let style = if cursor.row == row {
                Style::new().fg(Color::Reset)
            } else {
                Style::new().fg(colour)
            };
            let mut spans = vec![label(info.name, Some(row))];
            spans.extend(bars::param(info, shown.values[p], width, style, theme));
            lines.push(Line::from(spans));
        }
        lines
    };
    let shown = editor.shown();

    // What plays each track.
    lines.push(line(
        label("inst", Some(Row::Instrument)),
        columns
            .iter()
            .map(|c| {
                let Some(instrument) = editor.instrument(c.strip) else {
                    return empty();
                };
                let style = if at(c, Row::Instrument) {
                    theme.block(Color::Reset)
                } else {
                    c.style
                };
                vec![Span::styled(fit(instrument.kind.name(), CELL), style)]
            })
            .collect(),
    ));
    if let Some(shown) = shown.filter(|s| s.slot.is_none()) {
        lines.extend(panel(shown));
        lines.push(Line::raw(""));
    }

    // Each strip's chain, one effect per row; an empty chain shows a slot to add to.
    let longest = columns
        .iter()
        .map(|c| editor.chain(c.strip).len())
        .max()
        .unwrap_or(0)
        .max(1);
    for slot in 0..longest {
        let name = if slot == 0 { "effects" } else { "" };
        lines.push(line(
            label(name, (slot == 0).then_some(Row::Effect(0))),
            columns
                .iter()
                .map(|c| {
                    let chain = editor.chain(c.strip);
                    let here = at(c, Row::Effect(slot));
                    let (text, style) = match chain.get(slot) {
                        Some(fx) => (fx.kind.name().to_string(), c.style),
                        None if slot == 0 => ("·".into(), theme.faint),
                        None => (String::new(), Style::new()),
                    };
                    let style = if here {
                        theme.block(Color::Reset)
                    } else {
                        style
                    };
                    vec![Span::styled(fit(&text, CELL), style)]
                })
                .collect(),
        ));
    }

    if let Some(shown) = shown.filter(|s| s.slot.is_some()) {
        lines.extend(panel(shown));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
