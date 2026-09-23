// strips userinfo, query and fragment from a media url so it can be logged without leaking
// credentials or signed tokens
pub fn loggable_url(url: &str) -> String {
    let url = &url[..url.find(['?', '#']).unwrap_or(url.len())];
    let Some(authority_start) = url.find("://").map(|ix| ix + 3) else {
        return url.to_owned();
    };
    let authority_end = url[authority_start..]
        .find('/')
        .map_or(url.len(), |ix| authority_start + ix);
    match url[authority_start..authority_end].rfind('@') {
        Some(at) => format!(
            "{}{}",
            &url[..authority_start],
            &url[authority_start + at + 1..]
        ),
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::loggable_url;

    #[test]
    fn strips_userinfo_query_and_fragment() {
        assert_eq!(
            loggable_url("https://user:secret@example.com/video.mp4?token=abc#t=10"),
            "https://example.com/video.mp4"
        );
        assert_eq!(
            loggable_url("https://example.com/a@b/video.mp4?sig=x"),
            "https://example.com/a@b/video.mp4"
        );
        assert_eq!(
            loggable_url("https://user@example.com?x=1"),
            "https://example.com"
        );
        assert_eq!(loggable_url("/cache/video.mp4"), "/cache/video.mp4");
    }
}
