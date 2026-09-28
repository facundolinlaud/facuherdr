//! Agents panel grouped by user-defined features.
//!
//! Features come from the server snapshot. The panel lists each feature as a
//! header with its agents indented underneath, then an "ungrouped" block.
//! Dragging rows maps to exactly one `feature.*` request. Collapsing a
//! section is client-local view state.

use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

use super::agent_sidebar::{render_agent_list, render_agent_row, AgentRow};
use super::*;
use crate::api::schema::{AgentStatus, FeatureAssignPaneParams, FeatureMoveParams, Method};
use crate::protocol::ClientShellFeature;

const AGENT_INDENT: u16 = 2;
/// Metadata token an agent of `kind` reports to label its account, e.g.
/// `claude_account` = `facundo (45%) | opus 5.5`. Keying it by agent kind keeps a
/// label left by an earlier agent in the same pane off the current one's row.
fn account_token(kind: &str) -> String {
    format!("{kind}_account")
}

/// What a panel row stands for. Hit-testing and drag and drop use it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FeaturePanelItem {
    Feature {
        feature_id: String,
    },
    Ungrouped,
    Agent {
        pane_id: String,
        feature_id: Option<String>,
    },
}

/// A collapsible block of the panel: one feature, or the ungrouped agents.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum FeatureSection {
    Feature { feature_id: String },
    Ungrouped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SectionFold {
    Expanded,
    /// Hidden agents are summarized on the header: how many, and the most
    /// urgent status among them (none when the section has no agents).
    Collapsed {
        agent_count: usize,
        status: Option<AgentStatus>,
    },
}

pub(super) enum FeaturePanelEntry {
    FeatureHeader {
        feature_id: String,
        name: String,
        fold: SectionFold,
    },
    UngroupedHeader {
        fold: SectionFold,
    },
    Agent {
        feature_id: Option<String>,
        row: AgentRow,
    },
}

impl FeaturePanelEntry {
    fn item(&self) -> FeaturePanelItem {
        match self {
            Self::FeatureHeader { feature_id, .. } => FeaturePanelItem::Feature {
                feature_id: feature_id.clone(),
            },
            Self::UngroupedHeader { .. } => FeaturePanelItem::Ungrouped,
            Self::Agent { feature_id, row } => FeaturePanelItem::Agent {
                pane_id: row.pane_id.clone(),
                feature_id: feature_id.clone(),
            },
        }
    }

    fn line_count(&self) -> usize {
        match self {
            Self::FeatureHeader { .. } | Self::UngroupedHeader { .. } => 1,
            Self::Agent { row, .. } => row.rows.len(),
        }
    }
}

/// A pending drop: the request it would send and where to draw the drop line.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FeaturePanelDrop {
    pub(super) method: Method,
    pub(super) indicator_row: u16,
}

fn is_agent(snapshot: &ClientShellSnapshot, pane_id: &str) -> bool {
    snapshot.agents.iter().any(|agent| agent.pane_id == pane_id)
}

fn grouped_agent_ids<'a>(
    snapshot: &'a ClientShellSnapshot,
    feature: &'a ClientShellFeature,
) -> impl Iterator<Item = &'a String> {
    feature
        .pane_ids
        .iter()
        .filter(|pane_id| is_agent(snapshot, pane_id))
}

fn ungrouped_agent_ids(snapshot: &ClientShellSnapshot) -> Vec<String> {
    snapshot
        .agents
        .iter()
        .map(|agent| &agent.pane_id)
        .filter(|pane_id| {
            !snapshot
                .features
                .iter()
                .any(|feature| feature.pane_ids.contains(pane_id))
        })
        .cloned()
        .collect()
}

/// Agent pane ids in the order the feature panel shows them.
pub(super) fn feature_ordered_agent_pane_ids(snapshot: &ClientShellSnapshot) -> Vec<String> {
    let grouped = snapshot
        .features
        .iter()
        .flat_map(|feature| grouped_agent_ids(snapshot, feature).cloned());
    grouped.chain(ungrouped_agent_ids(snapshot)).collect()
}

fn section_fold(
    snapshot: &ClientShellSnapshot,
    section: &FeatureSection,
    agent_ids: &[&String],
    collapsed: &HashSet<FeatureSection>,
) -> SectionFold {
    if !collapsed.contains(section) {
        return SectionFold::Expanded;
    }
    let status = snapshot
        .agents
        .iter()
        .filter(|agent| agent_ids.contains(&&agent.pane_id))
        .map(|agent| agent.agent_status)
        .max_by_key(|status| status_priority(*status));
    SectionFold::Collapsed {
        agent_count: agent_ids.len(),
        status,
    }
}

/// The row title: the name of the agent's space, so renaming the space renames
/// the row. When several agents share a space, the agent's own name (or its
/// tab) tells them apart.
fn agent_name(snapshot: &ClientShellSnapshot, agent: &crate::protocol::ClientShellAgent) -> String {
    let space = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)
        .map(|workspace| workspace.label.clone())
        .unwrap_or_default();
    let agents_in_space = snapshot
        .agents
        .iter()
        .filter(|candidate| candidate.workspace_id == agent.workspace_id)
        .count();
    if agents_in_space < 2 {
        return space;
    }
    let tab = || {
        snapshot
            .tabs
            .iter()
            .find(|tab| tab.tab_id == agent.tab_id)
            .map(|tab| tab.label.clone())
    };
    match agent.name.clone().or_else(tab) {
        Some(detail) => format!("{space} · {detail}"),
        None => space,
    }
}

/// Status icon and name, with the reported account label on a second line
/// when the agent has one.
fn feature_agent_row(snapshot: &ClientShellSnapshot, pane_id: &str) -> Option<AgentRow> {
    use crate::ui::{ResolvedToken, ResolvedTokenKind};

    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)?;
    let token = |kind| ResolvedToken {
        kind,
        style: Default::default(),
    };
    let name_line = vec![
        token(ResolvedTokenKind::StateIcon),
        token(ResolvedTokenKind::Label(agent_name(snapshot, agent))),
    ];
    let account_token = agent.agent.as_deref().map(account_token);
    let account_line = agent
        .tokens
        .iter()
        .find(|(name, _)| Some(name) == account_token.as_ref())
        .map(|(_, value)| vec![token(ResolvedTokenKind::Custom(value.clone()))]);
    let rows = std::iter::once(name_line).chain(account_line).collect();
    Some(AgentRow {
        pane_id: agent.pane_id.clone(),
        status: agent.agent_status,
        focused: agent.focused,
        rows,
    })
}

pub(super) fn feature_panel_entries(
    snapshot: &ClientShellSnapshot,
    collapsed: &HashSet<FeatureSection>,
) -> Vec<FeaturePanelEntry> {
    let agent_entries = |agent_ids: Vec<&String>, feature_id: Option<&String>, fold| {
        let visible_ids = match fold {
            SectionFold::Expanded => agent_ids,
            SectionFold::Collapsed { .. } => Vec::new(),
        };
        visible_ids
            .into_iter()
            .filter_map(|pane_id| {
                Some(FeaturePanelEntry::Agent {
                    feature_id: feature_id.cloned(),
                    row: feature_agent_row(snapshot, pane_id)?,
                })
            })
            .collect::<Vec<_>>()
    };
    let grouped = snapshot.features.iter().flat_map(|feature| {
        let agent_ids = grouped_agent_ids(snapshot, feature).collect::<Vec<_>>();
        let section = FeatureSection::Feature {
            feature_id: feature.feature_id.clone(),
        };
        let fold = section_fold(snapshot, &section, &agent_ids, collapsed);
        let header = FeaturePanelEntry::FeatureHeader {
            feature_id: feature.feature_id.clone(),
            name: feature.name.clone(),
            fold,
        };
        std::iter::once(header).chain(agent_entries(agent_ids, Some(&feature.feature_id), fold))
    });
    let ungrouped_ids = ungrouped_agent_ids(snapshot);
    let ungrouped_refs = ungrouped_ids.iter().collect::<Vec<_>>();
    let ungrouped_fold = section_fold(
        snapshot,
        &FeatureSection::Ungrouped,
        &ungrouped_refs,
        collapsed,
    );
    let ungrouped_header =
        (!ungrouped_ids.is_empty()).then_some(FeaturePanelEntry::UngroupedHeader {
            fold: ungrouped_fold,
        });
    let ungrouped = agent_entries(ungrouped_refs, None, ungrouped_fold);
    grouped.chain(ungrouped_header).chain(ungrouped).collect()
}

pub(super) fn render_feature_panel_body(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    collapsed: &HashSet<FeatureSection>,
    drop_indicator_row: Option<u16>,
    hits: &mut ShellHitMap,
) {
    let entries = feature_panel_entries(snapshot, collapsed);
    render_agent_list(
        buffer,
        area,
        &entries,
        None,
        config,
        agent_scroll,
        hits,
        FeaturePanelEntry::line_count,
        |buffer, rect, entry, hits| {
            hits.feature_panel_rows.push((rect, entry.item()));
            render_entry(buffer, rect, entry, config, hits);
        },
    );
    if let Some(row) = drop_indicator_row
        .filter(|row| *row >= hits.agent_body.y && *row < hits.agent_body.bottom())
    {
        let line = "─".repeat(hits.agent_body.width as usize);
        buffer.set_stringn(
            hits.agent_body.x,
            row,
            line,
            hits.agent_body.width as usize,
            Style::default().fg(config.palette.accent),
        );
    }
}

fn render_entry(
    buffer: &mut Buffer,
    rect: Rect,
    entry: &FeaturePanelEntry,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    match entry {
        FeaturePanelEntry::FeatureHeader {
            feature_id,
            name,
            fold,
        } => {
            let style = Style::default()
                .fg(config.palette.subtext0)
                .add_modifier(Modifier::BOLD);
            let section = FeatureSection::Feature {
                feature_id: feature_id.clone(),
            };
            render_header(buffer, rect, name, style, *fold, section, config, hits);
        }
        FeaturePanelEntry::UngroupedHeader { fold } => {
            let style = Style::default()
                .fg(config.palette.overlay0)
                .add_modifier(Modifier::DIM);
            let section = FeatureSection::Ungrouped;
            render_header(
                buffer,
                rect,
                "ungrouped",
                style,
                *fold,
                section,
                config,
                hits,
            );
        }
        FeaturePanelEntry::Agent { row, .. } => {
            hits.agents.push((rect, row.pane_id.clone()));
            let indented = Rect {
                x: rect.x + AGENT_INDENT.min(rect.width),
                width: rect.width.saturating_sub(AGENT_INDENT),
                ..rect
            };
            render_agent_row(buffer, indented, row, config);
        }
    }
}

/// Draws ` title`, a `(count) icon` summary when collapsed, and a ▾/▸ toggle
/// on the right edge.
#[allow(clippy::too_many_arguments)] // Plain drawing inputs; bundling them would only rename them.
fn render_header(
    buffer: &mut Buffer,
    rect: Rect,
    title: &str,
    style: Style,
    fold: SectionFold,
    section: FeatureSection,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let text_width = rect.width.saturating_sub(3) as usize;
    let (title_end, _) = buffer.set_stringn(rect.x + 1, rect.y, title, text_width, style);
    if let SectionFold::Collapsed {
        agent_count,
        status,
    } = fold
    {
        let count_style = Style::default().fg(config.palette.overlay0);
        let right = rect.right().saturating_sub(2);
        let (count_end, _) = buffer.set_stringn(
            title_end,
            rect.y,
            format!(" ({agent_count})"),
            right.saturating_sub(title_end) as usize,
            count_style,
        );
        if let Some(status) = status.filter(|_| count_end + 2 <= right) {
            buffer.set_stringn(
                count_end + 1,
                rect.y,
                status_icon(status, config.status_indicators),
                1,
                Style::default().fg(status_color(status, &config.palette)),
            );
        }
    }
    let toggle = Rect::new(rect.right().saturating_sub(1), rect.y, 1, 1);
    let glyph = match fold {
        SectionFold::Expanded => "▾",
        SectionFold::Collapsed { .. } => "▸",
    };
    buffer.set_stringn(
        toggle.x,
        toggle.y,
        glyph,
        1,
        Style::default().fg(config.palette.accent),
    );
    // A theme color every palette defines, so headers stand apart from agent
    // rows. Applied last: writing a wide character such as an emoji resets the
    // background of the cell it spills into.
    buffer.set_style(rect, Style::default().bg(config.palette.surface0));
    hits.feature_section_toggles.push((toggle, section));
}

impl ClientShellState {
    pub(super) fn toggle_feature_section(
        &mut self,
        section: FeatureSection,
        outcome: &mut ClientShellInput,
    ) {
        if self.collapsed_feature_sections.contains(&section) {
            self.collapsed_feature_sections.remove(&section);
        } else {
            self.collapsed_feature_sections.insert(section);
        }
        self.persist_chrome_preferences(outcome);
        outcome.repaint = true;
    }
}

/// Where dropping `dragged` at `pointer_row` would land, given the visible rows.
pub(super) fn feature_panel_drop(
    dragged: &FeaturePanelItem,
    rows: &[(Rect, FeaturePanelItem)],
    pointer_row: u16,
    features: &[ClientShellFeature],
) -> Option<FeaturePanelDrop> {
    let (rect, hovered) = rows
        .iter()
        .find(|(rect, _)| pointer_row >= rect.y && pointer_row < rect.bottom())?;
    let upper_half = pointer_row < rect.y + rect.height.div_ceil(2);
    match dragged {
        FeaturePanelItem::Agent { pane_id, .. } => {
            agent_drop(pane_id, *rect, hovered, upper_half, features)
        }
        FeaturePanelItem::Feature { feature_id } => {
            feature_drop(feature_id, rows, hovered, features)
        }
        FeaturePanelItem::Ungrouped => None,
    }
}

/// Within its own feature an agent takes the hovered agent's place. Anywhere
/// else it lands before or after the hovered row depending on the half hovered.
fn agent_drop(
    pane_id: &str,
    rect: Rect,
    hovered: &FeaturePanelItem,
    upper_half: bool,
    features: &[ClientShellFeature],
) -> Option<FeaturePanelDrop> {
    let before = rect.y.saturating_sub(1);
    let after = rect.bottom();
    let (feature_id, index, indicator_row) = match hovered {
        FeaturePanelItem::Feature { feature_id } => (Some(feature_id.clone()), 0, after),
        FeaturePanelItem::Ungrouped => (None, 0, after),
        FeaturePanelItem::Agent {
            pane_id: hovered_pane_id,
            feature_id,
        } => {
            let hovered_position = feature_id
                .as_deref()
                .and_then(|feature_id| member_position(features, feature_id, hovered_pane_id))
                .unwrap_or(0);
            let current = feature_id
                .as_deref()
                .and_then(|feature_id| member_position(features, feature_id, pane_id));
            match current {
                Some(current) if current < hovered_position => {
                    (feature_id.clone(), hovered_position, after)
                }
                Some(_) => (feature_id.clone(), hovered_position, before),
                None if upper_half => (feature_id.clone(), hovered_position, before),
                None => (feature_id.clone(), hovered_position + 1, after),
            }
        }
    };
    let unchanged = match feature_id.as_deref() {
        Some(feature_id) => member_position(features, feature_id, pane_id) == Some(index),
        None => !features
            .iter()
            .any(|feature| feature.pane_ids.iter().any(|member| member == pane_id)),
    };
    if unchanged {
        return None;
    }
    Some(FeaturePanelDrop {
        method: Method::FeatureAssignPane(FeatureAssignPaneParams {
            pane_id: pane_id.to_string(),
            index: feature_id.as_ref().map(|_| index),
            feature_id,
        }),
        indicator_row,
    })
}

/// Features drop before the hovered feature's header. Hovering the ungrouped
/// block, or agents of the last feature, drops at the end.
fn feature_drop(
    dragged_id: &str,
    rows: &[(Rect, FeaturePanelItem)],
    hovered: &FeaturePanelItem,
    features: &[ClientShellFeature],
) -> Option<FeaturePanelDrop> {
    let current = features
        .iter()
        .position(|feature| feature.feature_id == dragged_id)?;
    let hovered_feature = match hovered {
        FeaturePanelItem::Feature { feature_id } => Some(feature_id.as_str()),
        FeaturePanelItem::Agent { feature_id, .. } => feature_id.as_deref(),
        FeaturePanelItem::Ungrouped => None,
    };
    let hovering_header = matches!(hovered, FeaturePanelItem::Feature { .. });
    let hovered_index =
        hovered_feature.and_then(|id| features.iter().position(|feature| feature.feature_id == id));
    let slot = match hovered_index {
        Some(index) if hovering_header => index,
        Some(index) => index + 1,
        None => features.len(),
    };
    let indicator_row = slot_indicator_row(rows, features.get(slot))?;
    // The server removes the feature before inserting it again.
    let index = if current < slot { slot - 1 } else { slot };
    if index == current {
        return None;
    }
    Some(FeaturePanelDrop {
        method: Method::FeatureMove(FeatureMoveParams {
            feature_id: dragged_id.to_string(),
            index,
        }),
        indicator_row,
    })
}

/// The drop line sits above the header of the feature at the slot, or above
/// the ungrouped block (or below the last row) when the slot is the end.
fn slot_indicator_row(
    rows: &[(Rect, FeaturePanelItem)],
    feature_at_slot: Option<&ClientShellFeature>,
) -> Option<u16> {
    let header = rows.iter().find(|(_, item)| match (item, feature_at_slot) {
        (FeaturePanelItem::Feature { feature_id }, Some(feature)) => {
            *feature_id == feature.feature_id
        }
        (FeaturePanelItem::Ungrouped, None) => true,
        _ => false,
    });
    match header {
        Some((rect, _)) => Some(rect.y.saturating_sub(1)),
        None if feature_at_slot.is_none() => rows.last().map(|(rect, _)| rect.bottom()),
        None => None,
    }
}

fn member_position(
    features: &[ClientShellFeature],
    feature_id: &str,
    pane_id: &str,
) -> Option<usize> {
    features
        .iter()
        .find(|feature| feature.feature_id == feature_id)?
        .pane_ids
        .iter()
        .position(|member| member == pane_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature(id: &str, panes: &[&str]) -> ClientShellFeature {
        ClientShellFeature {
            feature_id: id.into(),
            name: id.into(),
            pane_ids: panes.iter().map(|pane| pane.to_string()).collect(),
        }
    }

    fn header(id: &str) -> FeaturePanelItem {
        FeaturePanelItem::Feature {
            feature_id: id.into(),
        }
    }

    fn agent(pane: &str, feature: Option<&str>) -> FeaturePanelItem {
        FeaturePanelItem::Agent {
            pane_id: pane.into(),
            feature_id: feature.map(str::to_string),
        }
    }

    /// One-line rows stacked from row 10 down.
    fn rows(items: Vec<FeaturePanelItem>) -> Vec<(Rect, FeaturePanelItem)> {
        items
            .into_iter()
            .enumerate()
            .map(|(index, item)| (Rect::new(0, 10 + index as u16, 20, 1), item))
            .collect()
    }

    fn assign(drop: Option<FeaturePanelDrop>) -> Option<(Option<String>, Option<usize>, u16)> {
        let drop = drop?;
        match drop.method {
            Method::FeatureAssignPane(params) => {
                Some((params.feature_id, params.index, drop.indicator_row))
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    fn feature_move(drop: Option<FeaturePanelDrop>) -> Option<(usize, u16)> {
        let drop = drop?;
        match drop.method {
            Method::FeatureMove(params) => Some((params.index, drop.indicator_row)),
            other => panic!("unexpected {other:?}"),
        }
    }

    fn layout() -> (Vec<ClientShellFeature>, Vec<(Rect, FeaturePanelItem)>) {
        let features = vec![feature("a", &["p1", "p2"]), feature("b", &["p3"])];
        let rows = rows(vec![
            header("a"),
            agent("p1", Some("a")),
            agent("p2", Some("a")),
            header("b"),
            agent("p3", Some("b")),
            FeaturePanelItem::Ungrouped,
            agent("p4", None),
        ]);
        (features, rows)
    }

    #[test]
    fn agent_dropped_on_header_goes_first_in_that_feature() {
        let (features, rows) = layout();

        let drop = feature_panel_drop(&agent("p3", Some("b")), &rows, 10, &features);

        assert_eq!(assign(drop), Some((Some("a".into()), Some(0), 11)));
    }

    #[test]
    fn agent_reorders_within_its_feature_accounting_for_removal() {
        let (features, rows) = layout();

        let drop = feature_panel_drop(&agent("p1", Some("a")), &rows, 12, &features);

        assert_eq!(assign(drop), Some((Some("a".into()), Some(1), 13)));
    }

    #[test]
    fn agent_dropped_on_its_own_slot_does_nothing() {
        let (features, rows) = layout();

        assert_eq!(
            feature_panel_drop(&agent("p1", Some("a")), &rows, 11, &features),
            None
        );
    }

    #[test]
    fn agent_dropped_on_ungrouped_leaves_its_feature() {
        let (features, rows) = layout();

        let drop = feature_panel_drop(&agent("p2", Some("a")), &rows, 16, &features);

        assert_eq!(assign(drop), Some((None, None, 15)));
    }

    #[test]
    fn ungrouped_agent_dropped_into_ungrouped_does_nothing() {
        let (features, rows) = layout();

        assert_eq!(
            feature_panel_drop(&agent("p4", None), &rows, 15, &features),
            None
        );
    }

    #[test]
    fn feature_dropped_on_header_moves_before_it() {
        let (features, rows) = layout();

        let drop = feature_panel_drop(&header("b"), &rows, 10, &features);

        assert_eq!(feature_move(drop), Some((0, 9)));
    }

    #[test]
    fn feature_dropped_on_last_features_agents_moves_to_end() {
        let (features, rows) = layout();

        let drop = feature_panel_drop(&header("a"), &rows, 14, &features);

        assert_eq!(feature_move(drop), Some((1, 14)));
    }

    #[test]
    fn feature_dropped_where_it_already_is_does_nothing() {
        let (features, rows) = layout();

        assert_eq!(feature_panel_drop(&header("a"), &rows, 11, &features), None);
    }

    #[test]
    fn pointer_outside_rows_has_no_drop() {
        let (features, rows) = layout();

        assert_eq!(feature_panel_drop(&header("a"), &rows, 40, &features), None);
    }

    fn snapshot_with_agents(pane_ids: &[&str]) -> ClientShellSnapshot {
        let mut snapshot = super::super::tests::snapshot();
        let agent_in = |pane_id: &str| crate::protocol::ClientShellAgent {
            pane_id: pane_id.into(),
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            name: Some(pane_id.into()),
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: crate::api::schema::AgentStatus::Idle,
            state_change_seq: 0,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: false,
        };
        snapshot.agents = pane_ids.iter().map(|pane_id| agent_in(pane_id)).collect();
        snapshot
    }

    #[test]
    fn panel_orders_features_then_ungrouped_agents() {
        let mut snapshot = snapshot_with_agents(&["pane_1", "pane_2"]);
        snapshot.features = vec![
            feature("a", &["pane_2", "not_an_agent"]),
            feature("empty", &[]),
        ];

        let items = feature_panel_entries(&snapshot, &HashSet::new())
            .iter()
            .map(FeaturePanelEntry::item)
            .collect::<Vec<_>>();

        assert_eq!(
            items,
            vec![
                header("a"),
                agent("pane_2", Some("a")),
                header("empty"),
                FeaturePanelItem::Ungrouped,
                agent("pane_1", None),
            ]
        );
        assert_eq!(
            feature_ordered_agent_pane_ids(&snapshot),
            vec!["pane_2".to_string(), "pane_1".to_string()]
        );
    }

    /// Renders a checkout feature holding `pane_2` (working) above ungrouped
    /// `pane_1` (idle), and returns the trimmed text of each body line.
    fn render_lines(
        collapsed: &HashSet<FeatureSection>,
    ) -> (Vec<String>, ShellHitMap, ClientShellConfig) {
        let mut snapshot = snapshot_with_agents(&["pane_1", "pane_2"]);
        snapshot.agents[1].agent_status = AgentStatus::Working;
        snapshot.agents[1].agent = Some("claude".into());
        snapshot.agents[1].tokens = vec![
            ("claude_account".into(), "facundolilao (45%)".into()),
            ("codex_account".into(), "left by an earlier codex".into()),
        ];
        snapshot.features = vec![feature("feature-1", &["pane_2"])];
        snapshot.features[0].name = "checkout".into();
        let config = ClientShellConfig::from_config(&crate::config::Config::default());
        let area = Rect::new(0, 0, 36, 10);
        let mut buffer = Buffer::empty(area);
        let mut hits = ShellHitMap::default();

        render_feature_panel_body(
            &mut buffer,
            area,
            &snapshot,
            &config,
            &mut 0,
            collapsed,
            None,
            &mut hits,
        );

        let lines = (3..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        (lines, hits, config)
    }

    fn padded(left: &str, right: &str) -> String {
        let width = 36 - left.chars().count() - right.chars().count();
        format!("{left}{}{right}", " ".repeat(width))
    }

    #[test]
    fn expanded_headers_have_no_status_icon_and_agents_are_indented() {
        let (lines, hits, config) = render_lines(&HashSet::new());

        let idle = status_icon(AgentStatus::Idle, config.status_indicators);
        let working = status_icon(AgentStatus::Working, config.status_indicators);
        assert_eq!(lines[0], padded(" checkout", "▾"));
        assert_eq!(lines[1], format!("   {working} client-shell · pane_2"));
        assert_eq!(lines[2], "     facundolilao (45%)");
        assert_eq!(lines[3], padded(" ungrouped", "▾"));
        assert_eq!(lines[4], format!("   {idle} client-shell · pane_1"));
        assert_eq!(hits.agents.len(), 2);
        assert_eq!(hits.feature_section_toggles.len(), 2);
    }

    #[test]
    fn collapsed_header_hides_agents_and_summarizes_them() {
        let collapsed = HashSet::from([FeatureSection::Feature {
            feature_id: "feature-1".into(),
        }]);

        let (lines, hits, config) = render_lines(&collapsed);

        let idle = status_icon(AgentStatus::Idle, config.status_indicators);
        let working = status_icon(AgentStatus::Working, config.status_indicators);
        assert_eq!(lines[0], padded(&format!(" checkout (1) {working}"), "▸"));
        assert_eq!(lines[1], padded(" ungrouped", "▾"));
        assert_eq!(lines[2], format!("   {idle} client-shell · pane_1"));
        assert_eq!(hits.agents.len(), 1);
    }

    #[test]
    fn collapsed_status_is_the_most_urgent_member_status() {
        let mut snapshot = snapshot_with_agents(&["pane_1", "pane_2", "pane_3"]);
        snapshot.agents[0].agent_status = AgentStatus::Working;
        snapshot.agents[1].agent_status = AgentStatus::Blocked;
        let ids = snapshot
            .agents
            .iter()
            .map(|agent| &agent.pane_id)
            .collect::<Vec<_>>();
        let collapsed = HashSet::from([FeatureSection::Ungrouped]);

        let fold = section_fold(&snapshot, &FeatureSection::Ungrouped, &ids, &collapsed);

        assert_eq!(
            fold,
            SectionFold::Collapsed {
                agent_count: 3,
                status: Some(AgentStatus::Blocked),
            }
        );
    }

    #[test]
    fn collapsed_empty_feature_has_no_status() {
        let snapshot = snapshot_with_agents(&[]);
        let section = FeatureSection::Feature {
            feature_id: "empty".into(),
        };
        let collapsed = HashSet::from([section.clone()]);

        assert_eq!(
            section_fold(&snapshot, &section, &[], &collapsed),
            SectionFold::Collapsed {
                agent_count: 0,
                status: None,
            }
        );
    }

    #[test]
    fn agent_name_is_the_space_name_unless_agents_share_the_space() {
        let mut snapshot = snapshot_with_agents(&["pane_1"]);
        let alone = snapshot.agents[0].clone();
        assert_eq!(agent_name(&snapshot, &alone), "client-shell");

        let sibling = crate::protocol::ClientShellAgent {
            pane_id: "pane_2".into(),
            name: None,
            ..alone.clone()
        };
        snapshot.agents.push(sibling.clone());
        assert_eq!(agent_name(&snapshot, &alone), "client-shell · pane_1");
        assert_eq!(agent_name(&snapshot, &sibling), "client-shell · 1");
    }

    #[test]
    fn headers_get_the_theme_surface_background_and_keep_emoji_names_aligned() {
        let mut snapshot = snapshot_with_agents(&[]);
        snapshot.features = vec![feature("feature-1", &[])];
        snapshot.features[0].name = "🚀 launch".into();
        let config = ClientShellConfig::from_config(&crate::config::Config::default());
        let area = Rect::new(0, 0, 20, 6);
        let mut buffer = Buffer::empty(area);
        let mut hits = ShellHitMap::default();

        render_feature_panel_body(
            &mut buffer,
            area,
            &snapshot,
            &config,
            &mut 0,
            &HashSet::new(),
            None,
            &mut hits,
        );

        let header_y = 3;
        assert!((0..area.width).all(|x| buffer[(x, header_y)].bg == config.palette.surface0));
        assert_eq!(buffer[(1, header_y)].symbol(), "🚀");
        assert_eq!(buffer[(4, header_y)].symbol(), "l");
        assert_eq!(buffer[(area.width - 1, header_y)].symbol(), "▾");
        assert_ne!(buffer[(0, header_y + 1)].bg, config.palette.surface0);
    }
}
