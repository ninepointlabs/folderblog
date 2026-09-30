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
    crate::new_blog(&dir).unwrap();
    (tmp, dir)
}

#[test]
fn default_theme_passes_check_on_all_fixtures() {
    let (_t, dir) = new_default_blog();
    for f in crate::fixtures::names() {
        let rep = crate::check::check(&dir, Some(f.clone())).unwrap();
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
        if g.name == "pager" || g.name == "page" {
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
    let mut toml = String::from("title='t'\nbase_url='https://x.com'\n");
    let mut tables: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
    for k in crate::contract::CONFIG_KEYS {
        match k.name.split_once('.') {
            Some((t, key)) => {
                let v = match (t, key) {
                    ("deploy", "type") => "'git'".into(),
                    ("deploy", "command") => continue,
                    (_, _) if k.ty.starts_with("list") => "['x']".into(),
                    (_, _) if k.ty == "int" => "3".into(),
                    (_, _) if k.ty == "bool" => "true".into(),
                    _ => "'x'".to_string(),
                };
                tables.entry(t).or_default().push(format!("{key} = {v}"));
            }
            None if ["title", "base_url", "params"].contains(&k.name) => {}
            None => toml.push_str(&format!("{} = '/{{slug}}/'\n", k.name)),
        }
    }
    for (t, lines) in tables {
        toml.push_str(&format!("[{t}]\n{}\n", lines.join("\n")));
    }
    toml.push_str("[params]\nanything = 1\n");
    crate::config::Config::parse(&toml).unwrap_or_else(|e| panic!("{e:#}\n{toml}"));
    crate::config::Config::parse("title='t'\nbase_url='https://x.com'\n[deploy]\ntype='command'\ncommand='true'\n").unwrap();
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
