//! Markdown rendering. Images, links, headings and code blocks can be overridden by
//! optional theme templates under `theme/_markup/`. Highlighting emits classes only.

use crate::util::{html_escape, slugify};
use anyhow::{Result, anyhow};
use minijinja::{Environment, Value, context};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;

pub static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
pub const HIGHLIGHT_CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

pub const MARKUP_IMAGE: &str = "_markup/image.html";
pub const MARKUP_LINK: &str = "_markup/link.html";
pub const MARKUP_HEADING: &str = "_markup/heading.html";
pub const MARKUP_CODEBLOCK: &str = "_markup/codeblock.html";

pub struct Markup<'a> {
    pub env: &'a Environment<'a>,
    /// Which of the `_markup/*` templates exist in the theme.
    pub overrides: &'a HashSet<String>,
    pub site: Value,
    /// The item being rendered: {slug, url, source, kind}. Lets overrides namespace ids.
    pub item: Value,
    /// Makes footnote ids unique when several items share a page.
    pub slug: String,
}

/// Rewrites a URL reference found in the body (e.g. relative image paths) to a site URL.
pub type Resolver<'a> = &'a dyn Fn(&str) -> String;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Rendered {
    pub html: String,
    /// Title taken from a leading heading, which is then removed from `html`.
    pub title: Option<String>,
    pub excerpt_html: String,
    pub plain: String,
    pub headings: Vec<Heading>,
    pub has_more: bool,
}

pub const MORE_MARKER: &str = "<!--more-->";

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
}

enum Frame<'a> {
    Heading { level: HeadingLevel, id: Option<String>, classes: Vec<String>, events: Vec<Event<'a>> },
    Link { dest: String, title: String, events: Vec<Event<'a>> },
    Image { dest: String, title: String, events: Vec<Event<'a>> },
    Code { lang: String, text: String },
}

static ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(src|href|poster)\s*=\s*("([^"]*)"|'([^']*)')"#).unwrap());

/// Rewrite src/href/poster attribute values in a chunk of raw HTML.
pub fn rewrite_attrs(html: &str, f: &dyn Fn(&str) -> String) -> String {
    ATTR_RE
        .replace_all(html, |c: &regex::Captures| {
            let v = c.get(3).or(c.get(4)).map(|m| m.as_str()).unwrap_or("");
            let q = if c.get(3).is_some() { '"' } else { '\'' };
            format!("{}={q}{}{q}", &c[1], f(v))
        })
        .into_owned()
}

pub fn render(
    src: &str,
    take_title: bool,
    markup: Option<&Markup>,
    resolve: Resolver,
) -> Result<Rendered> {
    let mut out = Rendered::default();
    let (main_src, has_more) = (src, src.contains(MORE_MARKER));
    out.has_more = has_more;

    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut stack: Vec<Frame> = vec![];
    let mut events: Vec<Event> = vec![];
    let mut first_block = true;
    let mut skipping_title: Option<Vec<Event>> = None;
    let mut plain = String::new();
    let mut in_code = false;
    let mut current_fn: Option<String> = None;
    let mut images = 0usize;
    let fn_prefix = markup.map(|m| format!("fn-{}", m.slug)).unwrap_or_else(|| "fn".into());
    let mut footnotes: HashMap<String, usize> = HashMap::new();
    let fn_number = |label: &str, footnotes: &mut HashMap<String, usize>| -> usize {
        let n = footnotes.len() + 1;
        *footnotes.entry(label.to_string()).or_insert(n)
    };

    let parser = Parser::new_ext(main_src, options());
    for ev in parser {
        // Leading heading becomes the title.
        if first_block {
            match &ev {
                Event::Start(Tag::Heading { .. }) if take_title => {
                    skipping_title = Some(vec![]);
                    first_block = false;
                    continue;
                }
                Event::Start(_) | Event::Text(_) | Event::Html(_) | Event::Rule => first_block = false,
                _ => {}
            }
        }
        if let Some(buf) = skipping_title.as_mut() {
            if let Event::End(TagEnd::Heading(_)) = ev {
                let mut t = String::new();
                for e in buf.iter() {
                    if let Event::Text(s) | Event::Code(s) = e {
                        t.push_str(s);
                    }
                }
                out.title = Some(t.trim().to_string());
                skipping_title = None;
            } else {
                buf.push(ev);
            }
            continue;
        }

        match &ev {
            Event::Text(t) | Event::Code(t) => {
                if !in_code {
                    plain.push_str(t);
                }
            }
            Event::SoftBreak | Event::HardBreak => plain.push(' '),
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableCell) => plain.push(' '),
            _ => {}
        }

        match ev {
            Event::Start(Tag::Heading { level, id, classes, .. }) => stack.push(Frame::Heading {
                level,
                id: id.map(|s| s.to_string()),
                classes: classes.iter().map(|c| c.to_string()).collect(),
                events: vec![],
            }),
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                stack.push(Frame::Link { dest: resolve(&dest_url), title: title.to_string(), events: vec![] })
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                stack.push(Frame::Image { dest: resolve(&dest_url), title: title.to_string(), events: vec![] })
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split([' ', ',', '{']).next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                stack.push(Frame::Code { lang, text: String::new() })
            }
            Event::Text(t) if matches!(stack.last(), Some(Frame::Code { .. })) => {
                if let Some(Frame::Code { text, .. }) = stack.last_mut() {
                    text.push_str(&t);
                }
                plain.push_str(&t);
            }
            Event::End(TagEnd::Heading(_) | TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => {
                let frame = stack.pop().ok_or_else(|| anyhow!("unbalanced markdown events"))?;
                let html = finish_frame(frame, markup, &mut ids, &mut out.headings, &mut images)?;
                in_code = false;
                push(&mut stack, &mut events, Event::InlineHtml(CowStr::from(html)));
            }
            // Footnotes: ids carry the item slug so they stay unique when several items
            // share a page, and each definition links back to its reference.
            Event::FootnoteReference(label) => {
                let n = fn_number(&label, &mut footnotes);
                let id = slugify(&label);
                let html = format!(
                    "<sup class=\"footnote-ref\"><a href=\"#{fn_prefix}-{id}\" id=\"{fn_prefix}-ref-{id}\">{n}</a></sup>"
                );
                push(&mut stack, &mut events, Event::InlineHtml(CowStr::from(html)));
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                let n = fn_number(&label, &mut footnotes);
                let id = slugify(&label);
                let html = format!(
                    "<div class=\"footnote\" id=\"{fn_prefix}-{id}\"><span class=\"footnote-number\">{n}</span>\n"
                );
                current_fn = Some(id);
                push(&mut stack, &mut events, Event::Html(CowStr::from(html)));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                let id = current_fn.take().unwrap_or_default();
                let html = format!(
                    "<a class=\"footnote-back\" href=\"#{fn_prefix}-ref-{id}\" aria-label=\"Back to reference\">↩</a></div>\n"
                );
                push(&mut stack, &mut events, Event::Html(CowStr::from(html)));
            }
            Event::Html(h) => {
                let h = rewrite_attrs(&h, resolve);
                push(&mut stack, &mut events, Event::Html(CowStr::from(h)))
            }
            Event::InlineHtml(h) => {
                let h = rewrite_attrs(&h, resolve);
                push(&mut stack, &mut events, Event::InlineHtml(CowStr::from(h)))
            }
            other => push(&mut stack, &mut events, other),
        }
    }

    // Excerpt: everything before <!--more-->, else the first top-level paragraph.
    let mut excerpt_events: Vec<Event> = vec![];
    if has_more {
        for e in &events {
            if let Event::Html(h) | Event::InlineHtml(h) = e {
                if h.contains(MORE_MARKER) {
                    break;
                }
            }
            excerpt_events.push(e.clone());
        }
    } else {
        let mut depth = 0i32;
        let mut started = false;
        for e in &events {
            match e {
                Event::Start(Tag::Paragraph) if depth == 0 => started = true,
                _ => {}
            }
            if started {
                excerpt_events.push(e.clone());
            }
            match e {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if started && depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    pulldown_cmark::html::push_html(&mut out.excerpt_html, excerpt_events.into_iter());
    pulldown_cmark::html::push_html(&mut out.html, events.into_iter());
    out.plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok(out)
}

fn push<'a>(stack: &mut [Frame<'a>], events: &mut Vec<Event<'a>>, ev: Event<'a>) {
    match stack.last_mut() {
        Some(Frame::Heading { events, .. } | Frame::Link { events, .. } | Frame::Image { events, .. }) => {
            events.push(ev)
        }
        Some(Frame::Code { text, .. }) => {
            if let Event::Text(t) = ev {
                text.push_str(&t)
            }
        }
        None => events.push(ev),
    }
}

fn inner_html(events: Vec<Event>) -> String {
    let mut s = String::new();
    pulldown_cmark::html::push_html(&mut s, events.into_iter());
    s
}

fn inner_text(events: &[Event]) -> String {
    let mut s = String::new();
    for e in events {
        match e {
            Event::Text(t) | Event::Code(t) => s.push_str(t),
            Event::InlineHtml(h) | Event::Html(h) => s.push_str(&crate::util::strip_tags(h)),
            _ => {}
        }
    }
    s
}

fn render_override(markup: Option<&Markup>, name: &str, ctx: Value) -> Result<Option<String>> {
    let Some(m) = markup else { return Ok(None) };
    if !m.overrides.contains(name) {
        return Ok(None);
    }
    let tmpl = m.env.get_template(name)?;
    let ctx = context! { site => m.site.clone(), item => m.item.clone(), ..ctx };
    // Editors add a final newline; inside a paragraph it would become a stray space.
    Ok(Some(tmpl.render(ctx)?.trim_end_matches(['\n', '\r']).to_string()))
}

fn finish_frame(
    frame: Frame,
    markup: Option<&Markup>,
    ids: &mut HashMap<String, usize>,
    headings: &mut Vec<Heading>,
    images: &mut usize,
) -> Result<String> {
    Ok(match frame {
        Frame::Heading { level, id, classes, events } => {
            let text = inner_text(&events);
            let base = id.unwrap_or_else(|| {
                let s = slugify(&text);
                if s.is_empty() { "section".into() } else { s }
            });
            let n = ids.entry(base.clone()).or_insert(0);
            let id = if *n == 0 { base.clone() } else { format!("{base}-{n}") };
            *n += 1;
            let lvl = level as u8;
            headings.push(Heading { level: lvl, id: id.clone(), text: text.clone() });
            let html = inner_html(events);
            let ctx = context! { level => lvl, id => id, text => text, html => Value::from_safe_string(html.clone()), classes => classes };
            match render_override(markup, MARKUP_HEADING, ctx)? {
                Some(s) => s,
                None => {
                    let cls = if classes.is_empty() {
                        String::new()
                    } else {
                        format!(" class=\"{}\"", html_escape(&classes.join(" ")))
                    };
                    format!("<h{lvl} id=\"{}\"{cls}>{html}</h{lvl}>\n", html_escape(&id))
                }
            }
        }
        Frame::Link { dest, title, events } => {
            let html = inner_html(events.clone());
            let text = inner_text(&events);
            let external = crate::util::has_scheme(&dest) || dest.starts_with("//");
            let ctx = context! { url => dest.clone(), title => title.clone(), text => text, html => Value::from_safe_string(html.clone()), external => external };
            match render_override(markup, MARKUP_LINK, ctx)? {
                Some(s) => s,
                None => {
                    let t = if title.is_empty() { String::new() } else { format!(" title=\"{}\"", html_escape(&title)) };
                    format!("<a href=\"{}\"{t}>{html}</a>", html_escape(&dest))
                }
            }
        }
        Frame::Image { dest, title, events } => {
            let alt = inner_text(&events);
            *images += 1;
            let ctx = context! { url => dest.clone(), alt => alt.clone(), title => title.clone(), index => *images };
            match render_override(markup, MARKUP_IMAGE, ctx)? {
                Some(s) => s,
                None => {
                    let t = if title.is_empty() { String::new() } else { format!(" title=\"{}\"", html_escape(&title)) };
                    format!("<img src=\"{}\" alt=\"{}\"{t}>", html_escape(&dest), html_escape(&alt))
                }
            }
        }
        Frame::Code { lang, text } => {
            let highlighted = highlight(&lang, &text);
            let ctx = context! { lang => lang.clone(), code => Value::from_safe_string(highlighted.clone()), raw => text };
            match render_override(markup, MARKUP_CODEBLOCK, ctx)? {
                Some(s) => s,
                None if lang.is_empty() => format!("<pre><code>{highlighted}</code></pre>\n"),
                None => format!(
                    "<pre class=\"highlight\"><code class=\"language-{}\">{highlighted}</code></pre>\n",
                    html_escape(&lang)
                ),
            }
        }
    })
}

/// Highlight code into `<span class="hl-...">` markup. Unknown languages are escaped only.
pub fn highlight(lang: &str, code: &str) -> String {
    let ss = &*SYNTAXES;
    let syntax = if lang.is_empty() { None } else { ss.find_syntax_by_token(lang) };
    let Some(syntax) = syntax else { return html_escape(code) };
    let mut g = ClassedHTMLGenerator::new_with_class_style(syntax, ss, HIGHLIGHT_CLASS_STYLE);
    for line in syntect::util::LinesWithEndings::from(code) {
        if g.parse_html_for_line_which_includes_newline(line).is_err() {
            return html_escape(code);
        }
    }
    g.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Rendered {
        render(s, true, None, &|u| u.to_string()).unwrap()
    }

    #[test]
    fn leading_heading_becomes_title_and_is_removed() {
        let out = r("# Hello *there*\n\nBody text.\n");
        assert_eq!(out.title.as_deref(), Some("Hello there"));
        assert!(!out.html.contains("<h1"));
        assert!(out.html.contains("Body text."));
    }

    #[test]
    fn non_leading_heading_is_not_title() {
        let out = r("Intro.\n\n# Later\n");
        assert_eq!(out.title, None);
        assert!(out.html.contains("<h1 id=\"later\">Later</h1>"));
    }

    #[test]
    fn heading_ids_are_unique() {
        let out = render("Intro\n\n## A\n\n## A\n", false, None, &|u| u.to_string()).unwrap();
        assert!(out.html.contains("id=\"a\""));
        assert!(out.html.contains("id=\"a-1\""));
        assert_eq!(out.headings.len(), 2);
    }

    #[test]
    fn code_uses_classes_not_inline_colours() {
        let out = r("x\n\n```rust\nfn main() {}\n```\n");
        assert!(out.html.contains("class=\"language-rust\""));
        assert!(out.html.contains("hl-"));
        assert!(!out.html.contains("style="));
    }

    #[test]
    fn excerpt_first_paragraph_or_more_marker() {
        let out = r("First para.\n\nSecond.\n");
        assert_eq!(out.excerpt_html.trim(), "<p>First para.</p>");
        let out = r("A.\n\nB.\n\n<!--more-->\n\nC.\n");
        assert!(out.excerpt_html.contains("B."));
        assert!(!out.excerpt_html.contains("C."));
    }

    #[test]
    fn raw_html_allowed_and_rewritten() {
        let out = render("<div class=x><img src=\"a.png\"></div>\n", false, None, &|u| format!("/p/{u}")).unwrap();
        assert!(out.html.contains("<img src=\"/p/a.png\">"));
    }

    #[test]
    fn tables_and_footnotes() {
        let out = r("x[^1]\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[^1]: note\n");
        assert!(out.html.contains("<table>"));
        assert!(out.html.contains("<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fn-ref-1\">1</a></sup>"), "{}", out.html);
        assert!(out.html.contains("<div class=\"footnote\" id=\"fn-1\">"), "{}", out.html);
        assert!(out.html.contains("href=\"#fn-ref-1\""), "{}", out.html);
    }
}
