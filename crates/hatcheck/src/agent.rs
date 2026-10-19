//! The agents hatcheck keeps apart: Claude Code, Codex and Antigravity CLI (agy). One place
//! for what each is called and where it keeps things, so that the commands tell them apart by
//! matching on the agent, which the compiler checks is complete, rather than on its name as
//! text.

use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Agent {
    Claude,
    Codex,
    Agy,
}

impl Agent {
    /// Every agent, in the order the commands list them.
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Agy];
    /// Their keys, for an argument that takes one.
    pub const KEYS: [&'static str; 3] = ["claude", "codex", "agy"];

    /// The word --json, a PROFILE argument and the folder bindings use for it.
    pub fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Agy => "agy",
        }
    }

    pub fn from_key(key: &str) -> Option<Agent> {
        Agent::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Its name in messages.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Agy => "Antigravity CLI",
        }
    }

    /// Who replies in one of its conversations.
    pub fn speaker(self) -> &'static str {
        match self {
            Agent::Claude => "Claude",
            Agent::Codex => "Codex",
            Agent::Agy => "Antigravity",
        }
    }

    /// The start of the agent's profile folders in the home folder: a profile is ~/<base>-NAME,
    /// and ~/<base> is where plain claude or codex looks. Antigravity CLI keeps everything under
    /// ~/.gemini/antigravity-cli, which only HOME moves, so a profile of it would be a home
    /// folder of its own, ~/.agy-NAME; none is made yet (see paths::profile_folder).
    pub fn base(self) -> &'static str {
        match self {
            Agent::Claude => ".claude",
            Agent::Codex => ".codex",
            Agent::Agy => ".agy",
        }
    }

    /// The variable that points the agent at a profile; Antigravity CLI has none.
    pub fn variable(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("CLAUDE_CONFIG_DIR"),
            Agent::Codex => Some("CODEX_HOME"),
            Agent::Agy => None,
        }
    }

    /// The agent's own command.
    pub fn program(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Agy => "agy",
        }
    }

    /// The launcher that runs it with a profile; Antigravity CLI has no profiles, so none.
    pub fn launcher(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("cl"),
            Agent::Codex => Some("cx"),
            Agent::Agy => None,
        }
    }
}

/// Sorted by key, as the names sorted as text: agy, claude, codex.
impl Ord for Agent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(other.key())
    }
}

impl PartialOrd for Agent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_every_agent() {
        for (agent, key) in Agent::ALL.iter().zip(Agent::KEYS) {
            assert_eq!(agent.key(), key);
            assert_eq!(Agent::from_key(key), Some(*agent));
        }
        assert_eq!(Agent::from_key("x"), None);
        assert!(Agent::Agy < Agent::Claude && Agent::Claude < Agent::Codex);
    }
}
