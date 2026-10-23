//! Engine tests: content rules, routing, date stability, feed output, and the
//! contract documented in AGENTS.md.

use crate::build;
use crate::site::{LoadOptions, Site};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Blog {
    _tmp: tempfile::TempDir,
    pub dir: PathBuf,
}

impl Blog {
    pub fn new(toml_extra: &str) -> Blog {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("example.com");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("blog.toml"),
            format!("title = \"Test\"\nbase_url = \"https://example.com\"\n{toml_extra}\n"),
        )
        .unwrap();
        let b = Blog { _tmp: tmp, dir };
        b.write("theme/_layouts/post.html", "{{ post.title }}|{{ post.date }}|{{ post.content }}");
        b.write("theme/_layouts/page.html", "PAGE {{ page.title }}|{{ page.content }}");
        b.write("theme/index.html", "{% for p in posts %}{{ p.title }};{% endfor %}");
        b
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    pub fn site(&self) -> Site {
        Site::load(&self.dir, LoadOptions { persist_state: true, ..Default::default() }).unwrap()
    }

    pub fn build(&self) -> anyhow::Result<PathBuf> {
        build::build(&self.dir, None, LoadOptions { persist_state: true, ..Default::default() }, true).map(|r| r.out_dir)
    }

    pub fn read_out(&self, rel: &str) -> String {
        fs::read_to_string(build::public_dir(&self.dir).join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
}

fn post<'a>(site: &'a Site, slug: &str) -> &'a crate::site::Item {
    site.posts.iter().find(|p| p.slug == slug).unwrap_or_else(|| panic!("no post {slug}"))
}

// ---------- content rules ----------

#[test]
fn title_from_front_matter_then_heading_then_filename() {
    let b = Blog::new("");
    b.write("posts/fm.md", "---\ntitle: From FM\n---\n# Heading stays\n\nBody");
    b.write("posts/heading.md", "# From Heading\n\nBody");
    b.write("posts/my-file-name.md", "Just body.");
    let s = b.site();
    assert_eq!(post(&s, "fm").title, "From FM");
    assert!(post(&s, "fm").html.contains("Heading stays"), "heading kept when front matter has a title");
    assert_eq!(post(&s, "heading").title, "From Heading");
    assert!(!post(&s, "heading").html.contains("From Heading"), "leading heading removed from body");
    assert_eq!(post(&s, "my-file-name").title, "My file name");
}

#[test]
fn slug_from_filename_without_date_prefix() {
    let b = Blog::new("");
    b.write("posts/2023-04-05-Hello World.md", "x");
    b.write("posts/trip/index.md", "x");
    b.write("posts/custom.md", "---\nslug: Other Slug\n---\nx");
    let s = b.site();
    assert!(s.posts.iter().any(|p| p.slug == "hello-world"));
    assert!(s.posts.iter().any(|p| p.slug == "trip"));
    assert!(s.posts.iter().any(|p| p.slug == "other-slug"));
}

#[test]
fn date_rules_front_matter_then_filename_then_first_seen() {
    let b = Blog::new("");
    b.write("posts/2020-01-02-fm.md", "---\ndate: 2019-05-06\n---\nx");
    b.write("posts/2020-01-02-name.md", "x");
    b.write("posts/nodate.md", "x");
    let s = b.site();
    let fm = post(&s, "fm");
    assert!(fm.date.unwrap().0.to_rfc3339().starts_with("2019-05-06"));
    assert_eq!(fm.date.unwrap().1, crate::content::DateSource::FrontMatter);
    let name = post(&s, "name");
    assert!(name.date.unwrap().0.to_rfc3339().starts_with("2020-01-02"));
    assert_eq!(name.date.unwrap().1, crate::content::DateSource::Filename);
    assert_eq!(post(&s, "nodate").date.unwrap().1, crate::content::DateSource::FirstSeen);
}

#[test]
fn first_seen_date_is_stable_across_builds_and_edits() {
    let b = Blog::new("");
    b.write("posts/stable.md", "first version");
    let d1 = post(&b.site(), "stable").date.unwrap().0;
    std::thread::sleep(std::time::Duration::from_millis(1100));
    b.write("posts/stable.md", "edited version");
    let d2 = post(&b.site(), "stable").date.unwrap().0;
    assert_eq!(d1, d2, "editing must not move the date");
    // Moving to a bundle keeps the slug and therefore the date.
    fs::remove_file(b.dir.join("posts/stable.md")).unwrap();
    b.write("posts/stable/index.md", "now a bundle");
    let d3 = post(&b.site(), "stable").date.unwrap().0;
    assert_eq!(d1, d3);
    assert!(b.dir.join(".blog/state.json").exists());
}

#[test]
fn check_and_fixtures_do_not_persist_state() {
    let b = Blog::new("");
    b.write("posts/x.md", "x");
    Site::load(&b.dir, LoadOptions::default()).unwrap();
    assert!(!b.dir.join(".blog/state.json").exists());
}

#[test]
fn drafts_record_no_date_and_only_appear_in_preview() {
    let b = Blog::new("");
    b.write("drafts/wip.md", "x");
    b.write("posts/hidden.md", "---\ndraft: true\n---\nx");
    assert!(b.site().posts.iter().all(|p| p.slug != "wip"));
    let s = Site::load(&b.dir, LoadOptions { drafts: true, persist_state: true, ..Default::default() }).unwrap();
    assert!(s.posts.iter().any(|p| p.slug == "wip" && p.src.draft));
    let st = fs::read_to_string(b.dir.join(".blog/state.json")).unwrap_or_default();
    assert!(!st.contains("wip"), "drafts must not record first-seen dates");
    b.build().unwrap();
    assert!(!b.read_out("feed.xml").contains("hidden"));
    assert!(b.site().posts.iter().all(|p| p.slug != "hidden"), "draft: true stays out of real builds");
    assert!(!b.read_out("index.html").contains("hidden"));
}

#[test]
fn unknown_front_matter_passes_through() {
    let b = Blog::new("");
    b.write("theme/_layouts/post.html", "{{ post.meta.mood }}|{{ post.meta.nested.a }}|{{ post.meta.list[1] }}");
    b.write("posts/x.md", "---\nmood: sunny\nnested: {a: 1}\nlist: [a, b]\n---\nx");
    b.build().unwrap();
    let url = post(&b.site(), "x").route.clone();
    assert_eq!(b.read_out(&crate::util::url_to_file(&url)), "sunny|1|b");
}

#[test]
fn toml_front_matter() {
    let b = Blog::new("");
    b.write("posts/t.md", "+++\ntitle = \"Toml\"\ndate = 2021-02-03\n+++\nx");
    let s = b.site();
    assert_eq!(post(&s, "t").title, "Toml");
    assert!(post(&s, "t").date.unwrap().0.to_rfc3339().starts_with("2021-02-03"));
}

#[test]
fn content_files_are_never_written() {
    let b = Blog::new("");
    b.write("posts/p.md", "hello");
    let before = fs::metadata(b.dir.join("posts/p.md")).unwrap().modified().unwrap();
    b.build().unwrap();
    b.build().unwrap();
    assert_eq!(fs::read_to_string(b.dir.join("posts/p.md")).unwrap(), "hello");
    assert_eq!(fs::metadata(b.dir.join("posts/p.md")).unwrap().modified().unwrap(), before);
}

#[test]
fn deleting_a_file_unpublishes_it() {
    let b = Blog::new("");
    b.write("posts/2020-01-01-gone.md", "x");
    b.build().unwrap();
    assert!(b.dir.join(".blog/public/2020/01/gone/index.html").exists());
    fs::remove_file(b.dir.join("posts/2020-01-01-gone.md")).unwrap();
    b.build().unwrap();
    assert!(!b.dir.join(".blog/public/2020/01/gone/index.html").exists());
    assert!(!b.read_out("feed.xml").contains("gone"));
}

#[test]
fn bad_date_or_front_matter_fails_the_build() {
    let b = Blog::new("");
    b.write("posts/x.md", "---\ndate: next tuesday\n---\nx");
    assert!(b.build().unwrap_err().to_string().contains("unrecognised date"));
    b.write("posts/x.md", "---\ntitle: [unclosed\n---\nx");
    assert!(format!("{:#}", b.build().unwrap_err()).contains("front matter"));
}

// ---------- routing ----------

#[test]
fn post_and_page_urls() {
    let b = Blog::new("");
    b.write("posts/2024-05-06-hello.md", "x");
    b.write("pages/about.md", "# About\n\nme");
    b.write("pages/projects/thing.md", "x");
    b.write("pages/contact.html", "---\ntitle: Contact\n---\n<h1>{{ page.title }} on {{ site.title }}</h1>");
    b.build().unwrap();
    assert!(b.read_out("2024/05/hello/index.html").starts_with("Hello|2024-05-06"));
    assert_eq!(b.read_out("about/index.html"), "PAGE About|<p>me</p>\n");
    assert!(b.dir.join(".blog/public/projects/thing/index.html").exists());
    assert_eq!(b.read_out("contact/index.html").trim(), "<h1>Contact on Test</h1>");
}

#[test]
fn custom_permalink_pattern() {
    let b = Blog::new("permalink = \"/writing/{slug}.html\"");
    b.write("posts/2024-05-06-hello.md", "x");
    b.build().unwrap();
    assert!(b.dir.join(".blog/public/writing/hello.html").exists());
}

#[test]
fn theme_files_copied_rendered_or_private() {
    let b = Blog::new("");
    b.write("theme/assets/app.js", "const a = {{ not a template }};");
    b.write("theme/style.css.jinja", "body { color: {{ params.color }}; }");
    b.write("theme/search.json.jinja", "{{ posts | map(attribute='title') | list | tojson }}");
    b.write("theme/_partials/x.html", "private");
    b.write("blog.toml", "title = \"T\"\nbase_url = \"https://e.com\"\n[params]\ncolor = \"red\"\n");
    b.write("posts/a.md", "# A & B\n\nx");
    b.build().unwrap();
    assert_eq!(b.read_out("assets/app.js"), "const a = {{ not a template }};");
    assert_eq!(b.read_out("style.css"), "body { color: red; }");
    assert_eq!(b.read_out("search.json"), "[\"A \\u0026 B\"]", "JSON, not HTML-escaped");
    assert!(b.read_out("index.html").contains("A &amp; B"), "HTML is autoescaped");
    assert!(!b.dir.join(".blog/public/_partials").exists());
}

#[test]
fn route_header_each_and_paginate() {
    let b = Blog::new("");
    b.write("theme/tag.html", "{#---\neach = \"tags\"\nas = \"tag\"\nurl = \"/t/{{ tag.slug }}/\"\n---#}{{ tag.name }}:{{ tag.count }}");
    b.write(
        "theme/index.html",
        "{#---\npaginate = \"posts\"\nper_page = 2\nurl = \"{% if pager.number == 1 %}/{% else %}/p/{{ pager.number }}/{% endif %}\"\n---#}{{ pager.number }}/{{ pager.total }}:{% for p in pager.items %}{{ p.slug }},{% endfor %}|{{ pager.next }}",
    );
    b.write(
        "theme/tagpages.html",
        "{#---\neach = \"tags\"\npaginate = \"item.posts\"\nper_page = 1\nurl = \"/tp/{{ item.slug }}/{{ pager.number }}/\"\n---#}{{ item.name }} {{ pager.number }}",
    );
    b.write(
        "theme/series.html",
        "{#---\neach = \"posts | selectattr('meta.series') | groupby('meta.series')\"\nurl = \"/series/{{ item.grouper | slugify }}/\"\n---#}{{ item.grouper }}={{ item.list | length }}",
    );
    for (i, t) in ["a", "b", "c"].iter().enumerate() {
        b.write(&format!("posts/2024-01-0{}-{t}.md", i + 1), &format!("---\ntags: [Rust, x{t}]\nseries: S1\n---\nx"));
    }
    b.build().unwrap();
    assert_eq!(b.read_out("t/rust/index.html"), "Rust:3");
    assert_eq!(b.read_out("index.html"), "1/2:c,b,|/p/2/");
    assert_eq!(b.read_out("p/2/index.html"), "2/2:a,|");
    assert_eq!(b.read_out("tp/rust/3/index.html"), "Rust 3");
    assert_eq!(b.read_out("series/s1/index.html"), "S1=3");
}

#[test]
fn per_item_layout() {
    let b = Blog::new("");
    b.write("theme/_layouts/photo.html", "PHOTO {{ post.title }}");
    b.write("posts/2024-01-01-pic.md", "---\nlayout: photo\n---\nx");
    b.write("posts/2024-01-02-missing.md", "---\nlayout: nope\n---\nx");
    let err = format!("{:#}", b.build().unwrap_err());
    assert!(err.contains("theme/_layouts/nope.html"), "{err}");
    fs::remove_file(b.dir.join("posts/2024-01-02-missing.md")).unwrap();
    b.build().unwrap();
    assert_eq!(b.read_out("2024/01/pic/index.html"), "PHOTO Pic");
}

#[test]
fn path_collisions_are_errors() {
    let b = Blog::new("");
    b.write("pages/about.md", "x");
    b.write("theme/about.html", "{#---\nurl = \"/about/\"\n---#}x");
    let err = format!("{:#}", b.build().unwrap_err());
    assert!(err.contains("path collision: /about/index.html"), "{err}");
    let b = Blog::new("");
    b.write("theme/feed.xml.jinja", "mine");
    let err = format!("{:#}", b.build().unwrap_err());
    assert!(err.contains("/feed.xml"), "themes cannot replace the feed: {err}");
}

#[test]
fn bundles_publish_files_and_rewrite_relative_urls() {
    let b = Blog::new("");
    b.write("posts/trip/index.md", "---\ndate: 2024-02-03\n---\n![a](a.png)\n\n<img src=\"b.png\">\n\n[other](../2024-01-01-other.md)");
    b.write("posts/trip/a.png", "PNG");
    b.write("posts/trip/b.png", "PNG");
    b.write("posts/2024-01-01-other.md", "x");
    b.write("theme/index.html", "{% for p in posts %}{{ p.excerpt }}{% endfor %}");
    b.build().unwrap();
    let html = b.read_out("2024/02/trip/index.html");
    assert!(html.contains("src=\"/2024/02/trip/a.png\""), "{html}");
    assert!(html.contains("src=\"/2024/02/trip/b.png\""), "{html}");
    assert!(html.contains("href=\"/2024/01/other/\""), "{html}");
    assert_eq!(b.read_out("2024/02/trip/a.png"), "PNG");
    // The excerpt shown on the index keeps working because URLs are root-relative.
    assert!(b.read_out("index.html").contains("/2024/02/trip/a.png"));
    let s = b.site();
    assert_eq!(post(&s, "trip").files.len(), 2);
}

#[test]
fn markup_overrides() {
    let b = Blog::new("");
    b.write("theme/_markup/image.html", "<figure><img src=\"{{ url }}\" alt=\"{{ alt }}\"><figcaption>{{ alt }}</figcaption></figure>");
    b.write("theme/_markup/link.html", "<a href=\"{{ url }}\"{% if external %} rel=\"noopener\"{% endif %}>{{ html }}</a>");
    b.write("theme/_markup/heading.html", "<h{{ level }} id=\"{{ id }}\"><a href=\"#{{ id }}\">#</a> {{ html }}</h{{ level }}>");
    b.write("theme/_markup/codeblock.html", "<div class=\"code\" data-lang=\"{{ lang }}\">{{ code }}</div>");
    b.write("posts/x.md", "Intro\n\n## Sub *part*\n\n![Cat](/cat.jpg)\n\n[ext](https://x.org)\n\n```rust\nfn a() {}\n```\n");
    let s = b.site();
    let h = &post(&s, "x").html;
    assert!(h.contains("<figcaption>Cat</figcaption>"), "{h}");
    assert!(h.contains("rel=\"noopener\""), "{h}");
    assert!(h.contains("<h2 id=\"sub-part\"><a href=\"#sub-part\">#</a> Sub <em>part</em></h2>"), "{h}");
    assert!(h.contains("data-lang=\"rust\""), "{h}");
}

#[test]
fn base_path_sites() {
    let b = Blog::new("");
    b.write("blog.toml", "title = \"T\"\nbase_url = \"https://u.github.io/blog\"\n");
    b.write("theme/index.html", "{{ '/assets/x.css' | url }} {% for p in posts %}{{ p.url }} {{ p.permalink }}{% endfor %}");
    b.write("posts/2024-01-01-a.md", "![i](/img.png)");
    b.build().unwrap();
    // Output paths do not include the base path; URLs do.
    assert_eq!(b.read_out("index.html"), "/blog/assets/x.css /blog/2024/01/a/ https://u.github.io/blog/2024/01/a/");
    assert!(b.read_out("2024/01/a/index.html").contains("/blog/img.png"));
    assert!(b.read_out("feed.xml").contains("https://u.github.io/blog/img.png"));
}

// ---------- feed & sitemap ----------

#[test]
fn feed_has_full_content_with_absolute_urls_and_validates() {
    let b = Blog::new("description = \"D\"\nauthor = \"Tim\"");
    b.write("posts/2024-03-04-first.md", "# First & best\n\nPara with ![img](pic.png) and [link](/about/).\n\nSecond para.");
    b.write("posts/2024-03-05-second.md", "x ]]> y");
    b.build().unwrap();
    let feed = b.read_out("feed.xml");
    assert_eq!(crate::feed::validate_rss(&feed).unwrap(), 2);
    assert!(feed.contains("<title>First &amp; best</title>"));
    assert!(feed.contains("<link>https://example.com/2024/03/first/</link>"));
    assert!(feed.contains("src=\"https://example.com/posts/pic.png\""), "{feed}");
    assert!(feed.contains("href=\"https://example.com/about/\""));
    assert!(feed.contains("Second para."), "full content, not just the excerpt");
    assert!(feed.contains("<pubDate>Mon, 4 Mar 2024") || feed.contains("<pubDate>"), "{feed}");
    // Newest first.
    assert!(feed.find("second/").unwrap() < feed.find("first/").unwrap());
    let sitemap = b.read_out("sitemap.xml");
    assert!(sitemap.contains("<loc>https://example.com/2024/03/first/</loc>"));
    assert!(sitemap.contains("<loc>https://example.com/</loc>"));
}

#[test]
fn feed_limit_and_deterministic_output() {
    let b = Blog::new("[feed]\nlimit = 1");
    b.write("posts/2024-01-01-a.md", "a");
    b.write("posts/2024-01-02-b.md", "b");
    b.build().unwrap();
    let f1 = b.read_out("feed.xml");
    assert_eq!(crate::feed::validate_rss(&f1).unwrap(), 1);
    b.build().unwrap();
    assert_eq!(f1, b.read_out("feed.xml"), "rebuilding unchanged content gives identical output");
}

#[test]
fn feed_validator_catches_problems() {
    assert!(crate::feed::validate_rss("<rss><channel><title>x</title>").is_err());
    let bad = "<rss><channel><title>t</title><link>https://x</link><description>d</description><item><title>a</title><link>/rel/</link><guid>/rel/</guid><pubDate>x</pubDate></item></channel></rss>";
    assert!(crate::feed::validate_rss(bad).unwrap_err().to_string().contains("absolute"));
}

// ---------- failure safety ----------

#[test]
fn failed_build_keeps_last_good_output() {
    let b = Blog::new("");
    b.write("posts/2024-01-01-a.md", "a");
    b.build().unwrap();
    let good = b.read_out("2024/01/a/index.html");
    b.write("theme/_layouts/post.html", "{% if %}broken");
    b.write("posts/2024-01-01-a.md", "changed");
    let err = format!("{:#}", b.build().unwrap_err());
    assert!(err.contains("theme/_layouts/post.html:1"), "error names file and line: {err}");
    assert_eq!(b.read_out("2024/01/a/index.html"), good);
    let st = crate::status::load(&b.dir);
    assert!(!st.build.unwrap().ok);
    assert!(st.last_good_build.is_some());
}

#[test]
fn check_reports_broken_links_and_template_errors() {
    let b = Blog::new("");
    b.write("theme/index.html", "<a href=\"/nope/\">x</a><a href=\"{{ site.feed_url }}\">f</a><a href=\"https://ext\">e</a>");
    b.write("posts/2024-01-01-a.md", "[bad](/missing.png)");
    let rep = crate::check::check(&b.dir, None).unwrap();
    assert!(rep.errors.iter().any(|e| e.contains("\"/nope/\"")), "{:?}", rep.errors);
    assert!(rep.errors.iter().any(|e| e.contains("/missing.png")), "{:?}", rep.errors);
    assert!(!rep.errors.iter().any(|e| e.contains("feed.xml")), "{:?}", rep.errors);
    b.write("theme/index.html", "{{ posts | nosuchfilter }}");
    let rep = crate::check::check(&b.dir, None).unwrap();
    assert!(rep.errors.iter().any(|e| e.contains("theme/index.html:1")), "{:?}", rep.errors);
}

// ---------- default theme, fixtures and the AGENTS.md contract ----------

fn new_default_blog() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("example.org");
    crate::new_blog(&dir, None).unwrap();
    (tmp, dir)
}

fn enable_gallery(dir: &Path) {
    let cfg = fs::read_to_string(dir.join("blog.toml")).unwrap();
    fs::write(dir.join("blog.toml"), crate::gallery::set_enabled(&cfg, true).unwrap()).unwrap();
}

/// `folderblog podcast on`, plus the settings a real show needs.
fn enable_podcast(dir: &Path) {
    crate::podcast_cmd(dir, Some("on")).unwrap();
    let cfg = fs::read_to_string(dir.join("blog.toml")).unwrap();
    let mut doc: toml_edit::DocumentMut = cfg.parse().unwrap();
    doc["podcast"]["category"] = toml_edit::value("Technology");
    doc["podcast"]["owner_email"] = toml_edit::value("show@example.org");
    fs::write(dir.join("blog.toml"), doc.to_string()).unwrap();
}

#[test]
fn default_theme_passes_check_on_all_fixtures() {
    let (_t, dir) = new_default_blog();
    let (_t2, gdir) = new_default_blog();
    enable_gallery(&gdir);
    enable_podcast(&gdir);
    for (dir, f) in crate::fixtures::names().into_iter().map(|f| (&dir, f)).chain(crate::fixtures::names().into_iter().map(|f| (&gdir, f))) {
        let rep = crate::check::check(dir, Some(f.clone())).unwrap();
        assert!(rep.errors.is_empty(), "fixture {f}: {:#?}", rep.errors);
        let undefined: Vec<_> = rep.warnings.iter().filter(|w| w.contains("undefined")).collect();
        assert!(undefined.is_empty(), "fixture {f}: {undefined:#?}");
    }
    let rep = crate::check::check(&dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:#?}", rep.errors);
}

#[test]
fn agents_md_is_generated_and_current() {
    let (_t, dir) = new_default_blog();
    let text = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(crate::contract::is_current(&text));
    // Notes outside the markers survive regeneration.
    let with_notes = format!("{text}\n## My notes\nkeep me\n");
    assert!(crate::contract::update(Some(&with_notes)).contains("keep me"));
    for (name, _) in crate::cli::command_docs() {
        assert!(text.contains(&format!("folderblog {name}")), "{name} undocumented");
    }
}

fn eval(site: &Site, ctx: &std::collections::BTreeMap<String, minijinja::Value>, expr: &str) -> String {
    site.env.render_str(expr, ctx).unwrap_or_else(|e| panic!("{expr}: {e:#}")).to_lowercase()
}

#[test]
fn every_documented_field_exists() {
    let (_t, dir) = new_default_blog();
    enable_gallery(&dir);
    enable_podcast(&dir);
    let site = Site::load(&dir, LoadOptions { fixtures: Some("stress".into()), ..Default::default() }).unwrap();
    let plan = site.plan().unwrap();
    let post_out = plan
        .outputs
        .iter()
        .find(|o| matches!(&o.content, crate::render::Content::Render { ctx, .. } if ctx.contains_key("post")))
        .unwrap();
    let crate::render::Content::Render { ctx, .. } = &post_out.content else { unreachable!() };
    let mut ctx = ctx.clone();
    ctx.insert("pager".into(), minijinja::Value::from_serialize(std::collections::BTreeMap::<String, i32>::new()));
    for g in crate::contract::GLOBALS {
        if ["pager", "page", "photo", "album", "episode"].contains(&g.name) {
            continue;
        }
        assert_eq!(eval(&site, &ctx, &format!("{{{{ {} is defined }}}}", g.name)), "true", "global {}", g.name);
    }
    for f in crate::contract::SITE_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ site.{} is defined }}}}", f.name)), "true", "site.{}", f.name);
    }
    for f in crate::contract::ITEM_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ post.{} is defined }}}}", f.name)), "true", "post.{}", f.name);
        assert_eq!(eval(&site, &ctx, &format!("{{{{ pages[0].{} is defined }}}}", f.name)), "true", "page.{}", f.name);
    }
    for f in crate::contract::TAG_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ tags[0].{} is defined }}}}", f.name)), "true", "tag.{}", f.name);
    }
    for f in crate::contract::YEAR_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ years[0].{} is defined }}}}", f.name)), "true", "year.{}", f.name);
    }
    let groups: [(&str, &[crate::contract::Field]); 4] = [
        ("gallery", crate::contract::GALLERY_FIELDS),
        ("gallery.photos[0]", crate::contract::PHOTO_FIELDS),
        ("gallery.albums[0]", crate::contract::ALBUM_FIELDS),
        ("gallery.tags[0]", crate::contract::GALLERY_TAG_FIELDS),
    ];
    for (base, fields) in groups {
        for f in fields {
            assert_eq!(eval(&site, &ctx, &format!("{{{{ {base}.{} is defined }}}}", f.name)), "true", "{base}.{}", f.name);
        }
    }
    for f in crate::contract::PODCAST_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ podcast.{} is defined }}}}", f.name)), "true", "podcast.{}", f.name);
    }
    for f in crate::contract::EPISODE_FIELDS.iter().filter(|f| !f.name.starts_with('(')) {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ podcast.episodes[0].{} is defined }}}}", f.name)), "true", "episode.{}", f.name);
    }
    for f in crate::contract::ITEM_FIELDS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ podcast.episodes[0].{} is defined }}}}", f.name)), "true", "episode.{}", f.name);
    }
    for (name, global) in [("_gallery/photo.html", "photo"), ("_gallery/album.html", "album"), ("_podcast/episode.html", "episode")] {
        let o = plan.outputs.iter().find(|o| matches!(&o.content, crate::render::Content::Render { template, .. } if template == name)).unwrap();
        let crate::render::Content::Render { ctx: c, .. } = &o.content else { unreachable!() };
        assert_eq!(eval(&site, c, &format!("{{{{ {global} is defined and item is defined }}}}")), "true", "{global}");
    }
    // Pager fields, from the default theme's paginated index.
    let idx = plan.outputs.iter().find(|o| o.url == "/").unwrap();
    let crate::render::Content::Render { ctx: ictx, .. } = &idx.content else { unreachable!() };
    for f in crate::contract::PAGER_FIELDS {
        assert_eq!(eval(&site, ictx, &format!("{{{{ pager.{} is defined }}}}", f.name)), "true", "pager.{}", f.name);
    }
    for (name, _) in crate::contract::FILTERS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ '{name}' is filter }}}}")), "true", "filter {name}");
    }
    for name in crate::contract::BUILTIN_FILTERS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ '{name}' is filter }}}}")), "true", "builtin filter {name}");
    }
    for (name, _) in crate::contract::FUNCTIONS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ {name} is defined }}}}")), "true", "function {name}");
    }
    // Python-style methods are documented as working.
    assert_eq!(eval(&site, &ctx, "{{ 'a,b'.split(',') | length }}{{ 'abc'.startswith('a') }}"), "2true");
}

#[test]
fn every_documented_config_key_parses() {
    let value = |k: &crate::contract::Field| -> String {
        if k.name == "analytics.goatcounter" || k.name == "podcast.storage.public_url" {
            "'https://stats.example.com'".into()
        } else if k.name == "podcast.storage.type" {
            "'s3'".into()
        } else if k.name == "podcast.storage.endpoint" {
            "'https://s3.example.com'".into()
        } else if k.name == "podcast.storage.jurisdiction" {
            "'eu'".into()
        } else if k.name == "podcast.storage.bucket" {
            "'show-podcast'".into()
        } else if k.name == "podcast.type" {
            "'serial'".into()
        } else if k.name == "podcast.medium" {
            "'video'".into()
        } else if k.name == "podcast.funding" {
            "[{ url = 'https://x.com', text = 'Support' }]".into()
        } else if k.name == "podcast.persons" {
            "[{ name = 'Ada', role = 'host', href = 'https://x.com' }]".into()
        } else if k.ty.starts_with("list") {
            "['x']".into()
        } else if k.ty == "int" {
            "3".into()
        } else if k.ty == "bool" {
            "true".into()
        } else {
            "'x'".into()
        }
    };
    let mut toml = String::from("title='t'\nbase_url='https://x.com'\n");
    let mut tables: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
    let mut deploy_keys = vec![];
    for k in crate::contract::CONFIG_KEYS {
        match k.name.split_once('.') {
            Some(("deploy", "type")) => {}
            Some(("deploy", key)) => deploy_keys.push((key, k)),
            Some((t, key)) => tables.entry(t).or_default().push(format!("{key} = {}", value(k))),
            None if ["title", "base_url", "params"].contains(&k.name) => {}
            None => toml.push_str(&format!("{} = '/{{slug}}/'\n", k.name)),
        }
    }
    for (t, lines) in tables {
        toml.push_str(&format!("[{t}]\n{}\n", lines.join("\n")));
    }
    toml.push_str("[params]\nanything = 1\n");
    crate::config::Config::parse(&toml).unwrap_or_else(|e| panic!("{e:#}\n{toml}"));
    // Each deploy key is documented as "type: …"; every key must parse under each type it names.
    for ty in ["github", "git", "command"] {
        let mut d = format!("title='t'\nbase_url='https://x.com'\n[deploy]\ntype = '{ty}'\n");
        let mut n = 0;
        for (key, k) in &deploy_keys {
            let types = k.doc.split(':').next().unwrap_or("");
            if types.split(',').any(|t| t.trim() == ty) {
                d.push_str(&format!("{key} = {}\n", value(k)));
                n += 1;
            }
        }
        assert!(n > 0, "no documented keys for deploy type {ty}");
        crate::config::Config::parse(&d).unwrap_or_else(|e| panic!("{e:#}\n{d}"));
    }
}

#[test]
fn new_blog_with_github_login_publishes_to_pages() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("my-blog");
    crate::new_blog(&dir, Some(("NinePointLabs", "my-blog", None))).unwrap();
    let cfg = crate::config::Config::load(&dir).unwrap();
    assert_eq!(cfg.base_url, "https://ninepointlabs.github.io/my-blog");
    assert!(matches!(cfg.deploy, Some(crate::config::DeployConfig::Github { ref repo, ref domain, .. }) if repo == "my-blog" && domain.is_none()));
    let dir2 = tmp.path().join("example.com");
    crate::new_blog(&dir2, Some(("me", "example.com", Some("example.com")))).unwrap();
    let cfg = crate::config::Config::load(&dir2).unwrap();
    assert_eq!(cfg.base_url, "https://example.com");
    assert!(matches!(cfg.deploy, Some(crate::config::DeployConfig::Github { domain: Some(ref d), .. }) if d == "example.com"));
    // And the generated site passes check.
    let rep = crate::check::check(&dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
}

#[test]
fn git_target_writes_cname_for_custom_domain() {
    let b = Blog::new("");
    b.write("posts/2024-01-01-a.md", "a");
    let bare = b.dir.parent().unwrap().join("remote.git");
    std::process::Command::new("git").args(["init", "-q", "--bare"]).arg(&bare).status().unwrap();
    let out = b.build().unwrap();
    let t = crate::deploy::GitTarget {
        remote: bare.to_string_lossy().into(),
        branch: "gh-pages".into(),
        nojekyll: true,
        author_name: "t".into(),
        author_email: "t@x".into(),
        env: vec![],
        cname: Some("blog.example.com".into()),
    };
    use crate::deploy::Target;
    t.deploy(&b.dir, &out).unwrap();
    let show = std::process::Command::new("git").arg("--git-dir").arg(&bare).args(["show", "gh-pages:CNAME"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&show.stdout), "blog.example.com\n");
    // Redeploying keeps it (GitHub would otherwise lose the custom domain).
    b.write("posts/2024-01-02-b.md", "b");
    let out = b.build().unwrap();
    t.deploy(&b.dir, &out).unwrap();
    let show = std::process::Command::new("git").arg("--git-dir").arg(&bare).args(["show", "gh-pages:CNAME"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&show.stdout), "blog.example.com\n");
}

/// Run git in `dir` with no user or system config, so tests don't depend on the machine.
fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@x", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A blog with a `command` target that only touches a marker file next to the blog folder.
fn held_blog(extra: &str) -> (Blog, PathBuf) {
    let b = Blog::new(&format!("{extra}\n[deploy]\ntype = \"command\"\ncommand = 'touch \"$FOLDERBLOG_BLOG/../deployed\"'"));
    b.write("posts/2024-01-01-a.md", "a");
    b.build().unwrap();
    let marker = b.dir.parent().unwrap().join("deployed");
    (b, marker)
}

fn held_err(b: &Blog) -> String {
    match crate::deploy::deploy(&b.dir, false) {
        Ok(_) => panic!("deploy should be held"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn deploy_branch_unset_deploys_without_git() {
    let (b, marker) = held_blog("");
    assert!(matches!(crate::deploy::deploy(&b.dir, false).unwrap(), crate::deploy::Outcome::Deployed(_)));
    assert!(marker.exists());
}

#[test]
fn deploy_branch_holds_outside_a_git_repo() {
    let (b, marker) = held_blog("deploy_branch = \"main\"");
    assert!(held_err(&b).starts_with("held: deploy_branch is set but"));
    assert!(!marker.exists());
}

#[test]
fn deploy_branch_deploys_on_the_branch_and_holds_elsewhere() {
    let (b, marker) = held_blog("deploy_branch = \"main\"");
    git(&b.dir, &["init", "-q", "-b", "main"]);
    git(&b.dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    crate::deploy::source_ready(&b.dir, "main").unwrap();

    git(&b.dir, &["checkout", "-q", "-b", "draft"]);
    assert_eq!(held_err(&b), r#"held: on branch "draft", deploys only from "main""#);
    assert!(!marker.exists());
    // Recorded as a failed deploy, which is what status shows and the watcher retries.
    let d = crate::status::load(&b.dir).deploy.unwrap();
    assert!(!d.ok && d.error.unwrap().starts_with("held: "));

    git(&b.dir, &["checkout", "-q", "--detach", "main"]);
    assert_eq!(held_err(&b), r#"held: detached HEAD, deploys only from "main""#);

    git(&b.dir, &["checkout", "-q", "main"]);
    fs::write(b.dir.join(".git/MERGE_HEAD"), "0000000000000000000000000000000000000000\n").unwrap();
    assert_eq!(held_err(&b), "held: git operation in progress (MERGE_HEAD)");
    assert!(!marker.exists());

    fs::remove_file(b.dir.join(".git/MERGE_HEAD")).unwrap();
    assert!(matches!(crate::deploy::deploy(&b.dir, false).unwrap(), crate::deploy::Outcome::Deployed(_)));
    assert!(marker.exists());
    assert!(crate::status::load(&b.dir).deploy.unwrap().ok);
}

#[test]
fn deploy_branch_uses_the_worktree_git_dir() {
    let (b, _) = held_blog("");
    git(&b.dir, &["init", "-q", "-b", "main"]);
    git(&b.dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let wt = b.dir.parent().unwrap().join("wt");
    git(&b.dir, &["worktree", "add", "-q", "-b", "main2", wt.to_str().unwrap()]);
    crate::deploy::source_ready(&wt, "main2").unwrap();
    fs::create_dir_all(b.dir.join(".git/worktrees/wt/rebase-merge")).unwrap();
    let e = crate::deploy::source_ready(&wt, "main2").unwrap_err().to_string();
    assert_eq!(e, "held: git operation in progress (rebase-merge)");
    // The main checkout's own state is separate.
    crate::deploy::source_ready(&b.dir, "main").unwrap();
}

#[test]
fn every_documented_route_key_parses() {
    let header = format!(
        "{}\n{}\n{}",
        crate::render::ROUTE_OPEN,
        crate::contract::ROUTE_KEYS
            .iter()
            .map(|k| if k.name == "per_page" { "per_page = 5".to_string() } else { format!("{} = \"x\"", k.name) })
            .collect::<Vec<_>>()
            .join("\n"),
        crate::render::ROUTE_CLOSE
    );
    crate::render::parse_route_header(&header).unwrap().unwrap();
}

#[test]
fn documented_markup_templates_are_the_ones_used() {
    let names: Vec<&str> = crate::contract::MARKUP.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        vec![crate::markdown::MARKUP_IMAGE, crate::markdown::MARKUP_LINK, crate::markdown::MARKUP_HEADING, crate::markdown::MARKUP_CODEBLOCK]
    );
}

#[allow(dead_code)]
fn exists(p: &Path) -> bool {
    p.exists()
}

// ---------- fixes from the theme-agent flexibility test ----------

#[test]
fn resolve_url_resolves_front_matter_paths() {
    let b = Blog::new("");
    b.write("posts/cover.svg", "<svg/>");
    b.write("posts/trip/index.md", "---\ndate: 2024-02-03\ncover: hero.svg\n---\nx");
    b.write("posts/trip/hero.svg", "<svg/>");
    b.write("posts/2024-01-01-flat.md", "---\ncover: cover.svg\nlink: https://x.org/a.png\nabs: /y.png\n---\nx");
    b.write(
        "theme/_layouts/post.html",
        "{{ post.meta.cover | resolve_url(post) }}|{{ post.meta.link | resolve_url(post) }}|{{ post.meta.abs | resolve_url(post) }}",
    );
    b.build().unwrap();
    assert_eq!(b.read_out("2024/02/trip/index.html"), "/2024/02/trip/hero.svg||");
    assert_eq!(b.read_out("2024/01/flat/index.html"), "/posts/cover.svg|https://x.org/a.png|/y.png");
}

#[test]
fn markup_overrides_get_item_and_index_and_no_trailing_newline() {
    let b = Blog::new("");
    b.write("theme/_markup/image.html", "<span data-n=\"{{ index }}\" data-post=\"{{ item.slug }}\">{{ alt }}</span>\n");
    b.write("theme/_markup/heading.html", "<h{{ level }} id=\"{{ item.slug }}--{{ id }}\">{{ html }}</h{{ level }}>\n");
    b.write("posts/p.md", "Intro ![a](a.png), then ![b](b.png).\n\n## Part\n");
    let s = b.site();
    let h = &post(&s, "p").html;
    assert!(h.contains("<span data-n=\"1\" data-post=\"p\">a</span>, then <span data-n=\"2\""), "{h}");
    assert!(h.contains("id=\"p--part\""), "{h}");
}

#[test]
fn footnote_ids_are_unique_per_item() {
    let b = Blog::new("");
    b.write("posts/one.md", "a[^n]\n\n[^n]: first");
    b.write("posts/two.md", "b[^n]\n\n[^n]: second");
    let s = b.site();
    assert!(post(&s, "one").html.contains("id=\"fn-one-n\""));
    assert!(post(&s, "two").html.contains("id=\"fn-two-n\""));
    assert!(post(&s, "two").html.contains("href=\"#fn-two-ref-n\""));
}

#[test]
fn summary_keeps_punctuation_attached() {
    let b = Blog::new("");
    b.write("posts/p.md", "With **bold**, *italic*, and `code`.");
    let s = b.site();
    assert_eq!(post(&s, "p").value.get_attr("summary").unwrap().to_string(), "With bold, italic, and code.");
}

#[test]
fn merged_tag_slugs_warn() {
    let b = Blog::new("");
    b.write("posts/a.md", "---\ntags: [C++]\n---\nx");
    b.write("posts/b.md", "---\ntags: [c]\n---\nx");
    assert!(b.site().warnings.iter().any(|w| w.contains("share the slug")));
}

#[test]
fn documented_tests_exist() {
    let (_t, dir) = new_default_blog();
    let site = Site::load(&dir, LoadOptions::default()).unwrap();
    let ctx = std::collections::BTreeMap::new();
    for t in crate::contract::TESTS {
        assert_eq!(eval(&site, &ctx, &format!("{{{{ '{t}' is test }}}}")), "true", "test {t}");
    }
}

// ---------- gallery ----------

fn write_image(path: &Path, w: u32, h: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
    img.save(path).unwrap();
}

fn gallery_blog() -> Blog {
    let b = Blog::new("[gallery]\nenabled = true");
    for t in crate::gallery::TEMPLATES {
        let name = t.rsplit('/').next().unwrap();
        let body = match name {
            "index.html" => "{{ gallery.title }}|{% for p in gallery.photos %}{{ p.title }}={{ p.thumb }};{% endfor %}|{% for a in gallery.albums %}{{ a.title }}:{{ a.count }}:{{ a.cover.slug }};{% endfor %}",
            "album.html" => "{{ album.title }}|{% for p in album.photos %}{{ p.slug }};{% endfor %}",
            "tag.html" => "#{{ tag.name }}|{% for p in tag.photos %}{{ p.slug }};{% endfor %}",
            _ => "{{ photo.title }}|{{ photo.caption }}|{{ photo.width }}x{{ photo.height }}|{{ photo.date_source }}|{% for t in photo.tags %}{{ t.name }}@{{ t.url }};{% endfor %}|{{ photo.album.title }}|{{ photo.prev.url }}|{{ photo.next.url }}",
        };
        b.write(&format!("theme/{t}"), body);
    }
    b
}

#[test]
fn gallery_publishes_resized_photos_albums_and_tags() {
    let b = gallery_blog();
    write_image(&b.dir.join("gallery/2024-01-05-sunset.jpg"), 3000, 2000);
    write_image(&b.dir.join("gallery/IMG_2041.jpg"), 300, 400);
    write_image(&b.dir.join("gallery/trip/one.png"), 200, 100);
    write_image(&b.dir.join("gallery/trip/two.jpg"), 200, 100);
    write_image(&b.dir.join("gallery/trip/hidden.jpg"), 200, 100);
    b.write("gallery/index.md", "# Pictures\n\nIntro.\n");
    b.write("gallery/2024-01-05-sunset.md", "---\ntags: [Sky, dusk]\n---\n# Red sky\n\nOver the *bay*.\n");
    b.write("gallery/trip/index.md", "---\ntitle: The Trip\ntags: [travel]\ndate: 2024-02-01\ncover: two.jpg\n---\nWe went.\n");
    b.write("gallery/trip/one.png.md", "---\ndate: 2024-02-02\ntags: [sky]\n---\n");
    b.write("gallery/trip/two.md", "---\ndate: 2024-02-03\n---\n");
    b.write("gallery/trip/hidden.md", "---\ndraft: true\n---\n");
    b.build().unwrap();

    let idx = b.read_out("gallery/index.html");
    assert!(idx.starts_with("Pictures|"), "{idx}");
    assert!(idx.contains("Red sky=/gallery/sunset.thumb.jpg;"), "{idx}");
    assert!(idx.contains("=/gallery/img-2041.thumb.jpg;"), "camera names have no title: {idx}");
    assert!(idx.contains("The Trip:2:two;"), "{idx}");
    assert!(!idx.contains("hidden"), "drafts stay out of real builds");

    let sunset = b.read_out("gallery/sunset/index.html");
    assert!(sunset.starts_with("Red sky|<p>Over the <em>bay</em>.</p>|2048x1365|filename|"), "{sunset}");
    assert!(sunset.contains("Sky@/gallery/tags/sky/;dusk@/gallery/tags/dusk/;"), "{sunset}");
    // The published image is resized and the original is never published.
    let out = build::public_dir(&b.dir);
    let img = image::open(out.join("gallery/sunset.jpg")).unwrap();
    assert_eq!((img.width(), img.height()), (2048, 1365));
    let thumb = image::open(out.join("gallery/sunset.thumb.jpg")).unwrap();
    assert!(thumb.height() <= 560 && thumb.width() <= 1600);
    assert!(!out.join("gallery/2024-01-05-sunset.jpg").exists());
    // Transparent PNGs stay PNG; album tags apply to every photo; neighbours follow album order.
    let one = b.read_out("gallery/trip/one/index.html");
    assert!(one.contains("sky@/gallery/tags/sky/;travel@/gallery/tags/travel/;|The Trip||/gallery/trip/two/"), "{one}");
    assert!(out.join("gallery/trip/one.thumb.jpg").exists() || out.join("gallery/trip/one.thumb.png").exists());
    assert_eq!(b.read_out("gallery/trip/index.html"), "The Trip|one;two;");
    // Tags merge by slug ("Sky" and "sky") and list photos newest first.
    assert_eq!(b.read_out("gallery/tags/sky/index.html"), "#sky|one;sunset;");
    assert_eq!(b.read_out("gallery/tags/travel/index.html"), "#travel|two;one;");
    // Gallery pages are in the sitemap; drafts appear in preview.
    assert!(b.read_out("sitemap.xml").contains("/gallery/trip/two/"));
    let preview = Site::load(&b.dir, LoadOptions { drafts: true, ..Default::default() }).unwrap();
    assert!(preview.gallery.as_ref().unwrap().photos.iter().any(|p| p.slug == "hidden"));
}

#[test]
fn gallery_off_publishes_nothing_and_missing_templates_fail_clearly() {
    let b = Blog::new("");
    write_image(&b.dir.join("gallery/a.jpg"), 50, 50);
    b.build().unwrap();
    assert!(!build::public_dir(&b.dir).join("gallery").exists());
    let site = b.site();
    assert!(!site.globals["gallery"].get_attr("enabled").unwrap().is_true());

    let b = Blog::new("[gallery]\nenabled = true\nurl = \"photos\"");
    write_image(&b.dir.join("gallery/a.jpg"), 50, 50);
    let err = format!("{:#}", b.build().unwrap_err());
    assert!(err.contains("theme/_gallery/index.html") && err.contains("folderblog gallery on"), "{err}");
    for t in crate::gallery::TEMPLATES {
        b.write(&format!("theme/{t}"), "{{ item.url }}");
    }
    b.build().unwrap();
    assert_eq!(b.read_out("photos/a/index.html"), "/photos/a/");
}

#[test]
fn gallery_cache_is_reused_and_pruned() {
    let b = gallery_blog();
    write_image(&b.dir.join("gallery/a.jpg"), 400, 300);
    write_image(&b.dir.join("gallery/b.jpg"), 400, 300);
    b.build().unwrap();
    let cache = b.dir.join(".blog/cache/gallery");
    let files = |d: &Path| {
        let mut v: Vec<_> = fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        v.sort();
        v
    };
    let first = files(&cache);
    assert_eq!(first.len(), 6, "{first:?}");
    let mtime = |n: &str| fs::metadata(cache.join(n)).unwrap().modified().unwrap();
    let before = mtime(&first[0]);
    b.build().unwrap();
    assert_eq!(mtime(&first[0]), before, "unchanged photos are not reprocessed");
    fs::remove_file(b.dir.join("gallery/b.jpg")).unwrap();
    b.build().unwrap();
    assert_eq!(files(&cache).len(), 3);
    assert!(!build::public_dir(&b.dir).join("gallery/b").exists());
}

#[test]
fn gallery_slugs_never_collide() {
    let b = gallery_blog();
    write_image(&b.dir.join("gallery/tags.jpg"), 40, 40);
    write_image(&b.dir.join("gallery/trip.jpg"), 40, 40);
    write_image(&b.dir.join("gallery/trip/x.jpg"), 40, 40);
    write_image(&b.dir.join("gallery/trip/x.png"), 40, 40);
    b.build().unwrap();
    let out = build::public_dir(&b.dir);
    for p in ["gallery/tags-2/index.html", "gallery/trip/index.html", "gallery/trip-2/index.html", "gallery/trip/x/index.html", "gallery/trip/x-2/index.html"] {
        assert!(out.join(p).exists(), "{p}");
    }
}

// ---------- search and link previews ----------

#[test]
fn share_images_from_front_matter_and_the_site_default() {
    let b = Blog::new("");
    b.write("theme/_layouts/post.html", "{{ post.image }}|{{ post.image_width }}x{{ post.image_height }}");
    b.write("theme/index.html", "{{ site.image }}|{{ site.image_width }}x{{ site.image_height }}");
    write_image(&b.dir.join("posts/trip/hero.jpg"), 40, 30);
    b.write("posts/trip/index.md", "---\ndate: 2024-02-03\nimage: hero.jpg\n---\nx");
    b.write("posts/2024-01-01-ext.md", "---\nimage: https://cdn.x.org/a.png\n---\nx");
    b.write("posts/2024-01-02-none.md", "x");
    b.write("posts/2024-01-03-missing.md", "---\nimage: nope.png\n---\nx");
    // No og-default.png and no blog.toml `image`: no site image.
    b.build().unwrap();
    assert_eq!(b.read_out("index.html"), "|x");
    write_image(&b.dir.join("static/og-default.png"), 12, 6);
    b.build().unwrap();
    assert_eq!(b.read_out("2024/02/trip/index.html"), "https://example.com/2024/02/trip/hero.jpg|40x30");
    assert_eq!(b.read_out("2024/01/ext/index.html"), "https://cdn.x.org/a.png|x");
    assert_eq!(b.read_out("2024/01/none/index.html"), "|x");
    assert_eq!(b.read_out("index.html"), "https://example.com/og-default.png|12x6");
    assert!(b.site().warnings.iter().any(|w| w.contains("missing.md: image \"nope.png\"")), "{:?}", b.site().warnings);

    // blog.toml `image` wins, resolved under the base path.
    let b = Blog::new("image = \"/img/share.png\"");
    b.write("blog.toml", "title = \"T\"\nbase_url = \"https://x.org/blog\"\nimage = \"/img/share.png\"\n");
    b.write("theme/index.html", "{{ site.image }}|{{ site.image_width }}");
    write_image(&b.dir.join("static/img/share.png"), 9, 9);
    b.build().unwrap();
    assert_eq!(b.read_out("index.html"), "https://x.org/blog/img/share.png|9");
}

#[test]
fn robots_txt_by_default_unless_the_blog_has_one() {
    let b = Blog::new("");
    b.build().unwrap();
    assert_eq!(b.read_out("robots.txt"), "User-agent: *\nAllow: /\n\nSitemap: https://example.com/sitemap.xml\n");
    assert!(!b.read_out("sitemap.xml").contains("robots"));
    b.write("static/robots.txt", "User-agent: *\nDisallow: /secret/\n");
    b.build().unwrap();
    assert_eq!(b.read_out("robots.txt"), "User-agent: *\nDisallow: /secret/\n");
    fs::remove_file(b.dir.join("static/robots.txt")).unwrap();
    b.write("theme/robots.txt.jinja", "Sitemap: {{ site.base_url }}/sitemap.xml\n");
    b.build().unwrap();
    assert_eq!(b.read_out("robots.txt"), "Sitemap: https://example.com/sitemap.xml\n");
}

#[test]
fn default_theme_has_search_and_preview_tags() {
    let (_t, dir) = new_default_blog();
    let cfg = fs::read_to_string(dir.join("blog.toml")).unwrap();
    let cfg = cfg.replace("author = \"\"", "author = \"Ann <A>\"").replace("description = \"\"", "description = \"About & more\"");
    fs::write(dir.join("blog.toml"), &cfg).unwrap();
    fs::create_dir_all(dir.join("posts/trip")).unwrap();
    write_image(&dir.join("posts/trip/hero.jpg"), 40, 30);
    fs::write(dir.join("posts/trip/index.md"), "---\ntitle: Tom & \"Jerry\" </script>\ndate: 2024-05-01\nupdated: 2024-06-01\nimage: hero.jpg\n---\nWent & came back.\n").unwrap();
    fs::write(dir.join("posts/2024-04-01-plain.md"), "Just text.\n").unwrap();
    let rep = crate::check::check(&dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:#?}", rep.errors);
    let out = build::build(&dir, None, LoadOptions { persist_state: true, ..Default::default() }, true).unwrap().out_dir;
    let read = |p: &str| fs::read_to_string(out.join(p)).unwrap();
    let base = cfg.lines().find_map(|l| l.strip_prefix("base_url = \"")).unwrap().trim_end_matches('"').to_string();

    let post = read("2024/05/trip/index.html");
    let permalink = format!("{base}/2024/05/trip/");
    for tag in [
        "<meta name=\"description\" content=\"Went &amp; came back.\">".to_string(),
        format!("<link rel=\"canonical\" href=\"{permalink}\">"),
        "<meta property=\"og:type\" content=\"article\">".into(),
        "<meta property=\"og:title\" content=\"Tom &amp; &quot;Jerry&quot; &lt;/script&gt;\">".into(),
        format!("<meta property=\"og:image\" content=\"{permalink}hero.jpg\">"),
        "<meta property=\"og:image:width\" content=\"40\">".into(),
        format!("<meta name=\"twitter:image\" content=\"{permalink}hero.jpg\">"),
        "<meta name=\"twitter:card\" content=\"summary_large_image\">".into(),
    ] {
        assert!(post.contains(&tag), "post is missing {tag}\n{post}");
    }
    let ld = post.split("<script type=\"application/ld+json\">").nth(1).unwrap().split("</script>").next().unwrap();
    assert!(!ld.contains('<'), "{ld}");
    let ld: serde_json::Value = serde_json::from_str(ld).unwrap_or_else(|e| panic!("{e}: {ld}"));
    assert_eq!(ld["@type"], "BlogPosting");
    assert_eq!(ld["headline"], "Tom & \"Jerry\" </script>");
    assert_eq!(ld["mainEntityOfPage"]["@id"], permalink.as_str());
    assert_eq!(ld["image"], format!("{permalink}hero.jpg"));
    assert_eq!(ld["author"]["name"], "Ann <A>");
    assert!(ld["datePublished"].as_str().unwrap().starts_with("2024-05-01"));
    assert!(ld["dateModified"].as_str().unwrap().starts_with("2024-06-01"));

    // No image anywhere: no og:image, a small card, and no image in JSON-LD.
    let plain = read("2024/04/plain/index.html");
    assert!(!plain.contains("og:image") && plain.contains("content=\"summary\""), "{plain}");
    assert!(!plain.contains("\"image\""), "{plain}");

    // Every page: description, canonical, OpenGraph basics and a Twitter card.
    for p in ["index.html", "archive/index.html", "tags/index.html", "2024/04/plain/index.html"] {
        let html = read(p);
        for needle in ["<meta name=\"description\" content=\"", "<link rel=\"canonical\" href=\"https://", "og:title", "og:description", "og:type", "og:url", "og:site_name", "twitter:card"] {
            assert!(html.contains(needle), "{p} is missing {needle}");
        }
    }
    let home = read("index.html");
    assert_eq!(home.matches("<h1").count(), 1, "{home}");
    assert!(home.contains(&format!("<link rel=\"canonical\" href=\"{base}/\">")));
}

// ---------- analytics ----------

const GC: &str = "https://goatcounter-production-af4b.up.railway.app";

fn analytics_blog(toml_extra: &str) -> Blog {
    let b = Blog::new(&format!("[gallery]\nenabled = true\n{toml_extra}"));
    let head = |body: &str| format!("<html><head><title>t</title></head><body>{body}</body></html>");
    b.write("theme/_layouts/post.html", &head("{{ post.title }}"));
    b.write("theme/_layouts/page.html", &head("{{ page.title }}"));
    b.write("theme/index.html", &head("home"));
    b.write("theme/archive.html", &head("archive"));
    b.write("theme/404.html", &head("missing"));
    b.write("theme/tag.html", &format!("{{#---\neach = \"tags\"\nas = \"tag\"\nurl = \"/tags/{{{{ tag.slug }}}}/\"\n---#}}{}", head("{{ tag.name }}")));
    b.write("theme/style.css.jinja", "body{}</head>");
    b.write("theme/_gallery/index.html", &head("gallery"));
    for t in crate::gallery::TEMPLATES.iter().filter(|t| !t.ends_with("/index.html")) {
        b.write(&format!("theme/{t}"), &head("g"));
    }
    write_image(&b.dir.join("gallery/a.jpg"), 40, 40);
    b.write("posts/2024-01-02-hello.md", "---\ntags: [Rust]\n---\n# Hello\n\nx");
    b.write("pages/about.md", "# About\n\nx");
    b.write("static/plain.html", "<html><head></head><body>static</body></html>");
    b
}

const PAGE_TYPES: &[&str] = &[
    "index.html",
    "2024/01/hello/index.html",
    "about/index.html",
    "archive.html",
    "tags/rust/index.html",
    "404.html",
    "gallery/index.html",
    "gallery/a/index.html",
];

#[test]
fn analytics_config_absent_empty_valid_invalid() {
    let base = "title='t'\nbase_url='https://x.com'\n";
    let parse = |extra: &str| crate::config::Config::parse(&format!("{base}{extra}"));
    let c = parse("").unwrap();
    assert!(c.analytics.goatcounter.is_empty() && c.analytics.host_prefix);
    assert!(parse("[analytics]\ngoatcounter = ''").unwrap().analytics.goatcounter.is_empty());
    let c = parse(&format!("[analytics]\ngoatcounter = '{GC}/'\nhost_prefix = false")).unwrap();
    assert_eq!(c.analytics.goatcounter, GC);
    assert!(!c.analytics.host_prefix);
    for bad in ["http://stats.example.com", "stats.example.com", "https://", "https:///x", "https://a.com/\"><script>", "https://a b.com", "https://a.com/?x=1"] {
        let err = format!("{:#}", parse(&format!("[analytics]\ngoatcounter = '{bad}'")).unwrap_err());
        assert!(err.contains("analytics.goatcounter"), "{bad}: {err}");
    }
    assert!(parse("[analytics]\ngoatcounter = 'https://a.com'\ncookies = true").is_err());
    // `folderblog check` flags a bad value and passes without one.
    let b = analytics_blog("[analytics]\ngoatcounter = 'http://insecure.example.com'");
    let rep = crate::check::check(&b.dir, None).unwrap();
    assert!(rep.errors.iter().any(|e| e.contains("analytics.goatcounter")), "{:?}", rep.errors);
    let b = analytics_blog("");
    let rep = crate::check::check(&b.dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
}

#[test]
fn analytics_is_injected_into_every_rendered_page() {
    let b = analytics_blog(&format!("[analytics]\ngoatcounter = '{GC}'"));
    b.build().unwrap();
    let want = format!(
        "<script>window.goatcounter={{path:function(p){{return location.host+p}}}}</script>\n<script data-goatcounter=\"{GC}/count\" async src=\"{GC}/count.js\"></script>\n</head>"
    );
    for p in PAGE_TYPES.iter().filter(|p| **p != "404.html") {
        let html = b.read_out(p);
        assert!(html.contains(&want), "{p}: {html}");
        assert_eq!(html.matches("data-goatcounter").count(), 1, "{p}");
    }
    // The 404 page counts an event named "404 host/path" instead of a page view.
    let html = b.read_out("404.html");
    let want_404 = format!(
        "<script>window.goatcounter={{path:function(p){{return '404 '+location.host+p}},title:'404 Not Found',event:true}}</script>\n<script data-goatcounter=\"{GC}/count\" async src=\"{GC}/count.js\"></script>\n</head>"
    );
    assert!(html.contains(&want_404), "{html}");
    assert_eq!(html.matches("data-goatcounter").count(), 1);
    assert_eq!(html.matches("window.goatcounter").count(), 1);
    // Copied files and non-HTML templates are untouched.
    assert_eq!(b.read_out("plain.html"), "<html><head></head><body>static</body></html>");
    assert_eq!(b.read_out("style.css"), "body{}</head>");
    assert!(!b.read_out("feed.xml").contains("goatcounter"));

    let b = analytics_blog(&format!("[analytics]\ngoatcounter = '{GC}'\nhost_prefix = false"));
    b.build().unwrap();
    let html = b.read_out("index.html");
    assert!(!html.contains("window.goatcounter") && html.contains(&format!("<script data-goatcounter=\"{GC}/count\"")), "{html}");
    let html = b.read_out("404.html");
    assert!(
        html.contains("<script>window.goatcounter={path:function(p){return '404 '+p},title:'404 Not Found',event:true}</script>\n")
            && !html.contains("location.host"),
        "{html}"
    );
}

#[test]
fn analytics_off_by_default_changes_nothing() {
    let off = analytics_blog("");
    off.build().unwrap();
    let empty = analytics_blog("[analytics]\ngoatcounter = ''");
    empty.build().unwrap();
    for p in PAGE_TYPES {
        let html = off.read_out(p);
        assert!(!html.contains("goatcounter"), "{p}");
        assert_eq!(html, empty.read_out(p), "{p}");
    }
}

#[test]
fn analytics_not_doubled_and_escaped() {
    let b = analytics_blog(&format!("[analytics]\ngoatcounter = '{GC}'"));
    b.write("theme/index.html", "<html><head><script data-goatcounter=\"x\" async src=\"x.js\"></script></head></html>");
    b.write("theme/archive.html", "<HTML><HEAD></HEAD></HTML><head></head>");
    b.write("theme/404.html", "<p>no head here</p>");
    b.build().unwrap();
    assert_eq!(b.read_out("index.html").matches("data-goatcounter").count(), 1);
    let a = b.read_out("archive.html");
    assert!(a.find("data-goatcounter").unwrap() < a.find("</HEAD>").unwrap(), "{a}");
    assert_eq!(a.matches("data-goatcounter").count(), 1);
    assert_eq!(b.read_out("404.html"), "<p>no head here</p>");
    // A theme 404 page that loads GoatCounter itself is respected as-is.
    let own = "<html><head><script data-goatcounter=\"x\" async src=\"x.js\"></script></head></html>";
    b.write("theme/404.html", own);
    b.build().unwrap();
    assert_eq!(b.read_out("404.html"), own);
    for not_found in [false, true] {
        let s = crate::render::goatcounter_snippet("https://a.com/x&y", true, not_found);
        assert!(s.contains("\"https://a.com/x&amp;y/count\""), "{s}");
    }
}

#[test]
fn analytics_never_in_preview_drafts_or_fixtures() {
    let b = analytics_blog(&format!("[analytics]\ngoatcounter = '{GC}'"));
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("preview");
    // The options `folderblog preview` builds with.
    build::build(&b.dir, Some(&out), LoadOptions { drafts: true, ..Default::default() }, false).unwrap();
    for p in PAGE_TYPES {
        let html = fs::read_to_string(out.join(p)).unwrap();
        assert!(!html.contains("goatcounter"), "{p}");
    }
    let site = Site::load(&b.dir, LoadOptions { fixtures: Some("stress".into()), ..Default::default() }).unwrap();
    assert!(site.analytics_snippet().is_none() && site.analytics_snippet_for(true).is_none());
    assert!(b.site().analytics_snippet().is_some() && b.site().analytics_snippet_for(true).is_some());
}

// ---------- podcast ----------

const MP3: &[u8] = include_bytes!("../fixtures/stress/podcast/2024-05-01-pilot/episode.mp3");
const MP3_NO_XING: &[u8] = include_bytes!("../tests/media/cbr-no-xing.mp3");
const MP4: &[u8] = include_bytes!("../fixtures/stress/podcast/2024-06-01-on-camera/episode.mp4");
const COVER: &[u8] = include_bytes!("../fixtures/stress/podcast/cover.jpg");
const GUID1: &str = "11111111-2222-4333-8444-555555555555";

fn podcast_blog(extra: &str) -> Blog {
    let b = Blog::new(&format!(
        "[podcast]\nenabled = true\nguid = \"{}\"\ncategory = \"Technology\"\nauthor = \"Ada\"\nowner_email = \"ada@example.com\"\n{extra}\n[podcast.storage]\npublic_url = \"https://media.example.com/\"\n",
        crate::podcast::feed_guid("https://example.com/podcast/feed.xml")
    ));
    b.write("theme/_podcast/index.html", "{{ podcast.title }}|{% for e in podcast.episodes %}{{ e.title }}={{ e.audio.url }};{% endfor %}");
    b.write(
        "theme/_podcast/episode.html",
        "{{ episode.title }}|{{ episode.audio.url }}|{{ episode.audio.duration }}|{% for c in episode.chapters %}{{ c.start_hms }} {{ c.title }};{% endfor %}|{{ episode.transcript.url }}",
    );
    b.write("podcast/index.md", "# Ada Talks\n\nA show about things.\n");
    write_bytes(&b.dir.join("podcast/cover.jpg"), COVER);
    b
}

fn write_bytes(p: &Path, bytes: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

fn add_episode(b: &Blog, folder: &str, front: &str, file: &str, bytes: &[u8]) {
    b.write(&format!("podcast/{folder}/index.md"), &format!("---\n{front}\n---\nShow notes with a [link](/about/).\n"));
    write_bytes(&b.dir.join(format!("podcast/{folder}/{file}")), bytes);
}

fn check_errors(b: &Blog) -> Vec<String> {
    crate::check::check(&b.dir, None).unwrap().errors
}

#[test]
fn podcast_feed_has_every_tag_and_validates() {
    let b = podcast_blog("lightning_address = \"ada@rizful.com\"\nfunding = [{ url = \"https://example.com/give\", text = \"Give\" }]\npersons = [{ name = \"Ada\", role = \"host\", href = \"https://example.com/ada\" }]\nlicense = \"cc-by-4.0\"\nsubcategory = \"\"\n");
    add_episode(
        &b,
        "2024-05-01-pilot",
        &format!("title: Pilot\nguid: \"{GUID1}\"\nseason: 1\nepisode: 1\nexplicit: true\nimage: art.jpg\npersons: [{{ name: Sam, role: guest }}]\nvalue: [{{ name: Sam, address: sam@getalby.com, split: 50 }}]"),
        "episode.mp3",
        MP3,
    );
    write_bytes(&b.dir.join("podcast/2024-05-01-pilot/art.jpg"), COVER);
    b.write("podcast/2024-05-01-pilot/transcript.vtt", "WEBVTT\n\n00:00.000 --> 00:01.000\nHi\n");
    b.write("podcast/2024-05-01-pilot/chapters.json", r#"{"version":"1.2.0","chapters":[{"startTime":0,"title":"Intro"},{"startTime":65,"title":"Talk"}]}"#);
    b.write("pages/about.md", "# About\n");
    b.build().unwrap();
    let xml = b.read_out("podcast/feed.xml");
    let sha8 = {
        use sha2::Digest;
        let h = sha2::Sha256::digest(MP3);
        h.iter().take(4).map(|x| format!("{x:02x}")).collect::<String>()
    };
    for want in [
        "xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\"",
        "xmlns:podcast=\"https://podcastindex.org/namespace/1.0\"",
        "<title>Ada Talks</title>",
        "<link>https://example.com/podcast/</link>",
        "<description>A show about things.</description>",
        "<language>en</language>",
        "<atom:link href=\"https://example.com/podcast/feed.xml\" rel=\"self\" type=\"application/rss+xml\"/>",
        "<itunes:image href=\"https://example.com/podcast/cover.jpg\"/>",
        "<itunes:category text=\"Technology\"/>",
        "<itunes:explicit>false</itunes:explicit>",
        "<itunes:author>Ada</itunes:author>",
        "<itunes:owner><itunes:name>Ada</itunes:name><itunes:email>ada@example.com</itunes:email></itunes:owner>",
        "<itunes:type>episodic</itunes:type>",
        "<podcast:locked owner=\"ada@example.com\">yes</podcast:locked>",
        "<podcast:medium>podcast</podcast:medium>",
        "<podcast:funding url=\"https://example.com/give\">Give</podcast:funding>",
        "<podcast:person role=\"host\" href=\"https://example.com/ada\">Ada</podcast:person>",
        "<podcast:license>cc-by-4.0</podcast:license>",
        "<podcast:value type=\"lightning\" method=\"lnaddress\">",
        "<podcast:valueRecipient name=\"Ada\" type=\"lnaddress\" address=\"ada@rizful.com\" split=\"100\"/>",
        "<generator>folderblog",
        "<lastBuildDate>",
        "<guid isPermaLink=\"false\">11111111-2222-4333-8444-555555555555</guid>",
        "<link>https://example.com/podcast/pilot/</link>",
        "<pubDate>",
        "<content:encoded><![CDATA[<p>Show notes with a <a href=\"https://example.com/about/\">link</a>",
        &format!("<enclosure url=\"https://media.example.com/example.com/pilot-{sha8}.mp3\" length=\"{}\" type=\"audio/mpeg\"/>", MP3.len()),
        "<itunes:duration>3</itunes:duration>",
        "<itunes:episodeType>full</itunes:episodeType>",
        "<itunes:explicit>true</itunes:explicit>",
        "<itunes:season>1</itunes:season>",
        "<podcast:season>1</podcast:season>",
        "<itunes:episode>1</itunes:episode>",
        "<podcast:episode>1</podcast:episode>",
        "<itunes:image href=\"https://example.com/podcast/pilot/art.jpg\"/>",
        "<podcast:transcript url=\"https://example.com/podcast/pilot/transcript.vtt\" type=\"text/vtt\"/>",
        "<podcast:chapters url=\"https://example.com/podcast/pilot/chapters.json\" type=\"application/json+chapters\"/>",
        "<podcast:person role=\"guest\">Sam</podcast:person>",
        "<podcast:valueRecipient name=\"Sam\" type=\"lnaddress\" address=\"sam@getalby.com\" split=\"50\"/>",
    ] {
        assert!(xml.contains(want), "missing {want}\n{xml}");
    }
    assert!(xml.contains(&format!("<podcast:guid>{}</podcast:guid>", crate::podcast::feed_guid("https://example.com/podcast/feed.xml"))));
    assert_eq!(crate::podcast::validate_feed(&xml), Ok(1));
    // Episode page: hosted audio, chapters, transcript beside it.
    assert_eq!(
        b.read_out("podcast/pilot/index.html"),
        format!("Pilot|https://media.example.com/example.com/pilot-{sha8}.mp3|3|0:00 Intro;1:05 Talk;|/podcast/pilot/transcript.vtt")
    );
    assert!(b.read_out("podcast/pilot/chapters.json").contains("Intro"));
    let rep = crate::check::check(&b.dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:#?}", rep.errors);
    // Episodes stay out of the main feed and the post list by default.
    assert!(!b.read_out("feed.xml").contains("Pilot"));
    assert_eq!(b.read_out("index.html"), "");
}

#[test]
fn podcast_feed_validator_catches_problems() {
    let bad = r#"<?xml version="1.0"?><rss><channel><title>T</title><link>http://x.com/</link><description>d</description>
<itunes:explicit>yes</itunes:explicit><podcast:locked>maybe</podcast:locked>
<item><title>A</title><guid>g</guid><pubDate>x</pubDate><enclosure url="http://m/x.mp3" length="0" type="audio/ogg"/><itunes:duration>0</itunes:duration></item>
<item><title>B</title></item></channel></rss>"#;
    let errs = crate::podcast::validate_feed(bad).unwrap_err().join("\n");
    for want in [
        "channel: <link> must be an https:// URL",
        "<itunes:explicit> must be true or false",
        "<podcast:locked> must be yes or no",
        "episode 1: <enclosure url> must be an https:// URL",
        "episode 1: <enclosure> has no length",
        "type must be audio/mpeg or video/mp4",
        "episode 1: <itunes:duration> must be a number of seconds above 0",
        "episode 2: missing <enclosure>",
        "episode 2: missing <guid>",
        "channel: missing <language>",
        "channel: missing <itunes:image>",
        "channel: missing <podcast:guid>",
        "channel: missing <atom:link>",
    ] {
        assert!(errs.contains(want), "missing {want:?} in\n{errs}");
    }
    assert!(crate::podcast::validate_feed("<rss><channel>").is_err());
}

#[test]
fn media_duration_size_and_type_are_read_from_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = |n: &str, bytes: &[u8]| {
        let path = tmp.path().join(n);
        fs::write(&path, bytes).unwrap();
        path
    };
    let mp3 = crate::podcast::inspect(&p("a.mp3", MP3)).unwrap();
    assert_eq!((mp3.kind, mp3.bytes), (crate::podcast::MediaKind::Mp3, MP3.len() as u64));
    assert!((mp3.duration - 3.0).abs() < 0.1, "{}", mp3.duration);
    let cbr = crate::podcast::inspect(&p("b.mp3", MP3_NO_XING)).unwrap();
    assert!((cbr.duration - 2.0).abs() < 0.15, "frames counted without a Xing header: {}", cbr.duration);
    let mp4 = crate::podcast::inspect(&p("c.mp4", MP4)).unwrap();
    assert_eq!(mp4.kind.mime(), "video/mp4");
    assert!((mp4.duration - 2.0).abs() < 0.1, "{}", mp4.duration);
    assert_eq!(mp4.sha256.len(), 64);
    // The name must match what the file really is.
    let e = crate::podcast::inspect(&p("lies.mp4", MP3)).unwrap_err().to_string();
    assert!(e.contains("really an MP3"), "{e}");
    let e = crate::podcast::inspect(&p("noise.mp3", b"definitely not audio")).unwrap_err().to_string();
    assert!(e.contains("doesn't look like an MP3"), "{e}");
    let e = crate::podcast::inspect(&p("x.m4a", MP4)).unwrap_err().to_string();
    assert!(e.contains("export the episode as MP3"), "{e}");
    assert!(MP3.len() + MP3_NO_XING.len() + MP4.len() < 50_000, "fixture audio stays small");
}

#[test]
fn episode_guids_stay_stable_when_slug_date_or_base_url_change() {
    let b = podcast_blog("");
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    b.build().unwrap();
    assert!(b.read_out("podcast/feed.xml").contains(GUID1));
    // Rename the folder (new slug and date) and move the site.
    fs::rename(b.dir.join("podcast/2024-05-01-pilot"), b.dir.join("podcast/2024-06-02-renamed")).unwrap();
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap().replace("base_url = \"https://example.com\"", "base_url = \"https://new.example.net\"");
    fs::write(b.dir.join("blog.toml"), cfg).unwrap();
    b.build().unwrap();
    let xml = b.read_out("podcast/feed.xml");
    assert!(xml.contains(&format!("<guid isPermaLink=\"false\">{GUID1}</guid>")), "{xml}");
    assert!(xml.contains("https://new.example.net/podcast/renamed/"));
    // The show's guid was pinned in blog.toml, so moving the site keeps it too.
    assert!(xml.contains(&crate::podcast::feed_guid("https://example.com/podcast/feed.xml")));
}

#[test]
fn episode_without_guid_gets_one_from_state_and_check_asks_for_it() {
    let b = podcast_blog("");
    add_episode(&b, "2024-05-01-pilot", "title: Pilot", "episode.mp3", MP3);
    b.build().unwrap();
    let first = b.read_out("podcast/feed.xml");
    b.build().unwrap();
    assert_eq!(first, b.read_out("podcast/feed.xml"), "the handed-out guid is kept");
    let errs = check_errors(&b).join("\n");
    assert!(errs.contains("no guid; add `guid: \""), "{errs}");
}

#[test]
fn guid_changes_after_deploy_are_check_errors() {
    let b = podcast_blog("");
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    b.build().unwrap();
    crate::podcast::mark_deployed(&b.dir);
    assert!(check_errors(&b).is_empty());
    b.write("podcast/2024-05-01-pilot/index.md", "---\nguid: \"other\"\n---\nx");
    let errs = check_errors(&b).join("\n");
    assert!(errs.contains("guid changed from 11111111"), "{errs}");
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap();
    let old = crate::podcast::feed_guid("https://example.com/podcast/feed.xml");
    fs::write(b.dir.join("blog.toml"), cfg.replace(&old, "new-show-guid")).unwrap();
    let errs = check_errors(&b).join("\n");
    assert!(errs.contains("podcast.guid changed"), "{errs}");
}

#[test]
fn strict_podcast_rules_fail_with_clear_messages() {
    let b = podcast_blog("category = \"Gadgets\"\nfunding = [{ url = \"http://example.com/give\" }]\nlightning_address = \"not-an-address\"");
    // `category` appears twice in [podcast] now; drop the valid one so only the bad one is left.
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap().replacen("category = \"Technology\"\n", "", 1);
    fs::write(b.dir.join("blog.toml"), cfg).unwrap();
    add_episode(&b, "2024-05-01-a", &format!("guid: \"{GUID1}\"\nexplicit: \"yes\"\nseason: -1\nepisode_type: special"), "episode.mp3", MP3);
    b.write("podcast/2024-05-01-a/chapters.json", r#"{"chapters":[]}"#);
    b.write("podcast/2024-05-01-a/transcript.srt", "not a transcript");
    add_episode(&b, "2024-05-02-b", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    add_episode(&b, "2024-05-03-c", "guid: c", "episode.m4a", MP4);
    add_episode(&b, "2024-05-04-d", "guid: d\nimage: small.png", "episode.mp3", MP3);
    write_image(&b.dir.join("podcast/2024-05-04-d/small.png"), 300, 200);
    write_image(&b.dir.join("podcast/cover.jpg"), 1400, 1000);
    let errs = check_errors(&b).join("\n");
    for want in [
        "podcast.category \"Gadgets\" is not an Apple Podcasts category",
        "podcast.funding url must be https://",
        "podcast.lightning_address \"not-an-address\" is not a Lightning address",
        "explicit must be true or false",
        "season must be a whole number",
        "episode_type must be full, trailer or bonus",
        "chapters.json: \"version\" is required",
        "chapters.json: \"chapters\" is empty",
        "transcript.srt: cue 1: expected a number",
        "is already used by podcast/2024-05-01-a",
        "M4A/AAC audio isn't supported yet; export the episode as MP3",
        "show art podcast/cover.jpg must be square (it is 1400×1000 px)",
    ] {
        assert!(errs.contains(want), "missing {want:?} in\n{errs}");
    }
    let errs_d = errs.lines().filter(|l| l.contains("2024-05-04-d")).collect::<Vec<_>>().join("\n");
    assert!(errs_d.contains("must be square (it is 300×200 px)"), "{errs_d}");
}

#[test]
fn https_is_enforced_for_podcasts() {
    let b = podcast_blog("persons = [{ name = \"Ada\", href = \"http://example.com\" }]");
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap().replace("https://example.com\"\n[podcast]", "http://example.com\"\n[podcast]");
    fs::write(b.dir.join("blog.toml"), cfg.replacen("base_url = \"https://example.com\"", "base_url = \"http://example.com\"", 1)).unwrap();
    add_episode(&b, "2024-05-01-a", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    let errs = check_errors(&b).join("\n");
    assert!(errs.contains("base_url must be https:// for a podcast"), "{errs}");
    assert!(errs.contains("links must be https://"), "{errs}");
    assert!(errs.contains("<link> must be an https:// URL"), "{errs}");
    let toml = "title='t'\nbase_url='https://x.com'\n[podcast.storage]\npublic_url='http://media.x.com'\n";
    assert!(format!("{:#}", crate::config::Config::parse(toml).unwrap_err()).contains("must be an https:// URL"));
}

#[test]
fn audio_is_never_published_and_podcast_on_ignores_it_in_git() {
    let (_t, dir) = new_default_blog();
    enable_podcast(&dir);
    let gi = fs::read_to_string(dir.join(".gitignore")).unwrap();
    for pat in ["podcast/**/*.mp3", "podcast/**/*.mp4", "podcast/**/*.wav"] {
        assert!(gi.contains(pat), "{gi}");
    }
    assert!(gi.starts_with(".blog/\n"), "existing lines kept: {gi}");
    crate::podcast_cmd(&dir, Some("on")).unwrap();
    assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), gi, "running it again adds nothing");
    for t in crate::podcast::TEMPLATES {
        assert!(dir.join("theme").join(t).is_file(), "{t}");
    }
    // A real build of the stress fixture's episodes (copied into the blog's own podcast/).
    crate::util::copy_dir(&crate::site::fixture_root(&dir, "stress").unwrap().join("podcast"), &dir.join("podcast")).unwrap();
    let mut doc: toml_edit::DocumentMut = fs::read_to_string(dir.join("blog.toml")).unwrap().parse().unwrap();
    doc["podcast"]["storage"] = toml_edit::table();
    doc["podcast"]["storage"]["public_url"] = toml_edit::value("https://media.example.org");
    fs::write(dir.join("blog.toml"), doc.to_string()).unwrap();
    for opts in [LoadOptions { persist_state: true, ..Default::default() }, LoadOptions { drafts: true, local_media: true, ..Default::default() }] {
        let r = build::build(&dir, None, opts, false).unwrap();
        let media: Vec<_> = walkdir::WalkDir::new(&r.out_dir).into_iter().filter_map(|e| e.ok()).filter(|e| crate::podcast::is_media(e.path())).map(|e| e.path().display().to_string()).collect();
        assert!(media.is_empty(), "{media:?}");
        assert!(r.out_dir.join("podcast/pilot/index.html").is_file());
    }
    let rep = crate::check::check(&dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:#?}", rep.errors);
}

#[test]
fn podcast_off_changes_no_output() {
    let b = Blog::new("");
    b.write("posts/2024-01-01-hello.md", "# Hello\n\nBody.\n");
    b.write("theme/_layouts/base.html", "{% if podcast.enabled and podcast.count %}<a>{{ podcast.title }}</a>{% endif %}{{ posts | length }}");
    b.build().unwrap();
    let snapshot = |b: &Blog| -> Vec<(String, Vec<u8>)> {
        let out = build::public_dir(&b.dir);
        let mut v: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(&out)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| (e.path().strip_prefix(&out).unwrap().display().to_string(), fs::read(e.path()).unwrap()))
            .collect();
        v.sort();
        v
    };
    let before = snapshot(&b);
    // A podcast/ folder with episodes changes nothing while the podcast is off.
    add_episode(&b, "2024-05-01-a", &format!("guid: \"{GUID1}\"\nin_main_feed: true"), "episode.mp3", MP3);
    b.build().unwrap();
    assert_eq!(snapshot(&b), before);
}

#[test]
fn episodes_need_a_media_host_except_in_preview() {
    let b = podcast_blog("");
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap();
    let cfg = cfg.split("[podcast.storage]").next().unwrap().to_string();
    fs::write(b.dir.join("blog.toml"), cfg).unwrap();
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    let r = build::build(&b.dir, None, LoadOptions { persist_state: true, ..Default::default() }, false).unwrap();
    assert!(r.warnings.iter().any(|w| w.contains("nowhere to host the audio")), "{:?}", r.warnings);
    assert!(!b.read_out("podcast/feed.xml").contains("<item>"));
    assert!(!build::public_dir(&b.dir).join("podcast/pilot/index.html").exists());
    assert!(check_errors(&b).iter().any(|e| e.contains("nowhere to host the audio")));
    // Preview plays the local file.
    let site = Site::load(&b.dir, LoadOptions { drafts: true, local_media: true, ..Default::default() }).unwrap();
    let ep = &site.podcast.as_ref().unwrap().episodes[0];
    assert_eq!(ep.media.as_ref().unwrap().url.as_deref(), Some("/__folderblog/media/podcast/2024-05-01-pilot/episode.mp3"));
}

#[test]
fn draft_episodes_only_appear_in_preview() {
    let b = podcast_blog("");
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\"\ndraft: true"), "episode.mp3", MP3);
    b.write("podcast/2024-05-01-pilot/chapters.json", r#"{"version":"1.2.0","chapters":[{"startTime":0}]}"#);
    b.build().unwrap();
    let out = build::public_dir(&b.dir);
    assert!(!out.join("podcast/pilot").exists(), "no page and no attached files");
    assert!(!b.read_out("podcast/feed.xml").contains("<item>"));
    let site = Site::load(&b.dir, LoadOptions { drafts: true, local_media: true, ..Default::default() }).unwrap();
    assert_eq!(site.podcast.as_ref().unwrap().episodes.len(), 1);
}

#[test]
fn episodes_can_also_be_listed_with_posts() {
    let b = podcast_blog("");
    b.write("posts/2024-05-02-post.md", "# A post\n");
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\"\ntitle: Pilot"), "episode.mp3", MP3);
    add_episode(&b, "2024-05-03-two", "guid: two\ntitle: Two\nin_main_feed: true", "episode.mp4", MP4);
    b.build().unwrap();
    assert_eq!(b.read_out("index.html"), "Two;A post;");
    let feed = b.read_out("feed.xml");
    assert!(feed.contains("<title>Two</title>") && !feed.contains("<title>Pilot</title>"), "{feed}");
    assert!(feed.contains("type=\"video/mp4\""), "{feed}");
    crate::feed::validate_rss(&feed).unwrap();
    // The show-wide switch lists them all.
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap().replace("[podcast]\n", "[podcast]\nin_main_feed = true\n");
    fs::write(b.dir.join("blog.toml"), cfg).unwrap();
    b.build().unwrap();
    assert_eq!(b.read_out("index.html"), "Two;A post;Pilot;");
    // Mixed MP3 and MP4: an audio show. Only MP4 episodes: a video show.
    assert!(b.read_out("podcast/feed.xml").contains("<podcast:medium>podcast</podcast:medium>"));
    fs::remove_dir_all(b.dir.join("podcast/2024-05-01-pilot")).unwrap();
    b.build().unwrap();
    assert!(b.read_out("podcast/feed.xml").contains("<podcast:medium>video</podcast:medium>"));
}

#[test]
fn several_media_files_need_audio_in_front_matter() {
    let b = podcast_blog("");
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    write_bytes(&b.dir.join("podcast/2024-05-01-pilot/raw.mp3"), MP3_NO_XING);
    assert!(check_errors(&b).join("\n").contains("2 media files in podcast/2024-05-01-pilot/; set `audio: <file>`"));
    b.write("podcast/2024-05-01-pilot/index.md", &format!("---\nguid: \"{GUID1}\"\naudio: raw.mp3\n---\nx"));
    assert!(check_errors(&b).is_empty(), "{:?}", check_errors(&b));
}

// ---------- podcast storage ----------

fn mock_creds() -> crate::storage::Credentials {
    crate::storage::Credentials { access_key_id: "AKIDMOCK1234".into(), secret_access_key: "canary-secret-value".into() }
}

/// A podcast blog whose deploys upload to the local mock and publish with a command.
fn storage_blog(mock: &crate::mock_s3::Mock) -> (Blog, tempfile::TempDir) {
    let b = podcast_blog("");
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap();
    let cfg = cfg.replace(
        "[podcast.storage]\npublic_url = \"https://media.example.com/\"",
        &format!("[podcast.storage]\ntype = \"s3\"\nendpoint = \"{}\"\nbucket = \"show\"\npublic_url = \"https://media.example.com\"", mock.url),
    );
    fs::write(b.dir.join("blog.toml"), format!("{cfg}\n[deploy]\ntype = \"command\"\ncommand = \"touch deployed-marker\"\n")).unwrap();
    let conf = tempfile::tempdir().unwrap();
    crate::storage::save_credentials(conf.path(), "show", &mock_creds()).unwrap();
    (b, conf)
}

fn deploy(b: &Blog, conf: &Path) -> anyhow::Result<crate::deploy::Outcome> {
    b.build()?;
    crate::deploy::deploy_with(&b.dir, false, conf)
}

#[test]
fn deploy_uploads_episodes_signed_once_then_skips_them() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, conf) = storage_blog(&mock);
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    add_episode(&b, "2024-05-02-draft", "guid: d\ndraft: true", "episode.mp3", MP3_NO_XING);
    match deploy(&b, conf.path()).unwrap() {
        crate::deploy::Outcome::Deployed(d) => assert!(d.contains("uploaded 1 episode file"), "{d}"),
        _ => panic!("not deployed"),
    }
    assert!(b.dir.join("deployed-marker").exists());
    let reqs = mock.requests();
    let put: Vec<_> = reqs.iter().filter(|r| r.method == "PUT").collect();
    assert_eq!(put.len(), 1, "drafts are never uploaded: {reqs:#?}");
    let r = put[0];
    assert!(r.path.starts_with("/show/example.com/pilot-") && r.path.ends_with(".mp3"), "{}", r.path);
    assert!(r.headers["authorization"].starts_with("AWS4-HMAC-SHA256 Credential=AKIDMOCK1234/"), "{:?}", r.headers);
    assert!(r.headers.contains_key("x-amz-date"));
    assert_eq!(r.headers["x-amz-content-sha256"], crate::storage::sha256_hex(MP3));
    assert_eq!(r.headers["content-type"], "audio/mpeg");
    assert_eq!(r.body_len, MP3.len());
    assert!(mock.state.lock().unwrap().bad_signatures.is_empty());
    assert!(reqs.iter().any(|r| r.method == "HEAD"), "the enclosure is checked after upload");
    // An unchanged build is not redeployed; a changed one re-checks but doesn't re-upload.
    b.write("posts/new.md", "# New\n");
    deploy(&b, conf.path()).unwrap();
    assert_eq!(mock.requests().iter().filter(|r| r.method == "PUT").count(), 1, "the manifest skips re-uploads");
    // The secret never lands in the blog folder or its state.
    for e in walkdir::WalkDir::new(&b.dir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
        let text = fs::read(e.path()).unwrap();
        assert!(!String::from_utf8_lossy(&text).contains("canary-secret-value"), "{}", e.path().display());
    }
}

#[test]
fn a_failed_upload_holds_the_deploy() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, conf) = storage_blog(&mock);
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    mock.state.lock().unwrap().fail_puts = Some(500);
    let e = format!("{:#}", deploy(&b, conf.path()).err().expect("held"));
    assert!(e.starts_with("held:") && e.contains("feed never points at missing audio"), "{e}");
    assert!(!e.contains("canary-secret-value"));
    assert!(!b.dir.join("deployed-marker").exists(), "the site was not published");
    let st = crate::status::load(&b.dir);
    assert!(st.deploy.as_ref().is_some_and(|d| !d.ok), "the hold is recorded so the watcher retries");
    // Fixed: the next deploy goes out.
    mock.state.lock().unwrap().fail_puts = None;
    crate::deploy::deploy_with(&b.dir, false, conf.path()).unwrap();
    assert!(b.dir.join("deployed-marker").exists());
}

#[test]
fn a_wrong_size_on_the_host_holds_the_deploy_and_is_uploaded_again() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, conf) = storage_blog(&mock);
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    mock.state.lock().unwrap().truncate = true;
    let e = format!("{:#}", deploy(&b, conf.path()).err().expect("held"));
    assert!(e.contains("on the media host is") && e.contains("uploaded again"), "{e}");
    mock.state.lock().unwrap().truncate = false;
    crate::deploy::deploy_with(&b.dir, false, conf.path()).unwrap();
    assert_eq!(mock.requests().iter().filter(|r| r.method == "PUT").count(), 2);
}

#[test]
fn missing_keys_hold_the_deploy_with_a_clear_message() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, _conf) = storage_blog(&mock);
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    let empty = tempfile::tempdir().unwrap();
    let e = format!("{:#}", deploy(&b, empty.path()).err().expect("held"));
    assert!(e.contains("held: no saved key for the bucket \"show\"; run `folderblog podcast setup`"), "{e}");
}

#[test]
fn large_files_go_up_in_signed_parts() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("big.mp3");
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(&f, &data).unwrap();
    let mut store = crate::storage::S3Store::new(&mock.url, "show", "auto", mock_creds(), "https://media.example.com");
    store.multipart_threshold = 4_000;
    store.part_size = 3_000;
    use crate::storage::MediaStore;
    store.put("b/big.mp3", &f, "audio/mpeg").unwrap();
    let reqs = mock.requests();
    assert_eq!(reqs.iter().filter(|r| r.method == "PUT" && r.query.contains("partNumber")).count(), 4, "{reqs:#?}");
    assert!(reqs.iter().any(|r| r.method == "POST" && r.query == "uploads"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.query.contains("uploadId=upload-1")));
    assert_eq!(mock.state.lock().unwrap().objects["show/b/big.mp3"], data);
    assert_eq!(store.head("b/big.mp3").unwrap(), crate::storage::Stored::Size(10_000));
    assert_eq!(store.head("nope.mp3").unwrap(), crate::storage::Stored::Missing);
    store.delete("b/big.mp3").unwrap();
    assert_eq!(store.head("b/big.mp3").unwrap(), crate::storage::Stored::Missing);
    assert!(mock.state.lock().unwrap().bad_signatures.is_empty());
    // A failed part aborts the upload.
    mock.state.lock().unwrap().fail_puts = Some(503);
    let e = store.put("b/big.mp3", &f, "audio/mpeg").unwrap_err().to_string();
    assert!(e.contains("part 1 of 4") && e.contains("tried again"), "{e}");
    assert!(mock.requests().iter().any(|r| r.method == "DELETE" && r.query.contains("uploadId")));
    // Wrong keys are refused, and the error says what to do.
    let bad = crate::storage::S3Store::new(&mock.url, "show", "auto", crate::storage::Credentials { access_key_id: "AKIDMOCK1234".into(), secret_access_key: "wrong".into() }, "");
    mock.state.lock().unwrap().fail_puts = None;
    let e = bad.put("x.mp3", &f, "audio/mpeg").unwrap_err().to_string();
    assert!(e.contains("Access Key ID or Secret is wrong"), "{e}");
}

#[test]
fn heads_ask_for_an_uncompressed_reply_so_the_size_comes_back() {
    // The mock answers like R2: a client that accepts gzip gets no Content-Length.
    use crate::storage::{MediaStore, Stored};
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let store = crate::storage::S3Store::new(&mock.url, "show", "auto", mock_creds(), "https://media.example.com");
    store.put_bytes("check.txt", b"folderblog storage check 2026-10-23T12:00:00Z\n", "text/plain").unwrap();
    assert_eq!(store.head("check.txt").unwrap(), Stored::Size(46));
    let heads: Vec<_> = mock.requests().into_iter().filter(|r| r.method == "HEAD").collect();
    assert!(heads.iter().all(|r| r.headers.get("accept-encoding").map(String::as_str) == Some("identity")), "{heads:#?}");
    let public = crate::storage::public_head(&format!("{}/pub/check.txt", mock.url)).unwrap();
    assert_eq!(public, Stored::Size(46));
}

#[test]
fn a_head_reply_without_a_length_still_means_the_file_is_there() {
    use crate::storage::{MediaStore, Stored};
    let mock = crate::mock_s3::Mock::start(mock_creds());
    mock.state.lock().unwrap().head_without_length = true;
    let store = crate::storage::S3Store::new(&mock.url, "show", "auto", mock_creds(), &format!("https://{}", crate::mock_s3::R2_DEV));
    store.put_bytes("check.txt", b"hello\n", "text/plain").unwrap();
    assert_eq!(store.head("check.txt").unwrap(), Stored::UnknownSize);
    assert_eq!(store.head("nope.txt").unwrap(), Stored::Missing);
    assert_eq!(crate::storage::public_head(&format!("{}/pub/check.txt", mock.url)).unwrap(), Stored::UnknownSize);
    assert_eq!(crate::storage::public_head(&format!("{}/pub/nope.txt", mock.url)).unwrap(), Stored::Missing);
    store.delete("check.txt").unwrap();

    // Setup's check passes: "find it again" no longer reads a missing size as a missing file.
    let conf = tempfile::tempdir().unwrap();
    let url = mock.url.clone();
    let public_get = move |u: &str| -> anyhow::Result<(u16, String)> {
        let path = u.strip_prefix(&format!("https://{}/", crate::mock_s3::R2_DEV)).unwrap();
        let mut r = ureq::get(&format!("{url}/pub/{path}")).config().http_status_as_error(false).build().call()?;
        Ok((r.status().as_u16(), r.body_mut().read_to_string()?))
    };
    let mut input = std::io::Cursor::new(vec![]);
    let mut out = vec![];
    let mut hidden = |_: &str| -> anyhow::Result<String> { panic!("no prompts") };
    let mut opened = |u: &str| panic!("opened {u}");
    let mut io = setup_io(&mut input, &mut out, &mut hidden, &mut opened, &mock, conf.path(), &public_get);
    crate::setup::verify(&mut io, &store).unwrap_or_else(|e| panic!("{e:#}\n{}", String::from_utf8_lossy(&out)));
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(text.contains("ok    find it again") && !text.contains("FAIL"), "{text}");

    // And a deploy's skip-if-uploaded check doesn't hold the site or re-upload.
    let (b, conf) = storage_blog(&mock);
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\""), "episode.mp3", MP3);
    deploy(&b, conf.path()).unwrap();
    deploy(&b, conf.path()).unwrap();
    assert_eq!(mock.requests().iter().filter(|r| r.method == "PUT" && r.path.ends_with(".mp3")).count(), 1);
}

fn setup_io<'a>(
    input: &'a mut dyn std::io::BufRead,
    out: &'a mut Vec<u8>,
    hidden: &'a mut dyn FnMut(&str) -> anyhow::Result<String>,
    opened: &'a mut dyn FnMut(&str),
    mock: &crate::mock_s3::Mock,
    conf: &Path,
    public_get: &'a dyn Fn(&str) -> anyhow::Result<(u16, String)>,
) -> crate::setup::Io<'a> {
    crate::setup::Io {
        input,
        out,
        hidden,
        open: opened,
        cf_api: format!("{}/client/v4", mock.url),
        config_dir: conf.to_path_buf(),
        r2_endpoint: Some(mock.url.clone()),
        public_wait: std::time::Duration::from_millis(10),
        public_get,
    }
}

#[test]
fn setup_quick_path_makes_the_bucket_and_saves_keys_privately() {
    let token = crate::mock_s3::TOKEN;
    // R2's S3 keys for a token: its id and the SHA-256 of its value.
    let creds = crate::storage::Credentials { access_key_id: crate::mock_s3::TOKEN_ID.into(), secret_access_key: crate::storage::sha256_hex(token.as_bytes()) };
    let mock = crate::mock_s3::Mock::start(creds);
    let (_t, dir) = new_default_blog();
    enable_podcast(&dir);
    let conf = tempfile::tempdir().unwrap();
    let url = mock.url.clone();
    let public_get = move |u: &str| -> anyhow::Result<(u16, String)> {
        let path = u.strip_prefix(&format!("https://{}/", crate::mock_s3::R2_DEV)).expect("public URL is the r2.dev address");
        let mut r = ureq::get(&format!("{url}/pub/{path}")).config().http_status_as_error(false).build().call()?;
        Ok((r.status().as_u16(), r.body_mut().read_to_string()?))
    };
    let mut input = std::io::Cursor::new(format!("{token}\n").into_bytes());
    let mut out = vec![];
    let mut hidden = |_: &str| -> anyhow::Result<String> { panic!("scripted setup never prompts") };
    let mut opened = |u: &str| panic!("opened {u}");
    let opts = crate::setup::Opts {
        account_id: Some("acct123".into()),
        token_from_stdin: true,
        lightning_address: Some("show@rizful.com".into()),
        no_browser: true,
        ..Default::default()
    };
    let mut io = setup_io(&mut input, &mut out, &mut hidden, &mut opened, &mock, conf.path(), &public_get);
    let code = crate::setup::run(&dir, opts, &mut io).unwrap_or_else(|e| panic!("{e:#}\n{}", String::from_utf8_lossy(&out)));
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&out).to_string();
    for want in ["made the bucket example-org-podcast", "turned on pub-test.r2.dev", "ok    upload a tiny test file", "ok    download it from https://pub-test.r2.dev/", "ok    remove the test file", "A note on r2.dev"] {
        assert!(text.contains(want), "missing {want:?}:\n{text}");
    }
    assert!(!text.contains(token) && !text.contains(&crate::storage::sha256_hex(token.as_bytes())), "no secret in the output");
    let cfg = crate::config::Config::load(&dir).unwrap();
    let st = cfg.podcast.storage.unwrap();
    assert_eq!((st.kind.as_str(), st.account_id.as_str(), st.bucket.as_str(), st.public_url.as_str()), ("r2", "acct123", "example-org-podcast", "https://pub-test.r2.dev"));
    assert_eq!(cfg.podcast.lightning_address, "show@rizful.com");
    let toml = fs::read_to_string(dir.join("blog.toml")).unwrap();
    assert!(!toml.contains(token) && !toml.contains("secret"), "{toml}");
    let saved = crate::storage::load_credentials(conf.path(), "example-org-podcast").unwrap();
    assert_eq!(saved.access_key_id, crate::mock_s3::TOKEN_ID);
    assert!(mock.state.lock().unwrap().objects.is_empty(), "the check cleans up after itself");
    // --show masks the key.
    let mut out2 = vec![];
    let mut input2 = std::io::Cursor::new(vec![]);
    let mut io2 = setup_io(&mut input2, &mut out2, &mut hidden, &mut opened, &mock, conf.path(), &public_get);
    crate::setup::run(&dir, crate::setup::Opts { show: true, ..Default::default() }, &mut io2).unwrap();
    let shown = String::from_utf8_lossy(&out2).to_string();
    assert!(shown.contains("access key id: …abcd") && shown.contains("secret: set") && !shown.contains(crate::mock_s3::TOKEN_ID), "{shown}");
}

#[test]
fn setup_careful_path_checks_before_saving_anything() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (_t, dir) = new_default_blog();
    enable_podcast(&dir);
    let conf = tempfile::tempdir().unwrap();
    let before = fs::read_to_string(dir.join("blog.toml")).unwrap();
    // The bucket isn't public: the check fails and nothing is saved.
    let not_public = |_: &str| -> anyhow::Result<(u16, String)> { Ok((404, String::new())) };
    let mut input = std::io::Cursor::new(b"canary-secret-value\n".to_vec());
    let mut out = vec![];
    let mut hidden = |_: &str| -> anyhow::Result<String> { panic!("scripted") };
    let mut opened = |u: &str| panic!("opened {u}");
    let opts = crate::setup::Opts {
        account_id: Some("acct123".into()),
        bucket: Some("show".into()),
        public_url: Some("https://media.example.org".into()),
        access_key_id: Some("AKIDMOCK1234".into()),
        secret_from_stdin: true,
        no_browser: true,
        ..Default::default()
    };
    let mut io = setup_io(&mut input, &mut out, &mut hidden, &mut opened, &mock, conf.path(), &not_public);
    let e = format!("{:#}", crate::setup::run(&dir, opts.clone(), &mut io).unwrap_err());
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(e.contains("nothing was saved"), "{e}");
    assert!(text.contains("FAIL  download it from https://media.example.org/") && text.contains("Public Development URL"), "{text}");
    assert_eq!(fs::read_to_string(dir.join("blog.toml")).unwrap(), before);
    assert!(crate::storage::load_credentials(conf.path(), "show").is_err());
    // Public now: saved, with the key file private and the secret only there.
    let public = |u: &str| -> anyhow::Result<(u16, String)> {
        let key = u.rsplit('/').next().unwrap();
        let s = mock.state.lock().unwrap();
        Ok(s.objects.get(&format!("show/{key}")).map(|v| (200, String::from_utf8_lossy(v).to_string())).unwrap_or((404, String::new())))
    };
    let mut input = std::io::Cursor::new(b"canary-secret-value\n".to_vec());
    let mut out = vec![];
    let mut io = setup_io(&mut input, &mut out, &mut hidden, &mut opened, &mock, conf.path(), &public);
    crate::setup::run(&dir, opts, &mut io).unwrap();
    assert!(!String::from_utf8_lossy(&out).contains("canary-secret-value"));
    assert!(!fs::read_to_string(dir.join("blog.toml")).unwrap().contains("canary"));
    assert_eq!(crate::storage::load_credentials(conf.path(), "show").unwrap().secret_access_key, "canary-secret-value");
    let cfg = crate::config::Config::load(&dir).unwrap();
    assert_eq!(cfg.podcast.storage.unwrap().public_url, "https://media.example.org");
}

#[test]
fn setup_interactive_walks_a_new_user_through_signup() {
    let token = crate::mock_s3::TOKEN;
    let creds = crate::storage::Credentials { access_key_id: crate::mock_s3::TOKEN_ID.into(), secret_access_key: crate::storage::sha256_hex(token.as_bytes()) };
    let mock = crate::mock_s3::Mock::start(creds);
    let (_t, dir) = new_default_blog();
    enable_podcast(&dir);
    let conf = tempfile::tempdir().unwrap();
    let state = mock.state.clone();
    let public = move |u: &str| -> anyhow::Result<(u16, String)> {
        let key = u.rsplit('/').next().unwrap();
        let s = state.lock().unwrap();
        Ok(s.objects.iter().find(|(k, _)| k.ends_with(key)).map(|(_, v)| (200, String::from_utf8_lossy(v).to_string())).unwrap_or((404, String::new())))
    };
    // No account yet; Enter after signup; Enter after R2; quick path; account id; default bucket;
    // own domain with its zone; a Lightning address.
    let answers = "n\n\n\n1\nacct123\n\ny\n\nzone42\nme@rizful.com\n";
    let mut input = std::io::Cursor::new(answers.as_bytes().to_vec());
    let mut out = vec![];
    let mut hidden = |_: &str| -> anyhow::Result<String> { Ok(token.to_string()) };
    let mut links = vec![];
    let mut opened = |u: &str| links.push(u.to_string());
    let mut io = setup_io(&mut input, &mut out, &mut hidden, &mut opened, &mock, conf.path(), &public);
    crate::setup::run(&dir, crate::setup::Opts::default(), &mut io).unwrap_or_else(|e| panic!("{e:#}\n{}", String::from_utf8_lossy(&out)));
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(text.contains("ask for a card or PayPal to turn R2 on, even on the free plan"), "{text}");
    assert!(text.contains("connected media.example.org"), "{text}");
    assert_eq!(links, [crate::setup::SIGNUP_URL, crate::setup::R2_OVERVIEW_URL, crate::setup::TOKEN_TEMPLATE_URL]);
    let cfg = crate::config::Config::load(&dir).unwrap();
    assert_eq!(cfg.podcast.storage.unwrap().public_url, "https://media.example.org");
    assert!(!text.contains("A note on r2.dev"));
    let custom = mock.requests().into_iter().find(|r| r.path.ends_with("/domains/custom")).expect("custom domain connected");
    assert_eq!(custom.headers["cf-r2-jurisdiction"], "default");
    assert_eq!(crate::config::Config::load(&dir).unwrap().podcast.lightning_address, "me@rizful.com");
}

// ---------- MCP tools ----------

fn mcp_ctx(b: &Blog, conf: &Path) -> crate::mcp::Ctx {
    crate::mcp::Ctx { root: b.dir.parent().unwrap().to_path_buf(), config_dir: conf.to_path_buf() }
}

/// Every file outside `podcast/` with its contents, to prove the tools never write there.
fn outside_podcast(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".blog" && e.file_name() != "podcast")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| (e.path().display().to_string(), fs::read(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

fn tmp_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p.display().to_string()
}

#[test]
fn mcp_create_episode_writes_a_draft_inside_podcast_only() {
    let b = podcast_blog("");
    let conf = tempfile::tempdir().unwrap();
    let ctx = mcp_ctx(&b, conf.path());
    let src = tempfile::tempdir().unwrap();
    let audio = tmp_file(src.path(), "rec.mp3", MP3);
    let vtt = tmp_file(src.path(), "t.vtt", b"WEBVTT\n\n00:00.000 --> 00:01.000\nHello there\n");
    let before = outside_podcast(&b.dir);
    let args = |title: &str| crate::mcp::CreateArgs {
        blog: "example.com".into(),
        audio_path: audio.clone(),
        title: Some(title.into()),
        notes_markdown: Some("We talk about **things**.".into()),
        summary: Some("Things.".into()),
        season: Some(1),
        episode: None,
        episode_type: None,
        explicit: Some(false),
        tags: Some(vec!["Talk".into()]),
        persons: Some(vec![crate::mcp::PersonArg { name: "Ada".into(), role: Some("host".into()), href: None, img: None }]),
        transcript_path: Some(vtt.clone()),
        chapters: Some(vec![crate::mcp::ChapterArg { start: 0.0, title: "Intro".into(), url: None, img: None }]),
        date: Some("2024-05-01".into()),
    };
    let r = crate::mcp::create_episode(&ctx, &args("First things")).unwrap();
    assert_eq!(r["draft"], true);
    assert_eq!(r["created"], "podcast/2024-05-01-first-things/");
    assert_eq!(r["front_matter"]["episode"], 1);
    assert_eq!(r["duration"], "0:03");
    assert!(r["transcript_text"].as_str().unwrap().contains("Hello there"));
    let dir = b.dir.join("podcast/2024-05-01-first-things");
    for f in ["episode.mp3", "transcript.vtt", "chapters.json", "index.md"] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let index = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(index.contains("draft: true") && index.contains("guid:") && index.ends_with("We talk about **things**."), "{index}");
    // The next one is numbered after it; drafts never reach the build.
    let r2 = crate::mcp::create_episode(&ctx, &args("Second things")).unwrap();
    assert_eq!(r2["front_matter"]["episode"], 2);
    assert_eq!(r2["front_matter"]["season"], 1);
    b.build().unwrap();
    assert!(!b.read_out("podcast/feed.xml").contains("<item>"));
    assert_eq!(outside_podcast(&b.dir), before, "nothing outside podcast/ was written");
    // Refusals.
    let e = format!("{:#}", crate::mcp::create_episode(&ctx, &args("First things")).unwrap_err());
    assert!(e.contains("already exists"), "{e}");
    let mut bad = args("Third");
    bad.audio_path = tmp_file(src.path(), "rec.m4a", MP4);
    assert!(format!("{:#}", crate::mcp::create_episode(&ctx, &bad).unwrap_err()).contains("export the episode as MP3"));
    let mut bad = args("Fourth");
    bad.episode_type = Some("special".into());
    assert!(format!("{:#}", crate::mcp::create_episode(&ctx, &bad).unwrap_err()).contains("full, trailer or bonus"));
    assert!(!b.dir.join("podcast/2024-05-01-fourth").exists(), "a refused episode leaves nothing behind");
}

#[test]
fn mcp_update_and_chapters_validate_strictly() {
    let b = podcast_blog("");
    let conf = tempfile::tempdir().unwrap();
    let ctx = mcp_ctx(&b, conf.path());
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\"\ndraft: true\nmine: kept"), "episode.mp3", MP3);
    let up = |fields: serde_json::Value| crate::mcp::update_episode(&ctx, &crate::mcp::UpdateArgs { blog: "example.com".into(), slug: "pilot".into(), fields: fields.as_object().unwrap().clone() });
    let r = up(serde_json::json!({ "title": "The Pilot", "season": 2, "explicit": true, "notes_markdown": "New notes." })).unwrap();
    assert_eq!(r["front_matter"]["title"], "The Pilot");
    let index = fs::read_to_string(b.dir.join("podcast/2024-05-01-pilot/index.md")).unwrap();
    assert!(index.contains("mine: kept") && index.contains(GUID1) && index.ends_with("New notes."), "{index}");
    for (fields, want) in [
        (serde_json::json!({ "explicit": "yes" }), "must be true or false"),
        (serde_json::json!({ "season": 0 }), "whole number"),
        (serde_json::json!({ "guid": "new" }), "never changes"),
        (serde_json::json!({ "draft": false }), "use publish_episode"),
        (serde_json::json!({ "colour": "red" }), "not a field"),
        (serde_json::json!({ "persons": [{ "name": "A", "href": "http://x" }] }), "https://"),
    ] {
        let e = format!("{:#}", up(fields).unwrap_err());
        assert!(e.contains(want), "{want}: {e}");
    }
    let ch = |c: Vec<crate::mcp::ChapterArg>| crate::mcp::set_chapters(&ctx, &crate::mcp::ChaptersArgs { blog: "example.com".into(), slug: "pilot".into(), chapters: c });
    ch(vec![
        crate::mcp::ChapterArg { start: 0.0, title: "Intro".into(), url: None, img: None },
        crate::mcp::ChapterArg { start: 90.5, title: "Talk".into(), url: Some("https://example.com".into()), img: None },
    ])
    .unwrap();
    let json = fs::read_to_string(b.dir.join("podcast/2024-05-01-pilot/chapters.json")).unwrap();
    assert_eq!(crate::podcast::parse_chapters(&json).unwrap().len(), 2);
    let e = format!("{:#}", ch(vec![crate::mcp::ChapterArg { start: 0.0, title: "x".into(), url: Some("http://plain".into()), img: None }]).unwrap_err());
    assert!(e.contains("https://"), "{e}");
    // validate sees problems in drafts, as they would be once published.
    b.write("podcast/2024-05-01-pilot/chapters.json", "{}");
    let v = crate::mcp::validate(&ctx, &crate::mcp::ValidateArgs { blog: "example.com".into(), slug: Some("pilot".into()) }).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["errors"].to_string().contains("\\\"version\\\" is required"), "{v}");
}

#[test]
fn mcp_publish_needs_confirm_then_uploads_and_deploys() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, conf) = storage_blog(&mock);
    let ctx = mcp_ctx(&b, conf.path());
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\"\ndraft: true\ntitle: Pilot"), "episode.mp3", MP3);
    let publish = |confirm: bool| crate::mcp::publish_episode(&ctx, &crate::mcp::PublishArgs { blog: "example.com".into(), slug: "pilot".into(), confirm });
    let e = format!("{:#}", publish(false).unwrap_err());
    assert!(e.contains("not published") && e.contains("confirm: true"), "{e}");
    assert!(fs::read_to_string(b.dir.join("podcast/2024-05-01-pilot/index.md")).unwrap().contains("draft: true"));
    assert!(mock.requests().is_empty(), "nothing uploaded without confirm");
    let r = publish(true).unwrap();
    assert_eq!(r["published"], true);
    assert_eq!(r["episode_url"], "https://example.com/podcast/pilot/");
    assert_eq!(r["feed_url"], "https://example.com/podcast/feed.xml");
    assert!(r["enclosure"].as_str().unwrap().starts_with("https://media.example.com/example.com/pilot-"));
    assert!(b.dir.join("deployed-marker").exists());
    assert!(b.read_out("podcast/feed.xml").contains(GUID1));
    assert!(mock.requests().iter().any(|r| r.method == "PUT"));
    let status = crate::mcp::podcast_status(&ctx, &crate::mcp::BlogArgs { blog: "example.com".into() }).unwrap().to_string();
    assert!(status.contains("…1234") && !status.contains("canary-secret-value") && !status.contains("AKIDMOCK1234"), "{status}");
    for out in [r.to_string(), status] {
        assert!(!out.contains("canary-secret-value"));
    }
}

#[test]
fn mcp_publish_refuses_problems_and_respects_mcp_publish_false() {
    let mock = crate::mock_s3::Mock::start(mock_creds());
    let (b, conf) = storage_blog(&mock);
    let ctx = mcp_ctx(&b, conf.path());
    add_episode(&b, "2024-05-01-pilot", &format!("guid: \"{GUID1}\"\ndraft: true\nseason: zero"), "episode.mp3", MP3);
    let args = crate::mcp::PublishArgs { blog: "example.com".into(), slug: "pilot".into(), confirm: true };
    let e = format!("{:#}", crate::mcp::publish_episode(&ctx, &args).unwrap_err());
    assert!(e.contains("draft again") && e.contains("season must be a whole number"), "{e}");
    assert!(fs::read_to_string(b.dir.join("podcast/2024-05-01-pilot/index.md")).unwrap().contains("draft: true"));
    assert!(!b.dir.join("deployed-marker").exists());
    let cfg = fs::read_to_string(b.dir.join("blog.toml")).unwrap().replace("[podcast]\n", "[podcast]\nmcp_publish = false\n");
    fs::write(b.dir.join("blog.toml"), cfg).unwrap();
    let r = crate::mcp::publish_episode(&ctx, &args).unwrap();
    assert_eq!(r["published"], false);
    assert!(r["command"].as_str().unwrap().starts_with("folderblog podcast publish --episode pilot "), "{r}");
}

#[test]
fn build_lock_makes_a_second_build_or_deploy_wait_its_turn() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    let tmp = tempfile::tempdir().unwrap();
    let blog = tmp.path().to_path_buf();
    let first = crate::deploy::lock(&blog, || panic!("nobody else has the lock yet")).unwrap();
    let waited = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (b, w) = (blog.clone(), waited.clone());
    let second = std::thread::spawn(move || {
        let l = crate::deploy::lock(&b, || w.store(true, Ordering::SeqCst)).unwrap();
        tx.send(()).unwrap();
        drop(l);
    });
    // While the first holder keeps it, the second can't get in.
    assert!(rx.recv_timeout(std::time::Duration::from_millis(300)).is_err());
    assert!(waited.load(Ordering::SeqCst));
    drop(first);
    rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the second gets the lock once the first lets go");
    second.join().unwrap();
    // Free again afterwards.
    drop(crate::deploy::lock(&blog, || panic!("the lock should be free")).unwrap());
}

// ---------- sample templates/ ----------

/// Template text ready to use: `draft: true` removed and, with `all`, every commented-out
/// detail (`# key: value` and the indented lines under it) switched on.
fn use_template(text: &str, all: bool, guid: Option<&str>) -> String {
    let (front, body) = text.strip_prefix("---\n").unwrap().split_once("\n---\n").unwrap();
    let mut out = vec![];
    for line in front.lines() {
        if line == "draft: true" {
            continue;
        }
        if line.starts_with("# guid:") {
            if let Some(g) = guid {
                out.push(format!("guid: \"{g}\""));
            }
            continue;
        }
        let detail = line.strip_prefix("# ").filter(|l| {
            l.starts_with("  - ") || l.split_once(": ").or(l.strip_suffix(':').map(|k| (k, ""))).is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        });
        match detail {
            Some(d) if all => out.push(d.to_string()),
            _ => out.push(line.to_string()),
        }
    }
    format!("---\n{}\n---\n{body}", out.join("\n"))
}

#[test]
fn new_blogs_get_templates_and_the_command_only_adds_missing_ones() {
    let (_t, dir) = new_default_blog();
    assert!(dir.join("templates/post.md").is_file());
    assert!(dir.join("templates/podcast-episode/index.md").is_file());
    fs::write(dir.join("templates/post.md"), "mine").unwrap();
    fs::remove_file(dir.join("templates/podcast-episode/index.md")).unwrap();
    assert_eq!(crate::fixtures::write_templates(&dir).unwrap(), vec!["templates/podcast-episode/index.md"]);
    assert_eq!(fs::read_to_string(dir.join("templates/post.md")).unwrap(), "mine", "never overwrites");
    assert!(crate::fixtures::write_templates(&dir).unwrap().is_empty());
    // Templates are not content and don't make the watcher rebuild.
    assert!(crate::watch::relevant(dir.parent().unwrap(), &dir.join("templates/post.md")).is_none());
    let rep = crate::check::check(&dir, None).unwrap();
    assert!(rep.errors.is_empty(), "{:#?}", rep.errors);
}

#[test]
fn templates_are_valid_as_written_and_with_every_detail_switched_on() {
    let tpl = |p: &str| crate::fixtures::BLOG_TEMPLATES.get_file(p).unwrap().contents_utf8().unwrap().to_string();
    for all in [false, true] {
        let b = podcast_blog("lightning_address = \"ada@example.com\"");
        b.write("theme/_layouts/post-wide.html", "{{ item.title }}");
        b.write("posts/walk.md", &use_template(&tpl("post.md"), all, None));
        write_bytes(&b.dir.join("posts/cover.jpg"), COVER);
        let ep = "podcast/2026-03-21-first-episode";
        b.write(&format!("{ep}/index.md"), &use_template(&tpl("podcast-episode/index.md"), all, Some(GUID1)));
        write_bytes(&b.dir.join(format!("{ep}/episode.mp3")), MP3_NO_XING);
        write_bytes(&b.dir.join(format!("{ep}/art.jpg")), COVER);
        for f in ["transcript.vtt", "chapters.json"] {
            fs::copy(format!("fixtures/stress/podcast/2024-05-01-pilot/{f}"), b.dir.join(format!("{ep}/{f}"))).unwrap();
        }
        assert_eq!(check_errors(&b), Vec::<String>::new(), "all = {all}");
        let site = b.site();
        let post = site.posts.iter().find(|p| p.title == "A walk in the woods").expect("post published");
        assert_eq!(post.tags, ["walking", "nature"]);
        let show = site.podcast.as_ref().unwrap();
        let e = show.episodes.iter().find(|e| e.item.title == "First episode").expect("episode published");
        assert_eq!((e.guid.as_str(), e.season, e.number, e.persons.len()), (GUID1, Some(1), Some(1), 2));
        if all {
            assert_eq!(post.slug, "woods-walk");
            assert!(post.image.is_some());
            assert!(e.in_main_feed && e.image.is_some() && e.transcript.is_some() && e.chapters.is_some());
            assert_eq!(e.value.len(), 2);
        }
    }
}

// ---------- bringing existing blogs up to date ----------

#[test]
fn upgrade_adds_what_a_new_version_brings_once_and_never_touches_the_rest() {
    let (_t, dir) = new_default_blog();
    let snapshot = |d: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        let mut v: Vec<_> = walkdir::WalkDir::new(d)
            .into_iter()
            .filter_entry(|e| !matches!(e.file_name().to_str(), Some(".blog" | "templates" | "AGENTS.md")))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| (e.path().to_path_buf(), fs::read(e.path()).unwrap()))
            .collect();
        v.sort();
        v
    };
    // A fresh blog is already current.
    assert!(crate::upgrade::upgrade(&dir).unwrap().is_empty());

    // Make it look like a blog from an older folderblog: no templates, an out-of-date AGENTS.md
    // with the owner's own notes after the generated part.
    fs::remove_dir_all(dir.join("templates")).unwrap();
    fs::remove_file(dir.join(".blog/upgrade.json")).unwrap();
    let agents = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    let old = agents.replacen(&format!("Generated by folderblog {}.", env!("CARGO_PKG_VERSION")), "Generated by folderblog 0.0.1.", 1);
    assert_ne!(old, agents);
    fs::write(dir.join("AGENTS.md"), format!("{old}\n## My notes\nKeep me.\n")).unwrap();
    let before = snapshot(&dir);

    let changed = crate::upgrade::upgrade(&dir).unwrap();
    assert_eq!(changed, ["added templates/post.md", "added templates/podcast-episode/index.md", "refreshed AGENTS.md"]);
    let agents = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(crate::contract::is_current(&agents) && agents.ends_with("## My notes\nKeep me.\n"));
    assert_eq!(snapshot(&dir), before, "theme/, blog.toml and content are left alone");
    assert!(!crate::config::Config::load(&dir).unwrap().podcast.enabled, "features stay off");
    assert!(crate::upgrade::upgrade(&dir).unwrap().is_empty(), "only once per version");

    // A sample the owner deleted is not put back, even by a later version.
    fs::remove_file(dir.join("templates/post.md")).unwrap();
    let rec = dir.join(".blog/upgrade.json");
    fs::write(&rec, fs::read_to_string(&rec).unwrap().replace(env!("CARGO_PKG_VERSION"), "0.0.1")).unwrap();
    assert!(crate::upgrade::upgrade(&dir).unwrap().is_empty());
    assert!(!dir.join("templates/post.md").exists());
}
