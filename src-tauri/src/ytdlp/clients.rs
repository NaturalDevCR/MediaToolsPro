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
}
