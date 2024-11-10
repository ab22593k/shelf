# Shelf - Dotconf Manager

[![Shelf CI](https://github.com/ab22593k/shelf/actions/workflows/ci.yml/badge.svg)](https://github.com/ab22593k/shelf/actions/workflows/ci.yml)

Shelf is a command-line tool for managing dotconf files and generating git
commit messages using AI. It provides a simple interface to track dotfiles
across your system and integrates with multiple AI providers to automatically
generate meaningful commit messages through git hooks. With support for local
and cloud-based AI models, Shelf makes both dotfile management and git commits effortless.

## Features

- Track dotconf files from anywhere in your file system recursively
- List all tracked dotfiles
- Remove dotconf files recursively from database
- AI-powered git commit message generation with multiple providers:
  - Ollama (default, local)
  - OpenAI
  - Anthropic Claude
  - Google Gemini
- Git hooks integration for automatic commit message generation

## Installation

To install Shelf, you need to have Rust and Cargo installed on your system. If you don't have them, you can install them from [rustup.rs](https://rustup.rs/).

Once you have Rust and Cargo installed, you can build and install Shelf using the following command:

```
cargo install --path .
```
## Usage

Shelf provides commands for both dotfile management and git integration:

### Dotfile Management

```bash
# Add a new dotfile to track
slf cp ~/.bashrc

# List all tracked dotfiles
slf ls

# Remove a dotfile from tracking
slf rm ~/.bashrc

# Interactive selection of dotfiles to track
slf suggest -i

# Show help
slf --help
```

Each command can be run with `-h` or `--help` for more information.

### Git AI Integration

The `gitai` subcommand provides AI-powered git commit message generation:

```bash
# Generate commit message for staged changes
slf gitai commit

# Install git hook for automatic message generation
slf gitai commit --install

# Remove git hook
slf gitai commit --uninstall

# Configure AI provider
slf gitai config set provider openai
slf gitai config set openai_api_key "your-api-key"

# Use specific provider for one commit
slf gitai commit -p openai

# List current configuration
slf gitai config list
```

The GitAI features support multiple AI providers:
- **Ollama** (default): Local, privacy-friendly AI using models like Qwen
- **OpenAI**: Cloud-based using GPT models
- **Anthropic Claude**: Cloud-based using Claude models
- **Google Gemini**: Cloud-based using Gemini models

The git hook integrates seamlessly with your normal git workflow:
```bash
# Hook will automatically generate message if none provided
git commit

# Your message takes precedence
git commit -m "feat: your message"

# AI helps with amending
git commit --amend
```

## Shell Completion

Shelf supports generating shell completion scripts for various shells. You can generate these scripts using the `completion` subcommand:

```bash
# Generate completion script for Bash
slf completion bash > slf.bash

# Generate completion script for Zsh
slf completion zsh > _slf

# Generate completion script for Fish
slf completion fish > slf.fish
```

To use the completion scripts:

- For Bash, add the following line to your `~/.bashrc`:

  ```bash
  source /path/to/slf.bash
  ```

- For Zsh, place the `_slf` file in `~/.zfunc`, then add `source ~/.zfunc/_slf` in `~/.zshrc`.

- For Fish, place the `slf.fish` file in `~/.config/fish/completions`.

After setting up the completion script, restart your shell or source the respective configuration file to enable completions for the `slf` command.

## Configuration

GitAI settings are stored in `~/.config/shelf/gitai.json` (or `$XDG_CONFIG_HOME/shelf/gitai.json` if set). You can configure:

- `provider`: AI provider to use (`ollama`, `openai`, `anthropic`, `gemini`)
- `ollama_host`: Ollama server URL (default: `http://localhost:11434`)
- `ollama_model`: Ollama model to use (default: `qwen2.5-coder`)
- `openai_api_key`: OpenAI API key for GPT models
- `project_context`: Optional project-specific context for better commits

Example configuration:
```json
{
  "provider": "ollama",
  "ollama_host": "http://localhost:11434",
  "ollama_model": "qwen2.5-coder",
  "project_context": "Rust CLI tool for dotfile management"
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
cargo run -- [SUBCOMMAND]
```

Replace `[SUBCOMMAND]` with the command you want to run, such as `cp`, `ls`, or `rm`.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
