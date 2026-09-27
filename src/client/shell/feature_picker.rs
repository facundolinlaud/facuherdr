//! Picker opened by the "new agent" keybind: choose a feature, or type a new
//! name to create one, then start an agent in a new tab filed under it.

use crossterm::event::KeyCode;

use super::*;
use crate::api::schema::{FeatureChoice, FeatureStartAgentParams, Method};

#[derive(Debug)]
pub(super) struct ClientFeaturePickerOverlay {
    pub(super) workspace_id: String,
    /// `(feature_id, name)` in display order, captured when the picker opened.
    pub(super) features: Vec<(String, String)>,
    pub(super) query: TextEditor,
    pub(super) selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FeaturePickerChoice {
    Existing { feature_id: String, name: String },
    Create { name: String },
}

impl ClientFeaturePickerOverlay {
    /// A "create" entry leads while the query names no existing feature,
    /// followed by the features whose name contains the query.
    pub(super) fn choices(&self) -> Vec<FeaturePickerChoice> {
        let query = self.query.trim();
        let lowered = query.to_lowercase();
        let exact_match = self
            .features
            .iter()
            .any(|(_, name)| name.to_lowercase() == lowered);
        let create = (!query.is_empty() && !exact_match).then(|| FeaturePickerChoice::Create {
            name: query.to_string(),
        });
        let existing = self
            .features
            .iter()
            .filter(|(_, name)| name.to_lowercase().contains(&lowered))
            .map(|(feature_id, name)| FeaturePickerChoice::Existing {
                feature_id: feature_id.clone(),
                name: name.clone(),
            });
        create.into_iter().chain(existing).collect()
    }

    fn move_selection(&mut self, delta: isize) {
        let last = self.choices().len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }
}

impl ClientShellState {
    pub(super) fn open_feature_picker(&mut self, outcome: &mut ClientShellInput) {
        if !self.supports_feature_groups() {
            self.receive_endpoint_unavailable("This server does not support feature groups".into());
            outcome.repaint = true;
            return;
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let Some(workspace_id) = snapshot.focused_workspace_id.clone() else {
            return;
        };
        let features = snapshot
            .features
            .iter()
            .map(|feature| (feature.feature_id.clone(), feature.name.clone()))
            .collect();
        self.overlay = Some(ClientShellOverlay::FeaturePicker(
            ClientFeaturePickerOverlay {
                workspace_id,
                features,
                query: TextEditor::default(),
                selected: 0,
            },
        ));
        outcome.repaint = true;
    }

    pub(super) fn route_feature_picker_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some(ClientShellOverlay::FeaturePicker(picker)) = self.overlay.as_mut() else {
            return false;
        };
        outcome.repaint = true;
        let selected = picker.selected;
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.submit_feature_picker(selected, outcome),
            KeyCode::Up => picker.move_selection(-1),
            KeyCode::Down => picker.move_selection(1),
            _ => {
                if picker.query.handle_key(key) == Some(true) {
                    picker.selected = 0;
                }
            }
        }
        true
    }

    pub(super) fn paste_into_feature_picker(&mut self, text: &str) -> bool {
        let Some(ClientShellOverlay::FeaturePicker(picker)) = self.overlay.as_mut() else {
            return false;
        };
        if picker.query.insert(text) {
            picker.selected = 0;
        }
        true
    }

    pub(super) fn submit_feature_picker(&mut self, index: usize, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::FeaturePicker(picker)) = self.overlay.take() else {
            return;
        };
        let Some(choice) = picker.choices().into_iter().nth(index) else {
            self.overlay = Some(ClientShellOverlay::FeaturePicker(picker));
            return;
        };
        let feature = match choice {
            FeaturePickerChoice::Existing { feature_id, .. } => {
                FeatureChoice::Existing { feature_id }
            }
            FeaturePickerChoice::Create { name } => FeatureChoice::New { name },
        };
        self.push_endpoint_method(
            Method::FeatureStartAgent(FeatureStartAgentParams {
                workspace_id: picker.workspace_id,
                feature,
            }),
            outcome,
        );
        outcome.repaint = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(query: &str) -> ClientFeaturePickerOverlay {
        ClientFeaturePickerOverlay {
            workspace_id: "w1".into(),
            features: vec![
                ("feature-1".into(), "Checkout".into()),
                ("feature-2".into(), "Billing".into()),
            ],
            query: TextEditor::new(query, false),
            selected: 0,
        }
    }

    fn existing(id: &str, name: &str) -> FeaturePickerChoice {
        FeaturePickerChoice::Existing {
            feature_id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn empty_query_lists_all_features() {
        assert_eq!(
            picker("").choices(),
            [
                existing("feature-1", "Checkout"),
                existing("feature-2", "Billing")
            ]
        );
    }

    #[test]
    fn partial_query_offers_create_before_matches() {
        assert_eq!(
            picker(" check ").choices(),
            [
                FeaturePickerChoice::Create {
                    name: "check".into()
                },
                existing("feature-1", "Checkout"),
            ]
        );
    }

    #[test]
    fn exact_name_hides_create() {
        assert_eq!(
            picker("billing").choices(),
            [existing("feature-2", "Billing")]
        );
    }
}
