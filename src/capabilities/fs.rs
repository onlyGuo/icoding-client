use crate::policy::{FsOperation, Policy};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct FsCapabilities {
    policy: Policy,
}

#[derive(Debug, Deserialize)]
struct StatParams {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ListParams {
    path: PathBuf,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    include_hidden: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct ReadParams {
    path: PathBuf,
    #[serde(default = "default_encoding")]
    encoding: String,
    max_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WriteParams {
    path: PathBuf,
    #[serde(default = "default_write_mode")]
    mode: String,
    #[serde(default = "default_encoding")]
    encoding: String,
    content: Option<String>,
    content_base64: Option<String>,
    expected_sha256: Option<String>,
    #[serde(default)]
    create_parent_dirs: bool,
}

#[derive(Debug, Deserialize)]
struct MkdirParams {
    path: PathBuf,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Deserialize)]
struct MoveParams {
    from: PathBuf,
    to: PathBuf,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
struct DeleteParams {
    path: PathBuf,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    root: PathBuf,
    query: String,
    #[serde(default = "default_search_mode")]
    mode: String,
    #[serde(default)]
    include_hidden: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

impl FsCapabilities {
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    pub fn stat(&self, params: Value) -> Result<Value> {
        let params: StatParams =
            serde_json::from_value(params).context("invalid fs.stat parameters")?;
        let path = self
            .policy
            .check_dir_or_file(&params.path, FsOperation::Read)?;
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        entry_json(&path, &metadata, true)
    }

    pub fn list(&self, params: Value) -> Result<Value> {
        let params: ListParams =
            serde_json::from_value(params).context("invalid fs.list parameters")?;
        let root = self
            .policy
            .check_dir_or_file(&params.path, FsOperation::List)?;
        if !root.is_dir() {
            bail!("path is not a directory: {}", root.display());
        }

        let mut entries = Vec::new();
        let mut truncated = false;

        if params.recursive {
            for entry in WalkDir::new(&root).min_depth(1).into_iter() {
                let entry = entry
                    .with_context(|| format!("failed to walk directory {}", root.display()))?;
                if !params.include_hidden && is_hidden(entry.path()) {
                    continue;
                }
                self.policy
                    .check_dir_or_file(entry.path(), FsOperation::Read)?;
                let metadata = entry
                    .metadata()
                    .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
                entries.push(entry_json(entry.path(), &metadata, false)?);
                if entries.len() >= params.limit {
                    truncated = true;
                    break;
                }
            }
        } else {
            let directory = fs::read_dir(&root)
                .with_context(|| format!("failed to read directory {}", root.display()))?;
            for entry in directory {
                let entry = entry
                    .with_context(|| format!("failed to read an entry in {}", root.display()))?;
                let path = entry.path();
                if !params.include_hidden && is_hidden(&path) {
                    continue;
                }
                self.policy.check_dir_or_file(&path, FsOperation::Read)?;
                let metadata = entry
                    .metadata()
                    .with_context(|| format!("failed to inspect {}", path.display()))?;
                entries.push(entry_json(&path, &metadata, false)?);
                if entries.len() >= params.limit {
                    truncated = true;
                    break;
                }
            }
        }

        Ok(json!({
            "path": root,
            "entries": entries,
            "truncated": truncated
        }))
    }

    pub fn read(&self, params: Value) -> Result<Value> {
        let params: ReadParams =
            serde_json::from_value(params).context("invalid fs.read parameters")?;
        let path = self.policy.check_file_read(&params.path)?;
        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to inspect {}", path.display()))?;
        if !metadata.is_file() {
            bail!("path is not a file: {}", path.display());
        }
        let max_bytes = params
            .max_bytes
            .unwrap_or(self.policy.config().max_file_read_bytes)
            .min(self.policy.config().max_file_read_bytes);
        if metadata.len() > max_bytes {
            bail!("file is too large: {} bytes", metadata.len());
        }
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read file {}", path.display()))?;
        let sha256 = sha256_hex(&bytes);

        if params.encoding == "binary" {
            return Ok(json!({
                "path": path,
                "encoding": "binary",
                "content_base64": STANDARD.encode(bytes),
                "size": metadata.len(),
                "sha256": sha256
            }));
        }

        let content = String::from_utf8(bytes).context("file is not valid utf-8")?;
        Ok(json!({
            "path": path,
            "encoding": "utf-8",
            "content": content,
            "size": metadata.len(),
            "sha256": sha256
        }))
    }

    pub fn write(&self, params: Value) -> Result<Value> {
        let params: WriteParams =
            serde_json::from_value(params).context("invalid fs.write parameters")?;
        if params.encoding != "utf-8" && params.encoding != "binary" {
            bail!("unsupported encoding: {}", params.encoding);
        }
        let path = self.policy.check_file_write(&params.path)?;
        if params.create_parent_dirs
            && let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directory {}", parent.display())
            })?;
        }

        let bytes = if params.encoding == "binary" {
            let content = params
                .content_base64
                .context("content_base64 is required")?;
            STANDARD.decode(content).context("invalid base64 content")?
        } else {
            params.content.context("content is required")?.into_bytes()
        };
        if bytes.len() as u64 > self.policy.config().max_file_write_bytes {
            bail!("write content is too large: {} bytes", bytes.len());
        }

        let previous_sha256 = if path.exists() {
            Some(sha256_hex(&fs::read(&path).with_context(|| {
                format!(
                    "failed to read existing file {} before write",
                    path.display()
                )
            })?))
        } else {
            None
        };

        if let Some(expected) = &params.expected_sha256
            && previous_sha256.as_deref() != Some(expected.as_str())
        {
            bail!("file changed before write");
        }

        match params.mode.as_str() {
            "create_new" if path.exists() => bail!("file already exists: {}", path.display()),
            "create_new" | "overwrite" => fs::write(&path, &bytes)
                .with_context(|| format!("failed to write file {}", path.display()))?,
            "append" => {
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .with_context(|| format!("failed to open {} for append", path.display()))?;
                file.write_all(&bytes)
                    .with_context(|| format!("failed to append to {}", path.display()))?;
            }
            other => bail!("unsupported write mode: {other}"),
        }

        Ok(json!({
            "path": path,
            "mode": params.mode,
            "bytes_written": bytes.len(),
            "previous_sha256": previous_sha256,
            "sha256": sha256_hex(&fs::read(&path).with_context(|| {
                format!("failed to verify written file {}", path.display())
            })?)
        }))
    }

    pub fn mkdir(&self, params: Value) -> Result<Value> {
        let params: MkdirParams =
            serde_json::from_value(params).context("invalid fs.mkdir parameters")?;
        let path = self
            .policy
            .check_dir_or_file(&params.path, FsOperation::Write)?;
        if params.recursive {
            fs::create_dir_all(&path)
                .with_context(|| format!("failed to create directory {}", path.display()))?;
        } else {
            fs::create_dir(&path)
                .with_context(|| format!("failed to create directory {}", path.display()))?;
        }
        Ok(json!({ "path": path, "created": true }))
    }

    pub fn move_path(&self, params: Value) -> Result<Value> {
        let params: MoveParams =
            serde_json::from_value(params).context("invalid fs.move parameters")?;
        let from = self
            .policy
            .check_dir_or_file(&params.from, FsOperation::Write)?;
        let to = self.policy.check_file_write(&params.to)?;
        if to.exists() && !params.overwrite {
            bail!("destination already exists: {}", to.display());
        }
        fs::rename(&from, &to)
            .with_context(|| format!("failed to move {} to {}", from.display(), to.display()))?;
        Ok(json!({ "from": from, "to": to, "moved": true }))
    }

    pub fn delete(&self, params: Value) -> Result<Value> {
        let params: DeleteParams =
            serde_json::from_value(params).context("invalid fs.delete parameters")?;
        let path = self
            .policy
            .check_dir_or_file(&params.path, FsOperation::Delete)?;
        if path.is_dir() {
            if params.recursive {
                fs::remove_dir_all(&path).with_context(|| {
                    format!("failed to recursively delete directory {}", path.display())
                })?;
            } else {
                fs::remove_dir(&path)
                    .with_context(|| format!("failed to delete directory {}", path.display()))?;
            }
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to delete file {}", path.display()))?;
        }
        Ok(json!({ "path": path, "deleted": true }))
    }

    pub fn search(&self, params: Value) -> Result<Value> {
        let params: SearchParams =
            serde_json::from_value(params).context("invalid fs.search parameters")?;
        let root = self
            .policy
            .check_dir_or_file(&params.root, FsOperation::Read)?;
        let mut matches = Vec::new();
        let mut truncated = false;

        for entry in WalkDir::new(&root).into_iter() {
            let entry =
                entry.with_context(|| format!("failed to walk directory {}", root.display()))?;
            let path = entry.path();
            if !params.include_hidden && is_hidden(path) {
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            self.policy.check_dir_or_file(path, FsOperation::Read)?;

            if params.mode == "filename" {
                if entry.file_name().to_string_lossy().contains(&params.query) {
                    matches.push(json!({
                        "path": path,
                        "kind": "file",
                        "line": null,
                        "preview": entry.file_name().to_string_lossy()
                    }));
                }
            } else if params.mode == "content" {
                let metadata = entry
                    .metadata()
                    .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
                if metadata.len() > self.policy.config().max_file_read_bytes {
                    continue;
                }
                let content = match fs::read_to_string(path) {
                    Ok(content) => content,
                    Err(error) if error.kind() == std::io::ErrorKind::InvalidData => continue,
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("failed to read file {}", path.display()));
                    }
                };
                for (idx, line) in content.lines().enumerate() {
                    if line.contains(&params.query) {
                        matches.push(json!({
                            "path": path,
                            "kind": "file",
                            "line": idx + 1,
                            "preview": line
                        }));
                        break;
                    }
                }
            } else {
                bail!("unsupported search mode: {}", params.mode);
            }

            if matches.len() >= params.limit {
                truncated = true;
                break;
            }
        }

        Ok(json!({
            "root": root,
            "matches": matches,
            "truncated": truncated
        }))
    }
}

fn entry_json(path: &Path, metadata: &fs::Metadata, include_hash: bool) -> Result<Value> {
    let kind = if metadata.is_file() {
        "file"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.file_type().is_symlink() {
        "symlink"
    } else {
        "other"
    };
    let sha256 = if include_hash && metadata.is_file() {
        Some(sha256_hex(&fs::read(path).with_context(|| {
            format!("failed to read file {} for hashing", path.display())
        })?))
    } else {
        None
    };

    Ok(json!({
        "name": path.file_name().map(|name| name.to_string_lossy().to_string()),
        "path": path,
        "kind": kind,
        "size": if metadata.is_file() { Some(metadata.len()) } else { None },
        "modified_at": metadata.modified().ok().map(chrono::DateTime::<chrono::Utc>::from),
        "readonly": metadata.permissions().readonly(),
        "sha256": sha256
    }))
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn default_limit() -> usize {
    200
}

fn default_encoding() -> String {
    "utf-8".to_string()
}

fn default_write_mode() -> String {
    "overwrite".to_string()
}

fn default_search_mode() -> String {
    "filename".to_string()
}
