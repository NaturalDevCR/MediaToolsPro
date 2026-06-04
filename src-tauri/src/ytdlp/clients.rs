/// Ordered YouTube player clients to try when no cookies are present.
/// The durable fix for breakage is still keeping yt-dlp updated.
pub const YOUTUBE_CLIENT_CHAIN: &[&str] = &["tv", "android_vr", "web_safari"];

pub fn should_try_next_client(error: &str) -> bool {
    let normalized = error.to_lowercase();
    normalized.contains("http error 403")
        || normalized.contains("forbidden")
        || normalized.contains("po_token")
        || normalized.contains("po token")
        || normalized.contains("sign in to confirm")
        || normalized.contains("confirm you're not a bot")
        || normalized.contains("requested format is not available")
        || normalized.contains("unable to extract")
        || normalized.contains("nsig")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_non_empty() {
        assert!(!YOUTUBE_CLIENT_CHAIN.is_empty());
    }

    #[test]
    fn retries_on_403_and_potoken() {
        assert!(should_try_next_client("ERROR: HTTP Error 403: Forbidden"));
        assert!(should_try_next_client("Some formats may be missing; po_token required"));
        assert!(should_try_next_client("Sign in to confirm you're not a bot"));
    }

    #[test]
    fn does_not_retry_on_disk_error() {
        assert!(!should_try_next_client("OSError: No space left on device"));
    }

    // Regression guard: normalize_error() is presentation-only and strips the
    // diagnostic keywords this predicate depends on. The fallback loop MUST run
    // on the raw yt-dlp error, never the normalized one. If someone normalizes
    // before the retry decision again, this test fails.
    #[test]
    fn retry_predicate_must_run_on_raw_not_normalized_error() {
        use crate::ytdlp::errors::normalize_error;

        let raw_potoken = "ERROR: [youtube] po_token is required for this format";
        assert!(should_try_next_client(raw_potoken));
        assert!(!should_try_next_client(&normalize_error(raw_potoken, false)));

        let raw_unavailable = "ERROR: Video unavailable";
        assert!(super::super::looks_like_public_video_failure(raw_unavailable));
        assert!(!super::super::looks_like_public_video_failure(&normalize_error(
            raw_unavailable,
            false
        )));
    }
}
