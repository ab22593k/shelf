use anyhow::Result;
use clap_complete::{Generator, generate};

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
