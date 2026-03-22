mod structs;
use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{button, column, container, row, scrollable, text, text_input, tooltip},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::structs::*;

const BASE_URL: &str = "http://192.168.1.71:31356";

impl State {
    fn get_auth_header(&self) -> Option<String> {
        self.jwt_token
            .as_ref()
            .map(|token| format!("Bearer {token}"))
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
                self.files_loading = true;
                let client = Client::new();
                let auth_header = self.get_auth_header();
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
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    Message::FilesReceived,
                )
            }
            Message::FilesReceived(result) => {
                self.files_loading = false;
                match result {
                    Ok(files) => {
                        self.add_log(
                            format!("Files fetched: {}", files.len()),
                            LogType::GitBranch,
                        );
                        self.files = files.clone();
                        if self.download_folder.is_none() {
                            self.download_folder =
                                Some(std::env::current_dir().unwrap().join("downloads"));
                        }
                        if let Some(ref folder) = self.download_folder {
                            let _ = std::fs::create_dir_all(folder);
                        }
                        self.files_to_download = files.clone();
                        let folder = self.download_folder.clone().unwrap();
                        for file in &files {
                            self.tracked_files
                                .entry(file.name.clone())
                                .and_modify(|tracked| {
                                    tracked.version = file.version;
                                    tracked.file_id = file.id;
                                })
                                .or_insert_with(|| TrackedFile {
                                    _path: folder.join(&file.name),
                                    content: String::new(),
                                    last_modified: 0,
                                    version: file.version,
                                    file_id: file.id,
                                });
                        }
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

                        return Task::perform(
                            async move {
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
                            },
                            Message::FileDownloadedToFolder,
                        );
                    }
                }
                Task::none()
            }
            Message::FileDownloadedToFolder(result) => {
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
                let auth_header = self.get_auth_header();
                Task::perform(
                    async move {
                        let picked: Result<Option<std::path::PathBuf>, String> =
                            tokio::task::spawn_blocking(|| {
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
                            _size: bytes.len(),
                            bytes,
                            auth_header,
                        }))
                    },
                    Message::FileSelected,
                )
            }

            Message::FileSelected(result) => match result {
                Ok(Some(file_data)) => {
                    let client = Client::new();
                    let file_name = file_data.name.clone();
                    let bytes = file_data.bytes;
                    let auth_header = file_data.auth_header;

                    Task::perform(
                        async move {
                            let part =
                                reqwest::multipart::Part::bytes(bytes).file_name(file_name.clone());

                            let form = reqwest::multipart::Form::new().part("file", part);

                            let url = format!("{BASE_URL}/files");
                            let mut req = client.post(&url).multipart(form);
                            if let Some(token) = auth_header {
                                req = req.header("Authorization", token);
                                println!("{:?}", req);
                            }
                            let resp: reqwest::Response =
                                req.send().await.map_err(|e| e.to_string())?;

                            if resp.status().is_success() {
                                match resp.json::<serde_json::Value>().await {
                                    Ok(v) => Ok(v["uploaded"]
                                        .as_array()
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
            },

            Message::UploadResult(result) => {
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
                if let Some(file_info) = self.files_to_download.first() {
                    let client = Client::new();
                    let url = format!("{BASE_URL}/files/{}", file_info.name);
                    let file_name = file_info.name.clone();
                    let folder = self.download_folder.clone().unwrap();
                    let auth_header = self.get_auth_header();

                    return Task::perform(
                        async move {
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
                        },
                        Message::FileDownloadedToLocal,
                    );
                }
                Task::none()
            }
            Message::FileDownloadedToLocal(result) => match result {
                Ok((file_name, bytes, folder)) => {
                    let file_path = folder.join(&file_name);
                    if !file_path.exists() {
                        let _ = std::fs::write(&file_path, &bytes);
                    }
                    if let Ok(content) = std::str::from_utf8(&bytes) {
                        self.track_file(&file_name, content.to_string());
                    }
                    if let Some(file_info) = self.files.iter().find(|f| f.name == file_name) {
                        if let Some(tracked) = self.tracked_files.get_mut(&file_name) {
                            tracked.version = file_info.version;
                        }
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
            },
            Message::SyncFile(file_name, files_to_sync) => {
                if let Some(ref folder) = self.download_folder {
                    let file_path = folder.join(&file_name);
                    if file_path.exists() {
                        let client = Client::new();
                        let auth_header = self.get_auth_header();
                        let files_to_sync_clone = files_to_sync.clone();

                        let tracked = self.tracked_files.get(&file_name);
                        let file_id = tracked.map(|t| t.file_id).unwrap_or(0);
                        let version = tracked.map(|t| t.version).unwrap_or(1);

                        return Task::perform(
                            async move {
                                let bytes = match tokio::fs::read(&file_path).await {
                                    Ok(b) => b,
                                    Err(e) => return Err(format!("Read error: {e}")),
                                };

                                let size = bytes.len() as i64;

                                let sync_url = format!("{BASE_URL}/files/sync");

                                let file_part = reqwest::multipart::Part::bytes(bytes.to_vec())
                                    .file_name(file_name.clone());
                                let file_id_part =
                                    reqwest::multipart::Part::text(file_id.to_string());
                                let version_part =
                                    reqwest::multipart::Part::text(version.to_string());
                                let size_part = reqwest::multipart::Part::text(size.to_string());

                                let form = reqwest::multipart::Form::new()
                                    .part("file", file_part)
                                    .part("file_id", file_id_part)
                                    .part("version", version_part)
                                    .part("size", size_part);

                                let mut req = client.post(&sync_url).multipart(form);
                                if let Some(token) = auth_header.clone() {
                                    req = req.header("Authorization", token);
                                }

                                match req.send().await {
                                    Ok(resp) if resp.status().is_success() => {
                                        match resp.json::<serde_json::Value>().await {
                                            Ok(v) => {
                                                let new_version = v
                                                    .get("version")
                                                    .and_then(|v| v.as_i64())
                                                    .map(|v| v as i32)
                                                    .unwrap_or(version + 1);
                                                Ok((file_name, new_version))
                                            }
                                            Err(_) => Ok((file_name, version + 1)),
                                        }
                                    }
                                    Ok(resp) => {
                                        let status = resp.status();
                                        let body = resp.text().await.unwrap_or_default();
                                        Err(format!("Version conflict: {} (HTTP {})", body, status))
                                    }
                                    Err(e) => Err(e.to_string()),
                                }
                            },
                            move |result| Message::FileSyncedResult(result, files_to_sync_clone),
                        );
                    }
                }
                Task::none()
            }
            Message::FileSyncedResult(result, mut files_to_sync) => {
                match result {
                    Ok((name, new_version)) => {
                        files_to_sync.remove(0);
                        self.modified_files.remove(&name);
                        println!("File synced: {}", name);
                        self.add_log(format!("Synced: {}", name), LogType::GitModified);

                        if let Some(ref folder) = self.download_folder {
                            if let Ok(content) = std::fs::read_to_string(folder.join(&name)) {
                                self.track_file(&name, content);
                                if let Some(tracked) = self.tracked_files.get_mut(&name) {
                                    tracked.version = new_version;
                                }
                            }
                        }

                        if !files_to_sync.is_empty() {
                            return self.sync_next_file(files_to_sync);
                        }
                        self.add_log(
                            "All files synced successfully".to_string(),
                            LogType::Success,
                        );
                    }
                    Err(e) => {
                        let failed_file = files_to_sync.first().cloned().unwrap_or_default();
                        files_to_sync.remove(0);
                        if e.contains("Version conflict") && !failed_file.is_empty() {
                            if let Some(ref folder) = self.download_folder {
                                let file_path = folder.join(&failed_file);
                                if let Ok(content) = std::fs::read_to_string(&file_path) {
                                    let server_version = extract_version_from_error(&e);

                                    self.version_conflicts.insert(
                                        failed_file.clone(),
                                        VersionConflict {
                                            file_name: failed_file.clone(),
                                            local_content: content,
                                            server_version,
                                        },
                                    );

                                    self.add_log(
                                        format!(
                                            "Conflict detected for '{}'. Server version: {}",
                                            failed_file, server_version
                                        ),
                                        LogType::Error,
                                    );
                                }
                            }
                        } else {
                            eprintln!("Sync failed: {e}");
                            self.add_log(format!("Sync failed: {}", e), LogType::Error);
                        }

                        if !files_to_sync.is_empty() {
                            return self.sync_next_file(files_to_sync);
                        }
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
                return self.check_file_changes();
            }
            Message::SyncAllFiles => self.check_and_sync_files(),
            Message::Logout => {
                let client = Client::new();
                let auth_header = self.get_auth_header();

                Task::perform(
                    async move {
                        let url = format!("{BASE_URL}/auth/logout");
                        let mut req = client.post(&url);
                        if let Some(token) = auth_header {
                            req = req.header("Authorization", token);
                        }
                        let _ = req.send().await;
                        Message::AuthResult(Err("Logged out".to_string()))
                    },
                    |m| m,
                )
            }
            Message::ResolveConflictKeepLocal(file_name) => {
                self.version_conflicts.remove(&file_name);
                self.add_log(
                    format!("Conflict resolved: kept local version of '{}'", file_name),
                    LogType::Success,
                );
                self.modified_files.insert(file_name);
                Task::none()
            }
            Message::ResolveConflictKeepServer(file_name) => {
                self.version_conflicts.remove(&file_name);
                self.modified_files.remove(&file_name);
                self.add_log(
                    format!("Conflict resolved: kept server version of '{}'", file_name),
                    LogType::Info,
                );
                Task::none()
            }
        }
    }

    fn check_and_sync_files(&mut self) -> Task<Message> {
        if let Some(ref folder) = self.download_folder {
            for (file_name, tracked) in &mut self.tracked_files {
                let file_path = folder.join(file_name);
                if file_path.exists() {
                    if let Ok(new_content) = std::fs::read_to_string(&file_path) {
                        if new_content != tracked.content {
                            self.modified_files.insert(file_name.clone());
                        }
                    }
                }
            }
        }

        let files_to_sync: Vec<String> = self.modified_files.iter().cloned().collect();

        if files_to_sync.is_empty() {
            self.add_log("No files to sync".to_string(), LogType::Info);
            return Task::none();
        }

        self.add_log(
            format!("Found {} file(s) to sync", files_to_sync.len()),
            LogType::GitBranch,
        );
        self.sync_next_file(files_to_sync)
    }

    fn sync_next_file(&self, files_to_sync: Vec<String>) -> Task<Message> {
        if let Some(file_name) = files_to_sync.first().cloned() {
            return Task::perform(
                async move { Message::SyncFile(file_name, files_to_sync) },
                |m| m,
            );
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

            let (version, file_id) = self
                .tracked_files
                .get(file_name)
                .map(|t| (t.version, t.file_id))
                .unwrap_or((1, 0));

            self.tracked_files.insert(
                file_name.to_string(),
                TrackedFile {
                    _path: path,
                    content,
                    last_modified: now,
                    version,
                    file_id,
                },
            );
        }
    }

    fn check_file_changes(&mut self) -> Task<Message> {
        if let Some(ref folder) = self.download_folder {
            let mut changes_to_log: Vec<(String, String, String)> = Vec::new();

            for (file_name, tracked) in &mut self.tracked_files {
                let file_path = folder.join(file_name);
                if file_path.exists() {
                    if let Ok(new_content) = std::fs::read_to_string(&file_path) {
                        if new_content != tracked.content {
                            changes_to_log.push((
                                file_name.clone(),
                                tracked.content.clone(),
                                new_content.clone(),
                            ));
                            tracked.content = new_content;
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            tracked.last_modified = now;
                            self.modified_files.insert(file_name.clone());
                        }
                    }
                }
            }

            if changes_to_log.is_empty() {
                self.add_log("No changes spotted".to_string(), LogType::Info);
                return Task::none();
            }

            for (file_name, old_content, new_content) in &changes_to_log {
                self.show_diff(file_name, old_content, new_content);
            }

            self.add_log(
                format!("Found {} changed file(s)", changes_to_log.len()),
                LogType::Warning,
            );
        }
        Task::none()
    }

    fn show_diff(&mut self, file_name: &str, old_content: &str, new_content: &str) {
        self.add_log(format!("diff [{}]", file_name), LogType::DiffHeader);

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
        let mut all_content: Vec<Element<'_, Message>> = Vec::new();

        if !self.version_conflicts.is_empty() {
            let conflict_card = container(
                column![
                    row![text("КОНФЛИКТЫ ВЕРСИЙ").size(15).color(Theme::WARNING),]
                        .spacing(Theme::SPACING_SM)
                        .align_y(Alignment::Center),
                    text("Выберите версию для каждого файла:")
                        .size(12)
                        .color(Theme::TEXT_SECONDARY),
                ]
                .spacing(Theme::SPACING_SM),
            )
            .padding([12, 16])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.25, 0.18, 0.10).into()),
                border: iced::border::Border {
                    radius: Theme::RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

            all_content.push(conflict_card.into());
            all_content.push(container(column![].height(Length::Fixed(Theme::SPACING_MD))).into());

            for (file_name, conflict) in &self.version_conflicts {
                let conflict_item = container(
                    row![
                        text(format!("📄 {}", file_name))
                            .size(12)
                            .color(Theme::TEXT_PRIMARY),
                        button(
                            row![text("Локальная").size(11)]
                                .spacing(6)
                                .align_y(Alignment::Center),
                        )
                        .on_press(Message::ResolveConflictKeepLocal(file_name.clone()))
                        .padding([6, 12])
                        .style(|_, _| button::Style {
                            background: Some(Theme::SUCCESS.into()),
                            text_color: Color::WHITE,
                            border: iced::border::Border {
                                radius: Theme::RADIUS_SM.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        button(
                            row![
                                text(format!("Серверная (v{})", conflict.server_version)).size(11)
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::ResolveConflictKeepServer(file_name.clone()))
                        .padding([6, 12])
                        .style(|_, _| button::Style {
                            background: Some(Theme::INFO.into()),
                            text_color: Color::WHITE,
                            border: iced::border::Border {
                                radius: Theme::RADIUS_SM.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(Theme::SPACING_MD)
                    .align_y(Alignment::Center),
                )
                .padding([10, 14])
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Theme::BACKGROUND_TERTIARY.into()),
                    border: iced::border::Border {
                        radius: Theme::RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });

                all_content.push(conflict_item.into());
            }

            all_content.push(container(column![].height(Length::Fixed(Theme::SPACING_LG))).into());
        }

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
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(Message::ClearTerminal)
        .padding([8, 14])
        .style(|_, _| button::Style {
            background: Some(Theme::ERROR.into()),
            text_color: Color::WHITE,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let check_changes_button = button(
            row![text("Проверить").size(12)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(Message::FileChangesChecked)
        .padding([8, 14])
        .style(|_, _| button::Style {
            background: Some(Theme::INFO.into()),
            text_color: Color::WHITE,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let sync_button = button(
            row![text("Синхронизировать").size(12)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(Message::SyncAllFiles)
        .padding([8, 14])
        .style(|_, _| button::Style {
            background: Some(Theme::SUCCESS.into()),
            text_color: Color::WHITE,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let header =
            row![clear_button, check_changes_button, sync_button].spacing(Theme::SPACING_SM);

        let terminal_content = if all_content.is_empty() && logs.is_empty() {
            column![
                container(
                    column![
                        text("📋").size(40),
                        text("Нет логов").size(14).color(Theme::TEXT_MUTED),
                    ]
                    .spacing(Theme::SPACING_SM)
                    .align_x(Alignment::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fixed(150.0))
            ]
            .align_x(Alignment::Center)
        } else {
            let mut content = all_content;
            content.extend(logs);
            column(content)
        };

        let scrollable_terminal = container(
            scrollable(terminal_content)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(Theme::SPACING_MD)
        .style(|_| container::Style {
            background: Some(Theme::BACKGROUND_SECONDARY.into()),
            border: iced::border::Border {
                radius: Theme::RADIUS_MD.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        column![header, scrollable_terminal]
            .spacing(Theme::SPACING_MD)
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
        .padding([10, 18])
        .style(|_, _| button::Style {
            background: Some(Theme::BACKGROUND_TERTIARY.into()),
            text_color: Theme::TEXT_PRIMARY,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let upload_button = button(
            row![text("Загрузить").size(14),]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::UploadFile)
        .padding([10, 18])
        .style(|_, _| button::Style {
            background: Some(Theme::SUCCESS.into()),
            text_color: Color::WHITE,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let header_row = row![refresh_button, upload_button].spacing(Theme::SPACING_MD);

        let file_card_width: f32 = 130.0;
        let file_card_height: f32 = 150.0;
        let columns: usize = 5;

        let files_rows: Vec<Element<'_, Message>> = self
            .files
            .chunks(columns)
            .map(|chunk| {
                let file_buttons: Vec<Element<'_, Message>> = chunk
                    .iter()
                    .map(|file_info| {
                        let author_text = format!("@{}", file_info.author.login);
                        let size_text = if file_info.size < 1024 {
                            format!("{} B", file_info.size)
                        } else if file_info.size < 1024 * 1024 {
                            format!("{:.1} KB", file_info.size as f32 / 1024.0)
                        } else {
                            format!("{:.1} MB", file_info.size as f32 / (1024.0 * 1024.0))
                        };

                        let content = column![
                            container(text("📄").size(40))
                                .width(Length::Fill)
                                .center_x(Length::Fill)
                                .padding(8)
                                .style(|_| container::Style {
                                    background: Some(Theme::BACKGROUND_TERTIARY.into()),
                                    border: iced::border::Border {
                                        radius: Theme::RADIUS_SM.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }),
                            container(
                                text(&file_info.name)
                                    .size(12)
                                    .color(Theme::TEXT_PRIMARY)
                                    .shaping(text::Shaping::Advanced)
                            )
                            .width(Length::Fixed(file_card_width - 20.0))
                            .height(Length::Fixed(32.0))
                            .align_x(iced::alignment::Horizontal::Center)
                            .center_y(Length::Fixed(32.0)),
                            text(author_text).size(10).color(Theme::TEXT_MUTED),
                            text(size_text).size(10).color(Theme::TEXT_SECONDARY),
                        ]
                        .spacing(Theme::SPACING_SM)
                        .align_x(Alignment::Center);

                        let btn = button(content)
                            .on_press(Message::FileClicked(file_info.clone()))
                            .width(Length::Fixed(file_card_width))
                            .height(Length::Fixed(file_card_height))
                            .padding(12)
                            .style(|_, _| button::Style {
                                background: Some(Theme::CARD_BACKGROUND.into()),
                                border: iced::border::Border {
                                    radius: Theme::RADIUS_MD.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            });

                        tooltip(
                            btn,
                            text(&file_info.name).size(13),
                            tooltip::Position::FollowCursor,
                        )
                        .into()
                    })
                    .collect();

                let mut row_widgets: Vec<Element<'_, Message>> = file_buttons;
                while row_widgets.len() < columns {
                    row_widgets.push(container("").width(Length::Fixed(file_card_width)).into());
                }
                row(row_widgets).spacing(Theme::SPACING_MD).into()
            })
            .collect();

        let content = if self.files_loading {
            column![
                container(
                    column![
                        text("Загрузка файлов...")
                            .size(16)
                            .color(Theme::TEXT_SECONDARY),
                    ]
                    .spacing(Theme::SPACING_MD)
                    .align_x(Alignment::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fixed(200.0))
            ]
            .align_x(Alignment::Center)
        } else if self.upload_loading {
            column![
                container(
                    column![
                        text("Загрузка файла...")
                            .size(16)
                            .color(Theme::TEXT_SECONDARY),
                    ]
                    .spacing(Theme::SPACING_MD)
                    .align_x(Alignment::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fixed(200.0))
            ]
            .align_x(Alignment::Center)
        } else {
            let mut content_col: Vec<Element<'_, Message>> = vec![header_row.into()];

            if let Some(e) = &self.upload_error {
                content_col.push(
                    container(
                        row![text(e).size(13).color(Color::WHITE),]
                            .spacing(Theme::SPACING_SM)
                            .align_y(Alignment::Center),
                    )
                    .padding([10, 16])
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgb(0.45, 0.15, 0.15).into()),
                        border: iced::border::Border {
                            radius: Theme::RADIUS_SM.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into(),
                );
            }

            if self.files.is_empty() {
                content_col.push(
                    container(
                        column![
                            text("📂").size(48),
                            text("Нет файлов").size(16).color(Theme::TEXT_SECONDARY),
                            text("Загрузите первый файл")
                                .size(13)
                                .color(Theme::TEXT_MUTED),
                        ]
                        .spacing(Theme::SPACING_SM)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .center_x(Length::Fill)
                    .padding(Theme::SPACING_XL)
                    .into(),
                );
            } else {
                content_col.push(
                    column(files_rows)
                        .spacing(Theme::SPACING_MD)
                        .padding(Theme::SPACING_SM)
                        .into(),
                );
            }

            column(content_col).spacing(Theme::SPACING_MD)
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Theme::SPACING_MD)
            .into()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.is_authenticated {
            let main_content = self.create_main_window();
            let terminal_content = self.create_terminal_tab();

            let tab_style =
                move |active: bool, status: iced::widget::button::Status| -> button::Style {
                    let bg = if active {
                        Theme::PRIMARY
                    } else {
                        match status {
                            iced::widget::button::Status::Hovered => Theme::BACKGROUND_TERTIARY,
                            _ => Theme::BACKGROUND_SECONDARY,
                        }
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: Color::WHITE,
                        border: iced::border::Border {
                            radius: Theme::RADIUS_SM.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                };

            let tab1 = button(text("Главная").size(14))
                .on_press(Message::TabChanged(0))
                .padding([10, 20])
                .style(move |_, status| tab_style(self.active_tab == 0, status));

            let tab2 = button(text("Терминал").size(14))
                .on_press(Message::TabChanged(1))
                .padding([10, 20])
                .style(move |_, status| tab_style(self.active_tab == 1, status));

            let tab_bar = row![tab1, tab2].spacing(Theme::SPACING_SM);

            let user_info = self.current_user.as_ref().map(|user| {
                let role_badge = user
                    .role
                    .as_ref()
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                row![
                    text("👤").size(16),
                    column![
                        text(&user.username).size(14).color(Color::WHITE),
                        text(role_badge).size(10).color(Theme::TEXT_MUTED),
                    ]
                    .spacing(2)
                ]
                .spacing(Theme::SPACING_SM)
                .align_y(Alignment::Center)
            });

            let logout_button = button(
                row![text("Выйти").size(13)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::Logout)
            .padding([8, 16])
            .style(|_, _| button::Style {
                background: Some(Theme::ERROR.into()),
                text_color: Color::WHITE,
                border: iced::border::Border {
                    radius: Theme::RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

            let header_row = row![user_info, logout_button].spacing(Theme::SPACING_MD);

            let content = match self.active_tab {
                0 => main_content,
                _ => terminal_content,
            };

            return column![header_row, tab_bar, content]
                .spacing(Theme::SPACING_MD)
                .into();
        }

        let error_message = self.auth_state.error.as_ref().map(|error| {
            container(
                row![text(error).size(13).color(Color::WHITE),]
                    .spacing(Theme::SPACING_SM)
                    .align_y(Alignment::Center),
            )
            .padding([10, 16])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.45, 0.15, 0.15).into()),
                border: iced::border::Border {
                    radius: Theme::RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
        });

        let login_input = text_input("Логин", &self.auth_state.login)
            .on_input(Message::LoginChanged)
            .padding([12, 16])
            .size(15)
            .style(move |_: &_, _: iced::widget::text_input::Status| {
                iced::widget::text_input::Style {
                    background: Theme::BACKGROUND_TERTIARY.into(),
                    border: iced::border::Border {
                        radius: Theme::RADIUS_SM.into(),
                        ..Default::default()
                    },
                    icon: Theme::TEXT_MUTED,
                    placeholder: Theme::TEXT_MUTED,
                    value: Theme::TEXT_PRIMARY,
                    selection: Theme::PRIMARY,
                }
            });

        let password_input = text_input("Пароль", &self.auth_state.password)
            .on_input(Message::PasswordChanged)
            .secure(true)
            .padding([12, 16])
            .size(15)
            .style(move |_: &_, _: iced::widget::text_input::Status| {
                iced::widget::text_input::Style {
                    background: Theme::BACKGROUND_TERTIARY.into(),
                    border: iced::border::Border {
                        radius: Theme::RADIUS_SM.into(),
                        ..Default::default()
                    },
                    icon: Theme::TEXT_MUTED,
                    placeholder: Theme::TEXT_MUTED,
                    value: Theme::TEXT_PRIMARY,
                    selection: Theme::PRIMARY,
                }
            });

        let login_button = button(
            row![text("Войти").size(15)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::AuthSubmit)
        .padding([12, 32])
        .width(Length::Fill)
        .style(|_, _| button::Style {
            background: Some(Theme::PRIMARY.into()),
            text_color: Color::WHITE,
            border: iced::border::Border {
                radius: Theme::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let auth_form = column![
            column![
                text("D-DOX").size(32).color(Theme::PRIMARY),
                text("Document Management")
                    .size(14)
                    .color(Theme::TEXT_MUTED),
            ]
            .spacing(4)
            .align_x(Alignment::Center),
            container(column![].height(Length::Fixed(24.0))),
            login_input,
            password_input,
            login_button,
            error_message,
        ]
        .spacing(Theme::SPACING_MD)
        .align_x(Alignment::Center);

        let card = container(auth_form)
            .padding(Theme::SPACING_XL)
            .width(Length::Fixed(380.0))
            .style(|_| container::Style {
                background: Some(Theme::CARD_BACKGROUND.into()),
                border: iced::border::Border {
                    radius: Theme::RADIUS_MD.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        container(
            column![
                container(text("📁").size(64)).center_x(Length::Fixed(100.0)),
                card,
            ]
            .spacing(Theme::SPACING_LG)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(Theme::BACKGROUND_PRIMARY.into()),
            ..Default::default()
        })
        .into()
    }
}

fn extract_version_from_error(error: &str) -> i32 {
    if let Some(current_pos) = error.find("current") {
        let after_current = &error[current_pos + 7..];
        for ch in after_current.chars() {
            if ch.is_ascii_digit() {
                let num_str: String = after_current
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(num) = num_str.parse::<i32>() {
                    return num;
                }
            }
        }
    }
    0
}

fn main() -> iced::Result {
    iced::run(State::update, State::view)
}
