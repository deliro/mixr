# mixr - Music Flash Drive Filler

TUI/CLI program for copying randomly selected music from a large library onto a flash drive (or any destination directory).

## Modes

- **No arguments** -> interactive TUI (ratatui) with single-page wizard
- **With arguments** -> CLI mode with plain text progress

## CLI Interface (clap)

```
mixr [OPTIONS] <SOURCE> <DESTINATION>
```

### Positional arguments (CLI mode)

- `SOURCE` - path to music library (recursive scan)
- `DESTINATION` - path to flash drive / target directory

### Options

- `--size <SIZE>` - max bytes to copy (`8G`, `500M`). Default: free space on destination fs (via `fs2::available_space`)
- `--min-size <SIZE>` - skip files smaller than this (`1M`). Default: no filter
- `--no-live` - exclude files/directories with "live" in name (case-insensitive, word boundary)
- `--include <EXT,...>` - only these extensions. Overrides default list
- `--exclude <EXT,...>` - remove these from default list
- `--keep-names` - preserve original filenames instead of `00001.ext`

### Default extensions

`mp3, flac, ogg, wav, m4a, aac, wma`

### Include/exclude logic

- `--include` set -> use only include list (defaults ignored)
- `--exclude` set -> defaults minus exclude
- Both set -> include minus exclude

## Architecture

Single binary, Elm architecture (Model + Msg + update + view).

### Modules

```
src/
  main.rs       - entry point, clap, mode selection
  app.rs        - Elm Model + Msg + update + view
  scanner.rs    - recursive FS walk, filtering, shuffle
  copier.rs     - file copying, progress, error handling
  tui.rs        - ratatui rendering, event loop, input
  cli.rs        - text mode with line-by-line progress
  filters.rs    - filter chain: extensions, min-size, no-live
  types.rs      - shared types, newtypes, errors
```

### Data flow

1. `main.rs` parses clap args -> determines mode (TUI if no args, CLI if args present)
2. TUI mode: wizard on single page for parameter input
3. Parameters -> `scanner` (background thread) -> sends `Msg` via `mpsc`
4. After scan -> `copier` (background thread) -> sends `Msg` via `mpsc`
5. Main thread runs event loop: receives `Msg`, calls `update`, renders `view`

### Elm cycle

- `Model` - current state (phase, progress, file history, errors)
- `Msg` - event enum
- `update(model, msg) -> model` - pure function, updates state
- `view(model, frame)` - renders into ratatui Frame

## Phases

```
Phase::Setup    - TUI wizard (TUI mode only)
Phase::Scanning - recursive source walk, file counting
Phase::Copying  - copying to destination
```

## Model

- `phase: Phase`
- `config: Config` - source, dest, size, filters
- `scan: ScanState` - files found, files matched, last found file
- `copy: CopyState` - total bytes/files, copied bytes/files, current file name/size/progress, speed, file history (sliding window), error list
- `started_at: Instant`
- `terminal_size: (u16, u16)`

### Setup state (TUI wizard)

Single page with input fields:
- Source path, Destination path, Size (optional), Min size (optional)
- Extensions (include), Exclude
- Checkboxes: No live, Keep names
- Start button

Navigation: Tab/Shift+Tab between fields, Enter to start.
Validation on start: source exists, destination writable.

Fields adapt to terminal width.

## Msg enum

- `Key(KeyEvent)` - user input
- `Resize(u16, u16)` - terminal resize
- `Tick` - timer for speed/ETA updates
- `FileFound { path, matched }` - from scanner
- `ScanComplete { files: Vec<FileEntry> }` - scan done
- `CopyProgress { bytes_written }` - current file progress
- `CopyFileStart { name, size, index }` - new file started
- `CopyFileDone { index }` - file copied
- `CopyError { error, is_destination }` - source error -> skip, dest error -> stop
- `CopyComplete` - all done
- `Abort` - ctrl+c

## Scanner

1. `walkdir` recursively walks source
2. Each file checked through filter chain:
   - Extension in allowed set
   - Size >= min-size (if set)
   - No "live" in filename or parent dir name (if `--no-live`), case-insensitive, word boundary
3. Matching files collected into `Vec<FileEntry>` where `FileEntry = { path, size }`
4. Sends `Msg::FileFound` for UI updates during walk
5. On completion: shuffle via `rand::seq::SliceRandom`, pack into budget:
   - Budget = `--size` if specified, else `fs2::available_space(destination)`
   - Walk shuffled list, take files while they fit
   - File doesn't fit -> skip, try next (10 consecutive misses -> stop)
6. Sends `Msg::ScanComplete { files }`

### Scanner errors

- No permission on directory -> skip, log
- Source doesn't exist -> error before start, show and exit

## Copier

1. Receives `Vec<FileEntry>` - already selected and shuffled
2. Creates destination directory tree (`create_dir_all`)
3. For each file:
   - Sends `Msg::CopyFileStart { name, size, index }`
   - Determines destination path:
     - Default: `00001.ext`, `00002.ext`, ...
     - `--keep-names`: original name, deduplicate with `(1) name.ext` if exists
   - Opens source -> error (deleted, no perms) -> `Msg::CopyError { is_destination: false }` -> skip
   - Creates destination file -> error -> `Msg::CopyError { is_destination: true }` -> stop
   - Copies via 8KB buffer, sends `Msg::CopyProgress { bytes_written }` after each write
   - Write error -> `Msg::CopyError { is_destination: true }` -> stop
   - Success -> `Msg::CopyFileDone { index }`
4. On completion -> `Msg::CopyComplete`

### Why custom copy instead of `std::fs::copy`

- Need per-byte progress within file
- Need to distinguish read vs write errors
- Buffered IO for progress granularity control

### Graceful shutdown (Ctrl+C)

- Main thread sets `AtomicBool` flag
- Copier checks flag between files and between chunks
- On stop: delete partially copied file, send `Msg::Abort`

## No file validation

No md5/crc32 verification. Re-reading destination would double IO for minimal benefit.

## TUI Layout

Terminal-adaptive rendering. All elements scale with terminal width/height on every render via ratatui constraints.

### Phase::Setup

```
+- mixr ---------------------------------+
| Source:      [/path/to/music         ]  |
| Destination: [/volumes/usb           ]  |
| Size:        [auto                   ]  |
| Min size:    [                        ] |
| Extensions:  [mp3,flac,ogg,wav,...    ] |
| Exclude:     [                        ] |
| [x] No live   [ ] Keep names           |
|                                         |
|              [ Start ]                  |
|                                         |
| Tab: next  Shift+Tab: prev  Enter: go  |
+-----------------------------------------+
```

### Phase::Scanning

```
Scanning /path/to/music...
@ 1542 files found (834 matched)
/current/file/being/scanned.mp3
```

### Phase::Copying

File list window: up to 3 upcoming (dim), 1 current (white bold), up to 4 done/error (green/red).

```
  queued_3.mp3                           <- dim
  queued_2.mp3                           <- dim
> copying_now.mp3  (8.2M)               <- white, bold
  done_1.mp3                             <- green
  done_2.mp3                             <- green
  failed.mp3                             <- red

Current: [==============------] 72.1%  8.2M
Total:   [========------------] 34.2%  4.2G / 12.4G
2.3G/s  Elapsed: 00:01:23  ETA: 00:04:12
```

Progress bars pinned to bottom. File list has fixed max size (3+1+4=8 lines). Speed is rolling average.

## CLI Mode (with arguments)

Plain text to stderr, no ratatui:

```
Scanning /path/to/music... 1542 found, 834 matched
Copying 834 files (12.4G) to /volumes/usb

[  1/834]  00001.mp3 <- artist/album/song.mp3 (8.2M) ... ok
[  2/834]  00002.mp3 <- another/track.mp3 (4.1M) ... ok
[  3/834]  00003.mp3 <- live/concert.mp3 (12M) ... skipped (read error)
...

Done: 831/834 files, 12.1G copied in 00:05:23 (2.3G/s), 3 errors
```

- One line per file
- Exit code 0 if ok, 1 if destination write errors

## Error Handling

- **Source read error** (file deleted, no permissions): skip file, log in UI, continue
- **Destination write error** (flash drive broken/removed, disk full): stop immediately, show error
- **Source directory doesn't exist**: error before start, exit
- **Destination not writable**: error before start, exit

## Dependencies

- `clap` - CLI argument parsing
- `ratatui` + `crossterm` - TUI rendering
- `walkdir` - recursive directory traversal
- `rand` - shuffle
- `fs2` - cross-platform free disk space (statvfs on unix, GetDiskFreeSpaceExW on windows)

## Cross-platform

Works on Linux, macOS, Windows. Cross-platform disk space via `fs2`. Path handling via `std::path`. No platform-specific code.
