use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Clone)]
pub enum ProgressEvent {
    Progress {
        percent: f64,
        speed: String,
        eta: String,
        total: String,
    },
    Percent(f64),
    Destination(String),
    PostProcess(String),
}

fn progress_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?([^ ]+)\s+at\s+([^ ]+)\s+ETA\s+([^ ]+)")
            .unwrap()
    })
}

fn percent_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap())
}

pub fn parse_progress_line(line: &str) -> Option<ProgressEvent> {
    if let Some(rest) = line.split("Destination:").nth(1) {
        let dest = rest.trim();
        if !dest.is_empty() {
            return Some(ProgressEvent::Destination(dest.to_string()));
        }
    }
    if line.starts_with("[ExtractAudio]")
        || line.starts_with("[Merger]")
        || line.starts_with("[VideoRemuxer]")
        || line.starts_with("[VideoConvertor]")
        || line.starts_with("[EmbedThumbnail]")
        || line.starts_with("[Metadata]")
        || line.starts_with("[SponsorBlock]")
    {
        return Some(ProgressEvent::PostProcess(line.to_string()));
    }
    if let Some(captures) = progress_re().captures(line) {
        return Some(ProgressEvent::Progress {
            percent: captures[1].parse().unwrap_or(0.0),
            total: captures[2].to_string(),
            speed: captures[3].to_string(),
            eta: captures[4].to_string(),
        });
    }
    if let Some(captures) = percent_re().captures(line) {
        return Some(ProgressEvent::Percent(
            captures[1].parse().unwrap_or(0.0),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_progress() {
        let e = parse_progress_line("[download]  42.0% of ~10.00MiB at 1.20MiB/s ETA 00:05")
            .unwrap();
        match e {
            ProgressEvent::Progress {
                percent,
                ref total,
                ref speed,
                ref eta,
            } => {
                assert_eq!(percent, 42.0);
                assert_eq!(total, "10.00MiB");
                assert_eq!(speed, "1.20MiB/s");
                assert_eq!(eta, "00:05");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_destination() {
        let e = parse_progress_line("[download] Destination: /tmp/song.webm").unwrap();
        assert_eq!(e, ProgressEvent::Destination("/tmp/song.webm".into()));
    }

    #[test]
    fn parses_postprocess() {
        let e = parse_progress_line("[Merger] Merging formats into \"x.mp4\"").unwrap();
        assert!(matches!(e, ProgressEvent::PostProcess(_)));
    }

    #[test]
    fn ignores_unrelated() {
        assert_eq!(parse_progress_line("[youtube] abc: Downloading webpage"), None);
    }
}
