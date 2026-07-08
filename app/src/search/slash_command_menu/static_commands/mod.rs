pub mod bindings;
pub mod commands;

use bitflags::bitflags;
pub use commands::SlashCommandId;
use warpui::AppContext;

use crate::localization::t;

bitflags! {
    /// Specifies the requirements for a slash command to be available.
    ///
    /// Each flag represents a requirement that the session context must satisfy. The command is
    /// available when the session supports *all* of the command's requirement flags.
    ///
    /// A few common cases:
    /// * If neither [`Self::AGENT_VIEW`] nor [`Self::TERMINAL_VIEW`] is set, the command is available in all modes.
    ///   A command should *not* set both flags to be available in both modes - this results in requirements that cannot be satisfied.
    /// * Most `/fork`-like slash commands require [`Self::NO_LRC_CONTROL`] and [`Self::ACTIVE_CONVERSATION`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Availability: u16 {
        /// No requirements — always available.
        const ALWAYS = 0;
        /// Requires the agent view.
        const AGENT_VIEW = 1 << 0;
        /// Requires the terminal view.
        const TERMINAL_VIEW = 1 << 1;
        /// Requires a local session (not available in remote/cloud sessions).
        const LOCAL = 1 << 2;
        /// Requires a git repository.
        const REPOSITORY = 1 << 3;
        /// Requires that the agent is not currently in control of a long-running command.
        const NO_LRC_CONTROL = 1 << 4;
        /// Requires an active AI conversation.
        const ACTIVE_CONVERSATION = 1 << 5;
        /// Requires codebase context to be enabled.
        const CODEBASE_CONTEXT = 1 << 6;
        /// Requires AI to be globally enabled.
        const AI_ENABLED = 1 << 7;
        /// Requires a non-cloud-agent context.
        const NOT_CLOUD_AGENT = 1 << 8;
        /// Requires a cloud-agent context.
        const CLOUD_AGENT = 1 << 9;
        /// Set on the session context iff the slash command data source was constructed via
        /// `SlashCommandDataSource::for_cloud_mode_v2` *and* `FeatureFlag::CloudModeInputV2`
        /// is enabled. Commands that require this bit are hidden everywhere except the V2
        /// cloud-mode composing input.
        const CLOUD_MODE_V2_COMPOSER = 1 << 10;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Argument {
    pub hint_text: Option<&'static str>,
    pub is_optional: bool,
    /// If `true`, selecting the slash command from the menu (or via keybinding) will execute the
    /// slash command with no arguments.
    ///
    /// If `false`, selecting the slash command from the menu (or via keybinding) inserts the
    /// slash command into the input.
    ///
    /// Set this based on whether or not you want you think a user should always have the option to
    /// supply an argument.
    pub should_execute_on_selection: bool,
}

impl Argument {
    pub(super) fn optional() -> Self {
        Self {
            is_optional: true,
            ..Default::default()
        }
    }

    pub(super) fn required() -> Self {
        Self {
            is_optional: false,
            ..Default::default()
        }
    }

    pub(super) fn with_hint_text(mut self, text: &'static str) -> Self {
        self.hint_text = Some(text);
        self
    }

    pub(super) fn with_execute_on_selection(mut self) -> Self {
        self.should_execute_on_selection = true;
        self
    }

    /// Returns the localized hint text for this command argument.
    ///
    /// # Parameters
    /// - `command_name`: Slash command name used to resolve the matching translation key.
    /// - `app`: Application context used to read the current display language.
    ///
    /// # Returns
    /// Localized hint text when a translation exists, otherwise the command's default hint text.
    pub fn localized_hint_text(&self, command_name: &str, app: &AppContext) -> Option<String> {
        slash_command_hint_key(command_name)
            .map(|key| t(app, key))
            .or_else(|| self.hint_text.map(|text| text.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub icon_path: &'static str,
    /// Specifies the requirements for this command to be available. See [`Availability`].
    pub availability: Availability,
    /// Whether this command requires AI mode when executed.
    /// If true, AI mode will be activated when the command is accepted.
    pub auto_enter_ai_mode: bool,
    pub argument: Option<Argument>,
}

impl StaticCommand {
    /// Returns the localized description for this static slash command.
    ///
    /// # Parameters
    /// - `app`: Application context used to read the current display language.
    ///
    /// # Returns
    /// Localized description when a translation exists, otherwise the command's default description.
    pub fn localized_description(&self, app: &AppContext) -> String {
        slash_command_description_key(self.name)
            .map(|key| t(app, key))
            .unwrap_or_else(|| self.description.to_owned())
    }

    pub fn matches_filter(&self, filter_text: &str) -> bool {
        if filter_text.is_empty() {
            return true;
        }

        let filter_lower = filter_text.to_lowercase();
        self.name
            .to_lowercase()
            .get(1..)
            .unwrap_or("")
            .starts_with(&filter_lower)
    }

    pub fn is_active(&self, session_context: Availability) -> bool {
        session_context.contains(self.availability)
    }
}

fn slash_command_description_key(command_name: &str) -> Option<&'static str> {
    match command_name {
        "/add-mcp" => Some("slash-add-mcp-description"),
        "/add-prompt" => Some("slash-add-prompt-description"),
        "/add-rule" => Some("slash-add-rule-description"),
        "/agent" => Some("slash-agent-description"),
        "/changelog" => Some("slash-changelog-description"),
        "/cloud-agent" => Some("slash-cloud-agent-description"),
        "/compact" => Some("slash-compact-description"),
        "/compact-and" => Some("slash-compact-and-description"),
        "/continue-locally" => Some("slash-continue-locally-description"),
        "/conversations" => Some("slash-conversations-description"),
        "/cost" => Some("slash-cost-description"),
        "/create-environment" => Some("slash-create-environment-description"),
        "/create-new-project" => Some("slash-create-new-project-description"),
        "/docker-sandbox" => Some("slash-docker-sandbox-description"),
        "/environment" => Some("slash-environment-description"),
        "/export-to-clipboard" => Some("slash-export-to-clipboard-description"),
        "/export-to-file" => Some("slash-export-to-file-description"),
        "/feedback" => Some("slash-feedback-description"),
        "/fork" => Some("slash-fork-description"),
        "/fork-and-compact" => Some("slash-fork-and-compact-description"),
        "/fork-from" => Some("slash-fork-from-description"),
        "/handoff" => Some("slash-handoff-description"),
        "/harness" => Some("slash-harness-description"),
        "/host" => Some("slash-host-description"),
        "/index" => Some("slash-index-description"),
        "/init" => Some("slash-init-description"),
        "/model" => Some("slash-model-description"),
        "/new" => Some("slash-new-description"),
        "/open-code-review" => Some("slash-open-code-review-description"),
        "/open-file" => Some("slash-open-file-description"),
        "/open-mcp-servers" => Some("slash-open-mcp-servers-description"),
        "/open-project-rules" => Some("slash-open-project-rules-description"),
        "/open-repo" => Some("slash-open-repo-description"),
        "/open-rules" => Some("slash-open-rules-description"),
        "/open-settings-file" => Some("slash-open-settings-file-description"),
        "/open-skill" => Some("slash-open-skill-description"),
        "/orchestrate" => Some("slash-orchestrate-description"),
        "/plan" => Some("slash-plan-description"),
        "/pr-comments" => Some("slash-pr-comments-description"),
        "/profile" => Some("slash-profile-description"),
        "/prompts" => Some("slash-prompts-description"),
        "/queue" => Some("slash-queue-description"),
        "/remote-control" => Some("slash-remote-control-description"),
        "/rename-conversation" => Some("slash-rename-conversation-description"),
        "/rename-tab" => Some("slash-rename-tab-description"),
        "/rewind" => Some("slash-rewind-description"),
        "/set-tab-color" => Some("slash-set-tab-color-description"),
        "/skills" => Some("slash-skills-description"),
        "/usage" => Some("slash-usage-description"),
        _ => None,
    }
}

fn slash_command_hint_key(command_name: &str) -> Option<&'static str> {
    match command_name {
        "/compact" => Some("slash-compact-hint"),
        "/compact-and" => Some("slash-compact-and-hint"),
        "/continue-locally" => Some("slash-continue-locally-hint"),
        "/create-environment" => Some("slash-create-environment-hint"),
        "/create-new-project" => Some("slash-create-new-project-hint"),
        "/export-to-file" => Some("slash-export-to-file-hint"),
        "/fork" => Some("slash-fork-hint"),
        "/fork-and-compact" => Some("slash-fork-and-compact-hint"),
        "/handoff" => Some("slash-handoff-hint"),
        "/open-file" => Some("slash-open-file-hint"),
        "/orchestrate" => Some("slash-orchestrate-hint"),
        "/plan" => Some("slash-plan-hint"),
        "/queue" => Some("slash-queue-hint"),
        "/rename-conversation" => Some("slash-rename-conversation-hint"),
        "/rename-tab" => Some("slash-rename-tab-hint"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
