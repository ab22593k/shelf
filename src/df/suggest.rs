//! Manages and suggests common configuration files.

use anyhow::Result;
use colored::Colorize;
use rusqlite::Connection;

use super::Dotfs;

/// Collection of `Dotfile` suggestions organized by categories.
pub struct Suggestions {
    categories: Vec<Category>,
}

/// Category of `Dotfile`.
struct Category {
    name: &'static str,
    files: Vec<&'static str>,
}

impl Default for Suggestions {
    /// Creates new Suggestions with predefined categories.
    fn default() -> Self {
        Self {
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
                    name: "Wayland",
                    files: vec![
                        "~/.config/wayland-session",
                        "~/.config/waybar/config",
                        "~/.config/waybar/style.css",
                        "~/.config/river/init",
                        "~/.config/hypr/hyprland.conf",
                        "~/.config/foot/foot.ini",
                        "~/.config/mako/config",
                        "~/.config/gammastep/config.ini",
                        "~/.config/kanshi/config",
                        "~/.config/wofi/config",
                        "~/.config/wlroots/wlr-randr.ini",
                        "~/.config/weston.ini",
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
                    name: "Desktop WM",
                    files: vec![
                        "~/.config/i3/config",
                        "~/.config/sway/config",
                        "~/.dwm/config.h",
                    ],
                },
            ],
        }
    }
}

impl Suggestions {
    /// Prints configuration files by category.
    pub fn print_suggestions(&self) {
        use colored::*;

        println!(
            "{}",
            "Suggested configuration files to track:".bold().underline()
        );
        for category in &self.categories {
            println!("\n{}:", category.name.green().bold());
            for file in &category.files {
                println!("  {}", file.cyan());
            }
        }
    }

    /// Interactive multi-select interface for choosing configuration files.
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

pub(crate) async fn handle_fs_suggest(conn: &Connection, interactive: bool) -> Result<()> {
    let suggestions = Suggestions::default();

    if interactive {
        match suggestions.interactive_selection() {
            Ok(selected) => {
                for path in selected {
                    let expanded_path = shellexpand::tilde(&path).to_string();
                    if let Ok(mut file) = Dotfs::from_file(expanded_path).await {
                        if file.insert(conn).await.is_ok() {
                            println!("{} {}", "Added:".green().bold(), path);
                        }
                    }
                }
            }
            Err(e) => eprintln!("{} {}", "Selection failed:".red().bold(), e),
        }
    } else {
        suggestions.print_suggestions();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestions_default() {
        let suggestions = Suggestions::default();
        assert!(!suggestions.categories.is_empty());

        // Check shell category exists and contains expected files
        let shell_category = suggestions
            .categories
            .iter()
            .find(|c| c.name == "Shell")
            .expect("Shell category should exist");
        assert!(shell_category.files.contains(&"~/.bashrc"));
        assert!(shell_category.files.contains(&"~/.zshrc"));

        // Check VCS category exists and contains expected files
        let vcs_category = suggestions
            .categories
            .iter()
            .find(|c| c.name == "VCS")
            .expect("VCS category should exist");
        assert!(vcs_category.files.contains(&"~/.gitconfig"));
        assert!(vcs_category.files.contains(&"~/.gitignore_global"));

        // Verify each category has a name and files
        for category in &suggestions.categories {
            assert!(!category.name.is_empty());
            assert!(!category.files.is_empty());
        }
    }

    #[test]
    fn test_print_suggestions() {
        let suggestions = Suggestions::default();
        // Just verify it doesn't panic
        suggestions.print_suggestions();
    }
}
