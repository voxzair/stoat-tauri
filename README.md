# Stoat (Tauri)

Rust-переписанная (Tauri v2) оболочка десктоп-клиента
[stoatchat/for-desktop](https://github.com/stoatchat/for-desktop) (Electron). Грузит
тот же удалённый чат (`https://stoat.chat/app` по умолчанию), что и Electron-версия.
Цель — радикально меньшее потребление системных ресурсов.

## Структура

- `src-tauri/` — Rust (tauri v2, wry + WebKitGTK)
  - `src/config.rs` — конфиг (JSON в `~/.config/chat.stoat.StoatDesktop/`)
  - `src/autostart.rs` — автозапуск
  - `src/rpc.rs` — Discord Rich Presence
  - `src/tray.rs` — системный трей
  - `src/window.rs` — окно, состояние, IPC-команды, бейджи
  - `src/virtual_mic.rs` — PipeWire виртуальный sink/source (порт node-pipewire)
  - `src/bridge.rs` — JS-мост, эмулирующий прелоад Electron (`window.desktopConfig` / `window.native`)
  - `capabilities/default.json` — ACL, разрешающий удалённому origin IPC-доступ
- `dist/` — заглушка-фолбэк (реальный контент с удалённого URL)

## Настройка инстанса

По умолчанию грузится официальный `https://stoat.chat/app`. Чтобы указать свой
инстанс, задай переменную окружения `STOAT_APP_URL` при запуске:

```bash
STOAT_APP_URL=https://example.com/app ./target/release/stoat-tauri
```

и обнови `src-tauri/capabilities/default.json` → `remote.urls`, чтобы он совпадал
с origin твоего инстанса (иначе IPC-мост не заработает с удалённой страницей).

## Сборка

```bash
cd src-tauri
cargo build --release
# бинарь: target/release/stoat-tauri
```

## Реализовано

- Окно, грузящее удалённый чат; внешние ссылки открываются в браузере
- Single-instance lock
- Системный трей с меню (версия, показать/скрыть, выход)
- Персистентный конфиг (совместимый с форматом Electron — camelCase)
- Автозапуск при входе в систему
- Discord Rich Presence
- Минимизация в трей при закрытии; старт свёрнутым
- Сохранение позиции/размера окна
- IPC-мост: `desktopConfig` (get/set/autostart) и `native`
  (versions/minimise/maximise/close/setBadgeCount/isWayland)
- Кастомный фрейм (задаётся при создании окна)
- Бейджи: macOS dock, Linux libunity, Windows overlay-иконка
- PipeWire виртуальный sink/source для звука демонстрации экрана (Wayland)

## Замеры (Linux, WebKitGTK 2.52, 2026-11-07)

| Метрика | Electron (системный) | Tauri (release) |
|---|---|---|
| Бинарь | 218 MB | **6.5 MB** |
| Процессов | 10 | **3** |
| USS (суммарно) | 603 MB | **308 MB** |
| PSS (суммарно) | 800 MB | **361 MB** |

## Ограничения / TODO

- **Нативный Wayland** в тестовой песочнице не поднялся (Gdk Error 71) — гонял
  через XWayland (`GDK_BACKEND=x11`). На реальном десктопе проверить нативный Wayland.
- **Screen-share picker**: выбор окна/экрана в WebKitGTK идёт через xdg-desktop-portal
  (системный пикер), поэтому кастомный `onceScreenPicker`/`screenPickerCallback`
  из Electron не нужен и оставлен заглушкой. Стоит проверить `getDisplayMedia`
  на реальном десктопе.
- **Hardware acceleration** и **spellchecker** — сохраняются в конфиге, но не применяются.
- Смена кастомного фрейма на лету не поддерживается (Tauri не умеет переключать
  decorations у уже созданного окна).
- Фильтрация собственных PID в virtual_mic — best-effort (`/proc`-обход дерева процессов).

## Лицензия и благодарности

- Проект — производная от [stoatchat/for-desktop](https://github.com/stoatchat/for-desktop),
  лицензия **AGPL-3.0** (как у оригинала).
- Виртуальный микрофон (`src/virtual_mic.rs`) — порт
  [node-pipewire](https://github.com/kakxem/node-pipewire) (MIT).

## Оговорки

Electron мерил после ~2.5 ч работы (разогретый кеш), Tauri — через ~30 c.
Львиная доля памяти в обоих случаях — сам веб-чат; но даже контент в WebKitGTK
легче, чем в Chromium-рендерах Electron.
