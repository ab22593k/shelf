use crate::utils::{harvest_staged_changes, spin_progress};
use anyhow::{Context, Result, anyhow};
use clap::Args;
use rig::{
    agent::{Agent, AgentBuilder},
    client::{CompletionClient, ProviderClient},
    completion::{CompletionModel, Prompt, PromptError},
    providers::gemini,
};

const AGENT_PREAMBLE: &str = r#"You are a helpful code reviewer examining code changes.
Based on the following code diff, provide a detailed code review with constructive feedback,
focusing on code quality, maintainability, and potential issues or improvements.
"#;

const REVIEW_TEMPLATE: &str = r#"**Instructions for Review:** "Analyze the code thoroughly across the following key dimensions, providing findings, explanations, and clear suggestions for improvement for each relevant point. If a section is not applicable, briefly state why. Prioritize issues by their impact (Critical, High, Medium, Low)."

### 1. Code Structure and Design Principles
- **KISS Principle**: Evaluate adherence to the KISS principle. Is the code overly complex or complicated?
- **Modularity, Cohesion, and Coupling**: Assess modularity, cohesion (single, clearly defined function), and coupling (minimal dependencies).
- **Reusability**: Check for reusability. Does the code duplicate logic or data?
- **Composition vs. Inheritance**: Review composition vs. inheritance. Is composition preferred for polymorphism, or is inheritance used appropriately?
- **Design Smells**: Look for:
    - Cyclic dependencies (circular dependencies).
    - Feature density (component implements more than one functionality) or Feature envy.
    - Unstable dependencies (component depends on a less stable one).
    - Mashed components (single functionality scattered).
    - Ambiguous interfaces (unclear APIs or similar signatures).
    - Mesh components (heavily coupled without clear patterns).
    - First lady components (God components, with too much logic/functionality).
    - Bossy components (delegating all responsibility without adding value).
- **Control Structures**: Evaluate clarity of control structures and avoid unconditional jumps (like GOTOs).
- **Premature Optimization**: Consider premature optimization. Is optimization being done before it's necessary, potentially compromising design?

### 2. Data Structures
- **Appropriateness**: Are data structures appropriately chosen for the problem, considering data size, frequency of changes, search/sorting operations, uniqueness, and relationships between elements?
- **Performance**: Evaluate memory and time performance implications of the chosen data structures.
- **Usage**: Check if data structures are being forced to do operations not natively supported.

### 3. Naming and Formatting Conventions
- **Naming**: Assess if names (variables, functions, classes, packages) are meaningful, descriptive, and consistent within their context and across the codebase.
- **Keywords**: Identify usage of keywords as variable names.
- **Hardcoding**: Look for magic numbers or excessive hardcoding.
- **Parameterization**: Review parameterization: are there too many arguments to functions/methods?
- **Access Modifiers**: Check appropriate use of access modifiers (e.g., private, protected).
- **Formatting**: Evaluate formatting: line spacing, indentation (e.g., 4 whitespaces for Python blocks), and whitespace around operators and punctuation.

### 4. Comments and Documentation
- **Quality**: Are comments coherent, complete, and accurate with the code they describe, avoiding lies or outdated information?
- **Contracts**: Are pre- and post-conditions for methods/functions clearly described?
- **Types**: Are input and output types clearly defined and documented, especially for dynamically typed languages?
- **Error Handling**: Are exceptions and errors documented?
- **Inline Comments**: Is the use of inline comments appropriate, or are there too many/too few?
- **TODOs/FIX-MEs**: Are TODOs/FIX-ME comments present in production-ready code?
- **Grammar/Clarity**: Are comments free of typos, grammar errors, and texting abbreviations?
- **Relevance**: Do comments describe the current code only, not past versions or complaints?

### 5. Concurrency, Parallelism, and Performance
- **Thread Safety**: Is the code thread-safe? Are race conditions, deadlocks, and starvation avoided?
- **Locking**: Are locking mechanisms used properly where shared resources are accessed?
- **Immutability/Statelessness**: Are immutable objects truly immutable? Are stateless objects used where appropriate?
- **ACID Properties**: If applicable, evaluate ACID properties (Atomicity, Consistency, Isolation, Durability) for transactions.
- **Parallelism Suitability**: If the code involves parallelism, is it a suitable problem for parallelization? Consider task and data granularity, locality, and load balancing.
- **Bottlenecks**: Are potential performance bottlenecks identified and considered?

### 6. Security
- **CIA Triad**: Assess adherence to CIA triad (Confidentiality, Integrity, Availability).
- **Core Principles**: Review against core security principles: Least Privilege, Defense in Depth, Segregation of Duties (SoD), Fail Safe, Complete Mediation, Least Common Mechanism, Weakest Link.
- **Vulnerabilities**: Identify common security vulnerabilities:
    - Sensitive/private/confidential information logging or disclosure.
    - Hard-coded passwords or security keys.
    - Weak ciphers or inadequate encryption.
    - Lack of input validation against common attacks (e.g., SQL injection, XSS).
    - Missing or inadequate authentication/authorization mechanisms.
    - Backdoors or security by obscurity.
    - Audit trails presence.

### 7. Overall Design Alignment
- **Architecture**: Does the implementation reflect the intended architecture and problem statement?
- **Problem-Solving**: Does the code solve the right problem and meet functional and non-functional requirements (FURPS+)?
- **Technology Choices**: Are the technology choices adequate for the problem?
"#;

/// Handles code review functionality by generating prompts for a completion model.
///
/// It encapsulates the logic for building a review-specific prompt and interacting
/// with the underlying AI agent. This struct follows the builder pattern for configuration.
pub struct Reviewer<M: CompletionModel> {
    agent: Agent<M>,
    diff: Option<String>,
}

impl<M: CompletionModel> Reviewer<M> {
    /// Creates a new `Reviewer` with the given completion model.
    ///
    /// # Arguments
    ///
    /// * `model` - A `CompletionModel` that will be used to generate the review.
    pub fn new(model: M) -> Self {
        Self {
            agent: AgentBuilder::new(model).preamble(AGENT_PREAMBLE).build(),
            diff: None,
        }
    }

    /// Sets the code diff to be reviewed.
    ///
    /// This method consumes the `Reviewer` and returns a new instance with the
    /// diff configured, enabling the builder pattern.
    ///
    /// # Arguments
    ///
    /// * `diff` - A string slice containing the code changes to be reviewed.
    pub fn with_diff(mut self, diff: &str) -> Self {
        self.diff = Some(diff.to_string());
        self
    }

    /// Generates the code review by sending the constructed prompt to the agent.
    ///
    /// The final prompt is composed of a standard review template and the diff
    /// provided via `with_diff`.
    ///
    /// # Returns
    ///
    /// A `Result` which is `Ok` with the review string on success, or a `PromptError` on failure.
    pub async fn review(&self) -> Result<String, PromptError> {
        let diff_content = self
            .diff
            .as_deref()
            .unwrap_or("No diff was provided for review.");

        let prompt_body = format!("{REVIEW_TEMPLATE}\n\n<diff>\n{diff_content}\n</diff>",);

        self.agent.prompt(prompt_body).await
    }
}

#[derive(Args)]
pub struct ReviewCommand {
    /// Override the configured model.
    #[arg(short, long, default_value = "gemini-2.0-flash")]
    pub model: String,
}

pub async fn run(args: ReviewCommand) -> Result<()> {
    let reviews = handle_review_action(args.model.as_str()).await?;
    println!("{reviews}");
    Ok(())
}

async fn handle_review_action(model: &str) -> Result<String> {
    let agent = gemini::Client::from_env();

    let diff = harvest_staged_changes().context("Getting staged changes failed")?;

    let msg = spin_progress(|| async {
        let model = agent.completion_model(model);
        let reviewer = Reviewer::new(model).with_diff(&diff);

        reviewer.review().await.map_err(|e| anyhow!(e))
    })
    .await?;

    Ok(msg)
}
