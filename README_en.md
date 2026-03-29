# mixr

[Русская версия](README.md)

A utility for filling your flash drive with random music.

mixr scans a given directory, randomly selects audio files within a size budget, and copies them to the target drive with sequential naming for shuffled playback.

## Installation

### macOS / Linux

```bash
curl -sSf https://raw.githubusercontent.com/deliro/mixr/master/scripts/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/deliro/mixr/master/scripts/install.ps1 | iex
```

### From source

```bash
cargo install --git https://github.com/deliro/mixr
```

### From GitHub Release

Download the binary for your platform from the [releases page](https://github.com/deliro/mixr/releases).

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

# Convert everything to MP3 CBR 192 kbps
mixr ~/Music /Volumes/USB --encoding cbr --bitrate 192

# Convert to VBR medium quality (~190 kbps)
mixr ~/Music /Volumes/USB --encoding vbr --quality medium

# Skip tracks shorter than 30 seconds
mixr ~/Music /Volumes/USB --min-duration 30s
```

## Features

- Random file selection from libraries of any nesting depth
- Automatic free space detection on target drive
- On-the-fly MP3 transcoding (CBR or VBR) — FLAC, WAV, OGG, M4A and other formats are converted during copy; MP3 files above the bitrate threshold are automatically reencoded
- Double buffering — parallel read/transcode and write, no pauses between files
- Sequential renaming for shuffled playback order (00001.mp3, 00002.mp3, ...)
- Skips occupied numbers when files already exist on the drive
- Filtering by extension, file size, duration, and "live" in filename
- Supported formats: mp3, flac, ogg, wav, m4a, aac, wma (configurable)
- Single binary, no external dependencies (LAME statically linked)
- Cross-platform: Linux, macOS, Windows

## Environment variables

- `MIXR_LANG` — interface language (`en` or `ru`). Defaults to automatic detection.
