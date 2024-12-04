# Shelf: AI-based command-line tools for developers

[![Shelf CI](https://github.com/ab22593k/shelf/actions/workflows/ci.yml/badge.svg)](https://github.com/ab22593k/shelf/actions/workflows/ci.yml)

Shelf is a command-line tool for managing books or what we called `dotfile`s in the system, generating git commit
messages, and reviewing code using AI. It provides a simple interface to track files across your system and
integrates with multiple AI providers to automatically generate meaningful commit messages through git hooks
and perform comprehensive code reviews. With support for local and cloud-based AI models, Shelf makes configuration
files management, git commits, and code reviews effortless.

## Features

- Track dotfiles from anywhere in your file system recursively
- List all tracked dotfiles
- Remove dotfiles recursively from database
- AI-powered git commit message generation with multiple providers:
  - Groq
  - Google Gemini
  - Anthropic Claude
  - OpenAI
  - Xai grok
  - Ollama (local)
- Git hooks integration for automatic commit message generation

## Installation

To install Shelf, you need to have Rust and Cargo installed on your system. If you don't have them,
you can install them from [rustup.rs](https://rustup.rs/).

Once you have Rust and Cargo installed, you can build and install Shelf using the following command:

```
cargo install --path .
```
## Usage

Shelf provides commands for both dotfile management and git integration:

### Dotfile Management

```bash
# Add a new dotfile to track
shelf bo tarck ~/.bashrc

# List all tracked dotfiles
shelf bo list

# Remove a dotfile from tracking
shelf bo untarck ~/.bashrc

# Interactive selection of dotfiles to track
shelf bo suggest -i

# Show help
shelf --help
```

Each command can be run with `-h` or `--help` for more information.

### Git AI Integration

The `ai` subcommand provides AI-powered features:

```bash
# Configure AI provider
shelf ai config set provider openai
shelf ai config set openai_api_key "your-api-key"

# Use specific provider for one commit
shelf ai commit -p openai

# List current configuration
shelf ai config list
```

The AI-powered features support diffrent AI providers:
- **Groq** (default): GroqCloud-based models
- **Google Gemini**: Cloud-based using Gemini models
- **OpenAI**: Cloud-based using GPT models
- **Anthropic Claude**: Cloud-based using Claude models
- **XAI Grok**: Cloud-based using Grok models
- **Ollama**: Local, privacy-friendly AI using models like Qwen

The git hook integrates seamlessly with your normal git workflow:
```bash
# Hook will automatically generate message if none provided
git commit

# Your message takes precedence
git commit -m "feat: your message"

# AI helps with amending
git commit --amend
```

### Code Review with AI

Shelf can assist in code review by analyzing pull requests and providing AI-powered feedback:

```bash
# Review the current staged branch's changes
shelf ai review
```

The AI review provides:
- Code quality analysis
- Potential bug detection
- Style guide compliance checks
- Security vulnerability scanning
- Performance improvement suggestions
- Best practice recommendations

## Migration from v0.8.7 to newer versions

If you're upgrading from a v0.8.7 version of Shelf, here are the key changes and migration steps:

### Migration Steps

1. Convert your existing config:
```bash
# Migration hints
shelf migrate

# Apply changes
shelf migrate --fix
```

## Prompts

Prompt templates for commit messages and code reviews are stored in the user's configuration directory.
You can customize these templates to tailor the AI's output to your specific needs.


## Shell Completion

Shelf supports generating shell completion scripts for various shells. You can generate these
scripts using the `completion` subcommand:

```bash
# Generate completion script for Bash
shelf completion bash > shelf.bash

# Generate completion script for Zsh
shelf completion zsh > _shelf

# Generate completion script for Fish
shelf completion fish > shelf.fish
```

To use the completion scripts:

- For Bash, add the following line to your `~/.bashrc`:

  ```bash
  source /path/to/shelf.bash
  ```

- For Zsh, place the `_shelf` file in `~/.zfunc`, then add `source ~/.zfunc/_shelf` in `~/.zshrc`.

- For Fish, place the `shelf.fish` file in `~/.config/fish/completions`.

After setting up the completion script, restart your shell or source the respective configuration file to enable completions for the `shelf` command.

## Configuration

AI settings are stored in `~/.config/shelf/ai.json` (or `$XDG_CONFIG_HOME/shelf/ai.json` if set). You can configure:

- `provider`: AI provider to use (`openai`, `anthropic`, `gemini`, `groq`, `xai` and `ollama`)
- `model`: Ollama model to use (default: `qwen2.5-coder`)
- `openai_api_key`: OpenAI API key for GPT models
- `ollama_host`: Ollama server URL (default: `http://localhost:11434`)

Example configuration:
```json
{
  "provider": "ollama",
  "model": "qwen2.5-coder",
  "ollama_host": "http://localhost:11434" # Only if you are using custom host,
}
```
## Development

To build the project locally:

```
cargo build
```
To run tests:

```
cargo test
```
To run the project directly without installing:

```
cargo run --bin shelf -- [SUBCOMMAND]
```

Replace `[SUBCOMMAND]` with the command you want to run, such as `dotfile` or `ai`.

## Contributing

Contributions are welcome! Please feel free tor submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
