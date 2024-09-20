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

Here are some common commands you can use with Shelf:

1. Track a new dotfile:
   ```
   slf track /path/to/your/dotfile
   ```

2. List all tracked dotfiles:
   ```
   slf list
   ```

3. Remove a dotfile from tracking:
   ```
   slf remove dotfile_name
   ```

4. Sync all dotfiles:
   ```
   slf sync
   ```

   To sync to a custom directory:
   ```
   slf sync --outdir /path/to/custom/directory
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
cargo run -- [ARGUMENTS]
```

Replace `[ARGUMENTS]` with the command you want to run, such as `track`, `list`, `remove`, or `sync`.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
