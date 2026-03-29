use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

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

        let (num_str, unit_str) = s.split_at(idx.saturating_add(1));
        let num: f64 = num_str
            .parse()
            .map_err(|_| ParseSizeError::InvalidNumber(num_str.to_string()))?;

        if num <= 0.0_f64 {
            return Err(ParseSizeError::NegativeOrZero);
        }

        let unit_lower = unit_str.trim().to_lowercase();
        let multiplier = match unit_lower.as_str() {
            "g" | "gb" => GB,
            "m" | "mb" => MB,
            "k" | "kb" => KB,
            "b" | "" => 1_u64,
            other => return Err(ParseSizeError::UnknownUnit(other.to_string())),
        };

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            clippy::as_conversions
        )]
        let result = (num * multiplier as f64) as u64;
        Ok(Self(result))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ByteSize {
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Keep,
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbrQuality {
    High,
    Medium,
    Low,
}

impl VbrQuality {
    pub fn avg_bitrate_kbps(self) -> u16 {
        match self {
            Self::High => 245_u16,
            Self::Medium => 190_u16,
            Self::Low => 130_u16,
        }
    }

    #[allow(dead_code)]
    pub fn lame_quality(self) -> u8 {
        match self {
            Self::High => 0_u8,
            Self::Medium => 2_u8,
            Self::Low => 6_u8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: ByteSize,
    #[allow(dead_code)]
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub max_size: Option<ByteSize>,
    pub min_file_size: Option<ByteSize>,
    pub no_live: bool,
    pub keep_names: bool,
    pub overwrite: bool,
    pub allowed_extensions: Vec<String>,
    pub min_duration: Option<Duration>,
    pub encoding: Encoding,
    pub cbr_bitrate: Option<u16>,
    pub vbr_quality: Option<VbrQuality>,
}

pub const DEFAULT_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac", "wma"];

#[derive(Debug)]
#[allow(dead_code)]
pub enum Error {
    SourceNotFound(PathBuf),
    InvalidSize(ParseSizeError),
    ScanFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    WriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateDirFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    DiskSpaceQuery {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidDuration(ParseDurationError),
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
                write!(
                    f,
                    "disk space query failed for {}: {source}",
                    path.display()
                )
            }
            Self::InvalidDuration(e) => write!(f, "invalid duration: {e}"),
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

impl From<ParseDurationError> for Error {
    fn from(e: ParseDurationError) -> Self {
        Self::InvalidDuration(e)
    }
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600_u64;
    let m = (secs % 3600_u64) / 60_u64;
    let s = secs % 60_u64;
    if h > 0_u64 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseDurationError {
    Empty,
    Invalid(String),
    Zero,
}

impl fmt::Display for ParseDurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "duration string is empty"),
            Self::Invalid(s) => write!(f, "invalid duration: {s}"),
            Self::Zero => write!(f, "duration must be greater than zero"),
        }
    }
}

impl std::error::Error for ParseDurationError {}

pub fn parse_duration(s: &str) -> Result<Duration, ParseDurationError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ParseDurationError::Empty);
    }

    let total_secs = if let Some((min_str, sec_str)) = s.split_once(':') {
        let mins = min_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        let secs = sec_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        mins.saturating_mul(60).saturating_add(secs)
    } else if let Some(idx) = s.find('m') {
        let min_str = s
            .get(..idx)
            .ok_or_else(|| ParseDurationError::Invalid(s.to_string()))?;
        let rest = s.get(idx.saturating_add(1)..).unwrap_or("");
        let rest = rest.strip_suffix('s').unwrap_or(rest);
        let mins = min_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        let secs = if rest.is_empty() {
            0_u64
        } else {
            rest.parse::<u64>()
                .map_err(|_| ParseDurationError::Invalid(s.to_string()))?
        };
        mins.saturating_mul(60).saturating_add(secs)
    } else {
        let num_str = s.strip_suffix('s').unwrap_or(s);
        num_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?
    };

    if total_secs == 0 {
        return Err(ParseDurationError::Zero);
    }

    Ok(Duration::from_secs(total_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gigabytes() {
        assert_eq!(ByteSize::parse("8G").unwrap(), ByteSize(8 * GB));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            clippy::as_conversions
        )]
        let expected = (1.5_f64 * GB as f64) as u64;
        assert_eq!(ByteSize::parse("1.5GB").unwrap(), ByteSize(expected));
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

    #[test]
    fn parse_duration_bare_seconds() {
        assert_eq!(parse_duration("30").unwrap().as_secs(), 30);
        assert_eq!(parse_duration("120").unwrap().as_secs(), 120);
    }

    #[test]
    fn parse_duration_with_suffix() {
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
        assert_eq!(parse_duration("2m").unwrap().as_secs(), 120);
        assert_eq!(parse_duration("2m30s").unwrap().as_secs(), 150);
    }

    #[test]
    fn parse_duration_colon_format() {
        assert_eq!(parse_duration("2:30").unwrap().as_secs(), 150);
        assert_eq!(parse_duration("0:45").unwrap().as_secs(), 45);
        assert_eq!(parse_duration("10:00").unwrap().as_secs(), 600);
    }

    #[test]
    fn parse_duration_errors() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("0").is_err());
        assert!(parse_duration("-5").is_err());
    }
}
