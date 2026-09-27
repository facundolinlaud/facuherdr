//! User-defined feature groups that organize agent panes.
//!
//! A feature is an ordered, named list of panes. A pane belongs to at most one
//! feature. Features are shared session state: every client renders the same
//! grouping and it is persisted with the session.

use crate::layout::PaneId;

use super::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub members: Vec<PaneId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureError {
    EmptyName,
    NotFound,
}

impl FeatureError {
    pub fn message(self) -> &'static str {
        match self {
            Self::EmptyName => "feature name must not be empty",
            Self::NotFound => "feature not found",
        }
    }
}

fn next_feature_id(features: &[Feature]) -> String {
    let highest = features
        .iter()
        .filter_map(|feature| feature.id.strip_prefix("feature-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("feature-{}", highest + 1)
}

fn feature_name(name: &str) -> Result<String, FeatureError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(FeatureError::EmptyName);
    }
    Ok(name.to_string())
}

impl AppState {
    pub(crate) fn feature_index(&self, feature_id: &str) -> Result<usize, FeatureError> {
        self.features
            .iter()
            .position(|feature| feature.id == feature_id)
            .ok_or(FeatureError::NotFound)
    }

    pub(crate) fn create_feature(&mut self, name: &str) -> Result<String, FeatureError> {
        let name = feature_name(name)?;
        let id = next_feature_id(&self.features);
        self.features.push(Feature {
            id: id.clone(),
            name,
            members: Vec::new(),
        });
        self.mark_session_dirty();
        Ok(id)
    }

    pub(crate) fn rename_feature(
        &mut self,
        feature_id: &str,
        name: &str,
    ) -> Result<(), FeatureError> {
        let name = feature_name(name)?;
        let index = self.feature_index(feature_id)?;
        self.features[index].name = name;
        self.mark_session_dirty();
        Ok(())
    }

    /// Removes the feature. Its panes become ungrouped; they are not closed.
    pub(crate) fn delete_feature(&mut self, feature_id: &str) -> Result<(), FeatureError> {
        let index = self.feature_index(feature_id)?;
        self.features.remove(index);
        self.mark_session_dirty();
        Ok(())
    }

    /// Moves the feature so it ends up at `index`, clamped to the list.
    pub(crate) fn move_feature(
        &mut self,
        feature_id: &str,
        index: usize,
    ) -> Result<(), FeatureError> {
        let from = self.feature_index(feature_id)?;
        let feature = self.features.remove(from);
        let to = index.min(self.features.len());
        self.features.insert(to, feature);
        self.mark_session_dirty();
        Ok(())
    }

    /// Puts the pane into `feature_id` at `index` (end when absent), or ungroups
    /// it when `feature_id` is `None`. The caller validates that the pane exists.
    pub(crate) fn assign_pane_to_feature(
        &mut self,
        pane_id: PaneId,
        feature_id: Option<&str>,
        index: Option<usize>,
    ) -> Result<(), FeatureError> {
        let target = feature_id
            .map(|feature_id| self.feature_index(feature_id))
            .transpose()?;
        self.remove_panes_from_features(&[pane_id]);
        if let Some(target) = target {
            let members = &mut self.features[target].members;
            let to = index.unwrap_or(members.len()).min(members.len());
            members.insert(to, pane_id);
        }
        self.mark_session_dirty();
        Ok(())
    }

    /// Files `pane_id` under the feature `from_pane_id` belongs to, at the end.
    /// Leaves `pane_id` untouched when `from_pane_id` has no feature.
    pub(crate) fn inherit_feature(&mut self, pane_id: PaneId, from_pane_id: PaneId) {
        let Some(feature_id) = self
            .features
            .iter()
            .find(|feature| feature.members.contains(&from_pane_id))
            .map(|feature| feature.id.clone())
        else {
            return;
        };
        // The feature id was just read from the list, so assignment cannot fail.
        let _ = self.assign_pane_to_feature(pane_id, Some(&feature_id), None);
    }

    pub(crate) fn remove_panes_from_features(&mut self, pane_ids: &[PaneId]) {
        for feature in &mut self.features {
            feature.members.retain(|member| !pane_ids.contains(member));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_features(names: &[&str]) -> (AppState, Vec<String>) {
        let mut state = AppState::test_new();
        let ids = names
            .iter()
            .map(|name| state.create_feature(name).unwrap())
            .collect();
        (state, ids)
    }

    fn feature_for_pane(state: &AppState, pane_id: PaneId) -> Option<&Feature> {
        state
            .features
            .iter()
            .find(|feature| feature.members.contains(&pane_id))
    }

    fn names(state: &AppState) -> Vec<&str> {
        state
            .features
            .iter()
            .map(|feature| feature.name.as_str())
            .collect()
    }

    fn members(state: &AppState, feature_id: &str) -> Vec<u32> {
        let index = state.feature_index(feature_id).unwrap();
        state.features[index]
            .members
            .iter()
            .map(|pane_id| pane_id.raw())
            .collect()
    }

    #[test]
    fn create_appends_trimmed_names_with_unique_ids() {
        let (state, ids) = state_with_features(&[" checkout ", "billing"]);

        assert_eq!(names(&state), ["checkout", "billing"]);
        assert_eq!(ids, ["feature-1", "feature-2"]);
        assert!(state.session_dirty);
    }

    #[test]
    fn create_rejects_blank_names() {
        let mut state = AppState::test_new();

        assert_eq!(state.create_feature("   "), Err(FeatureError::EmptyName));
        assert!(state.features.is_empty());
    }

    #[test]
    fn new_ids_never_reuse_the_highest_existing_id() {
        let (mut state, ids) = state_with_features(&["a", "b"]);
        state.delete_feature(&ids[0]).unwrap();

        assert_eq!(state.create_feature("c").unwrap(), "feature-3");
    }

    #[test]
    fn rename_replaces_the_name() {
        let (mut state, ids) = state_with_features(&["a"]);

        state.rename_feature(&ids[0], "renamed").unwrap();

        assert_eq!(names(&state), ["renamed"]);
        assert_eq!(
            state.rename_feature("missing", "x"),
            Err(FeatureError::NotFound)
        );
        assert_eq!(
            state.rename_feature(&ids[0], " "),
            Err(FeatureError::EmptyName)
        );
    }

    #[test]
    fn delete_ungroups_members() {
        let (mut state, ids) = state_with_features(&["a"]);
        let pane = PaneId::from_raw(7);
        state
            .assign_pane_to_feature(pane, Some(&ids[0]), None)
            .unwrap();

        state.delete_feature(&ids[0]).unwrap();

        assert!(state.features.is_empty());
        assert!(feature_for_pane(&state, pane).is_none());
    }

    #[test]
    fn move_places_feature_at_clamped_index() {
        let (mut state, ids) = state_with_features(&["a", "b", "c"]);

        state.move_feature(&ids[0], 2).unwrap();
        assert_eq!(names(&state), ["b", "c", "a"]);

        state.move_feature(&ids[0], 0).unwrap();
        assert_eq!(names(&state), ["a", "b", "c"]);

        state.move_feature(&ids[1], 99).unwrap();
        assert_eq!(names(&state), ["a", "c", "b"]);
    }

    #[test]
    fn assign_orders_within_and_moves_across_features() {
        let (mut state, ids) = state_with_features(&["a", "b"]);
        let [one, two, three] = [1, 2, 3].map(PaneId::from_raw);
        state
            .assign_pane_to_feature(one, Some(&ids[0]), None)
            .unwrap();
        state
            .assign_pane_to_feature(two, Some(&ids[0]), None)
            .unwrap();
        state
            .assign_pane_to_feature(three, Some(&ids[0]), Some(0))
            .unwrap();
        assert_eq!(members(&state, &ids[0]), [3, 1, 2]);

        state
            .assign_pane_to_feature(two, Some(&ids[0]), Some(0))
            .unwrap();
        assert_eq!(members(&state, &ids[0]), [2, 3, 1]);

        state
            .assign_pane_to_feature(three, Some(&ids[1]), Some(42))
            .unwrap();
        assert_eq!(members(&state, &ids[0]), [2, 1]);
        assert_eq!(members(&state, &ids[1]), [3]);

        state.assign_pane_to_feature(one, None, None).unwrap();
        assert_eq!(members(&state, &ids[0]), [2]);
        assert!(feature_for_pane(&state, one).is_none());
    }

    #[test]
    fn assign_to_unknown_feature_keeps_current_membership() {
        let (mut state, ids) = state_with_features(&["a"]);
        let pane = PaneId::from_raw(1);
        state
            .assign_pane_to_feature(pane, Some(&ids[0]), None)
            .unwrap();

        assert_eq!(
            state.assign_pane_to_feature(pane, Some("missing"), None),
            Err(FeatureError::NotFound)
        );
        assert_eq!(members(&state, &ids[0]), [1]);
    }

    #[test]
    fn inherit_files_pane_under_callers_feature() {
        let (mut state, ids) = state_with_features(&["a", "b"]);
        let [caller, sibling, child] = [1, 2, 3].map(PaneId::from_raw);
        state
            .assign_pane_to_feature(sibling, Some(&ids[1]), None)
            .unwrap();
        state
            .assign_pane_to_feature(caller, Some(&ids[1]), Some(0))
            .unwrap();

        state.inherit_feature(child, caller);

        assert_eq!(members(&state, &ids[1]), [1, 2, 3]);
        assert!(feature_for_pane(&state, child).is_some_and(|feature| feature.id == ids[1]));
    }

    #[test]
    fn inherit_from_ungrouped_caller_changes_nothing() {
        let (mut state, ids) = state_with_features(&["a"]);
        let [caller, child] = [1, 2].map(PaneId::from_raw);
        state
            .assign_pane_to_feature(child, Some(&ids[0]), None)
            .unwrap();

        state.inherit_feature(child, caller);

        assert_eq!(members(&state, &ids[0]), [2]);
    }

    #[test]
    fn removing_panes_keeps_features() {
        let (mut state, ids) = state_with_features(&["a"]);
        let [one, two] = [1, 2].map(PaneId::from_raw);
        state
            .assign_pane_to_feature(one, Some(&ids[0]), None)
            .unwrap();
        state
            .assign_pane_to_feature(two, Some(&ids[0]), None)
            .unwrap();

        state.remove_panes_from_features(&[one]);

        assert_eq!(names(&state), ["a"]);
        assert_eq!(members(&state, &ids[0]), [2]);
    }
}
