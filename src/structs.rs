use iced::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum LogType {
    Info,
    Success,
    Warning,
    Error,
    GitBranch,
    GitAdded,
    GitModified,
    DiffHeader,
    DiffAdded,
    DiffRemoved,
}

impl LogType {
    pub fn color(&self) -> Color {
        match self {
            LogType::Info => Color::WHITE,
            LogType::Success => Color::from_rgb(0.2, 0.8, 0.2),
            LogType::Warning => Color::from_rgb(1.0, 0.8, 0.0),
            LogType::Error => Color::from_rgb(1.0, 0.3, 0.3),
            LogType::GitBranch => Color::from_rgb(0.0, 0.8, 0.8),
            LogType::GitAdded => Color::from_rgb(0.2, 0.8, 0.2),
            LogType::GitModified => Color::from_rgb(1.0, 0.8, 0.0),
            LogType::DiffHeader => Color::from_rgb(0.0, 0.8, 0.8),
            LogType::DiffAdded => Color::from_rgb(0.2, 0.8, 0.2),
            LogType::DiffRemoved => Color::from_rgb(1.0, 0.3, 0.3),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub message: String,
    pub log_type: LogType,
}

#[derive(Debug, Clone)]
pub struct TrackedFile {
    pub _path: PathBuf,
    pub content: String,
    pub last_modified: u64,
    pub version: i32,
    pub file_id: i32,
}

#[derive(Default)]
pub struct State {
    pub auth_state: AuthState,
    pub is_authenticated: bool,
    pub(crate) jwt_token: Option<String>,
    pub current_user: Option<UserInfo>,
    pub files: Vec<FileInfo>,
    pub files_loading: bool,
    pub upload_loading: bool,
    pub upload_error: Option<String>,
    pub download_folder: Option<PathBuf>,
    pub files_to_download: Vec<FileInfo>,
    pub modified_files: HashSet<String>,
    pub active_tab: u32,
    pub terminal_logs: Vec<LogEntry>,
    pub tracked_files: HashMap<String, TrackedFile>,
    pub version_conflicts: HashMap<String, VersionConflict>,
}

#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub file_name: String,
    pub local_content: String,
    pub server_version: i32,
}

pub struct AuthState {
    pub login: String,
    pub password: String,
    pub error: Option<String>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            login: String::new(),
            password: String::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorInfo {
    pub id: i32,
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileInfo {
    pub id: i32,
    pub name: String,
    pub size: i64,
    pub author: AuthorInfo,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub pid: String,
    pub login: String,
    pub exp: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleInfo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i32,
    pub username: String,
    pub login: String,
    pub role: Option<RoleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Clone)]
pub struct FileWithBytes {
    pub name: String,
    pub _size: usize,
    pub bytes: Vec<u8>,
    pub auth_header: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    LoginChanged(String),
    PasswordChanged(String),
    AuthSubmit,
    AuthResult(Result<AuthResponse, String>),
    FilesFetch,
    FilesReceived(Result<Vec<FileInfo>, String>),
    FileClicked(FileInfo),
    UploadFile,
    FileSelected(Result<Option<FileWithBytes>, String>),
    UploadResult(Result<String, String>),
    DownloadNextFile,
    FileDownloadedToLocal(Result<(String, Vec<u8>, PathBuf), String>),
    FileDownloadedToFolder(Result<(String, Vec<u8>, PathBuf), String>),
    SyncFile(String, Vec<String>),
    FileSyncedResult(Result<(String, i32), String>, Vec<String>),
    TabChanged(u32),
    ClearTerminal,
    FileChangesChecked,
    SyncAllFiles,
    Logout,
    ResolveConflictKeepLocal(String),
    ResolveConflictKeepServer(String),
}
