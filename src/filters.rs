use std::path::Path;
use std::time::Duration;

use crate::types::ByteSize;

pub enum FilterResult {
    Pass,
    Reject,
    NeedsDuration,
}

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

    pub fn check(&self, path: &Path, size: u64, duration: Option<Duration>) -> FilterResult {
        if !self.matches_extension(path) || !self.matches_min_size(size) || !self.matches_live(path)
        {
            return FilterResult::Reject;
        }

        match (self.min_duration, duration) {
            (Some(min), Some(d)) if d < min => FilterResult::Reject,
            (Some(_), None) => FilterResult::NeedsDuration,
            _ => FilterResult::Pass,
        }
    }

    pub fn matches_extension(&self, path: &Path) -> bool {
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
    use rstest::rstest;
    use std::path::Path;

    #[rstest]
    #[case("/music/song.mp3", true)]
    #[case("/music/song.FLAC", true)]
    #[case("/music/song.wav", false)]
    #[case("/music/noext", false)]
    fn extension_filter_matches(#[case] path: &str, #[case] passes: bool) {
        let f = FilterSet::new(
            vec!["mp3".to_string(), "flac".to_string()],
            None,
            None,
            false,
        );
        assert_eq!(
            matches!(f.check(Path::new(path), 1000, None), FilterResult::Pass),
            passes,
        );
    }

    #[test]
    fn empty_extensions_matches_all() {
        let f = FilterSet::new(vec![], None, None, false);
        assert!(matches!(
            f.check(Path::new("/music/anything.xyz"), 1000, None),
            FilterResult::Pass
        ));
    }

    #[rstest]
    #[case(1000, true)]
    #[case(2000, true)]
    #[case(999, false)]
    fn min_size_filter(#[case] size: u64, #[case] passes: bool) {
        let f = FilterSet::new(vec![], Some(ByteSize(1000)), None, false);
        assert_eq!(
            matches!(
                f.check(Path::new("/song.mp3"), size, None),
                FilterResult::Pass
            ),
            passes,
        );
    }

    #[rstest]
    #[case("/music/song live.mp3", false)]
    #[case("/music/Song (Live).mp3", false)]
    #[case("/music/live/song.mp3", false)]
    #[case("/music/Live At Wembley/song.mp3", false)]
    #[case("/music/olive/song.mp3", true)]
    #[case("/music/deliver.mp3", true)]
    #[case("/music/alive.mp3", true)]
    #[case("/music/Nolive/song.mp3", true)]
    #[case("/music/livewire.mp3", true)]
    #[case("/music/LiveNation/song.mp3", true)]
    fn live_filter(#[case] path: &str, #[case] passes: bool) {
        let f = FilterSet::new(vec![], None, None, true);
        assert_eq!(
            matches!(f.check(Path::new(path), 1000, None), FilterResult::Pass),
            passes,
        );
    }

    #[test]
    fn live_filter_disabled() {
        let f = FilterSet::new(vec![], None, None, false);
        assert!(matches!(
            f.check(Path::new("/music/live/song.mp3"), 1000, None),
            FilterResult::Pass
        ));
    }

    #[test]
    fn min_duration_passes_when_sufficient() {
        let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
        assert!(matches!(
            fs.check(Path::new("song.mp3"), 1000, Some(Duration::from_secs(120))),
            FilterResult::Pass
        ));
    }

    #[test]
    fn min_duration_rejects_when_too_short() {
        let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
        assert!(matches!(
            fs.check(Path::new("sample.mp3"), 1000, Some(Duration::from_secs(10))),
            FilterResult::Reject
        ));
    }

    #[test]
    fn min_duration_needs_probe_when_unknown() {
        let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
        assert!(matches!(
            fs.check(Path::new("song.mp3"), 1000, None),
            FilterResult::NeedsDuration
        ));
    }

    #[test]
    fn no_min_duration_passes_without_probe() {
        let fs = FilterSet::new(vec![], None, None, false);
        assert!(matches!(
            fs.check(Path::new("song.mp3"), 1000, None),
            FilterResult::Pass
        ));
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
