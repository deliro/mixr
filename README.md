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
cargo install --path .
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
```

## Возможности

- Случайный выбор файлов из библиотеки любой глубины вложенности
- Автоматическое определение свободного места на носителе
- Переименование файлов для воспроизведения в случайном порядке (00001.mp3, 00002.mp3, ...)
- Пропуск занятых номеров при наличии файлов на носителе
- Фильтрация по расширениям, размеру, наличию "live" в имени
- Поддержка форматов: mp3, flac, ogg, wav, m4a, aac, wma (настраивается)
- Кроссплатформенность: Linux, macOS, Windows

## Переменные окружения

- `MIXR_LANG` — язык интерфейса (`en` или `ru`). По умолчанию определяется автоматически.
