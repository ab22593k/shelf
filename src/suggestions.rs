//! This module provides functionality for managing and suggesting common dotfiles.
//! It includes methods for listing, displaying, and interactively selecting dotfiles.

use crate::dotfile::Dotfiles;
use anyhow::Result;
use std::collections::HashMap;

/// Represents a collection of dotfile suggestions organized by categories.
///
/// This struct contains a list of `Category` instances, each representing a group
/// of related dotfiles. It provides methods for retrieving, displaying, and
/// interactively selecting dotfiles across various categories.
pub struct Suggestions {
    categories: Vec<Category>,
}

/// Represents a category of dotfiles, containing a name and a list of file paths.
///
/// Each `Category` instance groups related dotfiles under a common name,
/// making it easier to organize and present suggestions to users.
struct Category {
    name: &'static str,
    files: Vec<&'static str>,
}

impl Suggestions {
    /// Creates a new `Suggestions` instance with predefined categories and dotfiles.
    ///
    /// This method initializes the `Suggestions` struct with a set of common dotfile
    /// categories and their associated files. The categories include Shell, Git, Vim,
    /// Emacs, Tmux, SSH, X11, macOS, IDE, and Window Managers.
    ///
    /// # Returns
    ///
    /// A new `Suggestions` instance containing predefined categories and dotfiles.
    fn new() -> Self {
        Suggestions {
            categories: vec![
                Category {
                    name: "Shell",
                    files: vec![
                        "~/.bashrc",
                        "~/.zshrc",
                        "~/.profile",
                        "~/.bash_profile",
                        "~/.bash_aliases",
                        "~/.zprofile",
                        "~/.config/fish/config.fish",
                        "~/.tcshrc",
                        "~/.cshrc",
                        "~/.kshrc",
                        "~/.config/xonsh/rc.xsh",
                        "~/.config/ion/initrc",
                        "~/.config/elvish/rc.elv",
                        "~/.config/nushell/config.nu",
                        "~/.config/PowerShell/Microsoft.PowerShell_profile.ps1",
                        "~/.yashrc",
                        "~/.mkshrc",
                        "~/.config/oil/oshrc",
                    ],
                },
                Category {
                    name: "VCS",
                    files: vec![
                        // Git
                        "~/.gitconfig",
                        "~/.gitignore_global",
                        "~/.gitmessage",
                        "~/.gitattributes",
                        // Mercurial
                        "~/.hgrc",
                        "~/.hgignore_global",
                        // Subversion
                        "~/.subversion/config",
                        "~/.subversion/servers",
                        // Bazaar
                        "~/.bazaar/bazaar.conf",
                        "~/.bazaar/ignore",
                        // Fossil
                        "~/.fossil",
                        // Perforce
                        "~/.p4config",
                        // CVS
                        "~/.cvsrc",
                        // Darcs
                        "~/.darcs/author",
                        "~/.darcs/defaults",
                    ],
                },
                Category {
                    name: "Tmux",
                    files: vec!["~/.tmux.conf"],
                },
                Category {
                    name: "SSH",
                    files: vec!["~/.ssh/config", "~/.ssh/known_hosts"],
                },
                Category {
                    name: "X11",
                    files: vec!["~/.xinitrc", "~/.Xresources", "~/.xprofile", "~/.Xmodmap"],
                },
                Category {
                    name: "macOS",
                    files: vec![
                        "~/Library/Preferences/",
                        "~/.config/karabiner/karabiner.json",
                        "~/.config/alacritty/alacritty.yml",
                    ],
                },
                Category {
                    name: "Editors",
                    files: vec![
                        // Vim
                        "~/.vimrc",
                        "~/.vim/",
                        "~/.config/nvim/init.vim",
                        // Emacs
                        "~/.emacs",
                        "~/.emacs.d/init.el",
                        "~/.doom.d/config.el",
                        // VS Code
                        "~/.vscode/settings.json",
                        "~/.config/Code/User/settings.json",
                        // JetBrains IDEs
                        "~/.ideavimrc",
                        "~/.intellij/config/options/",
                        // Sublime Text
                        "~/.config/sublime-text-3/Packages/User/Preferences.sublime-settings",
                        // Atom
                        "~/.atom/config.cson",
                        // Nano
                        "~/.nanorc",
                        // Notepad++
                        "~/AppData/Roaming/Notepad++/config.xml",
                        // Gedit
                        "~/.config/gedit/preferences.xml",
                        // Kate
                        "~/.config/katerc",
                        // TextMate
                        "~/Library/Application Support/TextMate/Bundles",
                        // BBEdit
                        "~/Library/Application Support/BBEdit/Setup",
                        // Brackets
                        "~/.brackets.json",
                        // Geany
                        "~/.config/geany/geany.conf",
                        // Komodo
                        "~/.komodoedit/X.Y/prefs.xml",
                        // Light Table
                        "~/.lighttable/settings.json",
                        // Coda
                        "~/Library/Application Support/Coda 2/",
                        // jEdit
                        "~/.jedit/properties",
                        // Eclipse
                        "~/.eclipse/org.eclipse.platform_*/configuration/config.ini",
                        // NetBeans
                        "~/.netbeans/X.Y/config/Preferences/org/netbeans/core/options/optionsfile.properties",
                    ],
                },
                Category {
                    name: "Window Managers",
                    files: vec![
                        "~/.config/i3/config",
                        "~/.config/sway/config",
                        "~/.dwm/config.h",
                    ],
                },
            ],
        }
    }

    /// Returns a HashMap of common dotfiles grouped by category.
    ///
    /// This method creates a HashMap where the keys are category names and the values
    /// are vectors of file paths associated with each category. It provides a convenient
    /// way to access all the suggested dotfiles organized by their respective categories.
    ///
    /// # Returns
    ///
    /// A HashMap where keys are category names (&str) and values are vectors of file paths (Vec<&str>).
    pub fn get_common_dotfiles(&self) -> HashMap<&'static str, Vec<&'static str>> {
        self.categories
            .iter()
            .map(|cat| (cat.name, cat.files.clone()))
            .collect()
    }

    /// Prints all suggested dotfiles grouped by category.
    ///
    /// This method displays a formatted list of all dotfile suggestions,
    /// organized by their respective categories. Each category name is
    /// printed as a header, followed by the list of associated file paths.
    ///
    /// # Example output:
    ///
    /// ```text
    /// Suggested dotfiles to track:
    ///
    /// Shell:
    ///   ~/.bashrc
    ///   ~/.zshrc
    ///   ...
    ///
    /// Git:
    ///   ~/.gitconfig
    ///   ~/.gitignore_global
    ///   ...
    /// ```
    pub fn print_suggestions(&self) {
        println!("Suggested dotfiles to track:");
        for category in &self.categories {
            println!("\n{}:", category.name);
            for file in &category.files {
                println!("  {}", file);
            }
        }
    }

    /// Provides an interactive interface for users to select dotfiles.
    ///
    /// This method uses the `dialoguer` crate to create a multi-select interface
    /// allowing users to choose categories and then specific files within those
    /// categories. It presents a two-step selection process:
    /// 1. Select categories of interest
    /// 2. For each selected category, choose specific files
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<String>` of selected file paths on success,
    /// or a boxed error on failure.
    ///
    /// # Errors
    ///
    /// This method may return an error if there are issues with the user interface
    /// or if the selection process is interrupted.
    pub fn interactive_selection(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        use dialoguer::MultiSelect;

        let category_names: Vec<&str> = self.categories.iter().map(|c| c.name).collect();

        let selected_categories = MultiSelect::new()
            .with_prompt("Select categories")
            .items(&category_names)
            .interact()
            .unwrap();

        let mut selected_files = Vec::new();

        for &index in &selected_categories {
            let category = &self.categories[index];
            let files = &category.files;

            println!("\nSelecting files for {}:", category.name);
            let selected = MultiSelect::new()
                .with_prompt("Select files")
                .items(files)
                .interact()
                .unwrap();

            for &file_index in &selected {
                selected_files.push(files[file_index].to_string());
            }
        }

        Ok(selected_files)
    }
}

pub fn get_common_dotfiles() -> HashMap<&'static str, Vec<&'static str>> {
    Suggestions::new().get_common_dotfiles()
}

pub fn print_suggestions() {
    Suggestions::new().print_suggestions()
}

pub fn interactive_selection() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Suggestions::new().interactive_selection()
}

pub async fn suggest_dotfiles(index: &mut Dotfiles, interactive: bool) -> Result<()> {
    if interactive {
        match interactive_selection() {
            Ok(selected_files) => {
                for file in selected_files {
                    let expanded_path = shellexpand::tilde(&file);
                    match index.add(expanded_path.as_ref()).await {
                        Ok(_) => println!("Added: {}", file),
                        Err(e) => eprintln!("Failed to add {}: {}", file, e),
                    }
                }
            }
            Err(e) => eprintln!("Error during interactive selection: {}", e),
        }
    } else {
        print_suggestions();
    }
    Ok(())
}
