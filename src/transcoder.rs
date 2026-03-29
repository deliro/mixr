use std::fs::File;
use std::path::Path;

use mp3lame_encoder::{Builder, FlushNoGap, InterleavedPcm};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::types::{Encoding, VbrQuality};

pub struct TranscodeConfig {
    pub encoding: Encoding,
    pub cbr_bitrate: Option<u16>,
    pub vbr_quality: Option<VbrQuality>,
}

fn bitrate_from_kbps(kbps: u16) -> mp3lame_encoder::Bitrate {
    match kbps {
        128_u16 => mp3lame_encoder::Bitrate::Kbps128,
        160_u16 => mp3lame_encoder::Bitrate::Kbps160,
        224_u16 => mp3lame_encoder::Bitrate::Kbps224,
        256_u16 => mp3lame_encoder::Bitrate::Kbps256,
        320_u16 => mp3lame_encoder::Bitrate::Kbps320,
        _ => mp3lame_encoder::Bitrate::Kbps192,
    }
}

fn vbr_quality_to_lame(q: VbrQuality) -> mp3lame_encoder::Quality {
    match q {
        VbrQuality::High => mp3lame_encoder::Quality::Best,
        VbrQuality::Medium => mp3lame_encoder::Quality::NearBest,
        VbrQuality::Low => mp3lame_encoder::Quality::Decent,
    }
}

pub fn transcode(
    source: &Path,
    config: &TranscodeConfig,
    on_chunk: &mut dyn FnMut(&[u8]),
) -> Result<(), String> {
    let file = File::open(source).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = source.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let mut format = probed.format;
    let track = format.default_track().ok_or("no default track")?;
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut encoder: Option<mp3lame_encoder::Encoder> = None;
    let mut mp3_buf: Vec<u8> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<i16>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };

        let spec = *decoded.spec();
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let duration = decoded.capacity() as u64;
        let buf = sample_buf.get_or_insert_with(|| SampleBuffer::<i16>::new(duration, spec));
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();

        let enc = if let Some(e) = &mut encoder {
            e
        } else {
            let channels = spec.channels.count();
            let channels_u8 = u8::try_from(channels).map_err(|e| e.to_string())?;
            let sample_rate = spec.rate;
            encoder = Some(build_encoder(config, channels_u8, sample_rate)?);
            encoder.as_mut().ok_or("encoder init failed")?
        };

        mp3_buf.reserve(mp3lame_encoder::max_required_buffer_size(samples.len()));
        enc.encode_to_vec(InterleavedPcm(samples), &mut mp3_buf)
            .map_err(|e| format!("{e:?}"))?;

        if !mp3_buf.is_empty() {
            on_chunk(&mp3_buf);
            mp3_buf.clear();
        }
    }

    if let Some(mut enc) = encoder {
        mp3_buf.reserve(7200_usize);
        enc.flush_to_vec::<FlushNoGap>(&mut mp3_buf)
            .map_err(|e| format!("{e:?}"))?;

        if !mp3_buf.is_empty() {
            on_chunk(&mp3_buf);
        }
    }

    Ok(())
}

fn build_encoder(
    config: &TranscodeConfig,
    channels: u8,
    sample_rate: u32,
) -> Result<mp3lame_encoder::Encoder, String> {
    let mut builder = Builder::new().ok_or("lame init failed")?;
    builder
        .set_num_channels(channels)
        .map_err(|e| format!("{e:?}"))?;
    builder
        .set_sample_rate(sample_rate)
        .map_err(|e| format!("{e:?}"))?;

    match config.encoding {
        Encoding::Cbr => {
            let kbps = config.cbr_bitrate.unwrap_or(192_u16);
            builder
                .set_brate(bitrate_from_kbps(kbps))
                .map_err(|e| format!("{e:?}"))?;
        }
        Encoding::Vbr => {
            builder
                .set_vbr_mode(mp3lame_encoder::VbrMode::default())
                .map_err(|e| format!("{e:?}"))?;
            let quality = config.vbr_quality.unwrap_or(VbrQuality::Medium);
            builder
                .set_vbr_quality(vbr_quality_to_lame(quality))
                .map_err(|e| format!("{e:?}"))?;
        }
        Encoding::Keep => {}
    }

    builder.build().map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn transcode_wav_to_cbr_mp3() {
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("test.wav");
        crate::probe::tests::create_wav(&wav_path, 44100_u32, 2_u16, 2_u32);

        let config = TranscodeConfig {
            encoding: Encoding::Cbr,
            cbr_bitrate: Some(128_u16),
            vbr_quality: None,
        };

        let mut output = Vec::new();
        transcode(&wav_path, &config, &mut |chunk| {
            output.extend_from_slice(chunk);
        })
        .unwrap();

        assert!(!output.is_empty());
        assert!(output.len() > 100_usize);
    }

    #[test]
    fn transcode_wav_to_vbr_mp3() {
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("test.wav");
        crate::probe::tests::create_wav(&wav_path, 44100_u32, 2_u16, 2_u32);

        let config = TranscodeConfig {
            encoding: Encoding::Vbr,
            cbr_bitrate: None,
            vbr_quality: Some(VbrQuality::Medium),
        };

        let mut output = Vec::new();
        transcode(&wav_path, &config, &mut |chunk| {
            output.extend_from_slice(chunk);
        })
        .unwrap();

        assert!(!output.is_empty());
    }
}
