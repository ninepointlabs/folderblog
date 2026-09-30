//! The theme contract: the single list of what templates can rely on. AGENTS.md is
//! generated from these tables, and tests check each entry against the engine.

pub struct Field {
    pub name: &'static str,
    pub ty: &'static str,
    pub doc: &'static str,
}

const fn f(name: &'static str, ty: &'static str, doc: &'static str) -> Field {
    Field { name, ty, doc }
}

pub const GLOBALS: &[Field] = &[
    f("site", "object", "Site settings from blog.toml; see **site** below."),
    f("params", "object", "The free-form `[params]` table from blog.toml (same as `site.params`)."),
    f("posts", "list of item", "All published posts, newest first. In `preview` this also includes drafts (with `draft = true`)."),
    f("pages", "list of item", "All pages, sorted by front matter `weight` (default 0) then title."),
    f("tags", "list of tag", "Every tag used by a post, sorted by name."),
    f("years", "list of year", "Posts grouped by year, newest year first."),
    f("data", "object", "Parsed files from `data/`: `data/links.toml` is `data.links`; subfolders nest (`data/a/b.json` is `data.a.b`)."),
    f("preview", "bool", "True in `folderblog preview` (drafts visible), false in real builds."),
    f("current_url", "string", "URL of the file being rendered, e.g. `/2024/05/hello/` (includes the base path)."),
    f("current_path", "string", "Output file being rendered, e.g. `2024/05/hello/index.html`."),
    f("template", "string", "The template file producing this output, e.g. `theme/_layouts/post.html`."),
    f("item", "item or any", "Posts/pages: the item being rendered. Routed templates with `each`: the current element."),
    f("post", "item", "Set when rendering a post (same value as `item`)."),
    f("page", "item", "Set when rendering a page (same value as `item`)."),
    f("pager", "pager", "Set in templates whose route header has `paginate`; see **pager** below."),
];

pub const SITE_FIELDS: &[Field] = &[
    f("title", "string", "`title` from blog.toml."),
    f("description", "string", "`description` from blog.toml (may be empty)."),
    f("author", "string", "`author` from blog.toml (may be empty)."),
    f("language", "string", "`language` from blog.toml, e.g. `en`. Use it for `<html lang>`."),
    f("base_url", "string", "Absolute site URL without trailing slash, e.g. `https://example.com`."),
    f("base_path", "string", "Path part of base_url (`\"\"` for a site at the domain root, `/blog` for a subfolder site)."),
    f("url", "string", "URL of the home page (`/`, or `/blog/` for a subfolder site)."),
    f("feed_url", "string", "URL of the engine's RSS feed (`/feed.xml`)."),
    f("feed_url_absolute", "string", "Absolute URL of the RSS feed."),
    f("sitemap_url", "string", "URL of the engine's sitemap (`/sitemap.xml`)."),
    f("params", "object", "The `[params]` table from blog.toml."),
    f("updated", "date or none", "Date of the newest post. Deterministic, so output only changes when content does."),
    f("generator", "string", "`folderblog <version>`."),
];

pub const ITEM_FIELDS: &[Field] = &[
    f("kind", "string", "`\"post\"` or `\"page\"`."),
    f("title", "string", "Front matter `title`, else the leading heading (removed from `content`), else the filename."),
    f("slug", "string", "From the filename (date prefix removed) or bundle folder name; front matter `slug` overrides."),
    f("url", "string", "Site URL of the item, e.g. `/2024/05/hello/`. Use this in links."),
    f("permalink", "string", "Absolute URL, e.g. `https://example.com/2024/05/hello/`."),
    f("date", "date or none", "RFC 3339 string. Posts always have one; pages only if front matter sets `date`."),
    f("date_source", "string or none", "`front_matter`, `filename`, or `first_seen` (when the engine first saw the post)."),
    f("year", "int or none", "Year of `date`."),
    f("month", "int or none", "Month of `date` (1-12)."),
    f("day", "int or none", "Day of `date`."),
    f("updated", "date or none", "Front matter `updated` (or `modified`/`lastmod`), if set."),
    f("content", "html", "Rendered body HTML (already safe; do not escape)."),
    f("excerpt", "html", "Front matter `excerpt`/`summary` rendered, else everything before `<!--more-->`, else the first paragraph."),
    f("summary", "string", "Plain-text version of `excerpt`, at most ~300 characters."),
    f("has_more", "bool", "True if the body contains a `<!--more-->` marker."),
    f("raw", "string", "The Markdown (or HTML page) source without front matter."),
    f("text", "string", "Plain text of the whole body (for search indexes)."),
    f("word_count", "int", "Words in `text`."),
    f("reading_time", "int", "Minutes at 220 words per minute, at least 1."),
    f("tags", "list of {name, slug}", "From front matter `tags` (a list or a comma-separated string)."),
    f("headings", "list of {level, id, text}", "Every heading in the body, in order, with its anchor id (for a table of contents)."),
    f("meta", "object", "All front matter exactly as written, including keys the engine does not know about."),
    f("draft", "bool", "True for files in `drafts/` or with `draft: true` (only ever visible in preview)."),
    f("layout", "string", "Layout name: front matter `layout`, else `post` or `page`. Renders `theme/_layouts/<layout>.html`."),
    f("source", "string", "Source file relative to the blog folder, e.g. `posts/hello.md`."),
    f("older", "link or none", "Posts only: the next older post as {title, url, slug, date}."),
    f("newer", "link or none", "Posts only: the next newer post as {title, url, slug, date}."),
    f("files", "list of file", "Files beside a bundle's `index.md`, as {name, url, ext, is_image}. Empty for single-file items."),
];

pub const TAG_FIELDS: &[Field] = &[
    f("name", "string", "Tag as first written."),
    f("slug", "string", "URL-safe form. The theme decides tag page URLs, e.g. `/tags/{{ tag.slug }}/`."),
    f("count", "int", "Number of posts."),
    f("posts", "list of item", "Posts with this tag, newest first."),
];

pub const YEAR_FIELDS: &[Field] = &[
    f("year", "int", "The year."),
    f("count", "int", "Number of posts."),
    f("posts", "list of item", "Posts from that year, newest first."),
];

pub const PAGER_FIELDS: &[Field] = &[
    f("number", "int", "Current page number, starting at 1."),
    f("total", "int", "Number of pages (at least 1, even for an empty list)."),
    f("per_page", "int", "Items per page."),
    f("total_items", "int", "Length of the whole list."),
    f("items", "list", "The items on this page."),
    f("prev", "string or none", "URL of the previous page."),
    f("next", "string or none", "URL of the next page."),
    f("first", "string", "URL of page 1."),
    f("last", "string", "URL of the last page."),
    f("urls", "list of string", "URLs of every page, in order."),
];

pub const ROUTE_KEYS: &[Field] = &[
    f("url", "template string", "Output URL for each rendered page. Ends in `/` for `<url>/index.html`. May use `item`, the `as` name, `pager.number`, and all globals."),
    f("each", "expression", "Render once per element of this Jinja expression, e.g. `tags` or `posts | selectattr(\"meta.series\") | groupby(\"meta.series\")`."),
    f("as", "name", "Extra variable name for the current element of `each` (it is always also `item`)."),
    f("paginate", "expression", "Split this list into pages; each page gets `pager`. Can be combined with `each` (paginate per tag)."),
    f("per_page", "int", "Items per page for `paginate` (default 10)."),
];

pub const MARKUP: &[(&str, &str)] = &[
    ("_markup/image.html", "`url`, `alt`, `title` — replaces `<img>` for Markdown images"),
    ("_markup/link.html", "`url`, `title`, `text`, `html` (inner markup, safe), `external` (bool) — replaces `<a>`"),
    ("_markup/heading.html", "`level`, `id`, `text`, `html` (inner markup, safe), `classes` — replaces `<hN>`"),
    ("_markup/codeblock.html", "`lang`, `code` (highlighted HTML, safe), `raw` (plain source) — replaces `<pre><code>`"),
];

/// Filters registered by folderblog itself, plus the extra ones from minijinja-contrib.
pub const FILTERS: &[(&str, &str)] = &[
    ("date", "`{{ post.date | date(\"%B %-d, %Y\") }}` — strftime format (default `%Y-%m-%d`). Accepts RFC 3339 or `YYYY-MM-DD` strings, or `\"now\"`."),
    ("url", "`{{ \"/assets/site.css\" | url }}` — prefix the site's base path to a site-absolute path. Use for every hand-written link so the theme works in a subfolder site."),
    ("absolute_url", "`{{ post.url | absolute_url }}` — full URL with scheme and host."),
    ("slugify", "`{{ \"Hello World\" | slugify }}` → `hello-world`."),
    ("markdown", "Render a Markdown string (e.g. a front matter field or data value) to HTML."),
    ("truncate_words", "`{{ post.summary | truncate_words(30) }}` — strip tags and cut to N words with an ellipsis."),
    ("plaintext", "Strip HTML tags and collapse whitespace."),
    ("striptags", "Strip HTML tags."),
    ("truncate", "Jinja `truncate(length=255, killwords=false, end=\"...\")`."),
    ("pluralize", "`{{ n }} post{{ n | pluralize }}`."),
    ("filesizeformat", "Human-readable byte size."),
];

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("now", "`now()` — current time as an RFC 3339 string. Avoid in real output: it changes every build and forces a redeploy."),
    ("range", "Jinja `range(n)`."),
    ("dict", "Jinja `dict(a=1)`."),
    ("namespace", "Jinja `namespace()` for values that change inside loops."),
    ("cycler", "Jinja `cycler(\"odd\", \"even\")`."),
    ("joiner", "Jinja `joiner(\", \")`."),
];

/// Built-in minijinja filters worth knowing, checked in tests like the rest.
pub const BUILTIN_FILTERS: &[&str] = &[
    "abs", "attr", "batch", "capitalize", "default", "dictsort", "escape", "first", "float", "groupby", "indent",
    "int", "items", "join", "last", "length", "list", "lower", "map", "max", "min", "reject", "rejectattr",
    "replace", "reverse", "round", "safe", "select", "selectattr", "slice", "sort", "split", "title", "tojson", "trim",
    "unique", "upper", "urlencode", "sum", "chain", "zip", "lines",
];

pub const CONFIG_KEYS: &[Field] = &[
    f("title", "string", "Site title (required)."),
    f("base_url", "string", "Absolute URL the site is served from (required). May include a path for subfolder sites."),
    f("description", "string", "Used by the feed and available to templates."),
    f("author", "string", "Used by the feed and available to templates."),
    f("language", "string", "Default `en`."),
    f("permalink", "string", "Post URL pattern; tokens `{year}` `{month}` `{day}` `{slug}`. Default `/{year}/{month}/{slug}/`."),
    f("feed.limit", "int", "Number of posts in `/feed.xml` (default 20)."),
    f("build.pre", "list of commands", "Shell commands run before a build (working directory: the blog folder)."),
    f("build.post", "list of commands", "Shell commands run after rendering, with `$FOLDERBLOG_OUT` set to the fresh output directory; they may add or change files there."),
    f("watch.ignore", "list of paths", "Paths (relative to the blog folder, prefix match) whose changes never trigger a rebuild."),
    f("deploy.type", "string", "`git` or `command`."),
    f("deploy.remote", "string", "git: remote URL or path."),
    f("deploy.branch", "string", "git: branch to push (default `gh-pages`)."),
    f("deploy.nojekyll", "bool", "git: add an empty `.nojekyll` to the pushed tree (default true)."),
    f("deploy.author_name", "string", "git: commit author name (default `folderblog`)."),
    f("deploy.author_email", "string", "git: commit author email."),
    f("deploy.command", "string", "command: shell command; `$FOLDERBLOG_OUT` is the directory to publish."),
    f("params", "table", "Anything you like; exposed to templates as `site.params` and `params`."),
];

pub const BEGIN: &str = "<!-- folderblog:contract:begin -->";
pub const END: &str = "<!-- folderblog:contract:end -->";

fn table(fields: &[Field]) -> String {
    let mut s = String::from("| name | type | description |\n|---|---|---|\n");
    for x in fields {
        s.push_str(&format!("| `{}` | {} | {} |\n", x.name, x.ty, x.doc.replace('|', "\\|")));
    }
    s
}

fn pairs(v: &[(&str, &str)]) -> String {
    v.iter().map(|(n, d)| format!("- `{n}` — {}\n", d)).collect()
}

/// The generated contract section of AGENTS.md.
pub fn contract_section() -> String {
    let commands: String = crate::cli::command_docs().iter().map(|(n, d)| format!("- `folderblog {n}` — {d}\n")).collect();
    format!(
        r#"{BEGIN}
<!-- Generated by folderblog {version}. Do not edit between these markers; run `folderblog agents-md --write` to refresh. Notes outside the markers are kept. -->

# Designing this blog

This folder is a complete blog for **folderblog**. Posts are Markdown files; the
engine turns them into a data model; **`theme/` decides every byte of the site**.
You can change anything in `theme/` without touching the engine. There is no
hidden base theme: if a file is not in `theme/`, it does not exist.

## The only rules the engine enforces

1. The engine owns: content discovery, Markdown rendering, post/page URLs,
   `/feed.xml` (RSS, full content, absolute URLs) and `/sitemap.xml`.
   A theme cannot produce those two paths; link to them with `site.feed_url`.
2. The engine never adds markup, classes, styles or scripts to theme output.
   (Only `folderblog preview` injects a live-reload `<script>`, and only while serving.)
3. Two outputs on one path is a build error that names both producers.
4. Never edit `posts/`, `pages/` or `drafts/` to make a design work. Those are the
   author's writing. Use `folderblog preview --fixtures stress` to test edge cases instead.

## How `theme/` becomes the site

- Any path with a segment starting with `_` (`_layouts/`, `_partials/`, `_macros/`,
  `_markup/`) is never output. Use it for templates you extend, include or import.
- Files ending in `.html` / `.htm` are **templates**, rendered to the same path
  (`theme/about.html` → `/about.html`, `theme/index.html` → `/`).
- Files ending in `.jinja` are templates rendered with the suffix removed
  (`theme/search.json.jinja` → `/search.json`, `theme/style.css.jinja` → `/style.css`,
  `theme/logo.svg.jinja` → `/logo.svg`).
- Every other file is copied byte for byte (`theme/assets/site.css` → `/assets/site.css`).
  Dotfiles are skipped except `.well-known/`, `.htaccess` and `.nojekyll`.
- Templates are Jinja (minijinja): `extends`, `block`, `include`, `import`/`macro`,
  `set`, `for … else`, `if`, `filter`, `with`, loop variables. Template names are paths
  relative to `theme/`: `{{% extends "_layouts/base.html" %}}`.
  Python-style string/list/dict methods work: `.startswith()`, `.split()`, `.items()`, `.get()`.
- Autoescaping is on for outputs ending in `.html`, `.htm`, `.xml`, `.svg`, `.rss`, `.atom`,
  and off for everything else (JSON, CSS, JS, text). In JSON use `| tojson`.
- Undefined values are lenient: `{{{{ post.meta.cover.url }}}}` is empty if `cover` is missing,
  and `{{% if post.meta.cover %}}` is false. `none` also prints as nothing. `folderblog check` warns when a template
  prints an undefined value, which is usually a typo.

### Posts and pages

- Each post renders `theme/_layouts/<layout>.html` at `post.url`; `layout` defaults to
  `post`. Pages default to `page`. Front matter `layout: photo` uses `_layouts/photo.html`,
  so one post can look unrelated to the next. A missing layout is a build error.
- In a layout, the item is `post` (or `page`) and also `item`.
- An `.html` file in `pages/` is itself a template, rendered with the full context at
  its page URL (it may `{{% extends %}}` theme templates). Use it for one-off designed pages.

### Route headers: one template, many pages

A template may start with a route header, a Jinja comment containing TOML:

```jinja
{{#---
each = "tags"
as = "tag"
url = "/tags/{{{{ tag.slug }}}}/"
---#}}
{{% extends "_layouts/base.html" %}}
{{% block main %}}<h1>{{{{ tag.name }}}}</h1>…{{% endblock %}}
```

{route_keys}
Without a header, a template renders once at its mirrored path. With only `url`, it
renders once at that URL (`theme/archive.html` with `url = "/archive/"` → `/archive/`).
With `paginate` and no `each`, page 1 is at `url` rendered with `pager.number = 1`:

```jinja
{{#---
paginate = "posts"
per_page = 10
url = "{{% if pager.number == 1 %}}/{{% else %}}/page/{{{{ pager.number }}}}/{{% endif %}}"
---#}}
```

`each` accepts any expression over the context, so a theme can make collections the
engine has never heard of:

- per tag: `each = "tags"`
- per year: `each = "years"`, `url = "/{{{{ item.year }}}}/"`
- per series (front matter `series:`): `each = "posts | selectattr('meta.series') | groupby('meta.series')"`,
  then `item.grouper` is the series name and `item.list` its posts
- per data record: `each = "data.projects"`
- paginated per tag: `each = "tags"` plus `paginate = "item.posts"`, with a `url` that
  uses both `item.slug` and `pager.number`

### Controlling Markdown output

Optional templates override how body Markdown renders. Each one receives only the
variables listed plus `site`; if a file is absent the engine's plain HTML is used.

{markup}
Code blocks without an override render as
`<pre class="highlight"><code class="language-LANG">` with `<span class="hl-…">` tokens
(classes only, never inline colours; the class names are TextMate scope atoms prefixed
with `hl-`, e.g. `hl-keyword`, `hl-string`, `hl-comment`, `hl-entity`, `hl-constant`,
`hl-storage`, `hl-support`, `hl-variable`, `hl-punctuation`). Headings get unique `id`s.
Raw HTML in Markdown is passed through. Footnotes, tables, task lists and
strikethrough are enabled. Relative image/link paths in a post are rewritten to site URLs.

## Template context

Every template sees these globals:

{globals}
### site

{site}
### item (every post and page)

{item}
### tag

{tag}
### year

{year}
### pager

{pager}
### Filters

folderblog filters:

{filters}
Also all standard Jinja filters, including: {builtin}.

### Functions

{functions}
Dates are RFC 3339 strings; format them with `| date(...)`. Sort with
`posts | sort(attribute="title")`, filter with `posts | selectattr("meta.featured")`,
group with `posts | groupby("year")`.

## Content the templates will receive

- `posts/*.md` or `posts/<slug>/index.md` (a bundle: images beside it are published with
  it and listed in `item.files`). Subfolders without `index.md` are just organisation.
- `pages/about.md` → `/about/`, `pages/projects/x.md` → `/projects/x/`, `pages/index.md` → `/`.
- `drafts/` — only in `preview`.
- `static/` — copied to the site root untouched (favicons, CNAME, downloads).
- `data/` — JSON/TOML/YAML exposed as `data`.
- Front matter is optional (YAML between `---` or TOML between `+++`). Known keys:
  `title`, `date`, `tags`, `slug`, `layout`, `draft`, `excerpt`/`summary`, `updated`,
  `url` (override the output URL), `weight` (page order). Everything else passes through
  to `item.meta`, so a theme can invent its own keys (`cover`, `subtitle`, `color`…).

## blog.toml

{config}
## Commands (run from the blog folder, or pass its path)

{commands}
## Checking your work

1. `folderblog check` — renders everything, reports template errors as `file:line`,
   validates the feed, and flags broken internal links and path collisions. Exit code 1
   on errors. Also run `folderblog check --fixtures stress` and `--fixtures empty`.
2. `folderblog preview` — http://127.0.0.1:4000 with live reload and drafts.
   `folderblog preview --fixtures stress` renders your theme against stress content:
   very long titles, no-title posts, images, bundles, code, tables, footnotes, raw HTML,
   deep tag lists, many years. Fixture sets: {fixtures}.
3. `folderblog data /2024/05/hello/` — prints the exact template context for that URL as
   JSON; `folderblog data` lists every output URL and the template that renders it.
4. `folderblog build --out /tmp/site` — a full build into any folder, to inspect files.

Toolchains (Tailwind, bundlers) are optional and never required. If you want one, add
commands to `[build] pre/post` in blog.toml, not to the theme. A post-build command can
write into `$FOLDERBLOG_OUT`, e.g. `npx tailwindcss -i theme/_src/in.css -o "$FOLDERBLOG_OUT/assets/site.css" --minify`.
Pre-build commands must not write into watched folders (it would retrigger builds) unless
that path is listed in `watch.ignore`.
{END}
"#,
        version = env!("CARGO_PKG_VERSION"),
        route_keys = table(ROUTE_KEYS),
        markup = MARKUP.iter().map(|(n, d)| format!("- `theme/{n}` — {d}\n")).collect::<String>(),
        globals = table(GLOBALS),
        site = table(SITE_FIELDS),
        item = table(ITEM_FIELDS),
        tag = table(TAG_FIELDS),
        year = table(YEAR_FIELDS),
        pager = table(PAGER_FIELDS),
        filters = pairs(FILTERS),
        builtin = BUILTIN_FILTERS.iter().map(|b| format!("`{b}`")).collect::<Vec<_>>().join(", "),
        functions = pairs(FUNCTIONS),
        config = table(CONFIG_KEYS),
        commands = commands,
        fixtures = crate::fixtures::names().iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", "),
    )
}

/// Replace (or add) the generated section, keeping anything outside the markers.
pub fn update(existing: Option<&str>) -> String {
    let section = contract_section();
    match existing {
        Some(text) => match (text.find(BEGIN), text.find(END)) {
            (Some(b), Some(e)) if e > b => {
                let after = &text[e + END.len()..];
                let after = after.strip_prefix('\n').unwrap_or(after);
                format!("{}{}{}", &text[..b], section, after)
            }
            _ => format!("{section}\n{text}"),
        },
        None => section,
    }
}

pub fn is_current(text: &str) -> bool {
    update(Some(text)) == text
}
