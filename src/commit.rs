use anyhow::Result;
use rig::{
    agent::{Agent, AgentBuilder},
    completion::{CompletionModel, Prompt, PromptError},
    message::Message,
};
use tracing::debug;

const PREAMBLE: &str = r#"You are an expert software developer writing a Git commit message.
Based on the following code diff and the project's commit history, complete the following commit
message prefix. Pay attention to the style and conventions used in previous commits.
"#;

const PROMPT_PREFIX: &str = r#"Write a concise and meaningful commit message.

Consider these guidelines:
- Use the imperative mood ('add', 'fix', 'update', not 'adds', 'fixed', 'updating')
- Start with a clear action verb
- Include the scope of the change (e.g., module or component) in the subject line if applicable
- Keep each line of the message under 80 characters for readability
- Keep the first line under 50 characters
- Explain what and why in the body if needed
- Reference related issues in a footer

Your response should be only the commit message text with no additional formatting or markup.
"#;

pub struct MsgCompletion<M: CompletionModel> {
    agent: Agent<M>,
    pub prompt: String,
}

impl<M: CompletionModel> MsgCompletion<M> {
    pub fn new(model: M) -> Self {
        Self {
            agent: AgentBuilder::new(model.clone()).preamble(PREAMBLE).build(),
            prompt: String::from(PROMPT_PREFIX),
        }
    }

    pub fn with_diff(mut self, diff: &str) -> Self {
        self.prompt.push_str(&format_diff_section(diff));
        self
    }

    pub fn with_history(mut self, commit_history: Vec<String>) -> Self {
        self.prompt
            .push_str(&format_history_section(&commit_history));
        self
    }

    pub fn with_issue(mut self, issue: &Option<usize>) -> Self {
        if let Some(ref_num) = issue {
            self.prompt.push_str(&format_issue_reference(*ref_num));
        }
        self
    }
}

fn format_diff_section(diff: &str) -> String {
    format!(
        "The code changes being committed are as follows:\n<diff>\n{}\n</diff>\n\n",
        diff
    )
}

fn format_history_section(history: &[String]) -> String {
    format!(
        "Relevant previous commit messages:\n<history>\n{}\n</history>\n\n",
        history.join("\n")
    )
}

fn format_issue_reference(n: usize) -> String {
    format!(
        "The issue reference should be always added as a footer with: Fixes #{}\n\n",
        n
    )
}

impl<M: CompletionModel> Prompt for MsgCompletion<M> {
    async fn prompt(&self, prompt: impl Into<Message> + Send) -> Result<String, PromptError> {
        debug!(
            "Generating commit message from prompt:\n{}\n\n",
            self.prompt
        );
        self.agent.prompt(prompt).await
    }
}
