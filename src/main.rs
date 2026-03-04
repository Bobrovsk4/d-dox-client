use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{button, column, container, row, text, text_input},
};
use reqwest::Client;

#[derive(Default)]
pub struct State {
    auth_state: AuthState,
    is_authenticated: bool,
    files: Vec<String>,
    files_loading: bool,
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

#[derive(Debug, Clone)]
pub enum Message {
    LoginChanged(String),
    PasswordChanged(String),
    AuthSubmit,
    AuthResult(Result<(), String>),
    FilesFetch,
    FilesReceived(Result<Vec<String>, String>),
    FileClicked(String),
}

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
                        let url = "http://localhost/auth";
                        let response = client
                            .post(url)
                            .json(&serde_json::json!({
                                "login": login,
                                "password": password,
                            }))
                            .send()
                            .await;

                        match response {
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
                    Err(e) => {
                        self.auth_state.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::FilesFetch => {
                self.files_loading = true;
                let client = Client::new();
                Task::perform(
                    async move {
                        let url = "http://localhost/files";
                        let response = client.get(url).send().await;
                        match response {
                            Ok(resp) if resp.status().is_success() => {
                                let files: Vec<String> = resp.json().await.unwrap_or_default();
                                Ok(files)
                            }
                            Ok(resp) => Err(format!("Ошибка: {}", resp.status())),
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
                        self.files = files;
                    }
                    Err(e) => {
                        self.files = vec![];
                        eprintln!("Ошибка загрузки файлов: {}", e);
                    }
                }
                Task::none()
            }
            Message::FileClicked(filename) => {
                eprintln!("{}", filename);
                Task::none()
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
                    .map(|filename| {
                        button(
                            column![text("-").size(48), text(filename).size(11),]
                                .spacing(8)
                                .align_x(Alignment::Center),
                        )
                        .on_press(Message::FileClicked(filename.clone()))
                        .width(Length::Fixed(file_size))
                        .height(Length::Fixed(file_size))
                        .padding(10)
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

        let content = if self.files_loading {
            column![text("Загрузка...").size(16),]
                .spacing(10)
                .align_x(Alignment::Center)
        } else {
            column![refresh_button, column(files_rows).spacing(10).padding(10),].spacing(10)
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.is_authenticated {
            return self.create_main_window();
        }

        let error_message: Option<container::Container<'_, Message>> =
            if let Some(error) = &self.auth_state.error {
                Some(
                    container(text(error).color(Color::WHITE))
                        .padding([5, 10])
                        .style(move |_theme: &iced::Theme| container::Style {
                            background: Some(Color::from_rgb(0.8, 0.2, 0.2).into()),
                            border: iced::border::Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                )
            } else {
                None
            };

        let auth_form = column![
            text("Авторизация").size(24),
            text_input("Логин", &self.auth_state.login).on_input(Message::LoginChanged),
            text_input("Пароль", &self.auth_state.password)
                .on_input(Message::PasswordChanged)
                .secure(true),
            button("Войти").on_press(Message::AuthSubmit),
            error_message
        ]
        .spacing(10)
        .align_x(Alignment::Center);

        container(
            container(auth_form)
                .padding(20)
                .width(Length::Fixed(300.0))
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(Color::from_rgb(0.15, 0.15, 0.15).into()),
                    border: iced::border::Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
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
