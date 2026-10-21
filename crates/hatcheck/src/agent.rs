//! The agents hatcheck keeps apart: Claude Code, Codex, Antigravity CLI (agy), Kimi Code CLI
//! and ZCode. One place for what each is called and where it keeps things, so that the
//! commands tell them apart by matching on the agent, which the compiler checks is complete,
//! rather than on its name as text.

use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Agent {
    Claude,
    Codex,
    Agy,
    Kimi,
    Zcode,
}

impl Agent {
    /// Every agent, in the order the commands list them.
    pub const ALL: [Agent; 5] = [
        Agent::Claude,
        Agent::Codex,
        Agent::Agy,
        Agent::Kimi,
        Agent::Zcode,
    ];
    /// Their keys, for an argument that takes one.
    pub const KEYS: [&'static str; 5] = ["claude", "codex", "agy", "kimi", "zcode"];

    /// The word --json, a PROFILE argument and the folder bindings use for it.
    pub fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Agy => "agy",
            Agent::Kimi => "kimi",
            Agent::Zcode => "zcode",
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
            Agent::Kimi => "Kimi Code CLI",
            Agent::Zcode => "ZCode",
        }
    }

    /// Its name before "CLI sessions" in the session list's heading: the name without CLI.
    pub fn heading_name(self) -> &'static str {
        match self {
            Agent::Agy => "Antigravity",
            Agent::Kimi => "Kimi Code",
            other => other.name(),
        }
    }

    /// Who replies in one of its conversations.
    pub fn speaker(self) -> &'static str {
        match self {
            Agent::Claude => "Claude",
            Agent::Codex => "Codex",
            Agent::Agy => "Antigravity",
            Agent::Kimi => "Kimi",
            Agent::Zcode => "ZCode",
        }
    }

    /// The start of the agent's profile folders in the home folder: a profile is ~/<base>-NAME,
    /// and ~/<base> is where plain claude, codex or kimi looks. Antigravity CLI keeps everything
    /// under ~/.gemini/antigravity-cli, and ZCode under ~/.zcode, which only HOME moves, so a
    /// profile of either would be a home folder of its own, ~/.agy-NAME or ~/.zcode-NAME; none
    /// is made yet (see paths::profile_folder).
    pub fn base(self) -> &'static str {
        match self {
            Agent::Claude => ".claude",
            Agent::Codex => ".codex",
            Agent::Agy => ".agy",
            Agent::Kimi => ".kimi-code",
            Agent::Zcode => ".zcode",
        }
    }

    /// The variable that points the agent at a profile; Antigravity CLI and ZCode have none.
    pub fn variable(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("CLAUDE_CONFIG_DIR"),
            Agent::Codex => Some("CODEX_HOME"),
            Agent::Kimi => Some("KIMI_CODE_HOME"),
            Agent::Agy | Agent::Zcode => None,
        }
    }

    /// Whether the agent keeps one folder per account that hatcheck names ~/<base>-NAME, so a
    /// bare NAME can mean one of its profiles: Claude Code's, Codex's and Kimi's.
    pub fn has_profiles(self) -> bool {
        self.variable().is_some()
    }

    /// The agent's own command.
    pub fn program(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Agy => "agy",
            Agent::Kimi => "kimi",
            Agent::Zcode => "zcode",
        }
    }

    /// The launcher that runs it with a profile; Antigravity CLI and ZCode have no profiles, so
    /// none.
    pub fn launcher(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("cl"),
            Agent::Codex => Some("cx"),
            Agent::Kimi => Some("ck"),
            Agent::Agy | Agent::Zcode => None,
        }
    }

    /// The agent's own prefix on its session IDs, which hatcheck leaves off when it names a
    /// session and puts back in the agent's commands and files: Kimi's session_<uuid> and
    /// ZCode's sess_<uuid>.
    pub fn id_prefix(self) -> &'static str {
        match self {
            Agent::Kimi => "session_",
            Agent::Zcode => "sess_",
            Agent::Claude | Agent::Codex | Agent::Agy => "",
        }
    }

    /// An ID or the start of one as hatcheck matches it: without the agent's prefix, which
    /// someone may paste along with it, and without one that is only the start of the prefix.
    pub fn bare_id(self, id: &str) -> &str {
        let prefix = self.id_prefix();
        if prefix.is_empty() {
            return id;
        }
        if let Some(rest) = id.strip_prefix(prefix) {
            return rest;
        }
        if !id.is_empty() && prefix.starts_with(id) {
            return ""; // "sess" alone: the start of every ID
        }
        id
    }

    /// The agent's own form of a session ID hatcheck names bare.
    pub fn full_id(self, bare: &str) -> String {
        format!("{}{bare}", self.id_prefix())
    }
}

/// Sorted by key, as the names sorted as text: agy, claude, codex, kimi, zcode.
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
        assert!(Agent::Codex < Agent::Kimi && Agent::Kimi < Agent::Zcode);
    }

    #[test]
    fn ids_lose_and_regain_the_agents_prefix() {
        let id = "0752ad88-cc62-4e00-9912-48e4146b7628";
        assert_eq!(Agent::Kimi.bare_id(&format!("session_{id}")), id);
        assert_eq!(Agent::Kimi.bare_id("0752ad88"), "0752ad88");
        assert_eq!(Agent::Kimi.bare_id("sess"), ""); // the start of the prefix: every session
        assert_eq!(Agent::Zcode.bare_id("sess_1fb068a4"), "1fb068a4");
        assert_eq!(Agent::Zcode.full_id("1fb068a4"), "sess_1fb068a4");
        assert_eq!(Agent::Claude.bare_id("session_x"), "session_x");
        assert_eq!(Agent::Codex.full_id(id), id);
    }
}
