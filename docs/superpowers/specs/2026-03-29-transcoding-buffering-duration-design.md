# Transcoding, Double Buffering, Min Duration

## New Dependencies

- `symphonia` — decode any audio format + read duration/bitrate from headers
- `mp3lame-encoder` — encode PCM → MP3 (static linking, single binary)

## New Form Fields

Field order in TUI setup form:

1. Source
2. Destination
3. Min size
4. Min duration *(new)*
5. Extensions
6. Exclude
7. Encoding *(new)*
8. Bitrate / Quality *(conditional, new)*
9. No live
10. Keep names
11. Overwrite

### Min duration

Text field. Parsing rules:
- `30` or `30s` → 30 seconds
- `2m` → 120 seconds
- `2:30` or `2m30s` → 150 seconds
- Empty → no filter

### Encoding

Dropdown with three options:
- **Keep original** (default) — no additional fields, files copied as-is
- **CBR** — shows **Bitrate** dropdown: 128, 160, 192, 224, 256, 320
- **VBR** — shows **Quality** dropdown: High (~245kbps), Medium (~190kbps), Low (~130kbps)

Bitrate/Quality field is conditional — only visible when Encoding is CBR or VBR.

### Conversion logic per file

When Encoding = Keep original → always copy.

When CBR or VBR:
- Non-MP3 files (FLAC, WAV, OGG, M4A, etc.) → always convert to MP3
- MP3 files → read bitrate from header:
  - Above threshold → reencode
  - At or below threshold → copy as-is
- VBR thresholds: High = 245, Medium = 190, Low = 130

Output filename: extension changes to `.mp3` when converting. `song.flac` → `song.mp3` (keep_names) or `00001.mp3` (numbered).

## Duration Detection

During scanning, determine duration from file headers (no full decode):

- **MP3**: Xing/VBRI header → exact. Fallback: `file_size / (bitrate / 8)` from first frame header
- **FLAC**: streaminfo header → `total_samples / sample_rate`
- **WAV**: header → `data_chunk_size / (sample_rate × channels × bits_per_sample / 8)`
- **OGG/M4A/other**: symphonia metadata probe → duration from container headers
- **Non-audio files**: `duration = None`, min-duration filter passes them through

All header-only reads, minimal impact on scan speed.

Filter: if `min_duration` is set and file has `Some(duration)` and `duration < min_duration` → file is excluded.

## Budget Estimation (Hybrid)

File list is built using estimated output sizes:
- Copy → `estimated_size = file_size`
- Convert → `estimated_size = duration_secs × target_bitrate_bytes_per_sec`
- VBR uses average bitrate for quality level

During copying, track actual written bytes. If budget exhausted → stop, skip remaining files. Worst case for VBR: slightly fewer files than estimated. Better than overflowing the drive.

## Double Buffering

### Current architecture
Single thread: read 64KB chunk → write chunk → repeat. Pause between files for close/create/open.

### New architecture

**Buffer size**: 64KB → 1MB.

**Two threads connected by bounded channel:**

- **Reader thread**: reads next file into memory (or decodes+encodes for conversion), sends chunks into `mpsc::sync_channel` with capacity 4 (4 × 1MB = 4MB backpressure)
- **Writer thread**: receives chunks, writes to destination, reports progress

Channel messages:
- `NewFile { index, name, original_path, size, is_converted }` — writer creates output file
- `Chunk(Vec<u8>)` — writer writes data
- `FileDone { index }` — writer closes and flushes file
- `Complete` / `Aborted` / `Error { ... }`

While writer writes file N to flash drive, reader reads/converts file N+1 from HDD. Different USB devices → true parallelism.

For transcoding: CPU-bound encode of file N+1 runs in parallel with I/O-bound write of file N.

### Conversion pipeline (inside reader thread)

1. Open file via `symphonia::Probe`
2. Decode frames → interleaved PCM samples
3. Feed PCM to `mp3lame-encoder` with configured params (CBR bitrate or VBR quality)
4. Send encoded MP3 chunks into the same channel as raw file chunks

Writer thread is agnostic — it writes whatever bytes arrive.

## UI Changes

### TUI file list

New statuses for the file being prepared by reader thread:

```
  upcoming_3.mp3                (dark gray)
  upcoming_2.flac               (dark gray)
  upcoming_1.mp3   preparing…   (yellow, dim)
> current.flac (4.2 MB)         (white, bold)
  done_1.mp3                    (green gradient)
```

- `preparing…` — reader is reading the file (copy path)
- `converting…` — reader is transcoding the file

New `FileStatus` variants: `Reading`, `Converting`.

### TUI conditional fields

Bitrate/Quality row is only rendered when Encoding ≠ Keep original. Field navigation skips hidden fields.

## CLI

### New arguments

```
--min-duration <value>     # 30, 30s, 2m, 2:30, 2m30s
--encoding <keep|cbr|vbr>  # default: keep
--bitrate <128|160|192|224|256|320>  # required with --encoding cbr
--quality <high|medium|low>          # optional with --encoding vbr, default: medium
```

Validation:
- `--bitrate` without `--encoding cbr` → error
- `--quality` without `--encoding vbr` → error
- `--encoding cbr` without `--bitrate` → error

### Log markers

```
[  1/1425]  00001.mp3 <- /media/music/song.flac (4.2 MB) [converting]
[  2/1425]  00002.mp3 <- /media/music/track.mp3 (3.1 MB)
[  3/1425]  00003.mp3 <- /media/music/loud.mp3 (8.5 MB) [reencoding]
```

- `[converting]` — non-MP3 converted to MP3
- `[reencoding]` — MP3 reencoded due to bitrate exceeding threshold
- No marker — copied as-is

## Type Changes

### New types

```
enum Encoding { Keep, Cbr, Vbr }
enum VbrQuality { High, Medium, Low }
```

### Modified types

- `FileEntry`: + `duration: Option<Duration>`, + `bitrate: Option<u32>`
- `FileStatus`: + `Reading`, + `Converting`
- `Config`: + `min_duration: Option<Duration>`, + `encoding: Encoding`, + `bitrate: Option<u16>`, + `quality: Option<VbrQuality>`

## Module Changes

| Module | Changes |
|--------|---------|
| `types.rs` | New types (`Encoding`, `VbrQuality`), duration parsing |
| `scanner.rs` | Read duration/bitrate via symphonia during scan |
| `filters.rs` | Min-duration filter |
| `copier.rs` | Double buffering (reader/writer threads), sync_channel 4×1MB, transcoding via symphonia+lame |
| `app.rs` | New form fields, conditional fields, estimated output size for budget, Reading/Converting statuses |
| `tui.rs` | Render preparing…/converting… labels, conditional field visibility |
| `cli.rs` | New CLI args, [converting]/[reencoding] log markers, stderr flush |
| `i18n.rs` | New strings for both locales |

## Out of Scope

- Parallel copying of two files simultaneously (replaced by double buffering, which is more effective for USB)
- Adding files to the queue on-the-fly if budget underflows
