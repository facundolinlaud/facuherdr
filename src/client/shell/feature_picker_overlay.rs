use super::*;

use super::super::feature_picker::FeaturePickerChoice;

pub(super) fn render_feature_picker_overlay(
    b: &mut Buffer,
    picker: &ClientFeaturePickerOverlay,
    p: &Palette,
) -> Option<OverlayRender> {
    let choices = picker.choices();
    let popup_height = (choices.len() + 7).clamp(10, 22) as u16;
    let popup = popup(b.area, 60, popup_height)?;
    let inner = panel(b, popup, p.accent, p.panel_bg)?;
    put_text(
        b,
        inner.x,
        inner.y,
        inner.width,
        "new agent in feature",
        Style::default()
            .fg(p.text)
            .bg(p.panel_bg)
            .add_modifier(Modifier::BOLD),
    );
    let search = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    put_text(
        b,
        search.x,
        search.y,
        search.width,
        " / ",
        Style::default().fg(p.text).bg(p.panel_bg),
    );
    let cursor = text_editor::render(
        b,
        Rect::new(search.x + 3, search.y, search.width.saturating_sub(4), 1),
        &picker.query,
        Style::default().fg(p.text).bg(p.panel_bg),
    );
    put_text(
        b,
        inner.x,
        inner.y + 2,
        inner.width,
        &"─".repeat(inner.width as usize),
        Style::default().fg(p.surface1).bg(p.panel_bg),
    );
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(5),
    );
    let visible_count = body.height.max(1) as usize;
    let start = picker
        .selected
        .saturating_sub(visible_count.saturating_sub(1));
    let mut row_hits = Vec::new();
    for (visible, (index, choice)) in choices
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_count)
        .enumerate()
    {
        let rect = Rect::new(body.x, body.y + visible as u16, body.width, 1);
        row_hits.push((rect, index));
        let style = if index == picker.selected {
            Style::default().fg(contrast(p)).bg(p.accent)
        } else {
            Style::default().fg(p.text).bg(p.panel_bg)
        };
        b.set_style(rect, style);
        let label = match choice {
            FeaturePickerChoice::Existing { name, .. } => format!(" {name}"),
            FeaturePickerChoice::Create { name } => format!(" + create \"{name}\""),
        };
        put_text(b, rect.x, rect.y, rect.width, &label, style);
    }
    if choices.is_empty() {
        put_text(
            b,
            body.x,
            body.y,
            body.width,
            " type a name to create a feature",
            Style::default().fg(p.overlay0).bg(p.panel_bg),
        );
    }
    let buttons = row(inner, &[11, 12], 2, inner.height.saturating_sub(1));
    let [primary, cancel] = buttons.as_slice() else {
        return None;
    };
    button(
        b,
        *primary,
        " ↵ start ",
        Style::default()
            .fg(contrast(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD),
    );
    button(
        b,
        *cancel,
        " esc cancel ",
        Style::default()
            .fg(p.text)
            .bg(p.surface0)
            .add_modifier(Modifier::BOLD),
    );
    Some(OverlayRender {
        area: popup,
        primary: *primary,
        cancel: *cancel,
        feature_picker_rows: row_hits,
        cursor,
        ..OverlayRender::default()
    })
}
