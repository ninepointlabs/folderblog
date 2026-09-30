//! The minijinja environment: loader, autoescaping, filters and functions.
//! Every filter and function registered here is listed in `contract::FILTERS` /
//! `contract::FUNCTIONS`, and a test checks the two agree.

use crate::content::split_front_matter;
use crate::util::{slugify, strip_tags};
use chrono::DateTime;
use minijinja::value::Kwargs;
use minijinja::{AutoEscape, Environment, Error, ErrorKind, State, UndefinedBehavior, Value};
use std::path::PathBuf;

pub const PAGE_TEMPLATE_PREFIX: &str = "@pages/";

/// Autoescape HTML and XML outputs only (judged by output name, ignoring a `.jinja` suffix).
pub fn autoescape_for(name: &str) -> AutoEscape {
    let n = name.strip_suffix(".jinja").unwrap_or(name);
    let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" | "xml" | "svg" | "rss" | "atom" | "xhtml" => AutoEscape::Html,
        _ => AutoEscape::None,
    }
}

pub struct EnvSettings {
    pub theme_dir: PathBuf,
    pub pages_dir: PathBuf,
    pub base_url: String,
    pub base_path: String,
    pub undefined: UndefinedBehavior,
}

pub fn make_env(s: EnvSettings) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(s.undefined);
    env.set_auto_escape_callback(autoescape_for);
    // Escape only & < > " ' (minijinja's default also escapes `/`, which makes URLs unreadable),
    // and print none as nothing rather than "None", like undefined.
    env.set_formatter(|out, state, value| {
        if value.is_none() {
            return Ok(());
        }
        if matches!(state.auto_escape(), AutoEscape::Html) && !value.is_safe() {
            if let Some(s) = value.as_str() {
                return out
                    .write_str(&crate::util::html_escape(s))
                    .map_err(|_| Error::new(ErrorKind::WriteFailure, "write failed"));
            }
        }
        minijinja::escape_formatter(out, state, value)
    });
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    minijinja_contrib::add_to_environment(&mut env);
    // contrib's `now` returns a time object; ours is a plain RFC 3339 string like every other date.
    env.add_function("now", |_: &State| -> Value { Value::from(crate::util::now_rfc3339()) });

    let theme_dir = s.theme_dir.clone();
    let pages_dir = s.pages_dir.clone();
    env.set_loader(move |name: &str| {
        if name.contains("..") {
            return Ok(None);
        }
        if let Some(rest) = name.strip_prefix(PAGE_TEMPLATE_PREFIX) {
            let path = pages_dir.join(rest);
            let Ok(text) = std::fs::read_to_string(&path) else { return Ok(None) };
            let text = text.replace("\r\n", "\n");
            // Replace front matter with blank lines so error line numbers match the file.
            return Ok(Some(match split_front_matter(&text) {
                Ok((_, n, body)) if n > 0 => "\n".repeat(n) + &body,
                _ => text,
            }));
        }
        match std::fs::read_to_string(theme_dir.join(name)) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::new(ErrorKind::InvalidOperation, format!("cannot read {name}: {e}"))),
        }
    });

    let base_path = s.base_path.clone();
    env.add_filter("url", move |v: String| -> String { prefix_base(&base_path, &v) });
    let base_path = s.base_path.clone();
    let base_url = s.base_url.clone();
    env.add_filter("absolute_url", move |v: String| -> String { absolute(&base_url, &base_path, &v) });
    env.add_filter("date", date_filter);
    env.add_filter("slugify", |v: String| slugify(&v));
    env.add_filter("markdown", |v: String| -> Result<Value, Error> {
        let r = crate::markdown::render(&v, false, None, &|u| u.to_string())
            .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;
        Ok(Value::from_safe_string(r.html))
    });
    env.add_filter("truncate_words", |v: String, n: usize| -> String {
        let text = strip_tags(&v);
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= n { words.join(" ") } else { format!("{}…", words[..n].join(" ")) }
    });
    env.add_filter("plaintext", |v: String| strip_tags(&v));
    env
}

pub fn prefix_base(base_path: &str, v: &str) -> String {
    if !v.starts_with('/') || v.starts_with("//") || base_path.is_empty() {
        return v.to_string();
    }
    if v == base_path || v.starts_with(&format!("{base_path}/")) {
        return v.to_string();
    }
    format!("{base_path}{v}")
}

pub fn absolute(base_url: &str, base_path: &str, v: &str) -> String {
    if crate::util::has_scheme(v) || v.starts_with("//") {
        return v.to_string();
    }
    let origin = &base_url[..base_url.len() - base_path.len()];
    let path = if v.starts_with('/') { prefix_base(base_path, v) } else { format!("{base_path}/{v}") };
    format!("{origin}{path}")
}

/// `{{ post.date | date("%B %-d, %Y") }}` — strftime formatting of an RFC 3339 or YYYY-MM-DD string.
fn date_filter(value: Value, format: Option<String>, kwargs: Kwargs) -> Result<String, Error> {
    kwargs.assert_all_used()?;
    if value.is_undefined() || value.is_none() {
        return Ok(String::new());
    }
    let s = value.to_string();
    let fmt = format.unwrap_or_else(|| "%Y-%m-%d".into());
    let d = if s == "now" {
        chrono::Local::now().fixed_offset()
    } else {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .or_else(|| crate::content::parse_date(&s))
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, format!("date filter: cannot parse {s:?}")))?
    };
    use std::fmt::Write;
    let mut out = String::new();
    write!(out, "{}", d.format(&fmt))
        .map_err(|_| Error::new(ErrorKind::InvalidOperation, format!("date filter: bad format {fmt:?}")))?;
    Ok(out)
}

/// Map a template name to the file a person would open.
pub fn template_file(name: &str) -> String {
    match name.strip_prefix(PAGE_TEMPLATE_PREFIX) {
        Some(rest) => format!("pages/{rest}"),
        None if name.starts_with('<') => name.to_string(),
        None => format!("theme/{name}"),
    }
}

/// "theme/x.html:12: message" for each error in the chain, innermost last.
pub fn describe_error(err: &Error) -> String {
    let mut lines = vec![];
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if let Some(me) = e.downcast_ref::<Error>() {
            let loc = match (me.name(), me.line()) {
                (Some(n), Some(l)) => format!("{}:{l}: ", template_file(n)),
                (Some(n), None) => format!("{}: ", template_file(n)),
                _ => String::new(),
            };
            let detail = me.detail().map(|d| format!(": {d}")).unwrap_or_default();
            lines.push(format!("{loc}{}{detail}", me.kind()));
        } else {
            lines.push(e.to_string());
        }
        cur = e.source();
    }
    lines.dedup();
    lines.join("\n  caused by: ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_with_base_path() {
        assert_eq!(prefix_base("", "/a/"), "/a/");
        assert_eq!(prefix_base("/blog", "/a/"), "/blog/a/");
        assert_eq!(prefix_base("/blog", "/blog/a/"), "/blog/a/");
        assert_eq!(absolute("https://x.com", "", "/a/"), "https://x.com/a/");
        assert_eq!(absolute("https://x.com/blog", "/blog", "/blog/a/"), "https://x.com/blog/a/");
        assert_eq!(absolute("https://x.com/blog", "/blog", "/a/"), "https://x.com/blog/a/");
    }

    #[test]
    fn autoescape_rules() {
        assert!(matches!(autoescape_for("index.html"), AutoEscape::Html));
        assert!(matches!(autoescape_for("feed.xml.jinja"), AutoEscape::Html));
        assert!(matches!(autoescape_for("search.json.jinja"), AutoEscape::None));
        assert!(matches!(autoescape_for("style.css.jinja"), AutoEscape::None));
    }
}
