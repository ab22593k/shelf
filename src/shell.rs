use anyhow::Result;
use clap_complete::{generate, Generator};

/// Generates a shell completion script as a UTF-8 string.
///
/// # Arguments
///
/// * `gen` - The shell completion generator to use
/// * `cmd` - The clap Command to generate completions for
///
/// # Returns
///
/// A Result containing the UTF-8 completion script string
pub fn completions_script<G: Generator>(r#gen: G, cmd: &mut clap::Command) -> Result<String> {
    let bin_name = cmd.get_bin_name().unwrap_or("shelf").to_string();

    let mut completion_buf = Vec::new();
    generate::<G, _>(r#gen, cmd, bin_name, &mut completion_buf);

    String::from_utf8(completion_buf).map_err(Into::into)
}
