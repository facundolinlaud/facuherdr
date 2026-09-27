use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureCreateParams {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureRenameParams {
    pub feature_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureTarget {
    pub feature_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureMoveParams {
    pub feature_id: String,
    /// Final position of the feature, clamped to the feature list.
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureAssignPaneParams {
    pub pane_id: String,
    /// Target feature. Absent removes the pane from its feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<String>,
    /// Final position within the target feature. Absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

/// Files `pane_id` under the feature that `from_pane_id` currently belongs to.
/// Does nothing when `from_pane_id` has no feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureInheritParams {
    pub pane_id: String,
    pub from_pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureStartAgentParams {
    pub workspace_id: String,
    pub feature: FeatureChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeatureChoice {
    Existing { feature_id: String },
    New { name: String },
}
