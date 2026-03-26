# mixr

[Русская версия](README.md)

A utility for filling your flash drive with random music.

mixr scans a given directory, randomly selects audio files within a size budget, and copies them to the target drive. Perfect for filling a USB stick with music for your car.

## Installation

```bash
cargo install --path .
```

## Usage

### TUI mode (interactive)

Run without arguments:

```bash
mixr
```

An interactive interface opens with input fields, path autocompletion, and drive selection via `Ctrl+D`.

### CLI mode

Specify source and destination:

```bash
mixr ~/Music /Volumes/USB
```

### Examples

```bash
# Fill a flash drive with music from ~/Music
mixr ~/Music /Volumes/USB

# Limit size to 4 GB, only mp3 and flac
mixr ~/Music /Volumes/USB --size 4G --include mp3,flac

# Exclude live recordings and files smaller than 1 MB
mixr ~/Music /Volumes/USB --no-live --min-size 1M

# Exclude wav and wma formats
mixr ~/Music /Volumes/USB --exclude wav,wma

# Keep original file names
mixr ~/Music /Volumes/USB --keep-names
```

## Features

- Random file selection within available space
- Extension filters (`--include`, `--exclude`)
- Live recording filter (`--no-live`)
- Minimum file size (`--min-size`)
- Total size limit (`--size`)
- Keep original names (`--keep-names`)
- Copy progress with speed and ETA indicators
- Path autocompletion in TUI
- Quick drive selection via `Ctrl+D`

## Environment variables

- `MIXR_LANG` - interface language (`en` or `ru`). Defaults to automatic detection based on system locale.
