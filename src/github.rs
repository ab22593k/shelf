use anyhow::Result;
use octocrab::Octocrab;

/// Creates a new GitHub repository using the octocrab client.
///
/// # Arguments
///
/// * `octocrab` - An authenticated Octocrab instance.
/// * `repo_name` - The name of the repository to create.
/// * `org` - Optional organization name to create the repository under.
///           If None, the repository will be created under the user's account.
/// * `private` - Whether the repository should be private.
///
/// # Returns
///
/// Returns `Ok(())` if the repository was created successfully, otherwise returns an `Err`
/// containing the error details.
pub async fn create_github_repo(
    octocrab: &Octocrab,
    repo_name: &str,
    org: Option<&str>,
    private: bool,
) -> Result<()> {
    let mut builder = if let Some(org_name) = org {
        // Organization repos are created via orgs endpoint
        octocrab.orgs(org_name).repos().create(repo_name)
    } else {
        // Personal repos are created via current authenticated user endpoint
        octocrab.repos().create(repo_name)
    };

    builder = builder.private(private);

    builder.send().await?;

    Ok(())
}
