//! Built-in stress content for testing themes (`--fixtures NAME`).

use include_dir::{Dir, include_dir};

pub static FIXTURES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/fixtures");
pub static DEFAULT_THEME: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes/default");
/// Sample post and episode with every front matter field, copied into a blog's `templates/`.
pub static BLOG_TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/scaffold/templates");

/// The sample templates as (path relative to the blog, e.g. `templates/post.md`, contents).
pub fn template_files() -> Vec<(String, &'static [u8])> {
    fn walk(d: &'static Dir<'static>, out: &mut Vec<(String, &'static [u8])>) {
        out.extend(d.files().map(|f| (format!("templates/{}", f.path().to_string_lossy()), f.contents())));
        d.dirs().for_each(|s| walk(s, out));
    }
    let mut files = vec![];
    walk(&BLOG_TEMPLATES, &mut files);
    files
}

/// Write any of the sample templates missing from `blog_dir/templates/`, except those `skip`
/// says no to. Never overwrites. Returns the paths it wrote, relative to the blog.
pub fn write_templates_except(blog_dir: &std::path::Path, skip: impl Fn(&str) -> bool) -> std::io::Result<Vec<String>> {
    let mut wrote = vec![];
    for (rel, contents) in template_files() {
        let dest = blog_dir.join(&rel);
        if dest.exists() || skip(&rel) {
            continue;
        }
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&dest, contents)?;
        wrote.push(rel);
    }
    Ok(wrote)
}

/// Write any of the sample templates missing from `blog_dir/templates/`. Never overwrites.
pub fn write_templates(blog_dir: &std::path::Path) -> std::io::Result<Vec<String>> {
    write_templates_except(blog_dir, |_| false)
}

pub fn names() -> Vec<String> {
    let mut v: Vec<String> = FIXTURES.dirs().map(|d| d.path().to_string_lossy().to_string()).collect();
    v.sort();
    v
}
