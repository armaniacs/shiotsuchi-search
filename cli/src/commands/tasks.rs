use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
#[command(about = crate::messages::TASKS_ABOUT)]
pub struct TasksArgs {
    pub keyword: Option<String>,
    #[arg(long, help = crate::messages::TASKS_ALL_HELP)]
    pub all: bool,
}

pub fn run_tasks(args: &TasksArgs, db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let db = shiotsuchi_core::db::NoteDatabase::open(db_path)?;
    let keyword = args.keyword.as_deref();
    let tasks = db.query_tasks(keyword, args.all)?;
    for task in &tasks {
        let status = if task.checked { "[x]" } else { "[ ]" };
        println!("  {} {}:{} {}", status, task.file_path, task.line_number, task.content);
    }
    println!("Total: {} tasks", tasks.len());
    Ok(())
}
