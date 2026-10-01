//! Routing and rendering: turns a loaded Site into a set of output files.
//!
//! Every file in theme/ is copied or rendered to a mirrored path, unless a path
//! segment starts with `_` (layouts, partials, macros) or it declares a route header.

use crate::content::{Kind, is_debris};
use crate::feed;
use crate::site::Site;
use crate::templates::{describe_error, prefix_base, template_file};
use crate::util::url_to_file;
use anyhow::{Context, Result, anyhow, bail};
use minijinja::Value;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ROUTE_OPEN: &str = "{#---";
pub const ROUTE_CLOSE: &str = "---#}";

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSpec {
    pub url: Option<String>,
    pub each: Option<String>,
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub paginate: Option<String>,
    pub per_page: Option<usize>,
}

pub enum Content {
    Copy(PathBuf),
    Bytes(Vec<u8>),
    Render { template: String, ctx: BTreeMap<String, Value> },
}

pub struct Output {
    /// Output file path relative to the output root, e.g. "2024/05/hi/index.html".
    pub path: String,
    /// URL path including base path.
    pub url: String,
    /// Human description of what produced this output (for collisions and `data`).
    pub origin: String,
    pub content: Content,
    pub lastmod: Option<String>,
}

pub struct Plan {
    pub outputs: Vec<Output>,
    pub errors: Vec<String>,
}

/// URL for an output file: `a/index.html` is served as `/a/`.
pub fn file_url(path: &str) -> String {
    let p = path.trim_start_matches('/');
    match p.strip_suffix("index.html") {
        Some(dir) if dir.is_empty() || dir.ends_with('/') => format!("/{dir}"),
        _ => format!("/{p}"),
    }
}

pub fn is_template(rel: &str) -> bool {
    rel.ends_with(".jinja") || rel.ends_with(".html") || rel.ends_with(".htm")
}

pub fn is_private(rel: &str) -> bool {
    rel.split('/').any(|seg| seg.starts_with('_'))
}

/// Parse an optional `{#--- toml ---#}` route header at the top of a template.
pub fn parse_route_header(src: &str) -> Result<Option<RouteSpec>> {
    let t = src.trim_start();
    let Some(rest) = t.strip_prefix(ROUTE_OPEN) else { return Ok(None) };
    let end = rest.find(ROUTE_CLOSE).ok_or_else(|| anyhow!("route header opened with {ROUTE_OPEN} but never closed with {ROUTE_CLOSE}"))?;
    let spec: RouteSpec = toml::from_str(&rest[..end]).context("invalid route header")?;
    if spec.per_page == Some(0) {
        bail!("per_page must be at least 1");
    }
    if spec.each.is_some() && spec.url.is_none() {
        bail!("a route header with `each` needs a `url` template");
    }
    Ok(Some(spec))
}

fn normalize_route(u: &str) -> String {
    let u = u.trim();
    if u.starts_with('/') { u.to_string() } else { format!("/{u}") }
}

impl Site {
    pub fn base_ctx(&self, route: &str, template: &str) -> BTreeMap<String, Value> {
        let mut ctx = self.globals.clone();
        ctx.insert("current_url".into(), Value::from(prefix_base(&self.base_path, route)));
        ctx.insert("current_path".into(), Value::from(url_to_file(route)));
        ctx.insert("template".into(), Value::from(template_file(template)));
        ctx
    }

    /// Work out every output without rendering anything.
    pub fn plan(&self) -> Result<Plan> {
        let mut outputs = vec![];
        let mut errors = vec![];

        // Posts and pages: the built-in case of "one template, many pages".
        for it in self.posts.iter().chain(self.pages.iter()) {
            let template = match (&it.src.body, it.src.kind) {
                (crate::content::Body::Html(_), Kind::Page) => {
                    let rel = it.src.rel.strip_prefix("pages").unwrap_or(&it.src.rel);
                    format!("{}{}", crate::templates::PAGE_TEMPLATE_PREFIX, rel.to_string_lossy().replace('\\', "/"))
                }
                _ => format!("_layouts/{}.html", self.layout_of(it)),
            };
            if !template.starts_with('@') && !self.theme_dir.join(&template).is_file() {
                errors.push(format!(
                    "{}: layout {:?} not found (expected theme/{template})",
                    it.src.rel.display(),
                    self.layout_of(it)
                ));
                continue;
            }
            let mut ctx = self.base_ctx(&it.route, &template);
            ctx.insert("item".into(), it.value.clone());
            ctx.insert(if it.src.kind == Kind::Post { "post" } else { "page" }.into(), it.value.clone());
            let lastmod = it
                .value
                .get_attr("updated")
                .ok()
                .filter(|v| !v.is_none())
                .or_else(|| it.value.get_attr("date").ok().filter(|v| !v.is_none()))
                .map(|v| v.to_string());
            outputs.push(Output {
                path: url_to_file(&it.route),
                url: prefix_base(&self.base_path, &it.route),
                origin: format!("{} (via {})", it.src.rel.display(), template_file(&template)),
                content: Content::Render { template, ctx },
                lastmod,
            });
            for (abs, out) in &it.files {
                outputs.push(Output {
                    path: out.trim_start_matches('/').to_string(),
                    url: prefix_base(&self.base_path, out),
                    origin: abs.strip_prefix(&self.content_root).unwrap_or(abs).display().to_string(),
                    content: Content::Copy(abs.clone()),
                    lastmod: None,
                });
            }
        }
        for (abs, out) in &self.loose_files {
            outputs.push(Output {
                path: out.trim_start_matches('/').to_string(),
                url: prefix_base(&self.base_path, out),
                origin: abs.strip_prefix(&self.content_root).unwrap_or(abs).display().to_string(),
                content: Content::Copy(abs.clone()),
                lastmod: None,
            });
        }

        if let Some(g) = &self.gallery {
            self.plan_gallery(g, &mut outputs, &mut errors);
        }

        // static/: copied untouched.
        let static_dir = self.blog_dir.join("static");
        for (abs, rel) in walk_files(&static_dir)? {
            outputs.push(Output {
                url: prefix_base(&self.base_path, &file_url(&rel)),
                path: rel.clone(),
                origin: format!("static/{rel}"),
                content: Content::Copy(abs),
                lastmod: None,
            });
        }

        // theme/: copied or rendered.
        for (abs, rel) in walk_files(&self.theme_dir)? {
            if is_private(&rel) {
                continue;
            }
            if !is_template(&rel) {
                outputs.push(Output {
                    url: prefix_base(&self.base_path, &file_url(&rel)),
                    path: rel.clone(),
                    origin: format!("theme/{rel}"),
                    content: Content::Copy(abs),
                    lastmod: None,
                });
                continue;
            }
            let src = std::fs::read_to_string(&abs).with_context(|| format!("reading theme/{rel}"))?;
            match parse_route_header(&src) {
                Err(e) => errors.push(format!("theme/{rel}: {e:#}")),
                Ok(spec) => match self.expand_route(&rel, spec.unwrap_or_default()) {
                    Ok(mut outs) => outputs.append(&mut outs),
                    Err(e) => errors.push(format!("theme/{rel}: {e:#}")),
                },
            }
        }

        // Engine-owned outputs.
        outputs.push(Output {
            path: feed::FEED_PATH.into(),
            url: prefix_base(&self.base_path, &format!("/{}", feed::FEED_PATH)),
            origin: "engine: RSS feed".into(),
            content: Content::Bytes(feed::rss(self).into_bytes()),
            lastmod: None,
        });
        let mut entries: Vec<(String, Option<String>)> = outputs
            .iter()
            .filter(|o| o.path.ends_with(".html") && o.path != "404.html" && !o.path.ends_with("/404.html"))
            .filter(|o| !matches!(&o.content, Content::Render { ctx, .. } if is_draft(ctx)))
            .map(|o| (o.url.clone(), o.lastmod.clone()))
            .collect();
        entries.sort();
        entries.dedup();
        outputs.push(Output {
            path: feed::SITEMAP_PATH.into(),
            url: prefix_base(&self.base_path, &format!("/{}", feed::SITEMAP_PATH)),
            origin: "engine: sitemap".into(),
            content: Content::Bytes(feed::sitemap(self, &entries).into_bytes()),
            lastmod: None,
        });

        // Two outputs on one path is an error, naming both producers.
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for o in &outputs {
            if o.path.split('/').any(|s| s == "..") || o.path.is_empty() {
                errors.push(format!("{}: invalid output path {:?}", o.origin, o.path));
                continue;
            }
            if let Some(prev) = seen.insert(o.path.as_str(), o.origin.as_str()) {
                errors.push(format!("path collision: /{} is produced by both {prev} and {}", o.path, o.origin));
            }
        }
        Ok(Plan { outputs, errors })
    }

    fn plan_gallery(&self, g: &crate::gallery::Gallery, outputs: &mut Vec<Output>, errors: &mut Vec<String>) {
        let missing: Vec<&str> = crate::gallery::TEMPLATES.iter().copied().filter(|t| !self.theme_dir.join(t).is_file()).collect();
        if !missing.is_empty() {
            errors.push(format!(
                "the gallery is on but theme/{} {} missing; run `folderblog gallery on` to add the gallery templates",
                missing.join(", theme/"),
                if missing.len() == 1 { "is" } else { "are" }
            ));
            return;
        }
        let gv = &self.gallery_values;
        let page = |route: &str, template: &str, name: &str, value: &Value, origin: String, lastmod: Option<String>| {
            let mut ctx = self.base_ctx(route, template);
            ctx.insert("item".into(), value.clone());
            ctx.insert(name.into(), value.clone());
            Output {
                path: url_to_file(route),
                url: prefix_base(&self.base_path, route),
                origin,
                content: Content::Render { template: template.into(), ctx },
                lastmod,
            }
        };
        let src = crate::gallery::GALLERY_DIR;
        outputs.push(page(&g.route, "_gallery/index.html", "gallery", &gv.root, format!("{src}/ (via theme/_gallery/index.html)"), None));
        for (a, v) in g.albums.iter().zip(&gv.albums) {
            outputs.push(page(&a.route, "_gallery/album.html", "album", v, format!("{} (via theme/_gallery/album.html)", a.source), None));
        }
        for (route, v) in &gv.tags {
            outputs.push(page(route, "_gallery/tag.html", "tag", v, format!("{src}/ tag (via theme/_gallery/tag.html)"), None));
        }
        for (p, v) in g.photos.iter().zip(&gv.photos) {
            let origin = format!("{} (via theme/_gallery/photo.html)", p.source);
            outputs.push(page(&p.route, "_gallery/photo.html", "photo", v, origin, Some(p.date.to_rfc3339())));
            let mut files = vec![(&p.image_route, &p.image_file)];
            if p.thumb_route != p.image_route {
                files.push((&p.thumb_route, &p.thumb_file));
            }
            for (route, file) in files {
                outputs.push(Output {
                    path: route.trim_start_matches('/').to_string(),
                    url: prefix_base(&self.base_path, route),
                    origin: p.source.clone(),
                    content: Content::Copy(file.clone()),
                    lastmod: None,
                });
            }
        }
    }

    fn expand_route(&self, rel: &str, spec: RouteSpec) -> Result<Vec<Output>> {
        let out_name = rel.strip_suffix(".jinja").unwrap_or(rel).to_string();
        let origin = format!("theme/{rel}");
        let globals_ctx = self.base_ctx("/", rel);

        let items: Vec<Option<Value>> = match &spec.each {
            Some(expr) => {
                let e = self.env.compile_expression(expr).map_err(|e| anyhow!("each = {expr:?}: {}", describe_error(&e)))?;
                let v = e.eval(&globals_ctx).map_err(|e| anyhow!("each = {expr:?}: {}", describe_error(&e)))?;
                let iter = v.try_iter().map_err(|_| anyhow!("each = {expr:?} did not produce a list (got {})", v.kind()))?;
                iter.map(Some).collect()
            }
            None => vec![None],
        };
        let alias = spec.alias.clone().unwrap_or_else(|| "item".into());

        let mut outs = vec![];
        for item in items {
            let mut ctx = globals_ctx.clone();
            if let Some(it) = &item {
                ctx.insert("item".into(), it.clone());
                ctx.insert(alias.clone(), it.clone());
            }
            let render_url = |number: Option<usize>| -> Result<String> {
                let Some(u) = &spec.url else { return Ok(file_url(&out_name)) };
                let mut c = ctx.clone();
                if let Some(n) = number {
                    c.insert("pager".into(), Value::from_serialize(BTreeMap::from([("number", n)])));
                }
                let s = self.env.render_str(u, &c).map_err(|e| anyhow!("url = {u:?}: {}", describe_error(&e)))?;
                Ok(normalize_route(&s))
            };
            match &spec.paginate {
                None => {
                    let route = render_url(None)?;
                    let mut c = ctx.clone();
                    c.insert("current_url".into(), Value::from(prefix_base(&self.base_path, &route)));
                    c.insert("current_path".into(), Value::from(url_to_file(&route)));
                    outs.push(Output {
                        path: url_to_file(&route),
                        url: prefix_base(&self.base_path, &route),
                        origin: origin.clone(),
                        content: Content::Render { template: rel.to_string(), ctx: c },
                        lastmod: None,
                    });
                }
                Some(expr) => {
                    let e = self.env.compile_expression(expr).map_err(|e| anyhow!("paginate = {expr:?}: {}", describe_error(&e)))?;
                    let v = e.eval(&ctx).map_err(|e| anyhow!("paginate = {expr:?}: {}", describe_error(&e)))?;
                    let list: Vec<Value> = v
                        .try_iter()
                        .map_err(|_| anyhow!("paginate = {expr:?} did not produce a list (got {})", v.kind()))?
                        .collect();
                    let per = spec.per_page.unwrap_or(10);
                    let total = list.len().div_ceil(per).max(1);
                    let routes: Vec<String> = (1..=total).map(|n| render_url(Some(n))).collect::<Result<_>>()?;
                    if spec.url.is_some() && total > 1 && routes[0] == routes[1] {
                        bail!("paginated url template must give page 2+ a different URL (use pager.number)");
                    }
                    let urls: Vec<String> = routes.iter().map(|r| prefix_base(&self.base_path, r)).collect();
                    for n in 1..=total {
                        let chunk: Vec<Value> = list.iter().skip((n - 1) * per).take(per).cloned().collect();
                        let url = |i: usize| urls.get(i).cloned().map(Value::from).unwrap_or(Value::from(()));
                        let pager = BTreeMap::from([
                            ("number", Value::from(n)),
                            ("total", Value::from(total)),
                            ("per_page", Value::from(per)),
                            ("total_items", Value::from(list.len())),
                            ("items", Value::from(chunk)),
                            ("prev", if n > 1 { url(n - 2) } else { Value::from(()) }),
                            ("next", url(n)),
                            ("first", url(0)),
                            ("last", url(total - 1)),
                            ("urls", Value::from(urls.clone())),
                        ]);
                        let route = &routes[n - 1];
                        let mut c = ctx.clone();
                        c.insert("pager".into(), Value::from_serialize(pager));
                        c.insert("current_url".into(), Value::from(prefix_base(&self.base_path, route)));
                        c.insert("current_path".into(), Value::from(url_to_file(route)));
                        outs.push(Output {
                            path: url_to_file(route),
                            url: prefix_base(&self.base_path, route),
                            origin: if total > 1 { format!("{origin} (page {n})") } else { origin.clone() },
                            content: Content::Render { template: rel.to_string(), ctx: c },
                            lastmod: None,
                        });
                    }
                }
            }
        }
        Ok(outs)
    }

    pub fn render_output(&self, o: &Output) -> Result<Vec<u8>, String> {
        match &o.content {
            Content::Bytes(b) => Ok(b.clone()),
            Content::Copy(p) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
            Content::Render { template, ctx } => {
                let t = self.env.get_template(template).map_err(|e| describe_error(&e))?;
                t.render(ctx).map(|s| s.into_bytes()).map_err(|e| describe_error(&e))
            }
        }
    }

    /// Render the plan into `dir` (which must not exist). Fails if any output fails.
    pub fn write_to(&self, plan: &Plan, dir: &Path) -> Result<usize> {
        let mut errors = plan.errors.clone();
        std::fs::create_dir_all(dir)?;
        let mut n = 0;
        for o in &plan.outputs {
            match self.render_output(o) {
                Ok(bytes) => {
                    let target = dir.join(&o.path);
                    if let Some(p) = target.parent() {
                        if let Err(e) = std::fs::create_dir_all(p) {
                            errors.push(format!("/{}: {e}", o.path));
                            continue;
                        }
                    }
                    if let Err(e) = std::fs::write(&target, bytes) {
                        errors.push(format!("/{} ({}): {e}", o.path, o.origin));
                        continue;
                    }
                    n += 1;
                }
                Err(e) => errors.push(format!("rendering {} from {}:\n  {e}", o.url, o.origin)),
            }
        }
        if !errors.is_empty() {
            bail!(BuildErrors(errors));
        }
        Ok(n)
    }
}

fn is_draft(ctx: &BTreeMap<String, Value>) -> bool {
    ctx.get("item").and_then(|i| i.get_attr("draft").ok()).map(|v| v.is_true()).unwrap_or(false)
}

#[derive(Debug)]
pub struct BuildErrors(pub Vec<String>);

impl std::fmt::Display for BuildErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} error(s):\n{}", self.0.len(), self.0.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"))
    }
}
impl std::error::Error for BuildErrors {}

/// Dotfiles that sites legitimately publish.
pub const KEEP_DOTFILES: &[&str] = &[".well-known", ".htaccess", ".nojekyll"];

/// All non-debris files under `dir`, as (absolute, forward-slash relative) pairs.
pub fn walk_files(dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut out = vec![];
    if !dir.is_dir() {
        return Ok(out);
    }
    for e in walkdir::WalkDir::new(dir).sort_by_file_name().into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        e.depth() == 0 || KEEP_DOTFILES.contains(&name.as_ref()) || !is_debris(&name)
    }) {
        let e = e?;
        if e.file_type().is_file() || (e.file_type().is_symlink() && e.path().is_file()) {
            let rel = e.path().strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
            out.push((e.path().to_path_buf(), rel));
        }
    }
    Ok(out)
}
