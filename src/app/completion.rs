use anyhow::{Context, Result};
use clap::{Args, CommandFactory};
use clap_complete::{Generator, Shell, generate};

#[derive(Args)]
pub struct CompletionCommand {
    /// The shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Shell,
}

pub async fn run(args: CompletionCommand) -> Result<()> {
    let mut cmd = super::Shelf::command();
    let script =
        conjure_completions(args.shell, &mut cmd).context("Printing completions failed")?;
    println!("{script}");
    Ok(())
}

/// Whispers shell magic as a UTF-8 string.
///
/// # Arguments
///
/// * `gen` - The shell's arcane sigil.
/// * `cmd` - The command's manifest.
///
/// # Returns
///
/// A Result containing the UTF-8 completion script string, a conduit to faster command summoning.
pub fn conjure_completions<G: Generator>(sigil: G, manifest: &mut clap::Command) -> Result<String> {
    let summon_name = manifest.get_bin_name().unwrap_or("shelf").to_string();

    let mut completion_scroll = Vec::new();
    generate::<G, _>(sigil, manifest, summon_name, &mut completion_scroll);

    String::from_utf8(completion_scroll).map_err(Into::into)
}
