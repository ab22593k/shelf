use anyhow::Result;

pub(super) enum UserAction {
    RegenerateMessage,
    CommitChanges,
    EditWithEditor,
    Quit,
    Cancelled,
}

pub(super) fn user_selection() -> Result<UserAction> {
    use dialoguer::{Select, theme::ColorfulTheme};
    let options = vec![
        "Regenerate message",
        "Edit with Editor",
        "Commit changes",
        "Quit",
    ];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do next?")
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
