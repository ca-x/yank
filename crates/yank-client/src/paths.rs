use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use std::path::PathBuf;

pub fn data_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "ca-x", "yank")
        .ok_or_else(|| anyhow!("unable to resolve an application data directory"))?;
    let path = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&path).with_context(|| format!("creating {}", path.display()))?;
    Ok(path)
}

pub fn database_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("yank.sqlite3"))
}
