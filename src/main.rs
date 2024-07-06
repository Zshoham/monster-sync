use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use daemonize::{Daemonize, Outcome};
use glob::glob;
use serde::Deserialize;
use std::path::PathBuf;
use sync::SyncHandle;

mod sync;

#[derive(Deserialize)]
struct SyncConfig {
    remote_user: String,
    remote_host: String,
    base_local_path: PathBuf,
    base_remote_path: PathBuf,
    sync_back: PathBuf,
    pid_file_path: PathBuf,
    pid_file_prefix: String,
    pid_file_extention: String,
    use_server: bool,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start { repo_name: String },
    Stop { repo_name: String },
    Status { repo_name: String },
    List,
}

fn start_sync(repo_name: &str, handle: SyncHandle) -> Result<()> {
    anyhow::ensure!(
        &handle.local_path.exists(),
        "Local repository {} does not exist.",
        handle.local_path.display()
    );
    anyhow::ensure!(
        !&handle.pid_file.exists(),
        "Sync is already running for {}.",
        repo_name
    );

    let daemonize = Daemonize::new()
        .pid_file(&handle.pid_file)
        .chown_pid_file(true)
        .working_directory("/");

    match daemonize.execute() {
        Outcome::Parent(Ok(_)) => Ok(()),
        Outcome::Parent(Err(err)) => Err(err),
        Outcome::Child(Ok(_)) => {
            sync::start_sync(&handle);
            Ok(())
        }
        Outcome::Child(Err(err)) => Err(err),
    }
    .context("Failed to create daemon")
}

fn stop_sync(repo_name: &str) -> Result<()> {
    let pid_file = format!("/tmp/repo_sync_{}.pid", repo_name);

    let pid_str = std::fs::read_to_string(&pid_file)
        .context(format!("Sync is not running for {}.", repo_name))?;

    let pid = pid_str
        .trim()
        .parse::<u32>()
        .context("Invalid PID in file")?;

    let system = sysinfo::System::new_all();

    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        process.kill_with(sysinfo::Signal::Term);
        println!("Sync stopped for {}. PID was: {}", repo_name, pid);
    } else {
        eprintln!("Process {} not found. Cleaning up.", pid);
    }

    std::fs::remove_file(&pid_file).context("Unable to remove PID file")?;

    Ok(())
}

fn status_sync(repo_name: &str) -> Result<()> {
    let pid_file = format!("/tmp/repo_sync_{}.pid", repo_name);

    let pid = std::fs::read_to_string(&pid_file)
        .context(format!("Sync is not running for {}.", repo_name))?
        .trim()
        .parse::<u32>()
        .context("Invalid PID in file")?;

    let system = sysinfo::System::new_all();

    if system.process(sysinfo::Pid::from_u32(pid)).is_some() {
        println!("Sync is running for {}. PID: {}", repo_name, pid);
    } else {
        println!(
            "PID file exists, but sync is not running for {}. Cleaning up.",
            repo_name
        );
        std::fs::remove_file(&pid_file).context("Unable to remove PID file")?;
    }

    Ok(())
}

fn list_syncs() -> Result<()> {
    println!("Currently running syncs:");
    println!("------------------------");
    let system = sysinfo::System::new_all();
    let mut found_syncs = false;

    for entry in glob("/tmp/repo_sync_*.pid")? {
        let path = entry?;
        let file_name = path.file_name().context("Invalid file name")?;
        let repo_name = file_name
            .to_str()
            .context("Invalid UTF-8 in file name")?
            .trim_start_matches("repo_sync_")
            .trim_end_matches(".pid");

        let pid = std::fs::read_to_string(&path)?.trim().parse::<u32>()?;

        if system.process(sysinfo::Pid::from_u32(pid)).is_some() {
            found_syncs = true;
            println!("Repository: {} (PID: {})", repo_name, pid);
        } else {
            println!(
                "Repository: {} (Sync not running, stale PID file)",
                repo_name
            );
            std::fs::remove_file(&path)?;
        }
    }

    if !found_syncs {
        println!("No active syncs found.");
    }

    Ok(())
}
fn main() -> Result<()> {
    let config = Config::builder()
        .add_source(config::Environment::with_prefix("MSYNC"))
        .add_source(config::File::with_name(
            dirs::config_dir()
                .context("Could not find config dir.")?
                .join("msync")
                .to_str()
                .context("Could not find config file.")?,
        ))
        .build()?
        .try_deserialize()
        .context("Failed parsing config")?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::Start { repo_name } => {
            start_sync(repo_name, sync::SyncHandle::new(&config, repo_name))
        }
        Commands::Stop { repo_name } => stop_sync(repo_name),
        Commands::Status { repo_name } => status_sync(repo_name),
        Commands::List => list_syncs(),
    }
}
