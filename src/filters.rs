use std::path::Path;
use std::time::Duration;

use crate::types::ByteSize;

pub struct FilterSet {
    allowed_extensions: Vec<String>,
    min_size: Option<ByteSize>,
    min_duration: Option<Duration>,
    no_live: bool,
}

impl FilterSet {
    pub fn new(
        allowed_extensions: Vec<String>,
        min_size: Option<ByteSize>,
        min_duration: Option<Duration>,
        no_live: bool,
    ) -> Self {
        Self {
            allowed_extensions,
            min_size,
            min_duration,
            no_live,
        }
    }

    pub fn matches(&self, path: &Path, size: u64, duration: Option<Duration>) -> bool {
        self.matches_extension(path)
            && self.matches_min_size(size)
            && self.matches_min_duration(duration)
            && self.matches_live(path)
    }

    fn matches_extension(&self, path: &Path) -> bool {
        if self.allowed_extensions.is_empty() {
            return true;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => return false,
        };
        self.allowed_extensions.iter().any(|a| a == &ext)
    }

    fn matches_min_size(&self, size: u64) -> bool {
        match self.min_size {
            Some(min) => size >= min.as_u64(),
            None => true,
        }
    }

    fn matches_min_duration(&self, duration: Option<Duration>) -> bool {
        match (self.min_duration, duration) {
            (Some(min), Some(d)) => d >= min,
            _ => true,
        }
    }

    fn matches_live(&self, path: &Path) -> bool {
        if !self.no_live {
            return true;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        !contains_live_word(&filename) && !contains_live_word(&parent)
    }
}

fn contains_live_word(s: &str) -> bool {
    s.split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "live")
}

pub fn resolve_extensions(
    include: Option<&Vec<String>>,
    exclude: Option<&Vec<String>>,
    defaults: &[&str],
) -> Vec<String> {
    let base: Vec<String> = match include {
        Some(inc) => inc.iter().map(|s| s.to_lowercase()).collect(),
        None => defaults.iter().map(|s| (*s).to_string()).collect(),
    };
    match exclude {
        Some(exc) => {
            let exc_lower: Vec<String> = exc.iter().map(|s| s.to_lowercase()).collect();
            base.into_iter()
                .filter(|e| !exc_lower.contains(e))
                .collect()
        }
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extension_filter_matches() {
        let f = FilterSet::new(
            vec!["mp3".to_string(), "flac".to_string()],
            None,
            None,
            false,
        );
        assert!(f.matches(Path::new("/music/song.mp3"), 1000, None));
        assert!(f.matches(Path::new("/music/song.FLAC"), 1000, None));
        assert!(!f.matches(Path::new("/music/song.wav"), 1000, None));
        assert!(!f.matches(Path::new("/music/noext"), 1000, None));
    }

    #[test]
    fn empty_extensions_matches_all() {
        let f = FilterSet::new(vec![], None, None, false);
        assert!(f.matches(Path::new("/music/anything.xyz"), 1000, None));
    }

    #[test]
    fn min_size_filter() {
        let f = FilterSet::new(vec![], Some(ByteSize(1000)), None, false);
        assert!(f.matches(Path::new("/song.mp3"), 1000, None));
        assert!(f.matches(Path::new("/song.mp3"), 2000, None));
        assert!(!f.matches(Path::new("/song.mp3"), 999, None));
    }

    #[test]
    fn live_filter() {
        let f = FilterSet::new(vec![], None, None, true);
        assert!(!f.matches(Path::new("/music/song live.mp3"), 1000, None));
        assert!(!f.matches(Path::new("/music/Song (Live).mp3"), 1000, None));
        assert!(!f.matches(Path::new("/music/live/song.mp3"), 1000, None));
        assert!(!f.matches(Path::new("/music/Live At Wembley/song.mp3"), 1000, None));
        assert!(f.matches(Path::new("/music/olive/song.mp3"), 1000, None));
        assert!(f.matches(Path::new("/music/deliver.mp3"), 1000, None));
        assert!(f.matches(Path::new("/music/alive.mp3"), 1000, None));
    }

    #[test]
    fn live_filter_disabled() {
        let f = FilterSet::new(vec![], None, None, false);
        assert!(f.matches(Path::new("/music/live/song.mp3"), 1000, None));
    }

    #[test]
    fn min_duration_filter() {
        let fs = FilterSet::new(vec![], None, None, false);
        assert!(fs.matches(Path::new("song.mp3"), 1000, Some(Duration::from_secs(120))));

        let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
        assert!(fs.matches(Path::new("song.mp3"), 1000, Some(Duration::from_secs(120))));
        assert!(!fs.matches(Path::new("sample.mp3"), 1000, Some(Duration::from_secs(10))));
    }

    #[test]
    fn min_duration_none_passes() {
        let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
        assert!(fs.matches(Path::new("unknown.bin"), 1000, None));
    }

    #[test]
    fn resolve_extensions_defaults() {
        let result = resolve_extensions(None, None, &["mp3", "flac"]);
        assert_eq!(result, vec!["mp3", "flac"]);
    }

    #[test]
    fn resolve_extensions_include_overrides() {
        let inc = vec!["ogg".to_string()];
        let result = resolve_extensions(Some(&inc), None, &["mp3", "flac"]);
        assert_eq!(result, vec!["ogg"]);
    }

    #[test]
    fn resolve_extensions_exclude() {
        let exc = vec!["flac".to_string()];
        let result = resolve_extensions(None, Some(&exc), &["mp3", "flac", "ogg"]);
        assert_eq!(result, vec!["mp3", "ogg"]);
    }

    #[test]
    fn resolve_extensions_include_and_exclude() {
        let inc = vec!["mp3".to_string(), "flac".to_string(), "ogg".to_string()];
        let exc = vec!["flac".to_string()];
        let result = resolve_extensions(Some(&inc), Some(&exc), &["wav"]);
        assert_eq!(result, vec!["mp3", "ogg"]);
    }
}
