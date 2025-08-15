use anyhow::Result;

pub(super) enum UserAction {
    RegenerateMessage,
    CommitChanges,
    EditWithEditor,
    Quit,
    Cancelled,
}

const PROMPT: &str = "What would you like to do next?";
const OPTION_REGENERATE: &str = "Regenerate message";
const OPTION_EDIT: &str = "Edit with Editor";
const OPTION_COMMIT: &str = "Commit changes";
const OPTION_QUIT: &str = "Quit";

pub(super) fn user_selection() -> Result<UserAction> {
    use dialoguer::{Select, theme::ColorfulTheme};
    let options = vec![OPTION_REGENERATE, OPTION_EDIT, OPTION_COMMIT, OPTION_QUIT];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(PROMPT)
        .default(0)
        .items(&options)
        .interact();

    match selection {
        Ok(0) => Ok(UserAction::RegenerateMessage),
        Ok(1) => Ok(UserAction::EditWithEditor),
        Ok(2) => Ok(UserAction::CommitChanges),
        Ok(3) => Ok(UserAction::Quit),
        _ => Ok(UserAction::Cancelled),
    }
}
