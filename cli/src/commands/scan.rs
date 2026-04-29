use clap::Args;

#[derive(Args, Debug)]
pub struct ScanArgs {
    #[arg(long, default_value = "500")]
    pub debounce: u64,
}

pub fn run_scan(
    _args: &ScanArgs,
    _notes_dir: &std::path::Path,
    _db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("scan: not yet implemented");
    Ok(())
}
