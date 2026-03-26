use std::fmt;
use std::path::PathBuf;

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSize(pub u64);

#[derive(Debug, PartialEq)]
pub enum ParseSizeError {
    Empty,
    InvalidNumber(String),
    NegativeOrZero,
    UnknownUnit(String),
}

impl fmt::Display for ParseSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "size string is empty"),
            Self::InvalidNumber(s) => write!(f, "invalid number: {s}"),
            Self::NegativeOrZero => write!(f, "size must be greater than zero"),
            Self::UnknownUnit(u) => write!(f, "unknown unit: {u}, expected G/M/K/B"),
        }
    }
}

impl std::error::Error for ParseSizeError {}

impl ByteSize {
    pub fn parse(s: &str) -> Result<Self, ParseSizeError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseSizeError::Empty);
        }

        let idx = s
            .rfind(|c: char| c.is_ascii_digit() || c == '.')
            .ok_or(ParseSizeError::Empty)?;

        let (num_str, unit_str) = s.split_at(idx + 1);
        let num: f64 = num_str
            .parse()
            .map_err(|_| ParseSizeError::InvalidNumber(num_str.to_string()))?;

        if num <= 0.0 {
            return Err(ParseSizeError::NegativeOrZero);
        }

        let unit_lower = unit_str.trim().to_lowercase();
        let multiplier = match unit_lower.as_str() {
            "g" | "gb" => GB,
            "m" | "mb" => MB,
            "k" | "kb" => KB,
            "b" | "" => 1,
            other => return Err(ParseSizeError::UnknownUnit(other.to_string())),
        };

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = (num * multiplier as f64) as u64;
        Ok(Self(result))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        if bytes >= GB {
            write!(f, "{:.1}G", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            write!(f, "{:.1}M", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            write!(f, "{:.1}K", bytes as f64 / KB as f64)
        } else {
            write!(f, "{bytes}B")
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: ByteSize,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub max_size: Option<ByteSize>,
    pub min_file_size: Option<ByteSize>,
    pub no_live: bool,
    pub keep_names: bool,
    pub allowed_extensions: Vec<String>,
}

pub const DEFAULT_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac", "wma"];

#[derive(Debug)]
#[allow(dead_code)]
pub enum Error {
    SourceNotFound(PathBuf),
    InvalidSize(ParseSizeError),
    ScanFailed { path: PathBuf, source: std::io::Error },
    ReadFailed { path: PathBuf, source: std::io::Error },
    WriteFailed { path: PathBuf, source: std::io::Error },
    CreateDirFailed { path: PathBuf, source: std::io::Error },
    DiskSpaceQuery { path: PathBuf, source: std::io::Error },
    Terminal(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound(p) => write!(f, "source not found: {}", p.display()),
            Self::InvalidSize(e) => write!(f, "invalid size: {e}"),
            Self::ScanFailed { path, source } => {
                write!(f, "scan failed at {}: {source}", path.display())
            }
            Self::ReadFailed { path, source } => {
                write!(f, "read failed {}: {source}", path.display())
            }
            Self::WriteFailed { path, source } => {
                write!(f, "write failed {}: {source}", path.display())
            }
            Self::CreateDirFailed { path, source } => {
                write!(f, "failed to create {}: {source}", path.display())
            }
            Self::DiskSpaceQuery { path, source } => {
                write!(f, "disk space query failed for {}: {source}", path.display())
            }
            Self::Terminal(e) => write!(f, "terminal error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseSizeError> for Error {
    fn from(e: ParseSizeError) -> Self {
        Self::InvalidSize(e)
    }
}

pub fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gigabytes() {
        assert_eq!(ByteSize::parse("8G").unwrap(), ByteSize(8 * GB));
        assert_eq!(ByteSize::parse("1.5GB").unwrap(), ByteSize((1.5 * GB as f64) as u64));
        assert_eq!(ByteSize::parse("8gb").unwrap(), ByteSize(8 * GB));
    }

    #[test]
    fn parse_megabytes() {
        assert_eq!(ByteSize::parse("500M").unwrap(), ByteSize(500 * MB));
        assert_eq!(ByteSize::parse("100mb").unwrap(), ByteSize(100 * MB));
    }

    #[test]
    fn parse_kilobytes() {
        assert_eq!(ByteSize::parse("900K").unwrap(), ByteSize(900 * KB));
    }

    #[test]
    fn parse_bytes() {
        assert_eq!(ByteSize::parse("1024B").unwrap(), ByteSize(1024));
        assert_eq!(ByteSize::parse("1024").unwrap(), ByteSize(1024));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(ByteSize::parse(""), Err(ParseSizeError::Empty));
        assert_eq!(ByteSize::parse("0G"), Err(ParseSizeError::NegativeOrZero));
        assert_eq!(
            ByteSize::parse("5X"),
            Err(ParseSizeError::UnknownUnit("x".to_string()))
        );
    }

    #[test]
    fn format_sizes() {
        assert_eq!(format!("{}", ByteSize(8 * GB)), "8.0G");
        assert_eq!(format!("{}", ByteSize(500 * MB)), "500.0M");
        assert_eq!(format!("{}", ByteSize(900 * KB)), "900.0K");
        assert_eq!(format!("{}", ByteSize(512)), "512B");
    }
}
