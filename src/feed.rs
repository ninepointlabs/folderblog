//! Engine-owned outputs: the RSS feed and the sitemap. Themes cannot replace these.

use crate::markdown::rewrite_attrs;
use crate::site::Site;
use crate::templates::absolute;
use crate::util::html_escape;
use anyhow::{Result, bail};

pub const FEED_PATH: &str = "feed.xml";
pub const SITEMAP_PATH: &str = "sitemap.xml";

fn cdata(s: &str) -> String {
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

/// Make every src/href/poster in `html` absolute.
pub fn absolutize(site: &Site, html: &str) -> String {
    rewrite_attrs(html, &|u| {
        if u.starts_with('/') && !u.starts_with("//") {
            absolute(&site.cfg.base_url, &site.base_path, u)
        } else {
            u.to_string()
        }
    })
}

pub fn rss(site: &Site) -> String {
    let cfg = &site.cfg;
    let base = format!("{}/", cfg.base_url);
    let posts: Vec<_> = site.posts.iter().filter(|p| !p.src.draft).take(cfg.feed.limit).collect();
    let updated = posts.iter().filter_map(|p| p.date.as_ref().map(|d| d.0)).max();
    let desc = if cfg.description.is_empty() { &cfg.title } else { &cfg.description };
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    x.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\" xmlns:content=\"http://purl.org/rss/1.0/modules/content/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<channel>\n");
    x.push_str(&format!("<title>{}</title>\n", html_escape(&cfg.title)));
    x.push_str(&format!("<link>{}</link>\n", html_escape(&base)));
    x.push_str(&format!("<description>{}</description>\n", html_escape(desc)));
    x.push_str(&format!("<language>{}</language>\n", html_escape(&cfg.language)));
    x.push_str(&format!(
        "<atom:link href=\"{}/{FEED_PATH}\" rel=\"self\" type=\"application/rss+xml\"/>\n",
        html_escape(&cfg.base_url)
    ));
    x.push_str(&format!("<generator>folderblog {}</generator>\n", env!("CARGO_PKG_VERSION")));
    if let Some(u) = updated {
        x.push_str(&format!("<lastBuildDate>{}</lastBuildDate>\n", u.to_rfc2822()));
    }
    for p in posts {
        let url = absolute(&cfg.base_url, &site.base_path, &crate::templates::prefix_base(&site.base_path, &p.route));
        x.push_str("<item>\n");
        x.push_str(&format!("<title>{}</title>\n", html_escape(&p.title)));
        x.push_str(&format!("<link>{}</link>\n", html_escape(&url)));
        x.push_str(&format!("<guid isPermaLink=\"true\">{}</guid>\n", html_escape(&url)));
        if let Some((d, _)) = &p.date {
            x.push_str(&format!("<pubDate>{}</pubDate>\n", d.to_rfc2822()));
        }
        if !cfg.author.is_empty() {
            x.push_str(&format!("<dc:creator>{}</dc:creator>\n", html_escape(&cfg.author)));
        }
        for t in &p.tags {
            x.push_str(&format!("<category>{}</category>\n", html_escape(t)));
        }
        x.push_str(&format!("<description>{}</description>\n", cdata(&absolutize(site, &p.excerpt_html))));
        x.push_str(&format!("<content:encoded>{}</content:encoded>\n", cdata(&absolutize(site, &p.html))));
        x.push_str("</item>\n");
    }
    x.push_str("</channel>\n</rss>\n");
    x
}

pub fn sitemap(site: &Site, entries: &[(String, Option<String>)]) -> String {
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (url, lastmod) in entries {
        x.push_str("<url><loc>");
        x.push_str(&html_escape(&absolute(&site.cfg.base_url, &site.base_path, url)));
        x.push_str("</loc>");
        if let Some(l) = lastmod {
            x.push_str(&format!("<lastmod>{}</lastmod>", html_escape(l)));
        }
        x.push_str("</url>\n");
    }
    x.push_str("</urlset>\n");
    x
}

/// Well-formedness plus the structural rules feed readers depend on.
pub fn validate_rss(xml: &str) -> Result<usize> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    let mut r = Reader::from_str(xml);
    let mut stack: Vec<String> = vec![];
    let mut items = 0;
    let mut item_fields: Vec<String> = vec![];
    let mut channel_fields: Vec<String> = vec![];
    let mut text_of: Option<String> = None;
    let mut problems = vec![];
    loop {
        match r.read_event() {
            Err(e) => bail!("feed is not well-formed XML at byte {}: {e}", r.buffer_position()),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.name().into_inner().to_string();
                if name == "item" {
                    items += 1;
                    item_fields.clear();
                }
                if stack.last().map(|s| s == "item").unwrap_or(false) {
                    item_fields.push(name.clone());
                }
                if stack.last().map(|s| s == "channel").unwrap_or(false) {
                    channel_fields.push(name.clone());
                }
                text_of = Some(name.clone());
                stack.push(name);
            }
            Ok(Event::Text(t)) => {
                if let (Some(n), true) = (&text_of, stack.iter().any(|s| s == "item")) {
                    if n == "link" || n == "guid" {
                        let v = t.into_inner().to_string();
                        if !(v.starts_with("http://") || v.starts_with("https://")) {
                            problems.push(format!("item {items}: <{n}> is not an absolute URL: {v}"));
                        }
                    }
                }
            }
            Ok(Event::CData(c)) => {
                let html = c.into_inner().to_string();
                for cap in regex::Regex::new(r#"(?i)\b(?:src|href)\s*=\s*["'](/[^/"'][^"']*|/)["']"#).unwrap().captures_iter(&html) {
                    problems.push(format!("item {items}: relative URL in content: {}", &cap[1]));
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().into_inner().to_string();
                if stack.pop().as_deref() != Some(name.as_str()) {
                    bail!("feed has mismatched </{name}>");
                }
                if name == "item" {
                    for req in ["title", "link", "guid", "pubDate"] {
                        if !item_fields.iter().any(|f| f == req) {
                            problems.push(format!("item {items}: missing <{req}>"));
                        }
                    }
                }
                text_of = None;
            }
            _ => {}
        }
    }
    for req in ["title", "link", "description"] {
        if !channel_fields.iter().any(|f| f == req) {
            problems.push(format!("channel missing <{req}>"));
        }
    }
    if !problems.is_empty() {
        bail!("feed problems:\n  {}", problems.join("\n  "));
    }
    Ok(items)
}
