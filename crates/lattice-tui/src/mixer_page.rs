//! Drawing the mixer page: the overview of every strip above (a strip per track, then the groups'
//! buses and the send and master buses), and the detail of the strip under the cursor below.

use lattice_core::{Kind, Knob, Locks, MIN_DB, Macro, knob};
use lattice_editor::{Editor, Field, PARAM_COLUMNS, Row, Shown, Strip};
use lattice_engine::Status;
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
/// Narrowest a strip gets, still room for `-11dB`, and the track strips it tries to fit.
const NARROW: usize = 7;
const FEW: usize = 4;
/// Width of a setting's name in the grid of settings.
const PARAM_LABEL: usize = 12;
/// Widest a setting's bar gets.
const PARAM_BAR: usize = 24;

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

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let cursor = editor.mixer_cursor();
    let columns = columns(editor, theme, view, area, cursor.strip);
    let mut lines = overview(editor, theme, view, &columns);
    lines.push(Line::raw(""));
    if let Some(column) = columns.iter().find(|c| c.index == cursor.strip) {
        lines.extend(detail(editor, status, theme, column, area.width as usize));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The strips that fit, scrolling the tracks to keep the cursor's in view. The buses always
/// show, on the right.
fn columns(
    editor: &Editor,
    theme: &Theme,
    view: &mut ViewState,
    area: Rect,
    at: usize,
) -> Vec<Column> {
    let tracks = editor.grid.tracks.len();
    // As heard: the conductor's mutes and solos over the tracks' own.
    let any_solo =
        (0..editor.grid.tracks.len()).any(|t| editor.heard_mix(t).is_some_and(|m| m.solo));
    let groups = &editor.grid.groups;
    let buses = groups.len() + 2;
    // Strips narrow, down to `NARROW`, until a few tracks fit beside the buses.
    let fitting = |cell: usize| {
        let room = (area.width as usize).saturating_sub(LABEL + buses * (cell + 1) + 2);
        (room / (cell + 1)).max(1)
    };
    let cell = (NARROW..=CELL)
        .rev()
        .find(|&cell| fitting(cell) >= tracks.min(FEW))
        .unwrap_or(NARROW);
    let fits = fitting(cell);
    view.strip_width = cell;
    if at < tracks {
        if at < view.first_strip {
            view.first_strip = at;
        } else if at >= view.first_strip + fits {
            view.first_strip = at + 1 - fits;
        }
    }
    view.first_strip = view.first_strip.min(tracks.saturating_sub(fits));
    view.strips_shown = fits;

    let shown = (editor.grid.tracks.iter().enumerate())
        .skip(view.first_strip)
        .take(fits);
    let track_columns = shown.map(|(i, track)| {
        let colour = theme.track(i);
        let mix = editor.heard_mix(i).unwrap_or(track.mix);
        let audible =
            !mix.mute && !track.pattern_mute && !editor.entry_mutes(i) && (mix.solo || !any_solo);
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
            // There's a meter for every track the engine can have.
            meter: view.meters.get(i).copied().unwrap_or(0.0),
        }
    });
    // A group's bus takes the colour of its first track.
    let group_buses = groups.iter().enumerate().map(|(g, group)| {
        (
            Strip::Group(g),
            group.name.to_uppercase(),
            theme.track(group.first),
            // And one for every group it can have.
            view.group_meters.get(g).copied().unwrap_or(0.0),
        )
    });
    let buses = group_buses.chain([
        (Strip::Send, "SEND".into(), theme.accent, view.bus_meters[0]),
        (
            Strip::Master,
            "MASTER".into(),
            theme.visual,
            view.bus_meters[1],
        ),
    ]);
    let bus_columns = buses
        .enumerate()
        .map(|(i, (strip, name, colour, meter))| Column {
            index: tracks + i,
            strip,
            name,
            colour,
            style: Style::new().fg(colour),
            meter,
        });
    track_columns.chain(bus_columns).collect()
}

/// Every strip's name, level, pan and meter, a column each.
fn overview(
    editor: &Editor,
    theme: &Theme,
    view: &ViewState,
    columns: &[Column],
) -> Vec<Line<'static>> {
    let cursor = editor.mixer_cursor();
    let on_level = cursor.row == Row::Strips;
    let tracks = editor.grid.tracks.len();
    let hidden = (
        view.first_strip > 0,
        view.first_strip + view.strips_shown < tracks,
    );
    let line = |label: &str, cells: Vec<Vec<Span<'static>>>| {
        let style = if on_level && label == "level" {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        };
        let mut spans = vec![Span::styled(fit(&format!(" {label}"), LABEL), style)];
        for (k, (c, cell)) in columns.iter().zip(cells).enumerate() {
            let after_track = (k.checked_sub(1).and_then(|k| columns.get(k)))
                .is_some_and(|before| !before.strip.is_bus());
            if c.strip.is_bus() && after_track {
                let more = if hidden.1 && k > 0 { "›" } else { " " };
                spans.push(Span::styled(format!("{more}│"), theme.faint));
            }
            spans.extend(cell);
            spans.push(Span::raw(" "));
        }
        Line::from(spans)
    };

    let cell = view.strip_width.max(NARROW);
    let names = columns
        .iter()
        .map(|c| {
            let mut style = c.style.add_modifier(Modifier::BOLD);
            if cursor.strip == c.index {
                style = theme.block(c.colour);
            }
            let (flag, flag_colour) = flag(editor, c.strip, theme);
            vec![
                Span::styled(fit(&c.name, cell - 1), style),
                Span::styled(
                    flag,
                    Style::new().fg(flag_colour).add_modifier(Modifier::BOLD),
                ),
            ]
        })
        .collect();
    let mut lines = vec![line(if hidden.0 { "‹" } else { "" }, names)];
    lines.push(line(
        "level",
        columns
            .iter()
            .map(|c| {
                // The cursor's cell is drawn in the terminal's own colour, like the pattern's.
                let style = if on_level && cursor.strip == c.index {
                    Style::new().fg(Color::Reset)
                } else {
                    c.style
                };
                bars::level(editor.strip_level(c.strip), cell, style, theme)
            })
            .collect(),
    ));
    lines.push(line(
        "pan",
        columns
            .iter()
            .map(|c| match c.strip {
                Strip::Track(i) if let Some(track) = editor.grid.tracks.get(i) => {
                    bars::pan(track.mix.pan, cell, c.style, theme)
                }
                _ => vec![Span::raw(" ".repeat(cell))],
            })
            .collect(),
    ));
    lines.push(line(
        "",
        columns
            .iter()
            .map(|c| bars::meter(c.meter, cell, c.colour, theme).to_vec())
            .collect(),
    ));
    lines
}

/// `M` when muted, `m` when muted in this pattern, `S` when soloed.
pub fn flag(editor: &Editor, strip: Strip, theme: &Theme) -> (&'static str, Color) {
    let track = match strip {
        Strip::Track(i) => editor.grid.tracks.get(i).zip(editor.heard_mix(i)),
        _ => None,
    };
    match track {
        Some((_, mix)) if mix.mute => ("M", theme.error),
        Some((t, _)) if t.pattern_mute => ("m", theme.error),
        Some(_) if matches!(strip, Strip::Track(i) if editor.entry_mutes(i)) => ("─", theme.error),
        Some((_, mix)) if mix.solo => ("S", theme.accent),
        _ => (" ", theme.accent),
    }
}

/// Where a strip's sound goes, and for a bus, where it comes from.
pub fn route(editor: &Editor, strip: Strip) -> String {
    let names = |tracks: &mut dyn Iterator<Item = &lattice_core::Track>| {
        tracks
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    match strip {
        Strip::Track(i) => {
            let group = editor
                .grid
                .group_of(i)
                .and_then(|g| editor.grid.groups.get(g));
            let mut route = match group {
                Some(group) => format!("→ {} → master", group.name),
                None => "→ master".into(),
            };
            if editor
                .grid
                .tracks
                .get(i)
                .is_some_and(|t| t.mix.send > MIN_DB)
            {
                route.push_str(", and → send");
            }
            route
        }
        // A group that's gone goes nowhere.
        Strip::Group(g) => match editor.grid.groups.get(g) {
            Some(group) => {
                let mut tracks = editor.grid.tracks.iter().skip(group.first).take(group.len);
                format!("{} → master", names(&mut tracks))
            }
            None => String::new(),
        },
        Strip::Send => {
            let mut sent = editor.grid.tracks.iter().filter(|t| t.mix.send > MIN_DB);
            match names(&mut sent) {
                none if none.is_empty() => "nothing sent yet → master".into(),
                some => format!("{some} → master"),
            }
        }
        Strip::Master => "→ out".into(),
    }
}

/// Everything about the cursor's strip: where it goes, its mix, its chain, and the settings of
/// the device on the chain under the cursor.
fn detail(
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    column: &Column,
    width: usize,
) -> Vec<Line<'static>> {
    let cursor = editor.mixer_cursor();
    let strip = column.strip;
    let colour = column.colour;
    let label_width = |text: &str, here: bool, width: usize| {
        let style = if here {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        };
        Span::styled(fit(&format!(" {text}"), width), style)
    };
    let label = |text: &str, here: bool| label_width(text, here, LABEL);
    // The value under the cursor in the terminal's own colour, as in the overview.
    let value_style = |here: bool| {
        if here {
            Style::new().fg(Color::Reset)
        } else {
            Style::new().fg(colour)
        }
    };

    // A rule with the strip's name and where its sound goes.
    let (flag, flag_colour) = flag(editor, strip, theme);
    let title = format!("── {} ", column.name);
    let route = format!(" {} ", route(editor, strip));
    let used = 1 + title.chars().count() + 2 + route.chars().count();
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(title, Style::new().fg(colour).add_modifier(Modifier::BOLD)),
        Span::styled(
            flag,
            Style::new().fg(flag_colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─", theme.faint),
        Span::styled(route, theme.dim),
        Span::styled("─".repeat(width.saturating_sub(used)), theme.faint),
    ])];

    // The mix, field after field.
    let mut spans = vec![label("mix", matches!(cursor.row, Row::Mix(_)))];
    for &field in strip.fields() {
        let here = cursor.row == Row::Mix(field);
        let name = match field {
            Field::Level => "level",
            Field::Pan => "pan",
            Field::Send => "send",
            Field::Choke => "choke",
        };
        let name_style = if here {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        };
        spans.push(Span::styled(format!("{name} "), name_style));
        let style = value_style(here);
        let track = match strip {
            Strip::Track(i) => editor.grid.tracks.get(i),
            _ => None,
        };
        match (field, track) {
            (Field::Level, _) => {
                spans.extend(bars::level(editor.strip_level(strip), CELL, style, theme));
            }
            (Field::Pan, Some(t)) => spans.extend(bars::pan(t.mix.pan, CELL, style, theme)),
            (Field::Send, Some(t)) => spans.extend(bars::level(t.mix.send, CELL, style, theme)),
            (Field::Choke, Some(t)) => {
                let text = match t.choke {
                    0 => " – ".to_string(),
                    n => format!(" {n} "),
                };
                let style = if here {
                    theme.block(Color::Reset)
                } else {
                    style
                };
                spans.push(Span::styled(text, style));
            }
            _ => {}
        }
        spans.push(Span::raw("   "));
    }
    lines.push(Line::from(spans));

    // The chain, in the order the sound goes through it.
    let on_chain = cursor.row == Row::Chain;
    let mut spans = vec![label("chain", on_chain)];
    // A sampler with the file it plays.
    let instrument = editor.instrument(strip).map(|i| {
        let file = i.file.as_deref().and_then(|f| f.file_name());
        let name = match file {
            Some(file) => format!("{} {}", i.kind.name(), file.to_string_lossy()),
            None => i.kind.name().to_string(),
        };
        (None, name)
    });
    let effects = (editor.chain(strip).iter().enumerate())
        .map(|(i, fx)| (Some(i), fx.kind.name().to_string()));
    let devices: Vec<(Option<usize>, String)> = instrument.into_iter().chain(effects).collect();
    if devices.is_empty() {
        spans.push(Span::styled("no effects: :fx-add adds one", theme.faint));
    }
    let shown = editor.shown();
    for (k, (slot, name)) in devices.iter().enumerate() {
        let slot = *slot;
        spans.push(Span::styled(if k > 0 { " → " } else { " " }, theme.faint));
        let is_shown = shown.is_some_and(|s| s.slot == slot);
        let style = if is_shown && on_chain {
            theme.block(Color::Reset)
        } else if is_shown {
            Style::new()
                .fg(colour)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(colour)
        };
        spans.push(Span::styled(name.clone(), style));
    }
    lines.push(Line::from(spans));

    // The settings of the device under the cursor, in a grid filled a row at a time.
    let Some(shown) = shown else {
        return lines;
    };
    lines.push(Line::raw(""));
    let track = match shown.strip {
        Strip::Track(t) => editor.grid.tracks.get(t),
        _ => None,
    };
    let macros = track.map_or(&[][..], |t| t.macros.as_slice());
    let knobs = track.map_or(&[][..], |t| t.knobs.as_slice());
    lines.extend(settings(
        &shown, macros, knobs, cursor.row, status, theme, colour, width,
    ));
    lines
}

/// The settings of an instrument or effect, in a grid filled a row at a time, with the one at
/// `row` under the cursor. While it plays, a setting a step has locked shows the lock's value;
/// one a knob turns is marked, or one a macro turns, with its number.
#[expect(
    clippy::too_many_arguments,
    reason = "each is a different part of what's drawn"
)]
pub fn settings(
    shown: &Shown,
    macros: &[Macro],
    knobs: &[Knob],
    row: Row,
    status: &Status,
    theme: &Theme,
    colour: Color,
    width: usize,
) -> Vec<Line<'static>> {
    let label = |text: &str, here: bool, mark: Option<char>| {
        let style = if here {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        };
        // A space after the mark, so it doesn't read as part of the name.
        let mark = Span::styled(
            format!("{} ", mark.unwrap_or(' ')),
            Style::new().fg(theme.visual),
        );
        [mark, Span::styled(fit(text, PARAM_LABEL - 2), style)]
    };
    let value_style = |here: bool| {
        if here {
            Style::new().fg(Color::Reset)
        } else {
            Style::new().fg(colour)
        }
    };
    let locks = match shown.strip {
        Strip::Track(t) => status.locks(t),
        _ => Locks::default(),
    };
    let slot = shown.slot.map(|s| s as u8);
    let cell = (width.saturating_sub(1) / PARAM_COLUMNS).max(PARAM_LABEL + 8);
    let bar = cell.saturating_sub(PARAM_LABEL + 3).min(PARAM_BAR);
    let mut lines = vec![];
    // A setting comes with its value, so neither is ever missing one.
    let rows = shown
        .params
        .chunks(PARAM_COLUMNS)
        .zip(shown.values.chunks(PARAM_COLUMNS));
    for (r, (params, values)) in rows.enumerate() {
        let mut spans = vec![];
        for (k, (info, &value)) in params.iter().zip(values).enumerate() {
            let p = r * PARAM_COLUMNS + k;
            let here = row == Row::Param(p);
            let lock = (locks.iter()).find(|l| l.fx == slot && u32::from(l.param) == info.id);
            match lock {
                Some(lock) => {
                    let mark = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
                    spans.push(Span::styled(
                        fit(&format!("◆{}", info.name), PARAM_LABEL),
                        mark,
                    ));
                    spans.extend(bars::param(info, lock.value, bar, mark, theme));
                }
                None => {
                    let mark = knob::mark(knobs, macros, slot, info.id as u8);
                    spans.extend(label(info.name, here, mark));
                    spans.extend(bars::param(info, value, bar, value_style(here), theme));
                }
            }
            spans.push(Span::raw(" ".repeat(cell - PARAM_LABEL - bar)));
        }
        lines.push(Line::from(spans));
    }
    lines
}
