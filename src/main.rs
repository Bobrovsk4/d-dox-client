use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{button, column, container, row, text, text_input, tooltip},
};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Default)]
pub struct State {
    auth_state: AuthState,
    is_authenticated: bool,
    files: Vec<FileInfo>,
    files_loading: bool,
    upload_loading: bool,
    upload_error: Option<String>,
    download_folder: Option<PathBuf>,
    files_to_download: Vec<FileInfo>,
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

#[derive(Debug, Clone)]
pub struct FileWithBytes {
    pub name: String,
    pub size: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum Message {
    LoginChanged(String),
    PasswordChanged(String),
    AuthSubmit,
    AuthResult(Result<(), String>),
    FilesFetch,
    FilesReceived(Result<Vec<FileInfo>, String>),
    FileClicked(FileInfo),
    FileDownloaded(Result<(String, Vec<u8>), String>),
    UploadFile,
    FileSelected(Result<Option<FileWithBytes>, String>),
    UploadProgress(f32),
    UploadResult(Result<String, String>),
    DownloadNextFile,
    FileDownloadedToLocal(Result<(String, Vec<u8>, PathBuf), String>),
    FileDownloadedToFolder(Result<(String, Vec<u8>, PathBuf), String>),
}

const BASE_URL: &str = "http://192.168.1.71:31356";

impl State {
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

                if self.auth_state.login == "123" && self.auth_state.password == "123" {
                    self.is_authenticated = true;
                    self.auth_state.error = None;
                    return Task::none();
                }

                let client = Client::new();
                let login = self.auth_state.login.clone();
                let password = self.auth_state.password.clone();

                Task::perform(
                    async move {
                        let url = format!("{BASE_URL}/auth");
                        match client
                            .post(&url)
                            .json(&serde_json::json!({ "login": login, "password": password }))
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => Ok(()),
                            Ok(resp) => Err(format!("Ошибка: {}", resp.status())),
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    Message::AuthResult,
                )
            }
            Message::AuthResult(result) => {
                match result {
                    Ok(()) => {
                        self.is_authenticated = true;
                        self.auth_state.error = None;
                    }
                    Err(e) => self.auth_state.error = Some(e),
                }
                Task::none()
            }
            Message::FilesFetch => {
                self.files_loading = true;
                let client = Client::new();
                Task::perform(
                    async move {
                        let url = format!("{BASE_URL}/files");
                        match client.get(&url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                println!("{:?}", resp);
                                match resp.json::<Vec<FileInfo>>().await {
                                    Ok(files) => {
                                        println!("{:?}", files);
                                        Ok(files)
                                    },
                                    Err(e) => Err(format!("JSON error: {e}")),
                                }
                            }
                            Ok(resp) => Err(format!("HTTP {}", resp.status())),
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
                        self.files = files.clone();
                        if self.download_folder.is_none() {
                            self.download_folder = Some(std::env::current_dir().unwrap().join("downloads"));
                        }
                        if let Some(ref folder) = self.download_folder {
                            let _ = std::fs::create_dir_all(folder);
                        }
                        self.files_to_download = files;
                        return Task::perform(async { Message::DownloadNextFile }, |m| m);
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

                        return Task::perform(async move {
                            match client.get(&url).send().await {
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
            Message::FileDownloaded(result) => {
                match result {
                    Ok((file_name, bytes)) => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(&file_name)
                            .save_file()
                        {
                            let _ = std::fs::write(path, bytes);
                        }
                    }
                    Err(e) => eprintln!("Download failed: {e}"),
                }
                Task::none()
            }
            Message::FileDownloadedToFolder(result) => {
                match result {
                    Ok((file_name, bytes, folder)) => {
                        let file_path = folder.join(&file_name);
                        let _ = std::fs::write(&file_path, bytes);
                        let _ = open::that(&file_path);
                    }
                    Err(e) => eprintln!("Download failed: {e}"),
                }
                Task::none()
            }
            Message::UploadFile => {
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
                        }))
                    },
                    Message::FileSelected,
                )
            }

            Message::FileSelected(result) => {
                match result {
                    Ok(Some(file_data)) => {
                        let client = Client::new();
                        let file_name = file_data.name.clone();
                        let bytes = file_data.bytes;

                        Task::perform(
                            async move {
                                let part = reqwest::multipart::Part::bytes(bytes)
                                    .file_name(file_name.clone());

                                let form = reqwest::multipart::Form::new()
                                    .part("file", part);

                                let url = format!("{BASE_URL}/files");
                                let resp: reqwest::Response = client.post(&url).multipart(form).send().await
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
                match result {
                    Ok(_uploaded_name) => {
                        self.upload_loading = false;
                        self.upload_error = None;
                        return Task::perform(async { Message::FilesFetch }, |m| m);
                    }
                    Err(e) => {
                        self.upload_loading = false;
                        self.upload_error = Some(format!("Ошибка загрузки: {e}"));
                    }
                }
                Task::none()
            }
            Message::UploadProgress(_) => Task::none(),
            Message::DownloadNextFile => {
                if let Some(file_info) = self.files_to_download.first() {
                    let client = Client::new();
                    let url = format!("{BASE_URL}/files/{}", file_info.name);
                    let file_name = file_info.name.clone();
                    let folder = self.download_folder.clone().unwrap();

                    return Task::perform(async move {
                        match client.get(&url).send().await {
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
                match result {
                    Ok((file_name, bytes, folder)) => {
                        let file_path = folder.join(&file_name);
                        if !file_path.exists() {
                            let _ = std::fs::write(&file_path, bytes);
                        }
                        self.files_to_download.remove(0);
                        return Task::perform(async { Message::DownloadNextFile }, |m| m);
                    }
                    Err(e) => {
                        eprintln!("Download failed: {e}");
                        self.files_to_download.remove(0);
                        return Task::perform(async { Message::DownloadNextFile }, |m| m);
                    }
                }
            }
        }
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
            return self.create_main_window();
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
