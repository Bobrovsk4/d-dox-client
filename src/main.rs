use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{button, column, container, row, text, text_input, tooltip, scrollable},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::path::PathBuf;
use std::collections::HashSet;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub enum LogType {
    Info,
    Success,
    Warning,
    Error,
    Debug,
    GitBranch,
    GitCommit,
    GitAdded,
    GitModified,
    GitDeleted,
    DiffHeader,
    DiffAdded,
    DiffRemoved,
}

impl LogType {
    fn color(&self) -> Color {
        match self {
            LogType::Info => Color::WHITE,
            LogType::Success => Color::from_rgb(0.2, 0.8, 0.2),
            LogType::Warning => Color::from_rgb(1.0, 0.8, 0.0),
            LogType::Error => Color::from_rgb(1.0, 0.3, 0.3),
            LogType::Debug => Color::from_rgb(0.5, 0.5, 0.5),
            LogType::GitBranch => Color::from_rgb(0.0, 0.8, 0.8),
            LogType::GitCommit => Color::WHITE,
            LogType::GitAdded => Color::from_rgb(0.2, 0.8, 0.2),
            LogType::GitModified => Color::from_rgb(1.0, 0.8, 0.0),
            LogType::GitDeleted => Color::from_rgb(1.0, 0.3, 0.3),
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
    pub path: PathBuf,
    pub content: String,
    pub last_modified: u64,
}

#[derive(Default)]
pub struct State {
    auth_state: AuthState,
    is_authenticated: bool,
    jwt_token: Option<String>,
    current_user: Option<UserInfo>,
    files: Vec<FileInfo>,
    files_loading: bool,
    upload_loading: bool,
    upload_error: Option<String>,
    download_folder: Option<PathBuf>,
    files_to_download: Vec<FileInfo>,
    modified_files: HashSet<String>,
    active_tab: u32,//
    terminal_logs: Vec<LogEntry>,
    tracked_files: HashMap<String, TrackedFile>,
}

struct AuthState {
    login: String,
    password: String,
    error: Option<String>,
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
pub struct FileInfo {
    pub name: String,
    pub size: usize,
    pub last_modified: Option<String>,
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
    pub size: usize,
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
    SyncFile(String),
    FileSynced(Result<String, String>),
    TabChanged(u32),
    ClearTerminal,
    FileChangesChecked,
    SyncAllFiles,
    Logout,
}

const BASE_URL: &str = "http://192.168.1.71:31356";

impl State {
    fn get_auth_header(&self) -> Option<String> {
        self.jwt_token.as_ref().map(|token| format!("Bearer {token}"))
    }
    
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoginChanged(login) => {
                self.auth_state.login = login;
                self.auth_state.error = None;
                Task::none()
            }
            Message::PasswordChanged(password) => {
                self.auth_state.password = password;
                self.auth_state.error = None;
                Task::none()
            }
            Message::AuthSubmit => {
                println!("AuthSubmit");
                if self.auth_state.login.is_empty() {
                    self.auth_state.error = Some("Введите логин".to_string());
                    return Task::none();
                }
                if self.auth_state.password.is_empty() {
                    self.auth_state.error = Some("Введите пароль".to_string());
                    return Task::none();
                }

                let client = Client::new();
                let login = self.auth_state.login.clone();
                let password = self.auth_state.password.clone();

                Task::perform(
                    async move {
                        let url = format!("{BASE_URL}/auth/login");
                        match client
                            .post(&url)
                            .json(&serde_json::json!({ "login": login, "password": password }))
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                match resp.json::<AuthResponse>().await {
                                    Ok(auth_resp) => Ok(auth_resp),
                                    Err(e) => Err(format!("JSON error: {e}")),
                                }
                            }
                            Ok(resp) => {
                                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                                    Err("Сервер не найден. Проверьте адрес.".to_string())
                                } else {
                                    Err(format!("Ошибка: {}", resp.status()))
                                }
                            }
                            Err(e) => Err(format!("Ошибка соединения: {e}")),
                        }
                    },
                    Message::AuthResult,
                )
            }
            Message::AuthResult(result) => {
                println!("AuthResult");
                match result {
                    Ok(auth_resp) => {
                        let secret = "v7SWenu8m9aPQuDkL6pw";
                        match decode::<Claims>(
                            &auth_resp.token,
                            &DecodingKey::from_secret(secret.as_bytes()),
                            &Validation::default(),
                        ) {
                            Ok(_) => {
                                self.jwt_token = Some(auth_resp.token);
                                self.current_user = Some(auth_resp.user);
                                self.is_authenticated = true;
                                self.auth_state.error = None;
                                return Task::perform(async { Message::FilesFetch }, |m| m);
                            }
                            Err(e) => {
                                self.auth_state.error = Some(format!("Invalid token: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        if e == "Logged out" {
                            self.jwt_token = None;
                            self.current_user = None;
                            self.is_authenticated = false;
                            self.auth_state.login.clear();
                            self.auth_state.password.clear();
                            self.auth_state.error = None;
                            self.files.clear();
                            self.add_log("User logged out".to_string(), LogType::Info);
                        } else {
                            self.auth_state.error = Some(e);
                        }
                    }
                }
                Task::none()
            }
            Message::FilesFetch => {
                println!("FilesFetch");
                self.files_loading = true;
                let client = Client::new();
                let auth_header = self.get_auth_header();
                println!("Auth header: {:?}", auth_header);
                Task::perform(
                    async move {
                        let url = format!("{BASE_URL}/files");
                        let mut req = client.get(&url);
                        if let Some(token) = auth_header {
                            req = req.header("Authorization", token);
                        }
                        match req.send().await {
                            Ok(resp) if resp.status().is_success() => {
                                match resp.json::<Vec<FileInfo>>().await {
                                    Ok(files) => Ok(files),
                                    Err(e) => Err(format!("JSON error: {e}")),
                                }
                            }
                            Ok(resp) => {
                                let status = resp.status();
                                let body = resp.text().await.unwrap_or_default();
                                println!("Server error response: {}", body);
                                Err(format!("HTTP {}", status))
                            },
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    Message::FilesReceived,
                )
            }
            Message::FilesReceived(result) => {
                println!("FilesReceived");
                self.files_loading = false;
                match result {
                    Ok(files) => {
                        self.add_log(format!("Files fetched: {} files", files.len()), LogType::GitBranch);
                        self.files = files.clone();
                        if self.download_folder.is_none() {
                            self.download_folder = Some(std::env::current_dir().unwrap().join("downloads"));
                        }
                        if let Some(ref folder) = self.download_folder {
                            let _ = std::fs::create_dir_all(folder);
                        }
                        self.files_to_download = files;
                        if !self.files_to_download.is_empty() {
                            return Task::perform(async { Message::DownloadNextFile }, |m| m);
                        }
                    }
                    Err(e) => {
                        self.files.clear();
                        eprintln!("Ошибка загрузки файлов: {e}");
                    }
                }
                Task::none()
            }
            Message::FileClicked(file_info) => {
                println!("FileClicked");
                if let Some(ref folder) = self.download_folder {
                    let file_path = folder.join(&file_info.name);
                    if file_path.exists() {
                        let _ = open::that(file_path);
                    } else {
                        let client = Client::new();
                        let url = format!("{BASE_URL}/files/{}", file_info.name);
                        let file_name = file_info.name.clone();
                        let folder = folder.clone();
                        let auth_header = self.get_auth_header();

                        return Task::perform(async move {
                            let mut req = client.get(&url);
                            if let Some(token) = auth_header {
                                req = req.header("Authorization", token);
                            }
                            match req.send().await {
                                Ok(resp) if resp.status().is_success() => {
                                    match resp.bytes().await {
                                        Ok(bytes) => Ok((file_name, bytes.to_vec(), folder)),
                                        Err(e) => Err(format!("Read error: {e}")),
                                    }
                                }
                                Ok(resp) => Err(format!("HTTP {}", resp.status())),
                                Err(e) => Err(e.to_string()),
                            }
                        }, Message::FileDownloadedToFolder);
                    }
                }
                Task::none()
            }
            Message::FileDownloadedToFolder(result) => {
                println!("FileDownloadedToFolder");
                match result {
                    Ok((file_name, bytes, folder)) => {
                        let file_path = folder.join(&file_name);
                        let _ = std::fs::write(&file_path, bytes);
                        let _ = open::that(&file_path);
                        self.add_log(format!("Downloaded: {}", file_name), LogType::GitAdded);
                    }
                    Err(e) => {
                        eprintln!("Download failed: {e}");
                        self.add_log(format!("Download failed: {}", e), LogType::Error);
                    }
                }
                Task::none()
            }
            Message::UploadFile => {
                println!("UploadFile");
                let auth_header = self.get_auth_header();
                Task::perform(
                    async move {
                        let picked: Result<Option<std::path::PathBuf>, String> = tokio::task::spawn_blocking(|| {
                            rfd::FileDialog::new()
                                .set_title("Выберите файл для загрузки")
                                .pick_file()
                        })
                        .await
                        .map_err(|e| format!("Dialog error: {e}"));

                        let Ok(picked) = picked else {
                            return Err(picked.unwrap_err());
                        };

                        let Some(path) = picked else {
                            return Ok(None);
                        };

                        let file_name = path
                            .file_name()
                            .and_then(|n: &std::ffi::OsStr| n.to_str())
                            .ok_or("Invalid filename")?
                            .to_string();

                        let bytes: Result<Vec<u8>, String> = tokio::fs::read(&path)
                            .await
                            .map_err(|e| format!("Read error: {e}"));

                        let bytes = bytes?;

                        Ok(Some(FileWithBytes {
                            name: file_name,
                            size: bytes.len(),
                            bytes,
                            auth_header,
                        }))
                    },
                    Message::FileSelected,
                )
            }

            Message::FileSelected(result) => {
                println!("FileSelected");
                match result {
                    Ok(Some(file_data)) => {
                        let client = Client::new();
                        let file_name = file_data.name.clone();
                        let bytes = file_data.bytes;
                        let auth_header = file_data.auth_header;

                        Task::perform(
                            async move {
                                let part = reqwest::multipart::Part::bytes(bytes)
                                    .file_name(file_name.clone());

                                let form = reqwest::multipart::Form::new()
                                    .part("file", part);

                                let url = format!("{BASE_URL}/files");
                                let mut req = client.post(&url).multipart(form);
                                if let Some(token) = auth_header {
                                    req = req.header("Authorization", token);
                                    println!("{:?}",req);
                                }
                                let resp: reqwest::Response = req.send().await
                                    .map_err(|e| e.to_string())?;

                                if resp.status().is_success() {
                                    match resp.json::<serde_json::Value>().await {
                                        Ok(v) => Ok(v["uploaded"].as_array()
                                            .and_then(|arr: &Vec<serde_json::Value>| arr.first())
                                            .and_then(|v: &serde_json::Value| v.as_str())
                                            .unwrap_or(&file_name)
                                            .to_string()),
                                        Err(_) => Ok(file_name),
                                    }
                                } else {
                                    Err(format!("HTTP {}", resp.status()))
                                }
                            },
                            Message::UploadResult,
                        )
                    }
                    Ok(None) => Task::none(),
                    Err(e) => {
                        self.upload_error = Some(format!("Ошибка загрузки: {e}"));
                        Task::none()
                    }
                }
            }

            Message::UploadResult(result) => {
                println!("UploadResult");
                match result {
                    Ok(_uploaded_name) => {
                        self.upload_loading = false;
                        self.upload_error = None;
                        self.add_log("File uploaded successfully".to_string(), LogType::Success);
                        return Task::perform(async { Message::FilesFetch }, |m| m);
                    }
                    Err(e) => {
                        self.upload_loading = false;
                        self.upload_error = Some(format!("Ошибка загрузки: {e}"));
                        self.add_log(format!("Upload failed: {}", e), LogType::Error);
                    }
                }
                Task::none()
            }
            Message::DownloadNextFile => {
                println!("DownloadNextFile");
                if let Some(file_info) = self.files_to_download.first() {
                    let client = Client::new();
                    let url = format!("{BASE_URL}/files/{}", file_info.name);
                    let file_name = file_info.name.clone();
                    let folder = self.download_folder.clone().unwrap();
                    let auth_header = self.get_auth_header();

                    return Task::perform(async move {
                        let mut req = client.get(&url);
                        if let Some(token) = auth_header {
                            req = req.header("Authorization", token);
                        }
                        match req.send().await {
                            Ok(resp) if resp.status().is_success() => {
                                match resp.bytes().await {
                                    Ok(bytes) => Ok((file_name, bytes.to_vec(), folder)),
                                    Err(e) => Err(format!("Read error: {e}")),
                                }
                            }
                            Ok(resp) => Err(format!("HTTP {}", resp.status())),
                            Err(e) => Err(e.to_string()),
                        }
                    }, Message::FileDownloadedToLocal);
                }
                Task::none()
            }
            Message::FileDownloadedToLocal(result) => {
                println!("FileDownloadedToLocal");
                match result {
                    Ok((file_name, bytes, folder)) => {
                        let file_path = folder.join(&file_name);
                        if !file_path.exists() {
                            let _ = std::fs::write(&file_path, &bytes);
                        }
                        if let Ok(content) = std::str::from_utf8(&bytes) {
                            self.track_file(&file_name, content.to_string());
                        }
                        self.files_to_download.remove(0);
                        if self.files_to_download.is_empty() {
                            return Task::none();
                        }
                        return Task::perform(async { Message::DownloadNextFile }, |m| m);
                    }
                    Err(e) => {
                        eprintln!("Download failed: {e}");
                        self.files_to_download.remove(0);
                        if self.files_to_download.is_empty() {
                            return Task::none();
                        }
                        return Task::perform(async { Message::DownloadNextFile }, |m| m);
                    }
                }
            }
            Message::SyncFile(file_name) => {
                println!("SyncFile");
                if let Some(ref folder) = self.download_folder {
                    let file_path = folder.join(&file_name);
                    if file_path.exists() && self.modified_files.contains(&file_name) {
                        self.modified_files.remove(&file_name);
                        let client = Client::new();
                        let file_name_clone = file_name.clone();
                        let auth_header = self.get_auth_header();

                        return Task::perform(async move {
                            let bytes = match tokio::fs::read(&file_path).await {
                                Ok(b) => b,
                                Err(e) => return Err(format!("Read error: {e}")),
                            };

                            let part = reqwest::multipart::Part::bytes(bytes.to_vec())
                                .file_name(file_name_clone.clone());
                            let form = reqwest::multipart::Form::new()
                                .part("file", part);
                            let url = format!("{BASE_URL}/files");

                            let mut req = client.post(&url).multipart(form);
                            if let Some(token) = auth_header {
                                req = req.header("Authorization", token);
                            }

                            match req.send().await {
                                Ok(resp) if resp.status().is_success() => Ok(file_name_clone),
                                Ok(resp) => Err(format!("HTTP {}", resp.status())),
                                Err(e) => Err(e.to_string()),
                            }
                        }, Message::FileSynced);
                    }
                }
                Task::none()
            }
            Message::FileSynced(result) => {
                println!("FileSynced");
                match result {
                    Ok(name) => {
                        println!("File synced: {}", name);
                        self.add_log(format!("Synced: {}", name), LogType::GitModified);
                    }
                    Err(e) => {
                        eprintln!("Sync failed: {e}");
                        self.add_log(format!("Sync failed: {}", e), LogType::Error);
                    }
                }
                Task::none()
            }
            Message::TabChanged(tab_index) => {
                self.active_tab = tab_index;
                Task::none()
            }
            Message::ClearTerminal => {
                self.terminal_logs.clear();
                Task::none()
            }
            Message::FileChangesChecked => {
                println!("FileChangesChecked");
                self.check_file_changes();
                Task::none()
            }
            Message::SyncAllFiles => {
                println!("SyncAllFiles");
                self.check_and_sync_files()
            }
            Message::Logout => {
                let client = Client::new();
                let auth_header = self.get_auth_header();
                
                Task::perform(async move {
                    let url = format!("{BASE_URL}/auth/logout");
                    let mut req = client.post(&url);
                    if let Some(token) = auth_header {
                        req = req.header("Authorization", token);
                    }
                    let _ = req.send().await;
                    Message::AuthResult(Err("Logged out".to_string()))
                }, |m| m)
            }
        }
    }

    fn check_and_sync_files(&mut self) -> Task<Message> {
        if let Some(ref folder) = self.download_folder {
            if let Ok(entries) = std::fs::read_dir(folder) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) {
                        if self.files.iter().any(|f| f.name == name) {
                            self.modified_files.insert(name);
                        }
                    }
                }
            }
        }
        if let Some(file_name) = self.modified_files.iter().next().cloned() {
            return Task::perform(async move {
                Message::SyncFile(file_name)
            }, |m| m);
        }
        Task::none()
    }

    fn add_log(&mut self, message: String, log_type: LogType) {
        self.terminal_logs.push(LogEntry { message, log_type });
        if self.terminal_logs.len() > 1000 {
            self.terminal_logs.remove(0);
        }
    }

    fn track_file(&mut self, file_name: &str, content: String) {
        if let Some(ref folder) = self.download_folder {
            let path = folder.join(file_name);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            self.tracked_files.insert(file_name.to_string(), TrackedFile {
                path,
                content,
                last_modified: now,
            });
        }
    }

    fn check_file_changes(&mut self) {
        if let Some(ref folder) = self.download_folder {
            let mut changes_to_log: Vec<(String, String, String)> = Vec::new();
            
            for (file_name, tracked) in &mut self.tracked_files {
                let file_path = folder.join(file_name);
                if file_path.exists() {
                    if let Ok(new_content) = std::fs::read_to_string(&file_path) {
                        if new_content != tracked.content {
                            changes_to_log.push((file_name.clone(), tracked.content.clone(), new_content.clone()));
                            tracked.content = new_content;
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            tracked.last_modified = now;
                        }
                    }
                }
            }

            if changes_to_log.len() == 0 {
                self.add_log("No changes spotted".to_string(), LogType::Info);
                return;
            }
            
            for (file_name, old_content, new_content) in changes_to_log {
                self.show_diff(&file_name, &old_content, &new_content);
            }
        }
    }

    fn show_diff(&mut self, file_name: &str, old_content: &str, new_content: &str) {        
        self.add_log(format!("diff [{}]", file_name), LogType::DiffHeader);
        self.add_log(format!("--- a/{}", file_name), LogType::DiffHeader);
        self.add_log(format!("+++ b/{}", file_name), LogType::DiffHeader);
        
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();
        
        let (removed, added) = simple_diff(&old_lines, &new_lines);
        
        for line in removed {
            self.add_log(line, LogType::DiffRemoved);
        }
        for line in added {
            self.add_log(line, LogType::DiffAdded);
        }
    }
}

fn simple_diff(old_lines: &[&str], new_lines: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut removed = Vec::new();
    let mut added = Vec::new();
    
    let mut old_idx = 0;
    let mut new_idx = 0;
    
    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        if old_idx < old_lines.len() && new_idx < new_lines.len() {
            if old_lines[old_idx] == new_lines[new_idx] {
                old_idx += 1;
                new_idx += 1;
            } else {
                removed.push(format!("-{}", old_lines[old_idx]));
                added.push(format!("+{}", new_lines[new_idx]));
                old_idx += 1;
                new_idx += 1;
            }
        } else if old_idx < old_lines.len() {
            removed.push(format!("-{}", old_lines[old_idx]));
            old_idx += 1;
        } else {
            added.push(format!("+{}", new_lines[new_idx]));
            new_idx += 1;
        }
    }

    (removed, added)
}

impl State {
    fn create_terminal_tab(&self) -> Element<'_, Message> {
        let logs: Vec<Element<'_, Message>> = self
            .terminal_logs
            .iter()
            .map(|entry| {
                text(&entry.message)
                    .size(12)
                    .color(entry.log_type.color())
                    .into()
            })
            .collect();

        let clear_button = button(
            row![text("Очистить").size(12)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::ClearTerminal)
        .padding([6, 12])
        .style(move |_: &_, _: iced::widget::button::Status| iced::widget::button::Style {
            background: Some(Color::from_rgb(0.6, 0.2, 0.2).into()),
            ..Default::default()
        });

        let check_changes_button = button(
            row![text("Проверить изменения").size(12)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::FileChangesChecked)
        .padding([6, 12])
        .style(move |_: &_, _: iced::widget::button::Status| iced::widget::button::Style {
            background: Some(Color::from_rgb(0.2, 0.6, 0.8).into()),
            ..Default::default()
        });

        let sync_button = button(
            row![text("Синхронизировать").size(12)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::SyncAllFiles)
        .padding([6, 12])
        .style(move |_: &_, _: iced::widget::button::Status| iced::widget::button::Style {
            background: Some(Color::from_rgb(0.2, 0.8, 0.3).into()),
            ..Default::default()
        });

        let header = row![clear_button, check_changes_button, sync_button].spacing(10);

        let terminal_content = if logs.is_empty() {
            column![text("Нет логов").color(Color::from_rgb(0.5, 0.5, 0.5)).size(14)]
                .align_x(Alignment::Center)
                .padding(20)
        } else {
            column(logs)
        };

        let scrollable_terminal = container(
            scrollable(terminal_content)
                .width(Length::Fill)
                .height(Length::Fill)
        )
        .padding(10)
        .style(move |_| container::Style {
            background: Some(Color::from_rgb(0.08, 0.08, 0.08).into()),
            border: iced::border::Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        column![header, scrollable_terminal]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    fn create_main_window(&self) -> Element<'_, Message> {
        let refresh_button = button(
            row![text("Обновить").size(14),]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::FilesFetch)
        .padding([8, 16]);

        let file_size: f32 = 100.0;
        let columns: usize = 5;

        let files_rows: Vec<Element<'_, Message>> = self
            .files
            .chunks(columns)
            .map(|chunk| {
                let file_buttons: Vec<Element<'_, Message>> = chunk
                    .iter()
                    .map(|file_info| {
                        let content = column![
                            text("🗎").size(44),
                            container(
                                text(&file_info.name)
                                    .size(11)
                                    .shaping(text::Shaping::Advanced)
                            )
                            .width(Length::Fixed(file_size))
                            .height(Length::Fixed(40.0))
                            .align_x(iced::alignment::Horizontal::Center)
                            .center_y(Length::Fixed(40.0))
                            .clip(true),
                            text(format!("{} KB", file_info.size / 1024)).size(9),
                        ]
                        .spacing(4)
                        .align_x(Alignment::Center);

                        let btn = button(content)
                            .on_press(Message::FileClicked(file_info.clone()))
                            .width(Length::Fixed(file_size))
                            .height(Length::Fixed(file_size))
                            .padding(10);

                        tooltip(btn, text(&file_info.name).size(14), tooltip::Position::FollowCursor)
                            .into()
                    })
                    .collect();

                let mut row_widgets: Vec<Element<'_, Message>> = file_buttons;
                while row_widgets.len() < columns {
                    row_widgets.push(container("").width(Length::Fixed(file_size)).into());
                }
                row(row_widgets).spacing(10).into()
            })
            .collect();

        let upload_button = button(
            row![text("Загрузить").size(14)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::UploadFile)
        .padding([8, 16])
        .style(move |_: &_, _: iced::widget::button::Status| iced::widget::button::Style {
            background: Some(Color::from_rgb(0.2, 0.6, 0.3).into()),
            ..Default::default()
        });

        let header_row = row![refresh_button, upload_button].spacing(10);

        let content = if self.files_loading {
            column![text("Загрузка списка...").size(16)].align_x(Alignment::Center)
        } else if self.upload_loading {
            column![
                text("Загрузка файла...").size(16),
            ].align_x(Alignment::Center)
        } else {
            let mut content_col: Vec<Element<'_, Message>> = vec![header_row.into()];
            
            if let Some(e) = &self.upload_error {
                content_col.push(
                    container(text(e).color(Color::from_rgb(1.0, 0.3, 0.3)))
                        .padding(5)
                        .into()
                );
            }
            
            content_col.push(column(files_rows).spacing(10).padding(10).into());
            
            column(content_col).spacing(10)
        };

        container(content).width(Length::Fill).height(Length::Fill).padding(20).into()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.is_authenticated {
            let main_content = self.create_main_window();
            let terminal_content = self.create_terminal_tab();

            let tab_button_style = |active: bool| -> container::Style {
                if active {
                    container::Style {
                        background: Some(Color::from_rgb(0.2, 0.6, 0.3).into()),
                        text_color: Some(Color::WHITE),
                        border: iced::border::Border {
                            radius: 10.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    container::Style {
                        background: Some(Color::from_rgb(0.15, 0.15, 0.15).into()),
                        text_color: Some(Color::WHITE),
                        border: iced::border::Border {
                            radius: 10.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }
            };

            let tab1 = button(
                container(text("Главная").size(14))
                    .padding([10, 20])
                    .style(move |_| tab_button_style(self.active_tab == 0))
            )
            .on_press(Message::TabChanged(0));

            let tab2 = button(
                container(text("Терминал").size(14))
                    .padding([10, 20])
                    .style(move |_| tab_button_style(self.active_tab == 1))
            )
            .on_press(Message::TabChanged(1));

            let tab_bar = row![tab1, tab2]
                .spacing(5);

            let user_info = self.current_user.as_ref().map(|user| {
                text(format!("{} ({})", user.username, user.login)).size(14)
            });
            
            let logout_button = button(
                row![text("Выйти").size(14)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::Logout)
            .padding([8, 16])
            .style(move |_: &_, _: iced::widget::button::Status| iced::widget::button::Style {
                background: Some(Color::from_rgb(0.8, 0.2, 0.2).into()),
                ..Default::default()
            });

            let header_row = row![user_info, logout_button].spacing(10);

            let content = match self.active_tab {
                0 => main_content,
                _ => terminal_content,
            };

            return column![header_row, tab_bar, content]
                .spacing(10)
                .into();
        }

        let error_message = self.auth_state.error.as_ref().map(|error| {
            container(text(error).color(Color::WHITE))
                .padding([5, 10])
                .style(|_| container::Style {
                    background: Some(Color::from_rgb(0.8, 0.2, 0.2).into()),
                    border: iced::border::Border { radius: 5.0.into(), ..Default::default() },
                    ..Default::default()
                })
        });

        let auth_form = column![
            text("Авторизация").size(24),
            text_input("Логин", &self.auth_state.login).on_input(Message::LoginChanged),
            text_input("Пароль", &self.auth_state.password)
                .on_input(Message::PasswordChanged)
                .secure(true),
            button("Войти").on_press(Message::AuthSubmit),
            error_message,
        ]
        .spacing(10)
        .align_x(Alignment::Center);

        container(
            container(auth_form)
                .padding(20)
                .width(Length::Fixed(300.0))
                .style(|_| container::Style {
                    background: Some(Color::from_rgb(0.15, 0.15, 0.15).into()),
                    border: iced::border::Border { radius: 10.0.into(), ..Default::default() },
                    ..Default::default()
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}

fn main() -> iced::Result {
    iced::run(State::update, State::view)
}
