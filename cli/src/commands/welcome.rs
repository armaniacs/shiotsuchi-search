//! Interactive welcome screen and onboarding wizard.
//!
//! When `shiotsuchi` is run without a subcommand, this module handles:
//! - Detecting missing config → prompting to create it
//! - Showing a categorized command menu (TTY)
//! - Running a 3-step onboarding wizard (init → index → search)
//! - Showing text guidance (non-TTY)

use crate::config::ShiotsuchiConfig;

/// Placeholder: will be replaced in subsequent tasks.
pub fn run_welcome(_cfg: &mut ShiotsuchiConfig) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("{}", "welcome module placeholder");
    Ok(())
}
