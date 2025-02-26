use anyhow::Result;
use rig::{
    agent::{Agent, AgentBuilder},
    completion::{CompletionModel, Prompt, PromptError},
    message::Message,
};

const AGENT_PREAMBLE: &str = r#"You are a helpful code reviewer examining code changes.
Based on the following code diff, provide a detailed code review with constructive feedback,
focusing on code quality, maintainability, and potential issues or improvements.
"#;

const REVIEW_TEMPLATE: &str = r#"**Project:** [Project Name], **PR/Commit:** [PR/Commit #]
**Changes:** [Brief Description]
**Focus:** [Key areas to review, e.g., security, performance, logic]

**Review:**

1.  **Correctness:** Verify functionality, edge cases, bugs.
2.  **Readability:** Assess clarity, style, comments.
3.  **Performance:** Check for bottlenecks, efficiency.
4.  **Security:** Identify vulnerabilities, data handling.
5.  **Tests:** Evaluate coverage, quality.
6.  **Standards:** Ensure adherence to project guidelines.
7.  **Dependencies:** Validate necessity, licensing.

**Feedback:**

* File: [File], Line: [Line], [Type: Suggestion/Question/Concern/Praise]: [Comment]
* Prioritize feedback by severity.
* Provide actionable suggestions.
"#;

// Review struct handles code review functionality using a completion model
pub struct Reviewer<M: CompletionModel> {
    agent: Agent<M>,
    pub prompt: String,
}

impl<M: CompletionModel> Reviewer<M> {
    pub fn new(model: M) -> Self {
        Self {
            agent: AgentBuilder::new(model.clone())
                .preamble(AGENT_PREAMBLE)
                .build(),
            prompt: String::from(REVIEW_TEMPLATE),
        }
    }

    pub fn with_diff(mut self, diff: &str) -> Self {
        self.prompt
            .push_str(&format!("<diff>\n{}\n</diff>\n\n", diff));
        self
    }
}

impl<M: CompletionModel> Prompt for Reviewer<M> {
    async fn prompt(&self, prompt: impl Into<Message> + Send) -> Result<String, PromptError> {
        let msg = self.agent.prompt(prompt).await?;
        Ok(msg)
    }
}
