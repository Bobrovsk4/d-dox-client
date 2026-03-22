use iced::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

pub struct Theme;

impl Theme {
    pub const BACKGROUND_PRIMARY: Color = Color::from_rgb(0.09, 0.09, 0.11);
    pub const BACKGROUND_SECONDARY: Color = Color::from_rgb(0.12, 0.12, 0.14);
    pub const BACKGROUND_TERTIARY: Color = Color::from_rgb(0.15, 0.15, 0.17);
    pub const CARD_BACKGROUND: Color = Color::from_rgb(0.14, 0.14, 0.16);

    pub const PRIMARY: Color = Color::from_rgb(0.37, 0.55, 0.95);
    pub const SUCCESS: Color = Color::from_rgb(0.24, 0.75, 0.45);
    pub const WARNING: Color = Color::from_rgb(0.95, 0.68, 0.24);
    pub const ERROR: Color = Color::from_rgb(0.95, 0.33, 0.33);
    pub const INFO: Color = Color::from_rgb(0.24, 0.65, 0.95);

    pub const TEXT_PRIMARY: Color = Color::from_rgb(0.95, 0.95, 0.95);
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.70, 0.70, 0.75);
    pub const TEXT_MUTED: Color = Color::from_rgb(0.45, 0.45, 0.50);

    pub const RADIUS_SM: f32 = 6.0;
    pub const RADIUS_MD: f32 = 10.0;

    pub const SPACING_SM: f32 = 8.0;
    pub const SPACING_MD: f32 = 16.0;
    pub const SPACING_LG: f32 = 24.0;
    pub const SPACING_XL: f32 = 32.0;
}

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
            LogType::Info => Theme::TEXT_PRIMARY,
            LogType::Success => Theme::SUCCESS,
            LogType::Warning => Theme::WARNING,
            LogType::Error => Theme::ERROR,
            LogType::GitBranch => Theme::INFO,
            LogType::GitAdded => Theme::SUCCESS,
            LogType::GitModified => Theme::WARNING,
            LogType::DiffHeader => Theme::INFO,
            LogType::DiffAdded => Theme::SUCCESS,
            LogType::DiffRemoved => Theme::ERROR,
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
    pub current_user: Option<UserResponse>,
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorInfo {
    pub id: i32,
    pub login: String,
}

#[allow(dead_code)]
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
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub login: String,
    pub role: Option<RoleResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleResponse {
    pub id: i32,
    pub name: String,
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
