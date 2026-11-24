use crate::jobs::is_video_format;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum VideoQuality {
    High,
    Balanced,
    Compact,
}

#[derive(Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EncodingSettings {
    sample_rate: Option<u32>,
    channels: Option<u32>,
    audio_bitrate_kbps: Option<u32>,
    pcm_bit_depth: Option<u32>,
    vorbis_quality: Option<u32>,
    flac_compression: Option<u32>,
    video_quality: Option<VideoQuality>,
}

impl EncodingSettings {
    pub(crate) fn validate(&self, format: &str) -> Result<(), String> {
        let sample_rates: &[u32] = match format {
            "opus" | "webm" => &[24_000, 48_000],
            "wav" | "aiff" | "flac" => &[44_100, 48_000, 96_000],
            _ => &[44_100, 48_000],
        };
        if self
            .sample_rate
            .is_some_and(|rate| !sample_rates.contains(&rate))
        {
            return Err("Sample rate is not supported for this format.".into());
        }
        if self
            .channels
            .is_some_and(|channels| !matches!(channels, 1 | 2))
        {
            return Err("Choose mono or stereo for the channel override.".into());
        }

        let bitrates: &[u32] = match format {
            "mp3" => &[96, 128, 160, 192, 256, 320],
            "m4a" | "aac" | "mp4" | "mov" | "mkv" => &[96, 128, 160, 192, 256],
            "opus" | "webm" => &[64, 96, 128, 160, 192, 256],
            _ => &[],
        };
        if self
            .audio_bitrate_kbps
            .is_some_and(|rate| !bitrates.contains(&rate))
        {
            return Err("Audio bitrate is not supported for this format.".into());
        }
        if self.pcm_bit_depth.is_some_and(|depth| {
            !matches!(format, "wav" | "aiff") || !matches!(depth, 16 | 24 | 32)
        }) {
            return Err("WAV and AIFF support PCM bit depths of 16, 24, and 32 bits.".into());
        }
        if self
            .vorbis_quality
            .is_some_and(|quality| format != "ogg" || !matches!(quality, 2 | 4 | 6 | 8 | 10))
        {
            return Err("Ogg Vorbis supports quality levels of 2, 4, 6, 8, and 10.".into());
        }
        if self
            .flac_compression
            .is_some_and(|level| format != "flac" || !matches!(level, 0 | 5 | 8 | 12))
        {
            return Err("FLAC supports compression levels of 0, 5, 8, and 12.".into());
        }
        if self.video_quality.is_some() && !is_video_format(format) {
            return Err("Video quality requires a video output format.".into());
        }
        Ok(())
    }

    pub(crate) fn apply_audio_args(&self, args: &mut Vec<String>, format: &str, normalize: bool) {
        let bit_depth = self.pcm_bit_depth.unwrap_or(16);
        let (codec, default_bitrate) = match (format, bit_depth) {
            ("mp3", _) => ("libmp3lame", Some(320)),
            ("opus", _) => ("libopus", Some(192)),
            ("webm", _) => ("libopus", Some(160)),
            ("flac", _) => ("flac", None),
            ("ogg", _) => ("libvorbis", None),
            ("wav", 24) => ("pcm_s24le", None),
            ("wav", 32) => ("pcm_s32le", None),
            ("wav", _) => ("pcm_s16le", None),
            ("aiff", 24) => ("pcm_s24be", None),
            ("aiff", 32) => ("pcm_s32be", None),
            ("aiff", _) => ("pcm_s16be", None),
            ("m4a" | "aac", _) => ("aac", Some(256)),
            (_, _) => ("aac", Some(192)),
        };
        args.extend(["-c:a".into(), codec.into()]);
        if let Some(bitrate) = self.audio_bitrate_kbps.or(default_bitrate) {
            args.extend(["-b:a".into(), format!("{bitrate}k")]);
        }
        if format == "ogg" {
            args.extend(["-q:a".into(), self.vorbis_quality.unwrap_or(6).to_string()]);
        }
        if let Some(level) = self.flac_compression {
            args.extend(["-compression_level:a".into(), level.to_string()]);
        }
        if let Some(rate) = self.sample_rate.or(normalize.then_some(48_000)) {
            args.extend(["-ar".into(), rate.to_string()]);
        }
        if let Some(channels) = self.channels {
            args.extend(["-ac".into(), channels.to_string()]);
        }
    }

    pub(crate) fn apply_video_args(&self, args: &mut Vec<String>, format: &str) {
        let quality = self.video_quality.unwrap_or(VideoQuality::Balanced);
        if format == "webm" {
            let crf = match quality {
                VideoQuality::High => "24",
                VideoQuality::Balanced => "32",
                VideoQuality::Compact => "38",
            };
            args.extend(["-c:v", "libvpx-vp9", "-crf", crf, "-b:v", "0"].map(String::from));
        } else {
            let crf = match quality {
                VideoQuality::High => "16",
                VideoQuality::Balanced => "20",
                VideoQuality::Compact => "28",
            };
            args.extend(["-c:v", "libx264", "-preset", "medium", "-crf", crf].map(String::from));
        }
    }
}
