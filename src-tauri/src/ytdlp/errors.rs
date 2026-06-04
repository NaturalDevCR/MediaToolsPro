pub fn normalize_error(raw: &str, had_cookies: bool) -> String {
    let normalized = raw.to_lowercase();

    if normalized.contains("confirm you're not a bot")
        || normalized.contains("sign in to confirm")
    {
        return "YouTube is asking to confirm you're not a bot. Import a fresh cookies.txt in Settings, then retry.".into();
    }
    if normalized.contains("private video")
        || normalized.contains("members-only")
        || normalized.contains("sign in")
    {
        return "This video requires sign-in. Import your cookies.txt in Settings and retry.".into();
    }
    if normalized.contains("video unavailable")
        || normalized.contains("not available")
        || normalized.contains("has been removed")
    {
        return "This video isn't available (it may be private, removed, or region-locked).".into();
    }
    if normalized.contains("po_token") || normalized.contains("po token") {
        return "YouTube needs a verification token for this format. Update yt-dlp (try the nightly channel in Settings) and retry.".into();
    }
    if normalized.contains("http error 403") || normalized.contains("forbidden") {
        return "Access was blocked (HTTP 403). Update yt-dlp in Settings and retry; if it persists, import cookies.".into();
    }
    if had_cookies && (normalized.contains("cookies") || normalized.contains("expired")) {
        return format!(
            "{} Your saved cookies may be expired - export a fresh cookies.txt and re-import it.",
            raw.trim()
        );
    }

    raw.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_check_suggests_cookies() {
        let msg = normalize_error("ERROR: Sign in to confirm you're not a bot", false);
        assert!(msg.to_lowercase().contains("cookies"));
    }

    #[test]
    fn unavailable_is_friendly() {
        let msg = normalize_error("ERROR: Video unavailable", false);
        assert!(
            msg.to_lowercase().contains("isn't available")
                || msg.to_lowercase().contains("not available")
        );
    }

    #[test]
    fn unknown_error_passes_through_trimmed() {
        let msg = normalize_error("  ERROR: weird thing happened  ", false);
        assert_eq!(msg, "ERROR: weird thing happened");
    }
}
