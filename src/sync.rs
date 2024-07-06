use chrono::Local;
use notify::{RecursiveMode, Watcher};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::channel;

use crate::SyncConfig;

pub struct SyncHandle<'a> {
    pub pid_file: PathBuf,
    pub local_path: PathBuf,
    pub remote_path: PathBuf,
    pub config: &'a SyncConfig,
}

impl<'a> SyncHandle<'a> {
    pub fn new(config: &'a SyncConfig, repo_name: &str) -> Self {
        let pid_file = config
            .pid_file_path
            .with_file_name(format!("{}_{}", config.pid_file_prefix, repo_name))
            .with_extension(config.pid_file_extention.clone());

        SyncHandle {
            pid_file,
            local_path: config.base_local_path.join(repo_name),
            remote_path: config.base_remote_path.join(repo_name),
            config,
        }
    }

    fn make_remote_url(&self) -> OsString {
        if self.config.use_server {
            format!(
                "rsync://{}/{}",
                self.config.remote_host,
                self.remote_path.display()
            )
            .into()
        } else {
            format!(
                "{}@{}:{}",
                self.config.remote_user,
                self.config.remote_host,
                self.remote_path.display()
            )
            .into()
        }
    }

    fn sync_to_remote(&self) {
        let output = Command::new("rsync")
            .args([
                "-avz".into(),
                "--delete".into(),
                self.local_path.clone().into(),
                "--exclude='.git/'".into(),
                "--filter=':- .gitignore'".into(),
                self.make_remote_url(),
            ])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    println!("Sync to remote completed at {}", Local::now());
                    self.sync_compile_commands();
                } else {
                    eprintln!(
                        "Error syncing to remote: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => eprintln!("Failed to execute rsync: {}", e),
        }
    }

    fn sync_compile_commands(&self) {
        let output = Command::new("rsync")
            .args([
                "-avz".into(),
                format!("--include='{}'", self.config.sync_back.display()).into(),
                "--exclude='*'".into(),
                self.make_remote_url(),
                self.local_path.clone().into(),
            ])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    println!("Compile commands synced at {}", Local::now());
                } else {
                    eprintln!(
                        "Error syncing compile commands: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => eprintln!("Failed to execute rsync for compile commands: {}", e),
        }
    }
}

pub fn start_sync(handle: &SyncHandle) {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(tx).unwrap();
    watcher
        .watch(&handle.local_path, RecursiveMode::Recursive)
        .unwrap();

    println!("Starting sync for {}", handle.local_path.display());

    loop {
        match rx.recv() {
            Ok(event) => {
                println!("Change detected: {:?}", event);
                handle.sync_to_remote();
            }
            Err(e) => eprintln!("Watch error: {:?}", e),
        }
    }
}
