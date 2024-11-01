# Shelf - Dotconf Manager

[![Shelf CI](https://github.com/ab22593k/shelf/actions/workflows/ci.yml/badge.svg)](https://github.com/ab22593k/shelf/actions/workflows/ci.yml)

Shelf is a command-line tool for managing dotconf files. It allows you to track, list, remove files with ease.

## Features

- Track dotconf files from anywhere in your file system recursively.
- List all tracked dotfiles.
- Remove dotconf files recursively from database.

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

## Shell Completion

Shelf supports generating shell completion scripts for various shells. You can generate these scripts using the `completion` subcommand:

```bash
# Generate completion script for Bash
slf completion bash > shelf.bash

# Generate completion script for Zsh
slf completion zsh > _shelf

# Generate completion script for Fish
slf completion fish > shelf.fish
```

To use the completion scripts:

- For Bash, add the following line to your `~/.bashrc`:
  ```bash
  source /path/to/shelf.bash
  ```

- For Zsh, place the `_shlf` file in `~/.zfunc`, then add `source ~/.zfunc/*` in `~/.zshrc`.

- For Fish, place the `shlf.fish` file in `~/.config/fish/completions`.

After setting up the completion script, restart your shell or source the respective configuration file to enable completions for the `slf` command.

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
