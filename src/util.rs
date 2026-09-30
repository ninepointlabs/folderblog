use anyhow::{Context, Result};
use std::path::Path;

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if (c.is_whitespace() || "-_./:,+&".contains(c)) && !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

pub fn title_from_slug(s: &str) -> String {
    let words = s.replace(['-', '_'], " ");
    let words = words.trim();
    let mut chars = words.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Untitled".into(),
    }
}

pub fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Resolve `rel` (a URL reference) against the directory URL `base_dir` ("/a/b/").
/// Returns None for references that are not relative paths (absolute, schemes, anchors).
pub fn resolve_relative(base_dir: &str, rel: &str) -> Option<String> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.starts_with('#')
        || rel.starts_with('?')
        || rel.starts_with("//")
        || has_scheme(rel)
    {
        return None;
    }
    let (path, suffix) = match rel.find(['?', '#']) {
        Some(i) => (&rel[..i], &rel[i..]),
        None => (rel, ""),
    };
    let mut parts: Vec<&str> = base_dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = format!("/{}", parts.join("/"));
    if path.ends_with('/') && out != "/" {
        out.push('/');
    }
    Some(out + suffix)
}

pub fn has_scheme(s: &str) -> bool {
    match s.find(':') {
        Some(i) => {
            let scheme = &s[..i];
            !scheme.is_empty()
                && scheme.chars().next().unwrap().is_ascii_alphabetic()
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        None => false,
    }
}

/// Map a site URL path to the file it is served from inside an output directory.
pub fn url_to_file(url_path: &str) -> String {
    let p = url_path.trim_start_matches('/');
    if p.is_empty() || p.ends_with('/') { format!("{p}index.html") } else { p.to_string() }
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Rust & Jinja  "), "rust-jinja");
        assert_eq!(slugify("Café au lait"), "café-au-lait");
        assert_eq!(slugify("my_file.name"), "my-file-name");
    }

    #[test]
    fn relative_resolution() {
        assert_eq!(resolve_relative("/2024/05/trip/", "a.jpg").unwrap(), "/2024/05/trip/a.jpg");
        assert_eq!(resolve_relative("/posts/", "../x/y.png").unwrap(), "/x/y.png");
        assert_eq!(resolve_relative("/p/", "img/a.png#x").unwrap(), "/p/img/a.png#x");
        assert!(resolve_relative("/p/", "https://x.com").is_none());
        assert!(resolve_relative("/p/", "mailto:a@b").is_none());
        assert!(resolve_relative("/p/", "#top").is_none());
        assert!(resolve_relative("/p/", "/abs").is_none());
    }
}
