use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) type Result<T> = std::result::Result<T, BenchError>;

#[derive(Debug)]
pub(crate) enum BenchError {
    Io(io::Error),
    Json(serde_json::Error),
    Message(String),
}

impl Display for BenchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BenchError {}

impl From<io::Error> for BenchError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for BenchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub(crate) fn err(message: impl Into<String>) -> BenchError {
    BenchError::Message(message.into())
}

pub(crate) fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git").args(["rev-parse", "--show-toplevel"]).output()?;
    if !output.status.success() {
        return Err(err("failed to resolve repository root with git"));
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
}

pub(crate) fn safe_id(value: &str) -> String {
    let id: String = value
        .chars()
        .map(
            |char| {
                if char.is_ascii_alphanumeric() || matches!(char, '-' | '_') { char } else { '-' }
            },
        )
        .collect();
    let trimmed = id.trim_matches('-');
    if trimmed.is_empty() { "case".to_owned() } else { trimmed.to_owned() }
}

pub(crate) fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(binary)).find(|candidate| candidate.is_file())
}

pub(crate) fn command_text(program: impl AsRef<OsStr>, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub(crate) fn run_checked(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(err(format!("command failed with status {status}: {command:?}")))
    }
}

pub(crate) fn resolve_repo_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() { Ok(path.to_path_buf()) } else { Ok(repo_root()?.join(path)) }
}

pub(crate) fn relative_tool_path(path: &Path) -> String {
    match repo_root().ok().and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf)) {
        Some(relative) => relative.display().to_string(),
        None => path
            .file_name()
            .and_then(OsStr::to_str)
            .map_or_else(|| path.display().to_string(), ToOwned::to_owned),
    }
}
