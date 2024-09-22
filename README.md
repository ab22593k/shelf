# Shelf - Dotfile Manager

[![Shelf CI](https://github.com/ab22593k/shelf/actions/workflows/ci.yml/badge.svg)](https://github.com/ab22593k/shelf/actions/workflows/ci.yml)

Shelf is a command-line tool for managing and syncing dotfiles across different machines. It allows you to track, list, remove, and sync your dotfiles with ease.

## Features

- Track dotfiles from anywhere in your file system
- List all tracked dotfiles
- Remove dotfiles from tracking
- Sync dotfiles to a specified directory or default location

## Installation

To install Shelf, you need to have Rust and Cargo installed on your system. If you don't have them, you can install them from [rustup.rs](https://rustup.rs/).

Once you have Rust and Cargo installed, you can build and install Shelf using the following command:

```
cargo install --path .
```
## Usage

Shelf provides several commands to manage your dotfiles:

```bash
# Add a new dotfile to track
shlf add ~/.bashrc

# List all tracked dotfiles
shlf ls

# Remove a dotfile from tracking
shlf rm .bashrc

# Create symlinks for all tracked dotfiles
shlf link

# Interactive selection of dotfiles to track
shlf suggest

# Show help
shlf --help
```

Each command can be run with `-h` or `--help` for more information.

# Push dotfiles to a remote Git repository
shlf push --repo username/dotfiles --token your_access_token --branch main

# Pull dotfiles from a remote Git repository
shlf pull

## Shell Completion

Shelf supports generating shell completion scripts for various shells. You can generate these scripts using the `completion` subcommand:

```bash
# Generate completion script for Bash
shlf completion bash > shlf.bash

# Generate completion script for Zsh
shlf completion zsh > _shlf

# Generate completion script for Fish
shlf completion fish > shlf.fish
```

To use the completion scripts:

- For Bash, add the following line to your `~/.bashrc`:
  ```bash
  source /path/to/shlf.bash
  ```

- For Zsh, place the `_shlf` file in `~/.zfunc`, don't forget to add `source ~/.zfunc/*` in `~/.zshrc`.

- For Fish, place the `shlf.fish` file in `~/.config/fish/completions`.

After setting up the completion script, restart your shell or source the respective configuration file to enable completions for the `shlf` command.

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

Replace `[SUBCOMMAND]` with the command you want to run, such as `add`, `ls`, `rm`, or `link`.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
