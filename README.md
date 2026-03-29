# mixr

[English version](README_en.md)

Утилита для записи случайной музыки на флешку.

mixr сканирует указанную папку, случайным образом выбирает аудиофайлы в пределах заданного размера и копирует их на целевой носитель с переименованием для воспроизведения в случайном порядке.

## Установка

### macOS / Linux

```bash
curl -sSf https://raw.githubusercontent.com/deliro/mixr/master/scripts/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/deliro/mixr/master/scripts/install.ps1 | iex
```

### Из исходников

```bash
cargo install --git https://github.com/deliro/mixr
```

### Из GitHub Release

Скачайте бинарник для вашей платформы со [страницы релизов](https://github.com/deliro/mixr/releases).

## Использование

### TUI режим

```bash
mixr
```

### CLI режим

```bash
mixr ~/Music /Volumes/USB
```

### Примеры

```bash
# Заполнить флешку до отказа
mixr ~/Music /Volumes/USB

# Ограничить размер до 4 ГБ, только mp3 и flac
mixr ~/Music /Volumes/USB --size 4G --include mp3,flac

# Исключить live-записи и файлы меньше 1 МБ
mixr ~/Music /Volumes/USB --no-live --min-size 1M

# Исключить форматы wav и wma
mixr ~/Music /Volumes/USB --exclude wav,wma

# Сохранить оригинальные имена файлов (по умолчанию 00001.mp3, 00002.mp3, ...)
mixr ~/Music /Volumes/USB --keep-names

# Перезаписать существующие файлы на флешке
mixr ~/Music /Volumes/USB --overwrite

# Конвертировать всё в MP3 CBR 192 kbps
mixr ~/Music /Volumes/USB --encoding cbr --bitrate 192

# Конвертировать в VBR среднего качества (~190 kbps)
mixr ~/Music /Volumes/USB --encoding vbr --quality medium

# Пропустить треки короче 30 секунд
mixr ~/Music /Volumes/USB --min-duration 30s
```

## Возможности

- Случайный выбор файлов из библиотеки любой глубины вложенности
- Автоматическое определение свободного места на носителе
- Конвертация на лету в MP3 (CBR или VBR) — FLAC, WAV, OGG, M4A и другие форматы перекодируются при копировании, MP3 с битрейтом выше порога перекодируются автоматически
- Double buffering — параллельное чтение/конвертация и запись, без пауз между файлами
- Переименование файлов для воспроизведения в случайном порядке (00001.mp3, 00002.mp3, ...)
- Пропуск занятых номеров при наличии файлов на носителе
- Фильтрация по расширениям, размеру, длительности, наличию "live" в имени
- Поддержка форматов: mp3, flac, ogg, wav, m4a, aac, wma (настраивается)
- Единый бинарник без внешних зависимостей (LAME встроен)
- Кроссплатформенность: Linux, macOS, Windows

## Переменные окружения

- `MIXR_LANG` — язык интерфейса (`en` или `ru`). По умолчанию определяется автоматически.
