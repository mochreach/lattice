//! Drawing the fx page: the chain of the strip under the mixer cursor, its lanes side by side as
//! columns read top to bottom, like the pattern's tracks, each ending in its out; and below, the
//! settings of the effect under the cursor, as on the mixer.

use lattice_core::{FxLane, FxRow, Kind, MIN_DB, is_feedback};
use lattice_editor::{Editor, Strip};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::bars;
use crate::mixer_page::{flag, route, settings};
use crate::theme::Theme;
use crate::ui::{CELL, fit};

/// Width of the row numbers down the left.
const LABEL: usize = 6;
/// Width of a lane.
const LANE: usize = 18;

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, status: &Status, theme: &Theme) {
    let width = area.width as usize;
    let strip = editor.fx_strip();
    let (name, colour) = look(editor, theme, strip);
    let chain = editor.chain(strip);
    let lanes = chain.lanes();
    let cursor = editor.fx_cursor();
    let on_lanes = !editor.in_fx_settings();
    let here = |lane: usize, row: usize, out: bool| {
        on_lanes && cursor.lane == lane && cursor.out == out && (out || cursor.row == row)
    };
    let cell = |text: String, style: Style, here: bool| {
        let style = if here {
            theme.block(Color::Reset)
        } else {
            style
        };
        vec![Span::styled(fit(&text, LANE - 1), style), Span::raw(" ")]
    };

    // A rule with the strip's name and where its sound goes, as on the mixer.
    let (flag, flag_colour) = flag(editor, strip, theme);
    let title = format!("── {name} ");
    let route = format!(" effects {} ", route(editor, strip));
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
    lines.push(Line::raw(""));

    // Each lane's letter, and what feeds it.
    let mut spans = vec![Span::raw(" ".repeat(LABEL))];
    for k in 0..lanes.len() {
        let mut from: Vec<String> = (k == 0).then(|| "in".to_string()).into_iter().collect();
        for (l, lane) in lanes.iter().enumerate() {
            for row in &lane.rows {
                if let FxRow::Send { to, .. } = row
                    && *to == k
                {
                    let letter = FxLane::letter(l).to_string();
                    if !from.contains(&letter) {
                        from.push(letter);
                    }
                }
            }
        }
        let style = Style::new().fg(colour).add_modifier(Modifier::BOLD);
        spans.push(Span::styled(FxLane::letter(k).to_string(), style));
        spans.push(Span::styled(
            fit(&format!(" ← {}", from.join(" ")), LANE - 1),
            theme.dim,
        ));
    }
    lines.push(Line::from(spans));

    // The rows, lane beside lane, numbered like the pattern's steps.
    let effects: Vec<Option<usize>> = chain.rows().map(|(_, _, effect)| effect).collect();
    let shown = editor.shown().filter(|s| s.slot.is_some());
    let rows = lanes.iter().map(|l| l.rows.len()).max().unwrap_or(0);
    let mut first = 0;
    let starts: Vec<usize> = lanes
        .iter()
        .map(|l| {
            first += l.rows.len();
            first - l.rows.len()
        })
        .collect();
    for r in 0..rows.max(1) {
        let mut spans = vec![Span::styled(fit(&format!(" {r:>2}"), LABEL), theme.faint)];
        for (k, lane) in lanes.iter().enumerate() {
            let here = here(k, r, false);
            spans.extend(match lane.rows.get(r) {
                Some(FxRow::Fx) => {
                    let effect = effects[starts[k] + r].unwrap_or(0);
                    let mut style = Style::new().fg(colour);
                    if !on_lanes && shown.is_some_and(|s| s.slot == Some(effect)) {
                        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    cell(chain[effect].kind.name().into(), style, here)
                }
                Some(&FxRow::Send { to, level }) => {
                    let (arrow, style) = if is_feedback(k, to) {
                        ("↺", Style::new().fg(theme.visual))
                    } else {
                        ("→", Style::new().fg(theme.accent))
                    };
                    let level = if level <= MIN_DB {
                        "off".into()
                    } else {
                        format!("{level:+.1} dB")
                    };
                    cell(
                        format!("{arrow} {} {level}", FxLane::letter(to)),
                        style,
                        here,
                    )
                }
                // The lane flows on down to its out.
                None if here => cell("·".into(), theme.faint, true),
                None => cell(" │".into(), theme.faint, false),
            });
        }
        lines.push(Line::from(spans));
    }

    // Each lane's level into the chain's output.
    let mut spans = vec![Span::styled(fit(" out", LABEL), theme.dim)];
    for (k, lane) in lanes.iter().enumerate() {
        let style = if here(k, 0, true) {
            Style::new().fg(Color::Reset)
        } else {
            Style::new().fg(colour)
        };
        spans.extend(bars::level(lane.out, CELL, style, theme));
        spans.push(Span::raw(" ".repeat(LANE.saturating_sub(CELL))));
    }
    lines.push(Line::from(spans));
    if chain.is_empty() && lanes.len() == 1 {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            " no effects yet: :fx-add adds one, and :fx-send B branches into a new lane",
            theme.faint,
        ));
    }

    if let Some(shown) = shown {
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!(" {}", shown.name), theme.dim));
        let row = editor.mixer_cursor().row;
        lines.extend(settings(&shown, row, status, theme, colour, width));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// A strip's name and colour, as the mixer draws it.
fn look(editor: &Editor, theme: &Theme, strip: Strip) -> (String, Color) {
    match strip {
        Strip::Track(i) => (editor.grid.tracks[i].name.to_uppercase(), theme.track(i)),
        Strip::Group(g) => {
            let group = &editor.grid.groups[g];
            (group.name.to_uppercase(), theme.track(group.first))
        }
        Strip::Send => ("SEND".into(), theme.accent),
        Strip::Master => ("MASTER".into(), theme.visual),
    }
}
