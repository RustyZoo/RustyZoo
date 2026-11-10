//! The text of `rustyzoo help` and of a command that is not understood.

/// The whole reference, the same file as on the project page.
pub const HELP: &str = include_str!("../../docs/usage.md");

pub const USAGE: &str = "\
usage: rustyzoo [--data <dir>] <command> [arguments]

`rustyzoo help` prints every command, option and setting.";

#[cfg(test)]
mod tests {
    use super::HELP;

    /// Every command the dispatcher knows is in the reference that `help` prints.
    #[test]
    fn help_names_every_command() {
        let source = include_str!("mod.rs");
        let mut seen = 0;
        for line in source.lines() {
            let Some(rest) = line.trim_start().strip_prefix("[\"") else {
                continue;
            };
            let name = rest.split('"').next().unwrap_or("");
            if name.contains('|') || name.is_empty() {
                continue;
            }
            seen += 1;
            assert!(
                HELP.lines()
                    .any(|l| l.contains("rustyzoo") && l.contains(&format!(" {name}"))),
                "docs/usage.md lacks {name}"
            );
        }
        assert!(seen > 20);
    }

    /// A first word that is not in the list opens a saved game, so the list has every command.
    #[test]
    fn the_list_of_commands_has_every_first_word() {
        let source = include_str!("mod.rs");
        let mut seen = 0;
        for line in source.lines() {
            let Some(rest) = line.trim_start().strip_prefix("[\"") else {
                continue;
            };
            for name in rest.split(']').next().unwrap_or("").split(" | ") {
                let name = name
                    .split('"')
                    .nth(if name.starts_with('"') { 1 } else { 0 });
                let name = name.unwrap_or("");
                if name.starts_with('-') || name.is_empty() {
                    continue;
                }
                seen += 1;
                assert!(
                    super::super::COMMANDS.contains(&name),
                    "{name} is not listed"
                );
            }
        }
        assert!(seen > 20);
    }
}
