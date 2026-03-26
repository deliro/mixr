# mixr

[Русская версия](README.md)

A utility for filling your flash drive with random music.

mixr scans a given directory, randomly selects audio files within a size budget, and copies them to the target drive with sequential naming for shuffled playback.

## Installation

```bash
cargo install --path .
```

## Usage

### TUI mode

```bash
mixr
```

### CLI mode

```bash
mixr ~/Music /Volumes/USB
```

### Examples

```bash
# Fill a flash drive to capacity
mixr ~/Music /Volumes/USB

# Limit size to 4 GB, only mp3 and flac
mixr ~/Music /Volumes/USB --size 4G --include mp3,flac

# Exclude live recordings and files smaller than 1 MB
mixr ~/Music /Volumes/USB --no-live --min-size 1M

# Exclude wav and wma formats
mixr ~/Music /Volumes/USB --exclude wav,wma

# Keep original file names (default is 00001.mp3, 00002.mp3, ...)
mixr ~/Music /Volumes/USB --keep-names

# Overwrite existing files on the drive
mixr ~/Music /Volumes/USB --overwrite
```

## Features

- Random file selection from libraries of any nesting depth
- Automatic free space detection on target drive
- Sequential renaming for shuffled playback order (00001.mp3, 00002.mp3, ...)
- Skips occupied numbers when files already exist on the drive
- Filtering by extension, file size, and "live" in filename
- Supported formats: mp3, flac, ogg, wav, m4a, aac, wma (configurable)
- Cross-platform: Linux, macOS, Windows

## Environment variables

- `MIXR_LANG` — interface language (`en` or `ru`). Defaults to automatic detection.
