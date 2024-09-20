use std::collections::HashMap;

struct DotfileCategory {
    name: &'static str,
    files: Vec<&'static str>,
}

impl DotfileCategory {
    fn new(name: &'static str, files: Vec<&'static str>) -> Self {
        DotfileCategory { name, files }
    }
}

fn get_all_categories() -> Vec<DotfileCategory> {
    vec![
        DotfileCategory::new(
            "Shell",
            vec![
                "~/.bashrc",
                "~/.zshrc",
                "~/.profile",
                "~/.bash_profile",
                "~/.bash_aliases",
                "~/.zprofile",
            ],
        ),
        DotfileCategory::new(
            "Git",
            vec![
                "~/.gitconfig",
                "~/.gitignore_global",
                "~/.gitmessage",
                "~/.gitattributes",
            ],
        ),
        DotfileCategory::new(
            "Vim",
            vec!["~/.vimrc", "~/.vim/", "~/.config/nvim/init.vim"],
        ),
        DotfileCategory::new(
            "Emacs",
            vec!["~/.emacs", "~/.emacs.d/init.el", "~/.doom.d/config.el"],
        ),
        DotfileCategory::new("Tmux", vec!["~/.tmux.conf"]),
        DotfileCategory::new("SSH", vec!["~/.ssh/config", "~/.ssh/known_hosts"]),
        DotfileCategory::new(
            "X11",
            vec!["~/.xinitrc", "~/.Xresources", "~/.xprofile", "~/.Xmodmap"],
        ),
        DotfileCategory::new(
            "macOS",
            vec![
                "~/Library/Preferences/",
                "~/.config/karabiner/karabiner.json",
                "~/.config/alacritty/alacritty.yml",
            ],
        ),
        DotfileCategory::new(
            "IDE",
            vec![
                "~/.vscode/settings.json",
                "~/.idea/config/options/",
                "~/.config/sublime-text-3/Packages/User/Preferences.sublime-settings",
            ],
        ),
        DotfileCategory::new(
            "Window Managers",
            vec![
                "~/.config/i3/config",
                "~/.config/sway/config",
                "~/.dwm/config.h",
            ],
        ),
    ]
}

pub fn get_common_dotfiles() -> HashMap<&'static str, Vec<&'static str>> {
    get_all_categories()
        .into_iter()
        .map(|cat| (cat.name, cat.files))
        .collect()
}

pub fn print_suggestions() {
    println!("Suggested dotfiles to track:");
    for category in get_all_categories() {
        println!("\n{}:", category.name);
        for file in category.files {
            println!("  {}", file);
        }
    }
}

pub fn interactive_selection() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use dialoguer::MultiSelect;

    let categories = get_all_categories();
    let category_names: Vec<&str> = categories.iter().map(|c| c.name).collect();

    let selected_categories = MultiSelect::new()
        .with_prompt("Select categories")
        .items(&category_names)
        .interact()
        .unwrap();

    let mut selected_files = Vec::new();

    for &index in &selected_categories {
        let category = &categories[index];
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
