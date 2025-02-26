# Shelf: Empower Development Journey with AI-Driven Tools

[![Shelf CI](https://github.com/ab22593k/shelf/actions/workflows/ci.yml/badge.svg)](https://github.com/ab22593k/shelf/actions/workflows/ci.yml)

Shelf is a command-line tool designed to simplify configuration file management and enhance your git
workflow with AI-powered features. It enables you to:

* Manage configuration files effectively across your system
* Automatically generate meaningful git commit messages using AI
* Obtain comprehensive, AI-driven code reviews

Integrated with git hooks and supporting multiple AI providers, Shelf adapts seamlessly to your development workflow.

---

## Usage

### File Management Commands

Track files:
```bash
shelf file track ~/.bashrc
```

List tracked files:
```bash
shelf file list
```

List only modified files:
```bash
shelf file list --dirty
```

Untrack files:
```bash
shelf file untrack ~/.bashrc
```

Save current changes:
```bash
shelf file save
```

### AI-Powered Git Commands

Generate a commit message for staged changes:
```bash
shelf commit [--model MODEL] [--fixes ISSUE] [--history N]
```

Review staged changes:
```bash
shelf review [--model MODEL]
```

### Configuration

Configure your AI provider (default is Google Gemini):
```bash
export GEMINI_API_KEY="your-key"
```

### Shell Completion

Generate shell completions:
```bash
# For bash
shelf completion bash

# For zsh
shelf completion zsh

# For fish
shelf completion fish
```

---

## Contributing

Contributions are welcome! Please submit a Pull Request with your improvements.

---

## License

Shelf is licensed under the MIT License. See the LICENSE file for details.
