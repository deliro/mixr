use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct AudioMeta {
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
}

pub fn probe(path: &Path) -> AudioMeta {
    probe_inner(path).unwrap_or(AudioMeta {
        duration: None,
        bitrate_kbps: None,
    })
}

fn probe_inner(path: &Path) -> Option<AudioMeta> {
    let file = File::open(path).ok()?;
    let file_size = file.metadata().ok()?.len();
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let track = probed.format.default_track()?;
    let params = &track.codec_params;

    let duration = params.time_base.zip(params.n_frames).map(|(tb, frames)| {
        let time = tb.calc_time(frames);
        #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
        let secs = time.seconds as f64 + time.frac;
        Duration::from_secs_f64(secs)
    });

    let bitrate_kbps = duration.and_then(|d| {
        let secs = d.as_secs_f64();
        if secs > 0.0_f64 {
            #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
            let bits_f64 = file_size.saturating_mul(8_u64) as f64;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::as_conversions
            )]
            let kbps = (bits_f64 / secs / 1000.0_f64) as u32;
            Some(kbps)
        } else {
            None
        }
    });

    Some(AudioMeta {
        duration,
        bitrate_kbps,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    #[allow(clippy::arithmetic_side_effects)]
    pub fn create_wav(path: &Path, sample_rate: u32, channels: u16, duration_secs: u32) {
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate
            .saturating_mul(u32::from(channels))
            .saturating_mul(u32::from(bits_per_sample) / 8);
        let block_align = channels.saturating_mul(bits_per_sample / 8);
        let data_size = byte_rate.saturating_mul(duration_secs);
        let file_size = 36_u32.saturating_add(data_size);

        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&file_size.to_le_bytes()).unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16_u32.to_le_bytes()).unwrap();
        f.write_all(&1_u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&sample_rate.to_le_bytes()).unwrap();
        f.write_all(&byte_rate.to_le_bytes()).unwrap();
        f.write_all(&block_align.to_le_bytes()).unwrap();
        f.write_all(&bits_per_sample.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_size.to_le_bytes()).unwrap();
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let silence = vec![0_u8; data_size as usize];
        f.write_all(&silence).unwrap();
    }

    #[test]
    fn probe_wav_duration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        create_wav(&path, 44100_u32, 2_u16, 5_u32);
        let meta = probe(&path);
        let dur = meta.duration.unwrap();
        assert!(dur.as_secs() >= 4_u64 && dur.as_secs() <= 6_u64);
    }

    #[test]
    fn probe_nonexistent_returns_none() {
        let meta = probe(Path::new("/nonexistent/file.mp3"));
        assert!(meta.duration.is_none());
        assert!(meta.bitrate_kbps.is_none());
    }
}
