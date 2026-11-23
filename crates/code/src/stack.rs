//! The technology a code project is built with. The agent proposes it after asking the user a
//! few questions, or the user simply names it; either way it is recorded here so every later
//! session builds on the same choice. All text is data from an untrusted source and is bounded.

use serde::{Deserialize, Serialize};

use crate::{CodeError, fail, plain_text};

const MAX_FIELD_CHARS: usize = 120;
const MAX_NOTES_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Web,
    Ios,
    Android,
    Desktop,
    Cli,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    /// The user named the stack; the agent did not ask.
    User,
    /// The agent asked questions and the user agreed to what it proposed.
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    /// How the user would say it, for example "Next.js with Postgres".
    pub name: String,
    pub target: Target,
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub framework: String,
    /// Why this stack, what the user answered, what was ruled out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub decided_by: DecidedBy,
}

impl Stack {
    pub fn validate(&self) -> Result<(), CodeError> {
        for (what, text) in [
            ("the stack name", &self.name),
            ("the language", &self.language),
        ] {
            if text.trim().is_empty() {
                return fail(format!("{what} is empty"));
            }
        }
        for (what, text) in [
            ("the stack name", &self.name),
            ("the language", &self.language),
            ("the framework", &self.framework),
        ] {
            plain_text(what, text, MAX_FIELD_CHARS, false)?;
        }
        plain_text("the notes", &self.notes, MAX_NOTES_CHARS, true)
    }
}

/// The questions an agent asks before it proposes a stack. Short on purpose: each answer changes
/// the choice, and a user who already knows what they want skips them all.
pub const QUESTIONS: &[&str] = &[
    "Where will it run: a website, an iPhone or Android app, a desktop app, or a command line tool?",
    "Who uses it, and what is the one thing it must do well on day one?",
    "Does it need accounts, a database, payments or files from the user?",
    "Is there code, a language or a framework you already use or must use?",
    "Where will it be hosted or published, and is there a budget or a deadline?",
    "Anything you have used before and disliked?",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn next() -> Stack {
        Stack {
            name: "Next.js".into(),
            target: Target::Web,
            language: "TypeScript".into(),
            framework: "Next.js".into(),
            notes: "SEO matters\nhosted on Vercel".into(),
            decided_by: DecidedBy::Agent,
        }
    }

    #[test]
    fn a_normal_stack_is_valid() {
        assert!(next().validate().is_ok());
    }

    #[test]
    fn empty_oversized_and_control_text_is_refused() {
        assert!(
            Stack {
                name: " ".into(),
                ..next()
            }
            .validate()
            .is_err()
        );
        assert!(
            Stack {
                language: String::new(),
                ..next()
            }
            .validate()
            .is_err()
        );
        assert!(
            Stack {
                framework: "a\u{7}b".into(),
                ..next()
            }
            .validate()
            .is_err()
        );
        assert!(
            Stack {
                notes: "x".repeat(2_001),
                ..next()
            }
            .validate()
            .is_err()
        );
        assert!(
            Stack {
                name: "a\nb".into(),
                ..next()
            }
            .validate()
            .is_err()
        );
    }
}
