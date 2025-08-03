use crate::utils::{harvest_staged_changes, spin_progress};
use anyhow::{Context, Result, anyhow};
use clap::Args;
use rig::{client::builder::DynClientBuilder, completion::Prompt};

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

#[derive(Args)]
pub struct ReviewCommand {
    /// Override the configured provider.
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,
    /// Override the configured model.
    #[arg(short, long, default_value = "gemini-2.0-flash")]
    pub model: String,
    /// Aspects of the code review to focus on.
    #[arg(short, long, default_value = None, value_delimiter = ',', num_args = 1..)]
    pub focus: Option<Vec<String>>,
}

pub(super) async fn run(args: ReviewCommand) -> Result<()> {
    // Pass the entire args struct to review_action
    let reviews = review_action(args).await?;
    println!("{reviews}");
    Ok(())
}

// Modify review_action to accept ReviewCommand directly
async fn review_action(args: ReviewCommand) -> Result<String> {
    let client = DynClientBuilder::new();
    let agent = client
        // Access provider and model from the args struct
        .agent(args.provider.as_str(), args.model.as_str())?
        .preamble(AGENT_PREAMBLE)
        .context(REVIEW_TEMPLATE)
        .context(
            &args
                .focus
                .map(|f| {
                    let mut s = f.join("\n- ");
                    s.insert_str(0, "- ");
                    s.insert_str(0, "Focusing on the following aspects:\n");
                    s
                })
                .unwrap_or_else(|| "Review all aspects of the code.".to_string()),
        )
        .temperature(0.2)
        .build();

    let diff = harvest_staged_changes().context("Getting staged changes failed")?;

    let msg = spin_progress(|| async {
        let reviews = agent.prompt(diff).await;

        reviews.map_err(|e| anyhow!(e))
    })
    .await?;

    Ok(msg)
}
