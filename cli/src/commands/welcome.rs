//! Interactive welcome screen and onboarding wizard.
//!
//! When `shiotsuchi` is run without a subcommand, this module handles:
//! - Detecting missing config → prompting to create it
//! - Showing a categorized command menu (TTY)
//! - Running a 3-step onboarding wizard (init → index → search)
//! - Showing text guidance (non-TTY)

use std::io::IsTerminal;
use std::path::Path;

use crate::config::{default_config_path, ShiotsuchiConfig};
use crate::messages;

/// Run the welcome/guidance screen when no subcommand is given.
///
/// - Non-TTY: prints a brief guidance message and exits with code 0.
/// - TTY: shows interactive banner and menu (to be implemented in later tasks).
pub fn run_welcome(
    cfg: &mut ShiotsuchiConfig,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = default_config_path();
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    if !is_tty {
        // Non-TTY: show text guidance and exit
        if !config_path.exists() {
            eprintln!("{}", messages::WELCOME_NON_TTY_NO_CONFIG);
        } else {
            println!("{}", messages::WELCOME_NON_TTY_HINT);
        }
        return Ok(());
    }

    // TTY path — will be implemented in subsequent tasks
    eprintln!("TTY mode not yet implemented");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::IsTerminal;

    #[test]
    fn test_stdin_is_not_terminal_in_test_env() {
        // In test environments, stdin is typically piped, not a terminal.
        assert!(!std::io::stdin().is_terminal());
    }
}
