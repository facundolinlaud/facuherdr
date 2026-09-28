use bytes::Bytes;

use crate::api::schema::{
    FeatureAssignPaneParams, FeatureChoice, FeatureCreateParams, FeatureInheritParams,
    FeatureMoveParams, FeatureRenameParams, FeatureStartAgentParams, FeatureTarget, ResponseResult,
};
use crate::app::features::FeatureError;
use crate::app::App;

use super::responses::{encode_error, encode_success};

fn feature_error(id: String, error: FeatureError) -> String {
    let code = match error {
        FeatureError::EmptyName => "invalid_feature_name",
        FeatureError::NotFound => "feature_not_found",
    };
    encode_error(id, code, error.message())
}

fn feature_result(id: String, result: Result<(), FeatureError>) -> String {
    match result {
        Ok(()) => encode_success(id, ResponseResult::Ok {}),
        Err(error) => feature_error(id, error),
    }
}

impl App {
    pub(super) fn handle_feature_list(&mut self, id: String) -> String {
        let features = self
            .state
            .features
            .iter()
            .map(|feature| crate::api::schema::FeatureInfo {
                feature_id: feature.id.clone(),
                name: feature.name.clone(),
                pane_ids: feature
                    .members
                    .iter()
                    .filter_map(|&pane_id| {
                        let (ws_idx, _) = self.find_pane(pane_id)?;
                        self.public_pane_id(ws_idx, pane_id)
                    })
                    .collect(),
            })
            .collect();
        encode_success(id, ResponseResult::FeatureList { features })
    }

    pub(super) fn handle_feature_create(
        &mut self,
        id: String,
        params: FeatureCreateParams,
    ) -> String {
        match self.state.create_feature(&params.name) {
            Ok(feature_id) => encode_success(id, ResponseResult::FeatureCreated { feature_id }),
            Err(error) => feature_error(id, error),
        }
    }

    pub(super) fn handle_feature_rename(
        &mut self,
        id: String,
        params: FeatureRenameParams,
    ) -> String {
        let result = self.state.rename_feature(&params.feature_id, &params.name);
        feature_result(id, result)
    }

    pub(super) fn handle_feature_delete(&mut self, id: String, target: FeatureTarget) -> String {
        let result = self.state.delete_feature(&target.feature_id);
        feature_result(id, result)
    }

    pub(super) fn handle_feature_move(&mut self, id: String, params: FeatureMoveParams) -> String {
        let result = self.state.move_feature(&params.feature_id, params.index);
        feature_result(id, result)
    }

    pub(super) fn handle_feature_assign_pane(
        &mut self,
        id: String,
        params: FeatureAssignPaneParams,
    ) -> String {
        let Some((_, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return encode_error(
                id,
                "pane_not_found",
                format!("pane {} not found", params.pane_id),
            );
        };
        let result =
            self.state
                .assign_pane_to_feature(pane_id, params.feature_id.as_deref(), params.index);
        feature_result(id, result)
    }

    pub(super) fn handle_feature_inherit(
        &mut self,
        id: String,
        params: FeatureInheritParams,
    ) -> String {
        let Some((_, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return encode_error(
                id,
                "pane_not_found",
                format!("pane {} not found", params.pane_id),
            );
        };
        let Some((_, from_pane_id)) = self.parse_pane_id(&params.from_pane_id) else {
            return encode_error(
                id,
                "pane_not_found",
                format!("pane {} not found", params.from_pane_id),
            );
        };
        self.state.inherit_feature(pane_id, from_pane_id);
        encode_success(id, ResponseResult::Ok {})
    }

    /// Opens a new space, types the configured agent command into it (with the
    /// task as its first message), and files the pane under the chosen feature.
    /// Without a feature, the task is prefixed with the feature commands so the
    /// user's message can say where the agent belongs. A feature created by
    /// this call is removed again when the agent cannot be started.
    pub(super) fn handle_feature_start_agent(
        &mut self,
        id: String,
        params: FeatureStartAgentParams,
    ) -> String {
        let Some(ws_idx) = self.parse_workspace_id(&params.workspace_id) else {
            return encode_error(
                id,
                "workspace_not_found",
                format!("workspace {} not found", params.workspace_id),
            );
        };
        let task = params
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|task| !task.is_empty());
        let target =
            match params.feature {
                Some(FeatureChoice::Existing { feature_id }) => {
                    match self.state.feature_index(&feature_id) {
                        Ok(_) => AgentFeature::Existing(feature_id),
                        Err(error) => return feature_error(id, error),
                    }
                }
                Some(FeatureChoice::New { name }) => match self.state.create_feature(&name) {
                    Ok(feature_id) => AgentFeature::Created(feature_id),
                    Err(error) => return feature_error(id, error),
                },
                None if task.is_some() => AgentFeature::ChosenByUser,
                None => return encode_error(
                    id,
                    "invalid_params",
                    "an agent started without a feature needs a task that says where it belongs",
                ),
            };
        let command_line = agent_command_line(
            &self.state.new_agent_command,
            task,
            target == AgentFeature::ChosenByUser,
        );
        let (agent_ws_idx, pane_id) = match self.open_agent_space(ws_idx, &command_line) {
            Ok(opened) => opened,
            Err(err) => {
                if let AgentFeature::Created(feature_id) = &target {
                    // Undo only what this call created; the feature id was just issued.
                    let _ = self.state.delete_feature(feature_id);
                }
                return encode_error(id, "feature_agent_start_failed", err.to_string());
            }
        };
        let feature_id = match target {
            AgentFeature::Existing(feature_id) | AgentFeature::Created(feature_id) => {
                // The feature was validated or created above, so assignment cannot fail.
                let _ = self
                    .state
                    .assign_pane_to_feature(pane_id, Some(&feature_id), None);
                Some(feature_id)
            }
            AgentFeature::ChosenByUser => None,
        };
        self.schedule_session_save();
        self.emit_workspace_open_events(agent_ws_idx);
        match self.public_pane_id(agent_ws_idx, pane_id) {
            Some(pane_id) => encode_success(
                id,
                ResponseResult::FeatureAgentStarted {
                    feature_id,
                    pane_id,
                },
            ),
            None => encode_error(id, "pane_not_found", "new agent pane disappeared"),
        }
    }

    /// Opens a focused space that starts where `source_ws_idx` would start a
    /// new space, types `command_line` into its shell, and returns the space
    /// index and its root pane.
    fn open_agent_space(
        &mut self,
        source_ws_idx: usize,
        command_line: &str,
    ) -> std::io::Result<(usize, crate::layout::PaneId)> {
        let cwd = self.resolved_new_workspace_cwd_from(source_ws_idx);
        let ws_idx = self.create_workspace_with_launch_env(cwd, true, Vec::new())?;
        let pane_id = self.state.workspaces[ws_idx].tabs[0].root_pane;
        let runtime = self
            .state
            .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
            .ok_or_else(|| std::io::Error::other("new agent space has no terminal"))?;
        let bytes = crate::app::api_helpers::encode_api_submission(runtime, command_line);
        runtime
            .try_send_bytes(Bytes::from(bytes))
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        Ok((ws_idx, pane_id))
    }
}

/// Where a started agent is filed.
#[derive(Debug, PartialEq, Eq)]
enum AgentFeature {
    Existing(String),
    /// Created by this request, so undone if the agent cannot start.
    Created(String),
    /// None yet: the user's task says where the agent belongs.
    ChosenByUser,
}

/// Context only: the user's own message says whether and where to file the agent.
const FEATURE_COMMANDS: &str = "Context: you run in Herdr, which groups agents into \
feature groups in its sidebar. You are not in one yet. Commands: \
\"${HERDR_BIN_PATH:-herdr}\" feature list shows the existing feature groups; \
\"${HERDR_BIN_PATH:-herdr}\" feature join \"<name>\" moves you into one; adding --create \
creates it first. Message from the user:";

/// The line typed into the new agent's shell: the configured command, then the
/// task as one shell-quoted argument so the agent starts with it as its first
/// message.
fn agent_command_line(command: &str, task: Option<&str>, ungrouped: bool) -> String {
    let Some(task) = task else {
        return command.to_string();
    };
    let message = if ungrouped {
        format!("{FEATURE_COMMANDS} {task}")
    } else {
        task.to_string()
    };
    format!("{command} {}", shell_single_quote(&message))
}

/// Quotes text for a POSIX shell: single quotes, with each embedded single quote
/// written as '\''.
fn shell_single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{
        FeatureAssignPaneParams, FeatureChoice, FeatureStartAgentParams, Method, Request,
    };

    fn app_with_workspace() -> crate::app::App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = crate::app::App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("features")];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app
    }

    fn request(app: &mut crate::app::App, method: Method) -> serde_json::Value {
        let response = app.handle_api_request(Request {
            id: "req".into(),
            method,
        });
        serde_json::from_str(&response).unwrap()
    }

    #[test]
    fn assign_pane_rejects_unknown_pane_and_feature() {
        let mut app = app_with_workspace();
        let feature_id = app.state.create_feature("checkout").unwrap();
        let pane_id = app.public_pane_id(0, app.state.workspaces[0].focused_pane_id().unwrap());

        let unknown_pane = request(
            &mut app,
            Method::FeatureAssignPane(FeatureAssignPaneParams {
                pane_id: "missing".into(),
                feature_id: Some(feature_id.clone()),
                index: None,
            }),
        );
        assert_eq!(unknown_pane["error"]["code"], "pane_not_found");

        let unknown_feature = request(
            &mut app,
            Method::FeatureAssignPane(FeatureAssignPaneParams {
                pane_id: pane_id.clone().unwrap(),
                feature_id: Some("missing".into()),
                index: None,
            }),
        );
        assert_eq!(unknown_feature["error"]["code"], "feature_not_found");

        let assigned = request(
            &mut app,
            Method::FeatureAssignPane(FeatureAssignPaneParams {
                pane_id: pane_id.unwrap(),
                feature_id: Some(feature_id),
                index: None,
            }),
        );
        assert!(assigned.get("error").is_none(), "{assigned}");
        assert_eq!(app.state.features[0].members.len(), 1);
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn start_agent_rejects_unknown_workspace_without_creating_feature() {
        let mut app = app_with_workspace();

        let response = request(
            &mut app,
            Method::FeatureStartAgent(FeatureStartAgentParams {
                workspace_id: "missing".into(),
                feature: Some(FeatureChoice::New {
                    name: "checkout".into(),
                }),
                prompt: None,
            }),
        );

        assert_eq!(response["error"]["code"], "workspace_not_found");
        assert!(app.state.features.is_empty());
    }

    #[test]
    fn command_line_without_task_is_the_configured_command() {
        assert_eq!(super::agent_command_line("claude", None, false), "claude");
    }

    #[test]
    fn command_line_passes_the_task_as_one_quoted_argument() {
        assert_eq!(
            super::agent_command_line("claude", Some("fix Bob's bug"), false),
            "claude 'fix Bob'\\''s bug'"
        );
    }

    #[test]
    fn ungrouped_agent_gets_the_feature_commands_as_context_before_the_message() {
        let line = super::agent_command_line("claude", Some("fix it"), true);

        assert!(line.starts_with("claude 'Context: you run in Herdr"));
        assert!(line.contains("feature list") && line.contains("feature join"));
        assert!(!line.contains("Before anything else"));
        assert!(line.ends_with("Message from the user: fix it'"));
    }

    #[test]
    fn ungrouped_agent_needs_a_task() {
        let mut app = app_with_workspace();
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = request(
            &mut app,
            Method::FeatureStartAgent(FeatureStartAgentParams {
                workspace_id,
                feature: None,
                prompt: Some("  ".into()),
            }),
        );

        assert_eq!(response["error"]["code"], "invalid_params");
        assert_eq!(app.state.workspaces.len(), 1);
    }
}
