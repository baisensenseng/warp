use serde::{Deserialize, Serialize};

use super::CLIAgentSession;
use crate::terminal::CLIAgent;

/// Minimal data needed to reopen a CLI agent conversation after Warp restarts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CLIAgentRestoreData {
    pub agent: CLIAgent,
    pub session_id: Option<String>,
    pub custom_command_prefix: Option<String>,
}

impl CLIAgentRestoreData {
    /// Builds restore data from an active CLI agent session.
    ///
    /// # Parameters
    /// - `session`: The in-memory CLI agent session to persist.
    ///
    /// # Returns
    /// `Some(CLIAgentRestoreData)` for local Claude or Codex sessions; `None` for unsupported or remote sessions.
    pub fn from_session(session: &CLIAgentSession) -> Option<Self> {
        if session.remote_host.is_some()
            || !matches!(session.agent, CLIAgent::Claude | CLIAgent::Codex)
        {
            return None;
        }

        Some(Self {
            agent: session.agent,
            session_id: session.session_context.session_id.clone(),
            custom_command_prefix: session.custom_command_prefix.clone(),
        })
    }

    /// Builds the shell command used to restore the CLI agent conversation.
    ///
    /// # Parameters
    /// None.
    ///
    /// # Returns
    /// A command string that can be queued in a restored terminal pane, or `None` for unsupported agents.
    pub fn command(&self) -> Option<String> {
        let command_prefix = self.command_prefix();
        let session_id = self.session_id.as_deref().and_then(safe_shell_token);

        let command = match (self.agent, session_id) {
            (CLIAgent::Claude, Some(session_id)) => {
                format!("{command_prefix} --resume {session_id}")
            }
            (CLIAgent::Claude, None) => format!("{command_prefix} --continue"),
            (CLIAgent::Codex, Some(session_id)) => format!("{command_prefix} resume {session_id}"),
            (CLIAgent::Codex, None) => format!("{command_prefix} resume --last"),
            (CLIAgent::Gemini, _)
            | (CLIAgent::Amp, _)
            | (CLIAgent::Droid, _)
            | (CLIAgent::OpenCode, _)
            | (CLIAgent::Copilot, _)
            | (CLIAgent::Pi, _)
            | (CLIAgent::Auggie, _)
            | (CLIAgent::CursorCli, _)
            | (CLIAgent::Goose, _)
            | (CLIAgent::Hermes, _)
            | (CLIAgent::Vibe, _)
            | (CLIAgent::Antigravity, _)
            | (CLIAgent::Unknown, _) => return None,
        };

        Some(command)
    }

    /// Resolves the executable name used for the restore command.
    ///
    /// # Parameters
    /// None.
    ///
    /// # Returns
    /// A safe shell token, preferring the user's original custom command prefix when possible.
    fn command_prefix(&self) -> &str {
        self.custom_command_prefix
            .as_deref()
            .and_then(safe_shell_token)
            .unwrap_or_else(|| self.agent.command_prefix())
    }
}

/// Accepts only simple shell tokens that do not require quoting.
///
/// # Parameters
/// - `value`: Candidate token to validate.
///
/// # Returns
/// `Some(trimmed_value)` when the token can be inserted into a shell command without quoting; otherwise `None`.
fn safe_shell_token(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .bytes()
        .all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':' | b'\\')
        })
        .then_some(trimmed)
}
