//! The agents hatcheck keeps apart: Claude Code, Codex, Antigravity CLI (agy), Kimi Code CLI,
//! ZCode, DeepSeek Harness (dsh), MiniMax Code (mcode), Qwen Code (qwen), GitHub Copilot CLI
//! (copilot) and OpenCode (opencode). One place for what each is called and where it keeps
//! things, so that the commands tell them apart by matching on the agent, which the compiler
//! checks is complete, rather than on its name as text.

use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Agent {
    Claude,
    Codex,
    Agy,
    Kimi,
    Zcode,
    Dsh,
    Mcode,
    Qwen,
    Copilot,
    Opencode,
}

impl Agent {
    /// Every agent, in the order the commands list them.
    pub const ALL: [Agent; 10] = [
        Agent::Claude,
        Agent::Codex,
        Agent::Agy,
        Agent::Kimi,
        Agent::Zcode,
        Agent::Dsh,
        Agent::Mcode,
        Agent::Qwen,
        Agent::Copilot,
        Agent::Opencode,
    ];
    /// Their keys, for an argument that takes one.
    pub const KEYS: [&'static str; 10] = [
        "claude", "codex", "agy", "kimi", "zcode", "dsh", "mcode", "qwen", "copilot", "opencode",
    ];

    /// The word --json, a PROFILE argument and the folder bindings use for it.
    pub fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Agy => "agy",
            Agent::Kimi => "kimi",
            Agent::Zcode => "zcode",
            Agent::Dsh => "dsh",
            Agent::Mcode => "mcode",
            Agent::Qwen => "qwen",
            Agent::Copilot => "copilot",
            Agent::Opencode => "opencode",
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
            Agent::Dsh => "DeepSeek Harness",
            Agent::Mcode => "MiniMax Code",
            Agent::Qwen => "Qwen Code",
            Agent::Copilot => "GitHub Copilot CLI",
            Agent::Opencode => "OpenCode",
        }
    }

    /// Its name before "CLI sessions" in the session list's heading: the name without CLI.
    pub fn heading_name(self) -> &'static str {
        match self {
            Agent::Agy => "Antigravity",
            Agent::Kimi => "Kimi Code",
            Agent::Mcode => "MiniMax",
            Agent::Qwen => "Qwen",
            Agent::Copilot => "Copilot",
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
            Agent::Dsh => "DeepSeek",
            Agent::Mcode => "MiniMax",
            Agent::Qwen => "Qwen",
            Agent::Copilot => "Copilot",
            Agent::Opencode => "OpenCode",
        }
    }

    /// The start of the agent's profile folders in the home folder: a profile is ~/<base>-NAME,
    /// and ~/<base> is where plain claude, codex, kimi, dsh, mcode, qwen or copilot looks.
    /// Antigravity CLI keeps everything under ~/.gemini/antigravity-cli, and ZCode under
    /// ~/.zcode, which only HOME moves, so a profile of either would be a home folder of its
    /// own, ~/.agy-NAME or ~/.zcode-NAME; none is made yet (see paths::profile_folder). OpenCode
    /// keeps its data in ~/.local/share/opencode, and a second store would need XDG_DATA_HOME or
    /// OPENCODE_DB and --standalone, since its background service keeps the store it started
    /// with: a product decision not taken, so ~/.opencode-NAME is a name and nothing more.
    pub fn base(self) -> &'static str {
        match self {
            Agent::Claude => ".claude",
            Agent::Codex => ".codex",
            Agent::Agy => ".agy",
            Agent::Kimi => ".kimi-code",
            Agent::Zcode => ".zcode",
            Agent::Dsh => ".dsh",
            Agent::Mcode => ".minimax",
            Agent::Qwen => ".qwen",
            Agent::Copilot => ".copilot",
            Agent::Opencode => ".opencode",
        }
    }

    /// The variable that points the agent at a profile; Antigravity CLI, ZCode and OpenCode
    /// have none.
    pub fn variable(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("CLAUDE_CONFIG_DIR"),
            Agent::Codex => Some("CODEX_HOME"),
            Agent::Kimi => Some("KIMI_CODE_HOME"),
            Agent::Dsh => Some("DSH_HOME"),
            Agent::Mcode => Some("MINIMAX_DATA_DIR"),
            Agent::Qwen => Some("QWEN_HOME"),
            Agent::Copilot => Some("COPILOT_HOME"),
            Agent::Agy | Agent::Zcode | Agent::Opencode => None,
        }
    }

    /// Whether the agent keeps one folder per account that hatcheck names ~/<base>-NAME, so a
    /// bare NAME can mean one of its profiles: Claude Code's, Codex's, Kimi's, DSH's, mcode's,
    /// qwen's and copilot's.
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
            Agent::Dsh => "dsh",
            Agent::Mcode => "mcode",
            Agent::Qwen => "qwen",
            Agent::Copilot => "copilot",
            Agent::Opencode => "opencode",
        }
    }

    /// The launcher that runs it with a profile; Antigravity CLI, ZCode, DSH, mcode, qwen,
    /// copilot and opencode have none.
    pub fn launcher(self) -> Option<&'static str> {
        match self {
            Agent::Claude => Some("cl"),
            Agent::Codex => Some("cx"),
            Agent::Kimi => Some("ck"),
            Agent::Agy
            | Agent::Zcode
            | Agent::Dsh
            | Agent::Mcode
            | Agent::Qwen
            | Agent::Copilot
            | Agent::Opencode => None,
        }
    }

    /// The agent's own prefix on its session IDs, which hatcheck leaves off when it names a
    /// session and puts back in the agent's commands and files: Kimi's session_<uuid>, ZCode's
    /// sess_<uuid>, DSH's session-<uuid>, mcode's mvs_<id> and opencode's ses_<id>.
    pub fn id_prefix(self) -> &'static str {
        match self {
            Agent::Kimi => "session_",
            Agent::Zcode => "sess_",
            Agent::Dsh => "session-",
            Agent::Mcode => "mvs_",
            Agent::Opencode => "ses_",
            Agent::Claude | Agent::Codex | Agent::Agy | Agent::Qwen | Agent::Copilot => "",
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

/// Sorted by key, as the names sorted as text: agy, claude, codex, copilot, dsh, kimi, mcode,
/// opencode, qwen, zcode.
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
        assert!(
            Agent::Codex < Agent::Copilot
                && Agent::Copilot < Agent::Dsh
                && Agent::Dsh < Agent::Kimi
                && Agent::Kimi < Agent::Mcode
                && Agent::Mcode < Agent::Opencode
                && Agent::Opencode < Agent::Qwen
                && Agent::Qwen < Agent::Zcode
        );
    }

    #[test]
    fn ids_lose_and_regain_the_agents_prefix() {
        let id = "0752ad88-cc62-4e00-9912-48e4146b7628";
        assert_eq!(Agent::Kimi.bare_id(&format!("session_{id}")), id);
        assert_eq!(Agent::Kimi.bare_id("0752ad88"), "0752ad88");
        assert_eq!(Agent::Kimi.bare_id("sess"), ""); // the start of the prefix: every session
        assert_eq!(Agent::Zcode.bare_id("sess_1fb068a4"), "1fb068a4");
        assert_eq!(Agent::Zcode.full_id("1fb068a4"), "sess_1fb068a4");
        assert_eq!(Agent::Dsh.bare_id(&format!("session-{id}")), id);
        assert_eq!(Agent::Dsh.full_id(id), format!("session-{id}"));
        assert_eq!(Agent::Dsh.bare_id("session"), "");
        assert_eq!(Agent::Mcode.bare_id("mvs_abc123"), "abc123");
        assert_eq!(Agent::Mcode.full_id("abc123"), "mvs_abc123");
        assert_eq!(Agent::Claude.bare_id("session_x"), "session_x");
        assert_eq!(Agent::Codex.full_id(id), id);
        assert_eq!(Agent::Qwen.bare_id(id), id);
        assert_eq!(Agent::Qwen.full_id(id), id);
        assert_eq!(Agent::Copilot.bare_id(id), id);
        assert_eq!(Agent::Opencode.bare_id("ses_abc"), "abc");
        assert_eq!(Agent::Opencode.bare_id("se"), "");
        assert_eq!(Agent::Opencode.full_id("abc"), "ses_abc");
    }
}
