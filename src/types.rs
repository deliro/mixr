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

        #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
        let multiplier_f64 = multiplier as f64;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions
        )]
        let result = (num * multiplier_f64) as u64;
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
            #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
            let val = bytes as f64 / GB as f64;
            write!(f, "{val:.1}G")
        } else if bytes >= MB {
            #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
            let val = bytes as f64 / MB as f64;
            write!(f, "{val:.1}M")
        } else if bytes >= KB {
            #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
            let val = bytes as f64 / KB as f64;
            write!(f, "{val:.1}K")
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
pub enum CbrBitrate {
    Kbps128,
    Kbps160,
    Kbps192,
    Kbps224,
    Kbps256,
    Kbps320,
}

impl CbrBitrate {
    pub fn as_kbps(self) -> u16 {
        match self {
            Self::Kbps128 => 128_u16,
            Self::Kbps160 => 160_u16,
            Self::Kbps192 => 192_u16,
            Self::Kbps224 => 224_u16,
            Self::Kbps256 => 256_u16,
            Self::Kbps320 => 320_u16,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Kbps128 => Self::Kbps160,
            Self::Kbps160 => Self::Kbps192,
            Self::Kbps192 => Self::Kbps224,
            Self::Kbps224 => Self::Kbps256,
            Self::Kbps256 | Self::Kbps320 => Self::Kbps320,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Kbps128 | Self::Kbps160 => Self::Kbps128,
            Self::Kbps192 => Self::Kbps160,
            Self::Kbps224 => Self::Kbps192,
            Self::Kbps256 => Self::Kbps224,
            Self::Kbps320 => Self::Kbps256,
        }
    }

    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            128_u16 => Some(Self::Kbps128),
            160_u16 => Some(Self::Kbps160),
            192_u16 => Some(Self::Kbps192),
            224_u16 => Some(Self::Kbps224),
            256_u16 => Some(Self::Kbps256),
            320_u16 => Some(Self::Kbps320),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbrQuality {
    Low,
    Medium,
    High,
}

impl VbrQuality {
    pub fn avg_bitrate_kbps(self) -> u16 {
        match self {
            Self::High => 245_u16,
            Self::Medium => 190_u16,
            Self::Low => 130_u16,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium | Self::High => Self::High,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Low | Self::Medium => Self::Low,
            Self::High => Self::Medium,
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
    pub cbr_bitrate: Option<CbrBitrate>,
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
    use rstest::rstest;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions
    )]
    const BYTES_1_5_GB: u64 = (1.5_f64 * GB as f64) as u64;

    #[rstest]
    #[case("8G", ByteSize(8 * GB))]
    #[case("1.5GB", ByteSize(BYTES_1_5_GB))]
    #[case("8gb", ByteSize(8 * GB))]
    #[case("500M", ByteSize(500 * MB))]
    #[case("100mb", ByteSize(100 * MB))]
    #[case("900K", ByteSize(900 * KB))]
    #[case("1024B", ByteSize(1024))]
    #[case("1024", ByteSize(1024))]
    fn parse_size_valid(#[case] input: &str, #[case] expected: ByteSize) {
        assert_eq!(ByteSize::parse(input).unwrap(), expected);
    }

    #[rstest]
    #[case("", ParseSizeError::Empty)]
    #[case("0G", ParseSizeError::NegativeOrZero)]
    fn parse_size_error(#[case] input: &str, #[case] expected: ParseSizeError) {
        assert_eq!(ByteSize::parse(input), Err(expected));
    }

    #[test]
    fn parse_size_unknown_unit() {
        assert_eq!(
            ByteSize::parse("5X"),
            Err(ParseSizeError::UnknownUnit("x".to_string()))
        );
    }

    #[rstest]
    #[case(8 * GB, "8.0G")]
    #[case(500 * MB, "500.0M")]
    #[case(900 * KB, "900.0K")]
    #[case(512, "512B")]
    fn format_sizes(#[case] bytes: u64, #[case] expected: &str) {
        assert_eq!(format!("{}", ByteSize(bytes)), expected);
    }

    #[rstest]
    #[case("30", 30)]
    #[case("120", 120)]
    #[case("30s", 30)]
    #[case("2m", 120)]
    #[case("2m30s", 150)]
    #[case("2:30", 150)]
    #[case("0:45", 45)]
    #[case("10:00", 600)]
    fn parse_duration_valid(#[case] input: &str, #[case] expected_secs: u64) {
        assert_eq!(parse_duration(input).unwrap().as_secs(), expected_secs);
    }

    #[rstest]
    #[case("")]
    #[case("abc")]
    #[case("0")]
    #[case("-5")]
    fn parse_duration_errors(#[case] input: &str) {
        assert!(parse_duration(input).is_err());
    }

    #[rstest]
    #[case(VbrQuality::High, 245)]
    #[case(VbrQuality::Medium, 190)]
    #[case(VbrQuality::Low, 130)]
    fn vbr_quality_avg_bitrate(#[case] quality: VbrQuality, #[case] expected: u16) {
        assert_eq!(quality.avg_bitrate_kbps(), expected);
    }

    #[rstest]
    #[case(CbrBitrate::Kbps128, 128)]
    #[case(CbrBitrate::Kbps192, 192)]
    #[case(CbrBitrate::Kbps320, 320)]
    fn cbr_bitrate_values(#[case] bitrate: CbrBitrate, #[case] expected: u16) {
        assert_eq!(bitrate.as_kbps(), expected);
    }

    #[rstest]
    #[case(CbrBitrate::Kbps128, CbrBitrate::Kbps160)]
    #[case(CbrBitrate::Kbps320, CbrBitrate::Kbps320)]
    fn cbr_bitrate_next(#[case] input: CbrBitrate, #[case] expected: CbrBitrate) {
        assert_eq!(input.next(), expected);
    }

    #[rstest]
    #[case(CbrBitrate::Kbps320, CbrBitrate::Kbps256)]
    #[case(CbrBitrate::Kbps128, CbrBitrate::Kbps128)]
    fn cbr_bitrate_prev(#[case] input: CbrBitrate, #[case] expected: CbrBitrate) {
        assert_eq!(input.prev(), expected);
    }

    #[rstest]
    #[case(128, Some(CbrBitrate::Kbps128))]
    #[case(320, Some(CbrBitrate::Kbps320))]
    #[case(999, None)]
    fn cbr_bitrate_from_u16(#[case] input: u16, #[case] expected: Option<CbrBitrate>) {
        assert_eq!(CbrBitrate::from_u16(input), expected);
    }

    #[rstest]
    #[case(VbrQuality::Low, VbrQuality::Medium)]
    #[case(VbrQuality::High, VbrQuality::High)]
    fn vbr_quality_next(#[case] input: VbrQuality, #[case] expected: VbrQuality) {
        assert_eq!(input.next(), expected);
    }

    #[rstest]
    #[case(VbrQuality::High, VbrQuality::Medium)]
    #[case(VbrQuality::Low, VbrQuality::Low)]
    fn vbr_quality_prev(#[case] input: VbrQuality, #[case] expected: VbrQuality) {
        assert_eq!(input.prev(), expected);
    }

    #[rstest]
    #[case(65, "01:05")]
    #[case(3661, "01:01:01")]
    #[case(0, "00:00")]
    fn format_duration_display(#[case] secs: u64, #[case] expected: &str) {
        assert_eq!(format_duration(Duration::from_secs(secs)), expected);
    }
}
