# rumax

Асинхронная Rust-библиотека для работы с внутренним API мессенджера **Max**.

[![Rust](https://img.shields.io/badge/Rust-2021-red.svg?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.3.0-blue.svg)](Cargo.toml)

> [!WARNING]
> Протокол может измениться без предупреждения, а использование сторонних клиентов может нарушать условия сервиса. Вы используете библиотеку на свой риск.

## Что это

**rumax** — асинхронный клиент на `tokio`, реализующий протокол Max поверх двух транспортов:
- **Mobile** (бинарный протокол поверх TLS/TCP)
- **Web** (WebSocket)

Библиотека лежит в основе неофициального клиента **[Max+](https://github.com/junioraww/maxplus)**.

## Возможности

- **Авторизация и сессии:** вход по номеру телефона и SMS-коду, регистрация, облачный пароль (2FA), QR-авторизация (`Web`) и вход по токену сессии.
- **Транспорт и сеть:** Mobile TLS и WebSocket, переключение `Identity::generate_mobile()` / `Identity::generate_web()`, настройка `ProxyConfig` (HTTP/SOCKS).
- **Сообщения:** отправка, редактирование, удаление, пересылка, закрепление, реакции и поиск по сообщениям.
- **Чаты, группы и каналы:** список чатов, управление участниками, пригласительные ссылки, работа с каналами.
- **Файлы и медиа:** загрузка и скачивание файлов, фото, видео, голосовых сообщений и видеокружков.
- **Прочее:** контакты (включая чёрный список), профиль пользователя, стикеры, сторис, боты, звонки (включая групповые) и телеметрия.

## Установка

Добавьте `rumax` в `Cargo.toml` напрямую из репозитория:

```toml
[dependencies]
rumax = { git = "https://github.com/junioraww/rumax", branch = "main" }
tokio = { version = "1", features = ["full"] }
```

Или через `cargo`:

```sh
cargo add --git https://github.com/junioraww/rumax
```

## Быстрый старт

### Подключение и вход по QR-коду (WebSocket)

```rust
use rumax::{models::Identity, MaxClient};
use std::sync::Arc;

#[tokio::main]
async fn main() -> rumax::ClientResult<()> {
    let client = Arc::new(MaxClient::new());
    let identity = Identity::generate_web();

    // false — WebSocket-транспорт (для Mobile TLS используйте true)
    client.connect(identity, false).await?;

    let session = client.qr_login_request().await?;
    println!("Откройте или отсканируйте QR-ссылку: {}", session.qr_link);

    let (_login, _state) = client.qr_login_confirm(&session).await?;
    println!("Успешный вход!");

    Ok(())
}
```

Для входа по номеру телефона и SMS используйте `Identity::generate_mobile()` с `client.connect(identity, true).await?`, затем `start_auth`, `check_code` и (при необходимости) `check_password` — подробный код есть в [examples/login.rs](examples/login.rs).

## Примеры и отладка

В корне клонированного репозитория доступны готовые примеры:

| Пример | Описание | Команда |
| :--- | :--- | :--- |
| `login` | Вход по номеру телефона, SMS и 2FA | `cargo run --example login` |
| `qr_login` | Вход по QR-коду через WebSocket | `cargo run --example qr_login` |
| `register` | Регистрация нового аккаунта | `cargo run --example register` |
| `token` | Вход по сохранённому токену (`.session.token`) | `cargo run --example token` |
| `media_test` | Проверка работы с медиафайлами (нужны `.device.id`, `.session.token` из `token` и `ffmpeg` с `libopus`, `libx264`, `aac`) | `cargo run --example media_test` |

Для подробного логирования передайте `RUST_LOG=debug` (перед публикацией логов удаляйте из них токены, пароли и коды):

```sh
RUST_LOG=debug cargo run --example login
```

## Зеркала и лицензия

- **GitHub:** [junioraww/rumax](https://github.com/junioraww/rumax)
- **Codeberg:** [junioraww/rumax](https://codeberg.org/junioraww/rumax)
- **Лицензия:** [MIT](LICENSE)
- **Основано на** [PyMax](https://github.com/MaxApiTeam/PyMax)
