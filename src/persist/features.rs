//! Persisted feature groups.
//!
//! Raw pane ids change on restore, so members are saved by workspace id and
//! the pane's public number within that workspace, which both survive restore.

use serde::{Deserialize, Serialize};

use crate::app::features::Feature;
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSnapshot {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub panes: Vec<FeaturePaneSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturePaneSnapshot {
    pub workspace_id: String,
    pub pane_number: usize,
}

pub fn capture_features(features: &[Feature], workspaces: &[Workspace]) -> Vec<FeatureSnapshot> {
    features
        .iter()
        .map(|feature| FeatureSnapshot {
            id: feature.id.clone(),
            name: feature.name.clone(),
            panes: feature
                .members
                .iter()
                .filter_map(|&pane_id| {
                    workspaces.iter().find_map(|workspace| {
                        Some(FeaturePaneSnapshot {
                            workspace_id: workspace.id.clone(),
                            pane_number: workspace.public_pane_number(pane_id)?,
                        })
                    })
                })
                .collect(),
        })
        .collect()
}

/// Rebuilds features against restored workspaces. Members whose pane was not
/// restored are dropped; the feature itself is kept.
pub fn restore_features(snapshots: &[FeatureSnapshot], workspaces: &[Workspace]) -> Vec<Feature> {
    snapshots
        .iter()
        .map(|snapshot| Feature {
            id: snapshot.id.clone(),
            name: snapshot.name.clone(),
            members: snapshot
                .panes
                .iter()
                .filter_map(|pane| {
                    let workspace = workspaces
                        .iter()
                        .find(|workspace| workspace.id == pane.workspace_id)?;
                    workspace
                        .public_pane_numbers
                        .iter()
                        .find(|(_, &number)| number == pane.pane_number)
                        .map(|(&pane_id, _)| pane_id)
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_panes(id: &str, panes: &[(u32, usize)]) -> Workspace {
        let mut workspace = Workspace::test_new(id);
        workspace.id = id.to_string();
        workspace.public_pane_numbers = panes
            .iter()
            .map(|&(raw, number)| (crate::layout::PaneId::from_raw(raw), number))
            .collect();
        workspace
    }

    #[test]
    fn features_round_trip_through_renumbered_panes() {
        let before = [workspace_with_panes("w1", &[(10, 1), (11, 2)])];
        let features = [
            Feature {
                id: "feature-1".into(),
                name: "checkout".into(),
                members: vec![
                    crate::layout::PaneId::from_raw(11),
                    crate::layout::PaneId::from_raw(10),
                ],
            },
            Feature {
                id: "feature-2".into(),
                name: "empty".into(),
                members: Vec::new(),
            },
        ];
        let snapshots = capture_features(&features, &before);

        let after = [workspace_with_panes("w1", &[(50, 1), (51, 2)])];
        let restored = restore_features(&snapshots, &after);

        assert_eq!(restored[0].name, "checkout");
        assert_eq!(
            restored[0].members,
            [
                crate::layout::PaneId::from_raw(51),
                crate::layout::PaneId::from_raw(50)
            ]
        );
        assert_eq!(restored[1].id, "feature-2");
        assert!(restored[1].members.is_empty());
    }

    #[test]
    fn members_of_unrestored_workspaces_are_dropped() {
        let snapshots = [FeatureSnapshot {
            id: "feature-1".into(),
            name: "a".into(),
            panes: vec![FeaturePaneSnapshot {
                workspace_id: "gone".into(),
                pane_number: 1,
            }],
        }];

        let restored = restore_features(&snapshots, &[workspace_with_panes("w1", &[(1, 1)])]);

        assert_eq!(restored.len(), 1);
        assert!(restored[0].members.is_empty());
    }
}
