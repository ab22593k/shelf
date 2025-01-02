Based on Jetberains research [paper](https://arxiv.org/pdf/2308.07655), which focuses on Large Language
Models (LLMs) for generating Git commit messages, here are a few system prompt options, ranging in
complexity and focus:

**Option 1 (Simple Zero-Shot Baseline):**

```
You are a helpful assistant that writes Git commit messages based on the provided diff.
Complete the following commit message prefix based on the given commit diff.
```

**Reasoning:** This is directly inspired by the paper's zero-shot baseline approach.
It's simple and relies on the LLM's general understanding of code changes and commit message conventions.
It's a good starting point to see how well the LLM performs without much guidance.

**Option 2 (Adding Context about the Task):**

```
You are an expert software developer tasked with writing clear and concise Git commit messages.
Given a code diff, complete the following partially written commit message. The commit message
should summarize the changes made in the diff.
```

**Reasoning:** This prompt provides a bit more context about the desired role and the goal of
the commit message. It emphasizes clarity and conciseness, which are important aspects of good
commit messages.

**Option 3 (Emphasizing Commit Message Conventions):**

```
You are a helpful assistant that generates Git commit messages following common conventions.
Given a code diff, complete the following commit message prefix. The commit message should
start with a verb in the imperative mood and briefly describe the change.
```

**Reasoning:** This prompt incorporates the common practice of starting commit messages with
an imperative verb (e.g., "Fix", "Add", "Refactor"). This can help the LLM generate messages
that adhere to established best practices.

**Option 4 (Leveraging Previous Commit History - if available):**

```
You are an expert software developer writing a Git commit message. Based on the following
code diff and the project's commit history, complete the following commit message prefix.
Pay attention to the style and conventions used in previous commits.

**Previous Commit Message:** [Insert a relevant previous commit message here]
```

**Reasoning:**  The paper highlights the potential of using commit history to improve commit
message generation. This prompt explicitly instructs the LLM to consider the style of past commits,
potentially leading to more consistent and project-specific messages. You'd need to dynamically
insert a relevant previous commit message into the prompt.

**Option 5 (Focusing on Diversity - if you want less conventional messages):**

```
You are a creative software developer writing a Git commit message. Based on the following code diff,
complete the following commit message prefix in a descriptive and informative way, even if it doesn't
strictly follow traditional commit message conventions.
```

**Reasoning:**  The paper discusses the restrictiveness of common commit message filters.
This prompt encourages the LLM to be more flexible and descriptive, potentially generating
messages that capture more nuance, even if they deviate from strict formatting rules.

**Key Considerations Based on the Paper:**

* **Completion vs. Generation:** The paper found that commit message *completion* (providing a prefix)
  is generally easier for models than generating from scratch. Therefore, all these prompts are designed
  for a completion task.
* **History Matters:** The research suggests that incorporating commit history can improve generation
  quality, especially for longer messages. Option 4 explicitly leverages this.
* **Diversity is Challenging:** The paper notes that generating messages that fall outside common
  filtering criteria is harder for current models. Option 5 is an attempt to explore this area.
* **LLMs Can Be Verbose:** The paper observed that GPT-3.5-turbo tends to generate longer messages.
  You might need to add constraints on length if you prefer shorter messages.
* **Zero-Shot is a Starting Point:** The paper focused on zero-shot learning. You could potentially
  improve results with few-shot prompting (providing examples of good commit messages).

**How to Use These Prompts:**

You would combine these system prompts with the actual diff of your code changes and any existing
commit message prefix you want to complete. For example:

```
**System Prompt (Option 2):** You are an expert software developer tasked with writing clear and
concise Git commit messages. Given a code diff, complete the following partially written commit message.
The commit message should summarize the changes made in the diff.

**User Prompt:**
```diff
--- a/src/main.rs
+++ b/src/main.rs
fn main() {
-    println!("Hello, world!");
+    println!("Hello, Rust!");
}
```

**Commit Message Prefix:** Fix: Update greeting to
```

The LLM would then complete the prefix based on the diff.

**Recommendation:**

Start with **Option 1** as a baseline. Then, experiment with **Option 2** or **Option 3** to see if
providing more context or emphasizing conventions improves the results. If you have access to commit
history, **Option 4** is worth trying. **Option 5** is for more experimental use if you want to explore
less conventional commit messages.

Remember to evaluate the quality of the generated messages and adjust your prompts accordingly.
The "best" prompt will likely depend on your specific project, team conventions, and desired level
of detail in commit messages.
