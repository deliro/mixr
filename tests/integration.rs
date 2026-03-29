use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use mixr::copier::{CopyMsg, copy_files};
use mixr::probe;
use mixr::transcoder::{TranscodeConfig, transcode};
use mixr::types::{ByteSize, Config, Encoding, FileEntry, VbrQuality};

fn fixture_mp3() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("Custody, Just Isac, MADZI, SFRNG - No Way Back [NCS Release].mp3")
}

fn base_config(src: &Path, dst: &Path) -> Config {
    Config {
        source: src.to_path_buf(),
        destination: dst.to_path_buf(),
        max_size: None,
        min_file_size: None,
        min_duration: None,
        no_live: false,
        keep_names: true,
        overwrite: false,
        allowed_extensions: vec![],
        encoding: Encoding::Keep,
        cbr_bitrate: None,
        vbr_quality: None,
    }
}

#[test]
fn probe_real_mp3_duration_and_bitrate() {
    let meta = probe::probe(&fixture_mp3());
    let duration = meta.duration.expect("should detect duration");
    assert!(
        duration.as_secs() > 100 && duration.as_secs() < 300,
        "expected ~3min song, got {}s",
        duration.as_secs()
    );
    let bitrate = meta.bitrate_kbps.expect("should detect bitrate");
    assert!(
        bitrate > 100 && bitrate < 400,
        "expected reasonable bitrate, got {bitrate}"
    );
}

#[test]
fn copy_real_mp3_without_transcoding() {
    let dst = tempfile::tempdir().unwrap();
    let mp3 = fixture_mp3();
    let original_size = std::fs::metadata(&mp3).unwrap().len();

    let meta = probe::probe(&mp3);
    let files = vec![FileEntry {
        path: mp3.clone(),
        size: ByteSize(original_size),
        duration: meta.duration,
        bitrate_kbps: meta.bitrate_kbps,
    }];

    let config = base_config(mp3.parent().unwrap(), dst.path());
    let (tx, rx) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    copy_files(&files, &config, &tx, &shutdown);

    let messages: Vec<CopyMsg> = rx.try_iter().collect();
    assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));

    let copied = dst
        .path()
        .join("Custody, Just Isac, MADZI, SFRNG - No Way Back [NCS Release].mp3");
    assert!(copied.exists(), "copied file should exist");

    let copied_size = std::fs::metadata(&copied).unwrap().len();
    assert_eq!(
        copied_size, original_size,
        "copied file should be same size"
    );
}

#[test]
fn transcode_real_mp3_to_cbr128() {
    let mp3 = fixture_mp3();
    let original_size = std::fs::metadata(&mp3).unwrap().len();

    let config = TranscodeConfig {
        encoding: Encoding::Cbr,
        cbr_bitrate: Some(128),
        vbr_quality: None,
    };

    let mut output = Vec::new();
    transcode(&mp3, &config, &mut |chunk| {
        output.extend_from_slice(chunk);
    })
    .expect("transcode should succeed");

    assert!(!output.is_empty(), "output should not be empty");
    assert!(
        (output.len() as u64) < original_size,
        "128kbps should be smaller than original: {} vs {original_size}",
        output.len()
    );

    let meta = probe_bytes_as_mp3(&output);
    assert!(
        meta.duration.is_some(),
        "transcoded output should have duration"
    );
}

#[test]
fn transcode_real_mp3_to_vbr_medium() {
    let mp3 = fixture_mp3();

    let config = TranscodeConfig {
        encoding: Encoding::Vbr,
        cbr_bitrate: None,
        vbr_quality: Some(VbrQuality::Medium),
    };

    let mut output = Vec::new();
    transcode(&mp3, &config, &mut |chunk| {
        output.extend_from_slice(chunk);
    })
    .expect("vbr transcode should succeed");

    assert!(!output.is_empty(), "vbr output should not be empty");
    assert!(
        output.len() > 100_000,
        "vbr output should be reasonable size"
    );
}

#[test]
fn copy_real_mp3_with_cbr_reencoding() {
    let dst = tempfile::tempdir().unwrap();
    let mp3 = fixture_mp3();
    let original_size = std::fs::metadata(&mp3).unwrap().len();

    let meta = probe::probe(&mp3);
    let files = vec![FileEntry {
        path: mp3.clone(),
        size: ByteSize(original_size),
        duration: meta.duration,
        bitrate_kbps: meta.bitrate_kbps,
    }];

    let mut config = base_config(mp3.parent().unwrap(), dst.path());
    config.encoding = Encoding::Cbr;
    config.cbr_bitrate = Some(128);

    let (tx, rx) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    copy_files(&files, &config, &tx, &shutdown);

    let messages: Vec<CopyMsg> = rx.try_iter().collect();
    assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));
    assert!(
        messages.iter().any(|m| matches!(
            m,
            CopyMsg::Preparing {
                converting: true,
                ..
            }
        )),
        "should have Preparing with converting=true"
    );

    let reencoded = dst
        .path()
        .join("Custody, Just Isac, MADZI, SFRNG - No Way Back [NCS Release].mp3");
    assert!(reencoded.exists(), "reencoded file should exist");

    let reencoded_size = std::fs::metadata(&reencoded).unwrap().len();
    assert!(
        reencoded_size < original_size,
        "reencoded 128kbps ({reencoded_size}) should be smaller than original ({original_size})"
    );
}

#[test]
fn copy_real_mp3_below_threshold_copies_as_is() {
    let dst = tempfile::tempdir().unwrap();
    let mp3 = fixture_mp3();
    let original_size = std::fs::metadata(&mp3).unwrap().len();

    let meta = probe::probe(&mp3);
    let files = vec![FileEntry {
        path: mp3.clone(),
        size: ByteSize(original_size),
        duration: meta.duration,
        bitrate_kbps: meta.bitrate_kbps,
    }];

    let mut config = base_config(mp3.parent().unwrap(), dst.path());
    config.encoding = Encoding::Cbr;
    config.cbr_bitrate = Some(320);

    let (tx, rx) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    copy_files(&files, &config, &tx, &shutdown);

    let messages: Vec<CopyMsg> = rx.try_iter().collect();
    assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));

    let copied = dst
        .path()
        .join("Custody, Just Isac, MADZI, SFRNG - No Way Back [NCS Release].mp3");
    assert!(copied.exists());

    let copied_size = std::fs::metadata(&copied).unwrap().len();
    assert_eq!(
        copied_size, original_size,
        "mp3 below 320kbps threshold should be copied as-is"
    );
}

fn probe_bytes_as_mp3(data: &[u8]) -> probe::AudioMeta {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), data).unwrap();
    let path = tmp.path().with_extension("mp3");
    std::fs::rename(tmp.path(), &path).unwrap();
    probe::probe(&path)
}
