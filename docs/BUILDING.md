# Сборка Stoat (Tauri) под Linux

## 1. Разово: WebKitGTK с WebRTC (обязательно для голоса)

Arch/cachyOS собирает `webkit2gtk-4.1` без WebRTC (`ENABLE_WEB_RTC` выключен) — из-за этого
`RTCPeerConnection` в вебвью не определён и голосовые каналы не работают. Нужно один раз
пересобрать системную библиотеку:

```fish
git clone https://gitlab.archlinux.org/archlinux/packaging/packages/webkit2gtk-4.1.git
cd webkit2gtk-4.1
```

В `PKGBUILD`, в блок `cmake_options`, добавить две строки:

```
-D ENABLE_WEB_RTC=ON
-D USE_LIBRICE=OFF
```

`USE_LIBRICE=OFF` обязателен — пакета `librice` нет ни в Arch, ни в AUR, и с ним
конфигурация падает. Этот флаг включает прежний libnice-бэкенд WebRTC (он рабочий).

```fish
makepkg -si   # ~25–40 мин, ~20 ГБ на время сборки
```

Защитить сборку от перезаписи при `pacman -Syu` — в `/etc/pacman.conf` в секцию `[options]`
(не в конец файла):

```
IgnorePkg = webkit2gtk-4.1
```

Проверка: `pkg-config --modversion webkit2gtk-4.1` показывает новую версию, а в
`libwebkit2gtk-4.1.so.0` присутствуют WebRTC-символы.

## 2. Зависимости сборки

```fish
sudo pacman -S --needed webkit2gtk-4.1 gtk3 pipewire libayatana-appindicator cargo
```

`cargo` можно поставить и через rustup — тогда при сборке пакета добавляй `--nodeps`.

## 3. Сборка приложения

```fish
cd src-tauri
cargo build --release --locked
```

Бинарь: `src-tauri/target/release/stoat-tauri`.

## 4. Выбор инстанса

Дефолт в коде — `https://stoat.chat/app`. Переопределяется переменной окружения:

```fish
STOAT_APP_URL=https://your.instance/app stoat-tauri
```

Для своего инстанса также поменяй `remote.urls` в `src-tauri/capabilities/default.json`
и Rich Presence в `src-tauri/src/rpc.rs`, иначе IPC и внешние ссылки останутся привязаны
к `stoat.chat`.

## 5. Сборка pacman-пакета

Публичный (из git, для дистрибуции):

```fish
git push
cd packaging/arch
makepkg -si
```

Локальный (из рабочего дерева, включает незакоммиченные правки):

```fish
rsync -a --exclude=.git --exclude=target --exclude=node_modules ~/stoat-tauri-spike/ /tmp/stoat-build/
cd /tmp/stoat-build/src-tauri && cargo build --release --locked
sudo install -Dm755 target/release/stoat-tauri /usr/bin/stoat-tauri
sudo install -Dm644 ../packaging/arch/stoat-tauri.desktop /usr/share/applications/stoat-tauri.desktop
sudo install -Dm644 icons/icon.png /usr/share/pixmaps/stoat-tauri.png
```

## 6. Установка готового пакета

```fish
sudo pacman -U stoat-tauri-0.1.0-1-x86_64.pkg.tar.zst
```