use std::path::Path;
use studio_core::{backup, gallery::Filters, Database, Result};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let usage="Usage: studio-maintenance status DATABASE | index DATABASE LIBRARY | backup DATABASE NEW_DIRECTORY [--include-pass-states] | verify BACKUP_DIRECTORY | restore BACKUP_DIRECTORY NEW_DIRECTORY | cleanup-duplicates DATABASE";
    let action = args.get(1).ok_or(usage)?;
    let source = Path::new(args.get(2).ok_or(usage)?);
    match action.as_str() {
        "cleanup-duplicates" => {
            let mut db = Database::open(source)?;
            println!("{}", db.cleanup_exact_duplicates()?);
        }
        "backup" | "restore" => {
            let include_states =
                action == "backup" && args.len() == 5 && args[4] == "--include-pass-states";
            if args.len() != 4 && !include_states {
                return Err(usage.into());
            }
            let destination = Path::new(&args[3]);
            let result = if action == "backup" {
                backup::create_with_options(source, destination, include_states)?
            } else {
                backup::restore(source, destination)?
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "verify" => {
            if args.len() != 3 {
                return Err(usage.into());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&backup::verify(source)?)?
            );
        }
        "status" | "index" => {
            if args.len() != if action == "status" { 3 } else { 4 } {
                return Err(usage.into());
            }
            let mut db = Database::open(source)?;
            if action == "status" {
                println!("{}", serde_json::to_string_pretty(&db.status()?)?);
                println!("{}", serde_json::to_string(&db.facets()?)?);
                println!("{}", serde_json::to_string(&db.latest_index_run()?)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&db.index_library_progress(
                        Path::new(&args[3]),
                        |done, total| {
                            if done % 100 == 0 || done == total {
                                eprintln!("Indexation {done}/{total}");
                            }
                        }
                    )?)?
                );
                println!("Total images: {}", db.gallery(&Filters::default())?.total);
            }
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}
