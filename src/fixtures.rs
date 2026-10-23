//! Built-in stress content for testing themes (`--fixtures NAME`).

use include_dir::{Dir, include_dir};

pub static FIXTURES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/fixtures");
pub static DEFAULT_THEME: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes/default");
/// Sample post and episode with every front matter field, copied into a blog's `templates/`.
pub static BLOG_TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/scaffold/templates");

/// Write any of the sample templates missing from `blog_dir/templates/`. Never overwrites.
/// Returns the paths it wrote, relative to the blog.
pub fn write_templates(blog_dir: &std::path::Path) -> std::io::Result<Vec<String>> {
    fn walk(d: &'static Dir<'static>, out: &mut Vec<&'static include_dir::File<'static>>) {
        out.extend(d.files());
        d.dirs().for_each(|s| walk(s, out));
    }
    let mut files = vec![];
    walk(&BLOG_TEMPLATES, &mut files);
    let mut wrote = vec![];
    for f in files {
        let dest = blog_dir.join("templates").join(f.path());
        if dest.exists() {
            continue;
        }
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&dest, f.contents())?;
        wrote.push(format!("templates/{}", f.path().to_string_lossy()));
    }
    Ok(wrote)
}

pub fn names() -> Vec<String> {
    let mut v: Vec<String> = FIXTURES.dirs().map(|d| d.path().to_string_lossy().to_string()).collect();
    v.sort();
    v
}
