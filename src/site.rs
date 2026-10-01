//! Loading a blog into the data model templates see.

use crate::config::Config;
use crate::content::{self, Body, DateSource, Kind, Source};
use crate::gallery::{self, Gallery};
use crate::markdown::{self, Markup};
use crate::state::State;
use crate::templates::{EnvSettings, make_env, prefix_base};
use crate::util::{resolve_relative, slugify, strip_tags};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, FixedOffset};
use minijinja::{Environment, UndefinedBehavior, Value};
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub drafts: bool,
    /// Render against built-in stress content instead of the blog's own content.
    pub fixtures: Option<String>,
    pub persist_state: bool,
    pub strict_undefined: bool,
}

pub struct Item {
    pub src: Source,
    pub slug: String,
    /// Site-relative URL path without base path, e.g. "/2024/05/hello/".
    pub route: String,
    pub date: Option<(DateTime<FixedOffset>, DateSource)>,
    pub title: String,
    pub html: String,
    pub excerpt_html: String,
    pub plain: String,
    pub raw: String,
    pub headings: Vec<markdown::Heading>,
    pub has_more: bool,
    pub tags: Vec<String>,
    /// Attached files: (absolute source, site-relative output path).
    pub files: Vec<(PathBuf, String)>,
    pub value: Value,
}

pub struct Site {
    pub blog_dir: PathBuf,
    pub content_root: PathBuf,
    pub theme_dir: PathBuf,
    pub cfg: Config,
    pub base_path: String,
    pub env: Environment<'static>,
    pub posts: Vec<Item>,
    pub pages: Vec<Item>,
    /// Loose files under posts/ and pages/: (absolute source, site-relative output path).
    pub loose_files: Vec<(PathBuf, String)>,
    pub globals: BTreeMap<String, Value>,
    pub warnings: Vec<String>,
    pub opts: LoadOptions,
    /// Set when `[gallery] enabled = true`.
    pub gallery: Option<Gallery>,
    pub gallery_values: GalleryValues,
}

/// Template values for the gallery, parallel to `Gallery::photos` / `albums` / `tag_index`.
#[derive(Default)]
pub struct GalleryValues {
    pub photos: Vec<Value>,
    pub albums: Vec<Value>,
    /// (route, tag value)
    pub tags: Vec<(String, Value)>,
    pub root: Value,
}

pub fn permalink(pattern: &str, slug: &str, date: Option<&DateTime<FixedOffset>>) -> String {
    let mut s = pattern.replace("{slug}", slug);
    if let Some(d) = date {
        s = s
            .replace("{year}", &format!("{:04}", d.year()))
            .replace("{month}", &format!("{:02}", d.month()))
            .replace("{day}", &format!("{:02}", d.day()));
    }
    s
}

fn json_to_value(j: &Json) -> Value {
    Value::from_serialize(j)
}

pub fn fixture_root(blog_dir: &Path, name: &str) -> Result<PathBuf> {
    let set = crate::fixtures::FIXTURES
        .get_dir(name)
        .with_context(|| format!("unknown fixture set {name:?} (have: {})", crate::fixtures::names().join(", ")))?;
    let root = blog_dir.join(crate::state::STATE_DIR).join("fixtures").join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    set.extract(&root)?;
    // include_dir extracts with the set's own prefix; flatten it.
    let nested = root.join(name);
    if nested.is_dir() {
        for e in std::fs::read_dir(&nested)? {
            let e = e?;
            std::fs::rename(e.path(), root.join(e.file_name()))?;
        }
        std::fs::remove_dir_all(&nested)?;
    }
    Ok(root)
}

impl Site {
    pub fn load(blog_dir: &Path, opts: LoadOptions) -> Result<Site> {
        let blog_dir = blog_dir.canonicalize().with_context(|| format!("no blog at {}", blog_dir.display()))?;
        let cfg = Config::load(&blog_dir)?;
        let theme_dir = blog_dir.join("theme");
        if !theme_dir.is_dir() {
            bail!("{} has no theme/ folder", blog_dir.display());
        }
        let (content_root, mut state) = match &opts.fixtures {
            Some(name) => (fixture_root(&blog_dir, name)?, State::ephemeral()),
            None => (blog_dir.clone(), State::load(&blog_dir, opts.persist_state)?),
        };
        let base_path = cfg.base_path();
        let mut env = make_env(EnvSettings {
            theme_dir: theme_dir.clone(),
            pages_dir: content_root.join("pages"),
            base_url: cfg.base_url.clone(),
            base_path: base_path.clone(),
            undefined: if opts.strict_undefined { UndefinedBehavior::SemiStrict } else { UndefinedBehavior::Chainable },
        });

        let found = content::discover(&content_root, opts.drafts)?;
        let mut warnings = found.warnings;

        // Pass 1: slugs, dates and routes, which the link resolver needs.
        struct Pre {
            src: Source,
            slug: String,
            date: Option<(DateTime<FixedOffset>, DateSource)>,
            route: String,
        }
        let mut pre = vec![];
        for src in found.sources {
            let slug = content::slug_for(&src);
            let date = content::resolve_date(&src, &slug, &mut state)?;
            let route = if let Some(u) = src.front.get("url").and_then(|v| v.as_str()) {
                let u = u.trim();
                if u.starts_with('/') { u.to_string() } else { format!("/{u}") }
            } else {
                match src.kind {
                    Kind::Post => permalink(&cfg.permalink, &slug, date.as_ref().map(|d| &d.0)),
                    Kind::Page => {
                        let mut parts: Vec<String> =
                            src.section_subdir.iter().map(|c| slugify(&c.to_string_lossy())).collect();
                        if !(src.bundle_dir.is_none() && src.stem == "index") {
                            parts.push(slug.clone());
                        }
                        if parts.is_empty() { "/".into() } else { format!("/{}/", parts.join("/")) }
                    }
                }
            };
            pre.push(Pre { src, slug, date, route });
        }
        let gallery = if cfg.gallery.enabled {
            let cache = if opts.fixtures.is_some() { "gallery-fixtures" } else { "gallery" };
            let cache_dir = blog_dir.join(crate::state::STATE_DIR).join("cache").join(cache);
            let args = gallery::LoadArgs {
                content_root: &content_root,
                cache_dir: &cache_dir,
                route: &cfg.gallery.url,
                base_path: &base_path,
                drafts: opts.drafts,
                prune_cache: true,
            };
            Some(gallery::load(args, &mut state, &mut warnings)?)
        } else {
            None
        };
        state.save()?;

        // Where each source-side path is published: content items and every attached/loose file.
        let mut published: HashMap<String, String> = HashMap::new();
        let source_site_path = |kind: Kind, section_rel: &Path| -> String {
            let rel = section_rel.to_string_lossy().replace('\\', "/");
            match kind {
                Kind::Post => format!("/posts/{rel}"),
                Kind::Page => format!("/{rel}"),
            }
        };
        let section_base = |kind: Kind| match kind {
            Kind::Post => content_root.join("posts"),
            Kind::Page => content_root.join("pages"),
        };
        let mut item_files: HashMap<usize, Vec<(PathBuf, String)>> = HashMap::new();
        let mut loose_files = vec![];
        for p in pre.iter() {
            let base = if p.src.draft && p.src.rel.starts_with("drafts") {
                content_root.join("drafts")
            } else {
                section_base(p.src.kind)
            };
            if let Ok(rel) = p.src.abs.strip_prefix(&base) {
                published.insert(source_site_path(p.src.kind, rel), prefix_base(&base_path, &p.route));
            }
            if let Some(bundle) = &p.src.bundle_dir {
                if let Ok(rel) = bundle.strip_prefix(&base) {
                    published.insert(
                        format!("{}/", source_site_path(p.src.kind, rel)),
                        prefix_base(&base_path, &p.route),
                    );
                }
            }
        }
        for (abs, kind, rel) in &found.loose_files {
            let site_src = source_site_path(*kind, rel);
            // Attached to a bundle?
            let owner = pre.iter().position(|p| p.src.bundle_dir.as_ref().is_some_and(|b| abs.starts_with(b)));
            let out = match owner {
                Some(i) => {
                    let b = pre[i].src.bundle_dir.as_ref().unwrap();
                    let inner = abs.strip_prefix(b)?.to_string_lossy().replace('\\', "/");
                    let dir = if pre[i].route.ends_with('/') {
                        pre[i].route.clone()
                    } else {
                        pre[i].route.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or("/".into())
                    };
                    let out = format!("{dir}{inner}");
                    item_files.entry(i).or_default().push((abs.clone(), out.clone()));
                    out
                }
                None => {
                    loose_files.push((abs.clone(), site_src.clone()));
                    site_src.clone()
                }
            };
            published.insert(site_src, prefix_base(&base_path, &out));
        }

        let overrides: HashSet<String> = [
            markdown::MARKUP_IMAGE,
            markdown::MARKUP_LINK,
            markdown::MARKUP_HEADING,
            markdown::MARKUP_CODEBLOCK,
        ]
        .iter()
        .filter(|n| theme_dir.join(n).is_file())
        .map(|s| s.to_string())
        .collect();

        let site_value = Self::site_value(&cfg, &base_path);

        // Pass 2: render bodies.
        let mut src_dirs: HashMap<String, String> = HashMap::new();
        let mut posts = vec![];
        let mut pages = vec![];
        for (i, p) in pre.into_iter().enumerate() {
            let fm_title = p.src.front.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
            let src_dir_site = {
                let base = if p.src.rel.starts_with("drafts") {
                    content_root.join("drafts")
                } else {
                    section_base(p.src.kind)
                };
                let rel = p.src.abs.parent().unwrap().strip_prefix(&base).unwrap_or(Path::new(""));
                let s = source_site_path(p.src.kind, rel);
                if s.ends_with('/') { s } else { format!("{s}/") }
            };
            src_dirs.insert(p.src.rel.to_string_lossy().replace('\\', "/"), src_dir_site.clone());
            let resolver = |u: &str| -> String {
                match resolve_relative(&src_dir_site, u) {
                    Some(abs) => {
                        let (path, suffix) = match abs.find(['?', '#']) {
                            Some(i) => (&abs[..i], &abs[i..]),
                            None => (abs.as_str(), ""),
                        };
                        match published.get(path) {
                            Some(url) => format!("{url}{suffix}"),
                            None => prefix_base(&base_path, &abs),
                        }
                    }
                    None if u.starts_with('/') && !u.starts_with("//") => prefix_base(&base_path, u),
                    None => u.to_string(),
                }
            };
            let (title, html, excerpt, plain, raw, headings, has_more) = match &p.src.body {
                Body::Markdown(text) => {
                    let item_ctx = Value::from_serialize(BTreeMap::from([
                        ("slug", p.slug.clone()),
                        ("url", prefix_base(&base_path, &p.route)),
                        ("source", p.src.rel.to_string_lossy().replace('\\', "/")),
                        ("kind", if p.src.kind == Kind::Post { "post".into() } else { "page".into() }),
                    ]));
                    let markup = Markup {
                        env: &env,
                        overrides: &overrides,
                        site: site_value.clone(),
                        item: item_ctx,
                        slug: p.slug.clone(),
                    };
                    let r = markdown::render(text, fm_title.is_none(), Some(&markup), &resolver)
                        .with_context(|| format!("rendering {}", p.src.rel.display()))?;
                    let excerpt = match p.src.front.get("excerpt").or(p.src.front.get("summary")).and_then(|v| v.as_str()) {
                        Some(e) => markdown::render(e, false, None, &resolver)?.html,
                        None => r.excerpt_html.clone(),
                    };
                    let title = fm_title.or(r.title.clone()).unwrap_or_else(|| content::title_fallback(&p.src));
                    (title, r.html, excerpt, r.plain, text.clone(), r.headings, r.has_more)
                }
                Body::Html(text) => {
                    let title = fm_title.unwrap_or_else(|| content::title_fallback(&p.src));
                    (title, String::new(), String::new(), String::new(), text.clone(), vec![], false)
                }
            };
            let item = Item {
                slug: p.slug,
                route: p.route,
                date: p.date,
                title,
                html,
                excerpt_html: excerpt,
                plain,
                raw,
                headings,
                has_more,
                tags: content::tags_of(&p.src.front),
                files: item_files.remove(&i).unwrap_or_default(),
                value: Value::UNDEFINED,
                src: p.src,
            };
            match item.src.kind {
                Kind::Post => posts.push(item),
                Kind::Page => pages.push(item),
            }
        }
        // `{{ post.meta.cover | resolve_url(post) }}`: resolve a path written in front matter
        // exactly like a path written in the body.
        {
            let published = published.clone();
            let base_path = base_path.clone();
            env.add_filter("resolve_url", move |v: String, item: Value| -> String {
                let source = item.get_attr("source").ok().map(|s| s.to_string()).unwrap_or_default();
                let dir = src_dirs.get(&source).cloned().unwrap_or_else(|| "/".into());
                match resolve_relative(&dir, &v) {
                    Some(abs) => {
                        let (path, suffix) = match abs.find(['?', '#']) {
                            Some(i) => (&abs[..i], &abs[i..]),
                            None => (abs.as_str(), ""),
                        };
                        match published.get(path) {
                            Some(url) => format!("{url}{suffix}"),
                            None => prefix_base(&base_path, &abs),
                        }
                    }
                    None if v.starts_with('/') && !v.starts_with("//") => prefix_base(&base_path, &v),
                    None => v,
                }
            });
        }
        let mut slug_names: BTreeMap<String, String> = BTreeMap::new();
        for p in &posts {
            for t in &p.tags {
                let slug = slugify(t);
                match slug_names.get(&slug) {
                    Some(prev) if prev != t => warnings.push(format!(
                        "tags {prev:?} and {t:?} share the slug {slug:?} and are merged into one tag ({})",
                        p.src.rel.display()
                    )),
                    None => {
                        slug_names.insert(slug, t.clone());
                    }
                    _ => {}
                }
            }
        }
        posts.sort_by(|a, b| {
            let da = a.date.as_ref().map(|d| d.0);
            let db = b.date.as_ref().map(|d| d.0);
            db.cmp(&da).then(a.slug.cmp(&b.slug))
        });
        pages.sort_by(|a, b| {
            let wa = a.src.front.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let wb = b.src.front.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.0);
            wa.partial_cmp(&wb).unwrap().then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        let mut site = Site {
            blog_dir: blog_dir.clone(),
            content_root,
            theme_dir,
            cfg,
            base_path,
            env,
            posts,
            pages,
            loose_files,
            globals: BTreeMap::new(),
            warnings,
            opts,
            gallery,
            gallery_values: GalleryValues::default(),
        };
        site.build_values()?;
        Ok(site)
    }

    fn site_value(cfg: &Config, base_path: &str) -> Value {
        let mut m = BTreeMap::new();
        m.insert("title", Value::from(cfg.title.clone()));
        m.insert("description", Value::from(cfg.description.clone()));
        m.insert("author", Value::from(cfg.author.clone()));
        m.insert("language", Value::from(cfg.language.clone()));
        m.insert("base_url", Value::from(cfg.base_url.clone()));
        m.insert("base_path", Value::from(base_path.to_string()));
        m.insert("url", Value::from(prefix_base(base_path, "/")));
        m.insert("feed_url", Value::from(prefix_base(base_path, &format!("/{}", crate::feed::FEED_PATH))));
        m.insert("feed_url_absolute", Value::from(format!("{}/{}", cfg.base_url, crate::feed::FEED_PATH)));
        m.insert("sitemap_url", Value::from(prefix_base(base_path, &format!("/{}", crate::feed::SITEMAP_PATH))));
        m.insert("params", Value::from_serialize(&cfg.params));
        m.insert("generator", Value::from(format!("folderblog {}", env!("CARGO_PKG_VERSION"))));
        Value::from_serialize(m)
    }

    fn link_value(&self, it: &Item) -> Value {
        let mut m = BTreeMap::new();
        m.insert("title", Value::from(it.title.clone()));
        m.insert("url", Value::from(prefix_base(&self.base_path, &it.route)));
        m.insert("slug", Value::from(it.slug.clone()));
        m.insert("date", it.date.as_ref().map(|d| Value::from(d.0.to_rfc3339())).unwrap_or(Value::from(())));
        Value::from_serialize(m)
    }

    fn item_value(&self, it: &Item, kind: &str, older: Option<Value>, newer: Option<Value>) -> Value {
        let url = prefix_base(&self.base_path, &it.route);
        let mut m: BTreeMap<&str, Value> = BTreeMap::new();
        m.insert("kind", Value::from(kind));
        m.insert("title", Value::from(it.title.clone()));
        m.insert("slug", Value::from(it.slug.clone()));
        m.insert("url", Value::from(url.clone()));
        m.insert("permalink", Value::from(crate::templates::absolute(&self.cfg.base_url, &self.base_path, &url)));
        let none = || Value::from(());
        match &it.date {
            Some((d, source)) => {
                m.insert("date", Value::from(d.to_rfc3339()));
                m.insert("date_source", Value::from_serialize(source));
                m.insert("year", Value::from(d.year()));
                m.insert("month", Value::from(d.month()));
                m.insert("day", Value::from(d.day()));
            }
            None => {
                for k in ["date", "date_source", "year", "month", "day"] {
                    m.insert(k, none());
                }
            }
        }
        let updated = ["updated", "modified", "lastmod"]
            .iter()
            .find_map(|k| it.src.front.get(*k).and_then(|v| v.as_str()).and_then(content::parse_date))
            .map(|d| Value::from(d.to_rfc3339()))
            .unwrap_or_else(none);
        m.insert("updated", updated);
        m.insert("content", Value::from_safe_string(it.html.clone()));
        m.insert("excerpt", Value::from_safe_string(it.excerpt_html.clone()));
        let summary_src = strip_tags(&it.excerpt_html);
        m.insert("summary", Value::from(truncate_chars(&summary_src, 300)));
        m.insert("has_more", Value::from(it.has_more));
        m.insert("raw", Value::from(it.raw.clone()));
        m.insert("text", Value::from(it.plain.clone()));
        let wc = it.plain.split_whitespace().count();
        m.insert("word_count", Value::from(wc));
        m.insert("reading_time", Value::from(wc.div_ceil(220).max(1)));
        let tags: Vec<Value> = it
            .tags
            .iter()
            .map(|t| {
                let mut tm = BTreeMap::new();
                tm.insert("name", t.clone());
                tm.insert("slug", slugify(t));
                Value::from_serialize(tm)
            })
            .collect();
        m.insert("tags", Value::from(tags));
        m.insert("headings", Value::from_serialize(&it.headings));
        m.insert("meta", json_to_value(&Json::Object(it.src.front.clone())));
        m.insert("draft", Value::from(it.src.draft));
        m.insert("layout", Value::from(self.layout_of(it)));
        m.insert("source", Value::from(it.src.rel.to_string_lossy().replace('\\', "/")));
        m.insert("older", older.unwrap_or_else(none));
        m.insert("newer", newer.unwrap_or_else(none));
        let files: Vec<Value> = it
            .files
            .iter()
            .map(|(abs, out)| {
                let mut fm = BTreeMap::new();
                fm.insert("name", Value::from(abs.file_name().unwrap().to_string_lossy().to_string()));
                fm.insert("url", Value::from(prefix_base(&self.base_path, out)));
                let ext = abs.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                fm.insert("ext", Value::from(ext.clone()));
                fm.insert(
                    "is_image",
                    Value::from(matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "svg")),
                );
                Value::from_serialize(fm)
            })
            .collect();
        m.insert("files", Value::from(files));
        Value::from_serialize(m)
    }

    pub fn layout_of(&self, it: &Item) -> String {
        match it.src.front.get("layout").and_then(|v| v.as_str()) {
            Some(l) => l.trim_end_matches(".html").to_string(),
            None => match it.src.kind {
                Kind::Post => "post".into(),
                Kind::Page => "page".into(),
            },
        }
    }

    fn build_values(&mut self) -> Result<()> {
        let links: Vec<Value> = self.posts.iter().map(|p| self.link_value(p)).collect();
        let n = self.posts.len();
        let values: Vec<Value> = (0..n)
            .map(|i| {
                let newer = if i > 0 { Some(links[i - 1].clone()) } else { None };
                let older = if i + 1 < n { Some(links[i + 1].clone()) } else { None };
                self.item_value(&self.posts[i], "post", older, newer)
            })
            .collect();
        for (p, v) in self.posts.iter_mut().zip(values) {
            p.value = v;
        }
        let pvalues: Vec<Value> = self.pages.iter().map(|p| self.item_value(p, "page", None, None)).collect();
        for (p, v) in self.pages.iter_mut().zip(pvalues) {
            p.value = v;
        }

        // Tags and years.
        let mut tags: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
        let mut years: BTreeMap<i32, Vec<Value>> = BTreeMap::new();
        for p in &self.posts {
            for t in &p.tags {
                tags.entry(slugify(t)).or_insert_with(|| (t.clone(), vec![])).1.push(p.value.clone());
            }
            if let Some((d, _)) = &p.date {
                years.entry(d.year()).or_default().push(p.value.clone());
            }
        }
        let mut tag_list: Vec<(String, String, Vec<Value>)> =
            tags.into_iter().map(|(slug, (name, posts))| (slug, name, posts)).collect();
        tag_list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        let tag_values: Vec<Value> = tag_list
            .into_iter()
            .map(|(slug, name, posts)| {
                let mut m = BTreeMap::new();
                m.insert("name", Value::from(name));
                m.insert("slug", Value::from(slug));
                m.insert("count", Value::from(posts.len()));
                m.insert("posts", Value::from(posts));
                Value::from_serialize(m)
            })
            .collect();
        let year_values: Vec<Value> = years
            .into_iter()
            .rev()
            .map(|(y, posts)| {
                let mut m = BTreeMap::new();
                m.insert("year", Value::from(y));
                m.insert("count", Value::from(posts.len()));
                m.insert("posts", Value::from(posts));
                Value::from_serialize(m)
            })
            .collect();

        let mut site = Self::site_value(&self.cfg, &self.base_path);
        // site.updated: newest post date (deterministic, unlike a build timestamp).
        let updated = self.posts.iter().filter_map(|p| p.date.as_ref().map(|d| d.0)).max();
        let mut sm: BTreeMap<String, Value> = BTreeMap::new();
        if let Ok(iter) = site.try_iter() {
            for k in iter {
                let key = k.to_string();
                sm.insert(key.clone(), site.get_item(&k).unwrap_or_default());
            }
        }
        sm.insert("updated".into(), updated.map(|d| Value::from(d.to_rfc3339())).unwrap_or(Value::from(())));
        site = Value::from_serialize(sm);

        let data = load_data(&self.blog_dir.join("data"), &mut self.warnings)?;
        let g = &mut self.globals;
        g.insert("site".into(), site);
        g.insert("params".into(), Value::from_serialize(&self.cfg.params));
        g.insert("posts".into(), Value::from(self.posts.iter().map(|p| p.value.clone()).collect::<Vec<_>>()));
        g.insert("pages".into(), Value::from(self.pages.iter().map(|p| p.value.clone()).collect::<Vec<_>>()));
        g.insert("tags".into(), Value::from(tag_values));
        g.insert("years".into(), Value::from(year_values));
        g.insert("data".into(), data);
        g.insert("preview".into(), Value::from(self.opts.drafts));
        self.gallery_values = self.gallery_values();
        self.globals.insert("gallery".into(), self.gallery_values.root.clone());
        Ok(())
    }

    fn photo_link(&self, p: &gallery::Photo) -> Value {
        let mut m = BTreeMap::new();
        m.insert("title", Value::from(p.title.clone()));
        m.insert("alt", Value::from(p.alt.clone()));
        m.insert("url", Value::from(prefix_base(&self.base_path, &p.route)));
        m.insert("thumb", Value::from(prefix_base(&self.base_path, &p.thumb_route)));
        Value::from_serialize(m)
    }

    fn gallery_values(&self) -> GalleryValues {
        let url = |r: &str| prefix_base(&self.base_path, r);
        let none = || Value::from(());
        let Some(g) = &self.gallery else {
            let mut m: BTreeMap<&str, Value> = BTreeMap::new();
            m.insert("enabled", Value::from(false));
            m.insert("title", Value::from("Gallery"));
            m.insert("url", Value::from(url(&self.cfg.gallery.url)));
            m.insert("permalink", Value::from(crate::templates::absolute(&self.cfg.base_url, &self.base_path, &url(&self.cfg.gallery.url))));
            m.insert("intro", Value::from_safe_string(String::new()));
            m.insert("meta", Value::from_serialize(serde_json::Map::new()));
            m.insert("count", Value::from(0));
            for k in ["photos", "albums", "tags"] {
                m.insert(k, Value::from(Vec::<Value>::new()));
            }
            return GalleryValues { root: Value::from_serialize(m), ..Default::default() };
        };
        let tag_index = gallery::tag_index(g);
        let tag_url = |name: &str| url(&format!("{}tags/{}/", g.route, slugify(name)));
        let album_link = |i: usize| {
            let a = &g.albums[i];
            let mut m = BTreeMap::new();
            m.insert("title", Value::from(a.title.clone()));
            m.insert("slug", Value::from(a.slug.clone()));
            m.insert("url", Value::from(url(&a.route)));
            Value::from_serialize(m)
        };
        // Neighbours follow the album's order for album photos, else the gallery's order.
        let mut neighbours: Vec<(Option<usize>, Option<usize>)> = vec![(None, None); g.photos.len()];
        let mut seqs: Vec<Vec<usize>> = g.albums.iter().map(|a| a.photos.clone()).collect();
        seqs.push((0..g.photos.len()).filter(|&i| g.photos[i].album.is_none()).collect());
        for seq in &seqs {
            for (k, &i) in seq.iter().enumerate() {
                neighbours[i] = (k.checked_sub(1).map(|k| seq[k]), seq.get(k + 1).copied());
            }
        }
        let photos: Vec<Value> = g
            .photos
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut m: BTreeMap<&str, Value> = BTreeMap::new();
                let page = url(&p.route);
                m.insert("kind", Value::from("photo"));
                m.insert("title", Value::from(p.title.clone()));
                m.insert("alt", Value::from(p.alt.clone()));
                m.insert("slug", Value::from(p.slug.clone()));
                m.insert("url", Value::from(page.clone()));
                m.insert("permalink", Value::from(crate::templates::absolute(&self.cfg.base_url, &self.base_path, &page)));
                m.insert("image", Value::from(url(&p.image_route)));
                m.insert("thumb", Value::from(url(&p.thumb_route)));
                m.insert("width", Value::from(p.info.width));
                m.insert("height", Value::from(p.info.height));
                m.insert("thumb_width", Value::from(p.info.thumb_width));
                m.insert("thumb_height", Value::from(p.info.thumb_height));
                let ratio = if p.info.height == 0 { 1.0 } else { p.info.width as f64 / p.info.height as f64 };
                m.insert("ratio", Value::from((ratio * 10000.0).round() / 10000.0));
                m.insert("caption", Value::from_safe_string(p.caption_html.clone()));
                m.insert("date", Value::from(p.date.to_rfc3339()));
                m.insert("date_source", Value::from_serialize(p.date_source));
                m.insert("year", Value::from(p.date.year()));
                m.insert("month", Value::from(p.date.month()));
                m.insert("day", Value::from(p.date.day()));
                let tags: Vec<Value> = p
                    .tags
                    .iter()
                    .map(|t| {
                        Value::from_serialize(BTreeMap::from([("name", t.clone()), ("slug", slugify(t)), ("url", tag_url(t))]))
                    })
                    .collect();
                m.insert("tags", Value::from(tags));
                m.insert("album", p.album.map(album_link).unwrap_or_else(none));
                m.insert("exif", p.info.exif.as_ref().map(Value::from_serialize).unwrap_or_else(none));
                m.insert("meta", Value::from_serialize(&p.front));
                m.insert("draft", Value::from(p.draft));
                m.insert("source", Value::from(p.source.clone()));
                m.insert("sidecar", p.sidecar.clone().map(Value::from).unwrap_or_else(none));
                let (prev, next) = neighbours[i];
                m.insert("prev", prev.map(|j| self.photo_link(&g.photos[j])).unwrap_or_else(none));
                m.insert("next", next.map(|j| self.photo_link(&g.photos[j])).unwrap_or_else(none));
                Value::from_serialize(m)
            })
            .collect();
        let albums: Vec<Value> = g
            .albums
            .iter()
            .map(|a| {
                let mut m: BTreeMap<&str, Value> = BTreeMap::new();
                let page = url(&a.route);
                m.insert("kind", Value::from("album"));
                m.insert("title", Value::from(a.title.clone()));
                m.insert("slug", Value::from(a.slug.clone()));
                m.insert("url", Value::from(page.clone()));
                m.insert("permalink", Value::from(crate::templates::absolute(&self.cfg.base_url, &self.base_path, &page)));
                m.insert("description", Value::from_safe_string(a.description_html.clone()));
                m.insert("date", a.date.map(|d| Value::from(d.to_rfc3339())).unwrap_or_else(none));
                // `cover: file.jpg` in the album's index.md, else its first photo.
                let cover = a
                    .front
                    .get("cover")
                    .and_then(|v| v.as_str())
                    .and_then(|c| a.photos.iter().find(|&&i| g.photos[i].source.rsplit('/').next() == Some(c.trim())))
                    .or(a.photos.first());
                m.insert("cover", cover.map(|&i| photos[i].clone()).unwrap_or_else(none));
                m.insert("photos", Value::from(a.photos.iter().map(|&i| photos[i].clone()).collect::<Vec<_>>()));
                m.insert("count", Value::from(a.photos.len()));
                let tags: Vec<Value> = a
                    .tags
                    .iter()
                    .map(|t| Value::from_serialize(BTreeMap::from([("name", t.clone()), ("slug", slugify(t)), ("url", tag_url(t))])))
                    .collect();
                m.insert("tags", Value::from(tags));
                m.insert("meta", Value::from_serialize(&a.front));
                m.insert("source", Value::from(a.source.clone()));
                Value::from_serialize(m)
            })
            .collect();
        let tags: Vec<(String, Value)> = tag_index
            .iter()
            .map(|(slug, name, idx)| {
                let route = format!("{}tags/{slug}/", g.route);
                let mut m: BTreeMap<&str, Value> = BTreeMap::new();
                m.insert("kind", Value::from("tag"));
                m.insert("name", Value::from(name.clone()));
                m.insert("slug", Value::from(slug.clone()));
                m.insert("url", Value::from(url(&route)));
                m.insert("count", Value::from(idx.len()));
                m.insert("photos", Value::from(idx.iter().map(|&i| photos[i].clone()).collect::<Vec<_>>()));
                (route, Value::from_serialize(m))
            })
            .collect();
        let mut m: BTreeMap<&str, Value> = BTreeMap::new();
        m.insert("enabled", Value::from(true));
        m.insert("title", Value::from(g.title.clone()));
        m.insert("url", Value::from(url(&g.route)));
        m.insert("permalink", Value::from(crate::templates::absolute(&self.cfg.base_url, &self.base_path, &url(&g.route))));
        m.insert("intro", Value::from_safe_string(g.intro_html.clone()));
        m.insert("meta", Value::from_serialize(&g.front));
        m.insert("count", Value::from(photos.len()));
        m.insert("photos", Value::from(photos.clone()));
        m.insert("albums", Value::from(albums.clone()));
        m.insert("tags", Value::from(tags.iter().map(|t| t.1.clone()).collect::<Vec<_>>()));
        GalleryValues { photos, albums, tags, root: Value::from_serialize(m) }
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > n / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':']))
}

/// data/*.json|toml|yaml -> data.<stem>; subfolders nest.
fn load_data(dir: &Path, warnings: &mut Vec<String>) -> Result<Value> {
    fn walk(dir: &Path, warnings: &mut Vec<String>) -> Result<serde_json::Map<String, Json>> {
        let mut m = serde_json::Map::new();
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if content::is_debris(&name) {
                continue;
            }
            if p.is_dir() {
                m.insert(name, Json::Object(walk(&p, warnings)?));
                continue;
            }
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&p)?;
            let parsed: Result<Json> = match p.extension().and_then(|e| e.to_str()) {
                Some("json") => serde_json::from_str(&text).map_err(Into::into),
                Some("toml") => toml::from_str::<toml::Value>(&text).map(content::toml_to_json).map_err(Into::into),
                Some("yaml" | "yml") => serde_yaml::from_str(&text).map_err(Into::into),
                _ => continue,
            };
            match parsed {
                Ok(v) => {
                    m.insert(stem, v);
                }
                Err(e) => bail!("data/{name}: {e}"),
            }
        }
        Ok(m)
    }
    if !dir.is_dir() {
        return Ok(Value::from_serialize(serde_json::Map::new()));
    }
    Ok(Value::from_serialize(Json::Object(walk(dir, warnings)?)))
}
