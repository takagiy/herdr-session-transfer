//! Which herdr agents this plugin can transfer, and their txcript harnesses.

use txcript::HarnessId;

/// One transferable harness. Add a row here to support another harness.
struct Harness {
    /// The agent name herdr reports for a pane.
    agent: &'static str,
    id: HarnessId,
    /// The name users know it by.
    name: &'static str,
}

const HARNESSES: &[Harness] = &[
    Harness {
        agent: "claude",
        id: HarnessId::ClaudeCode,
        name: "Claude Code",
    },
    Harness {
        agent: "codex",
        id: HarnessId::Codex,
        name: "Codex",
    },
];

/// Every listed harness, in table order.
pub fn all() -> impl Iterator<Item = HarnessId> {
    HARNESSES.iter().map(|h| h.id)
}

/// The name users know `harness` by, or txcript's name when it is not listed.
pub fn display_name(harness: HarnessId) -> &'static str {
    HARNESSES
        .iter()
        .find(|h| h.id == harness)
        .map_or(harness.as_str(), |h| h.name)
}

/// The harness of the agent herdr calls `agent`.
pub fn harness_for_agent(agent: &str) -> Option<HarnessId> {
    HARNESSES.iter().find(|h| h.agent == agent).map(|h| h.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_agent_gives_its_harness() {
        // Given herdr names Claude Code's agent "claude"
        // When the table looks it up
        let harness = harness_for_agent("claude");
        // Then it gives txcript's Claude Code harness
        assert_eq!(harness, Some(HarnessId::ClaudeCode));
    }

    #[test]
    fn unlisted_agent_has_no_harness() {
        // Given herdr names an agent the table does not list
        // When the table looks it up
        let harness = harness_for_agent("devin");
        // Then it has no harness for it
        assert_eq!(harness, None);
    }

    #[test]
    fn listed_harness_has_a_display_name() {
        // Given the table lists Codex
        // When the picker asks for its display name
        let name = display_name(HarnessId::Codex);
        // Then the table gives the name users know it by
        assert_eq!(name, "Codex");
    }
}
