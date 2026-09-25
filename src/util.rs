
pub fn pathname(url: &str) -> String {
    // Remove scheme://
    let (had_scheme, s) = match url.split_once("://") {
        Some((_, rest)) => (true, rest),
        None => (false, url),
    };

    // Remove host
    let s: &str = if had_scheme {
        s.find('/').map(|i| &s[i..]).unwrap_or("/")
    } else if let Some(i) = s.find('/') {
        let head = &s[..i];
        if !head.is_empty() && head.contains('.') {
            &s[i..]
        } else {
            s
        }
    } else {
        s
    };

    // Use fragment if s has '#'
    let s = s.split_once('#').map(|(_, f)| f).unwrap_or(s);

    // Remove '/' and 'm/' prefix
    let s = s.trim_start_matches('/');
    let s = s.strip_prefix("m/").unwrap_or(s);

    // Get path / query
    let (path, query) = match s.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (s, None),
    };

    // Get id property from query
    let id = query.and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "id")
            .map(|(_, v)| v)
    });

    // Splice together
    match id {
        Some(v) => format!("/{}/{}", path.trim_end_matches('/'), v),
        None => format!("/{}", path.trim_end_matches("/")),
    }
}

pub fn extract_album_id(url: &str) -> Option<u64> {
    let pathname = pathname(url);
    eprintln!("PATHNAME -> {}", pathname);
    let id_str = match pathname.split_once("/album/") {
        Some((_, rest)) => rest,
        None => return None,
    };
    match id_str.parse::<u64>() {
        Ok(id) => Some(id),
        Err(_) => None,
    }
}