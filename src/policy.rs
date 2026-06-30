use crate::config::PolicyConfig;
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy)]
pub enum FsOperation {
    Read,
    Write,
    Delete,
    List,
}

#[derive(Debug, Clone)]
pub struct Policy {
    config: PolicyConfig,
}

impl Policy {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &PolicyConfig {
        &self.config
    }

    pub fn check_file_read(&self, path: &Path) -> Result<PathBuf> {
        let path = normalize_for_existing(path)?;
        self.ensure_path_allowed(&path, FsOperation::Read)?;
        Ok(path)
    }

    pub fn check_file_write(&self, path: &Path) -> Result<PathBuf> {
        let path = normalize_for_write(path)?;
        self.ensure_path_allowed(&path, FsOperation::Write)?;
        Ok(path)
    }

    pub fn check_dir_or_file(&self, path: &Path, operation: FsOperation) -> Result<PathBuf> {
        let path = match operation {
            FsOperation::Write => normalize_for_write(path)?,
            FsOperation::Read | FsOperation::Delete | FsOperation::List => {
                normalize_for_existing(path)?
            }
        };
        self.ensure_path_allowed(&path, operation)?;
        Ok(path)
    }

    pub fn check_shell_exec(&self, cwd: &Path) -> Result<PathBuf> {
        if !self.config.shell_exec_enabled {
            bail!("shell execution is disabled by local policy");
        }
        let cwd = normalize_for_existing(cwd)?;
        if !cwd.is_dir() {
            bail!("command cwd is not a directory: {}", cwd.display());
        }
        self.ensure_path_allowed(&cwd, FsOperation::Read)?;
        Ok(cwd)
    }

    fn ensure_path_allowed(&self, path: &Path, _operation: FsOperation) -> Result<()> {
        reject_suspicious_components(path)?;

        for blocked in &self.config.blocked_paths {
            if let Ok(blocked) = normalize_lenient(blocked)
                && path.starts_with(&blocked)
            {
                bail!("path is blocked by policy: {}", path.display());
            }
        }

        let allowed = self
            .config
            .allowed_roots
            .iter()
            .filter_map(|root| normalize_lenient(root).ok())
            .any(|root| path.starts_with(root));

        if !allowed {
            bail!("path is outside allowed roots: {}", path.display());
        }

        Ok(())
    }
}

fn normalize_for_existing(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))
}

fn normalize_for_write(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    reject_suspicious_components(&path)?;

    if path.exists() {
        return normalize_for_existing(&path);
    }

    let mut cursor = path.as_path();
    let mut missing = Vec::<OsString>::new();
    loop {
        if cursor.exists() {
            let mut normalized = cursor
                .canonicalize()
                .with_context(|| format!("failed to canonicalize {}", cursor.display()))?;
            for component in missing.iter().rev() {
                normalized.push(component);
            }
            return Ok(normalized);
        }

        let name = cursor
            .file_name()
            .context("path must include a file or directory name")?;
        missing.push(name.to_os_string());
        cursor = cursor
            .parent()
            .context("failed to find existing parent directory")?;
    }
}

fn normalize_lenient(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        normalize_for_existing(path)
    } else {
        normalize_for_write(path)
    }
}

fn reject_suspicious_components(path: &Path) -> Result<()> {
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            bail!("path contains parent traversal: {}", path.display());
        }
    }
    Ok(())
}
