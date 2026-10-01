//! The photo gallery: every picture in `gallery/` becomes a photo with its own page,
//! subfolders become albums, and an optional sidecar `photo.md` beside `photo.jpg`
//! gives it a title, caption, tags and date. Only resized copies are published:
//! re-encoding strips EXIF, so camera GPS never reaches the site.

use crate::content::{self, DateSource};
use crate::markdown;
use crate::state::State;
use crate::templates::prefix_base;
use crate::util::{slugify, strip_tags, title_from_slug};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const GALLERY_DIR: &str = "gallery";
/// Theme templates the gallery renders with; `folderblog gallery on` installs them.
pub const TEMPLATES: &[&str] = &["_gallery/index.html", "_gallery/album.html", "_gallery/tag.html", "_gallery/photo.html"];

/// Bump when the derivative images change, so caches rebuild.
const CACHE_VERSION: &str = "2";
/// Longest edge of the published image.
const LARGE_MAX: u32 = 2048;
/// Thumbnails fit in this box: tall enough for sharp grid rows on 2x screens.
const THUMB_MAX: (u32, u32) = (1600, 560);

pub fn is_photo(p: &Path) -> bool {
    matches!(ext_of(p).as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif")
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// What we learn from an image file once (cached): dimensions, EXIF, derivative files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
    /// Extension of the published image and thumbnail (`jpg`, `png`, or the original's for GIFs).
    pub ext: String,
    pub thumb_ext: String,
    /// GIFs are published as they are (resizing would drop the animation).
    pub passthrough: bool,
    pub taken: Option<String>,
    pub exif: Option<Exif>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Exif {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub aperture: Option<String>,
    pub shutter: Option<String>,
    pub iso: Option<String>,
    pub focal_length: Option<String>,
}

pub struct Photo {
    /// Path relative to the blog (or fixture) root, e.g. `gallery/paris/eiffel.jpg`.
    pub source: String,
    pub sidecar: Option<String>,
    pub slug: String,
    pub album: Option<usize>,
    pub title: String,
    pub alt: String,
    pub caption_html: String,
    pub date: DateTime<FixedOffset>,
    pub date_source: DateSource,
    pub tags: Vec<String>,
    pub draft: bool,
    pub front: Map<String, Json>,
    pub info: Info,
    /// Site routes (without base path).
    pub route: String,
    pub image_route: String,
    pub thumb_route: String,
    /// Files to publish at image_route / thumb_route.
    pub image_file: PathBuf,
    pub thumb_file: PathBuf,
}

pub struct Album {
    pub slug: String,
    pub title: String,
    pub route: String,
    pub source: String,
    pub description_html: String,
    pub date: Option<DateTime<FixedOffset>>,
    pub tags: Vec<String>,
    pub front: Map<String, Json>,
    /// Indexes into `Gallery::photos`, in album order (oldest first unless `order: newest`).
    pub photos: Vec<usize>,
}

pub struct Gallery {
    pub title: String,
    pub route: String,
    pub intro_html: String,
    pub front: Map<String, Json>,
    /// Newest first.
    pub photos: Vec<Photo>,
    /// Newest first.
    pub albums: Vec<Album>,
}

/// A sidecar's front matter and Markdown body.
struct Sidecar {
    rel: String,
    front: Map<String, Json>,
    body: String,
}

fn read_sidecar(root: &Path, path: &Path) -> Result<Sidecar> {
    let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {rel}"))?.replace("\r\n", "\n");
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
    let (front, _, body) = content::split_front_matter(&text).with_context(|| format!("{rel}: bad front matter"))?;
    Ok(Sidecar { rel, front, body })
}

/// Title and HTML from a sidecar body: front matter `title`, else a leading heading.
fn render_body(sc: &Sidecar, base_path: &str) -> Result<(Option<String>, String)> {
    let fm_title = sc.front.get("title").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
    let resolve = |u: &str| if u.starts_with('/') && !u.starts_with("//") { prefix_base(base_path, u) } else { u.to_string() };
    let r = markdown::render(&sc.body, fm_title.is_none(), None, &resolve).with_context(|| format!("rendering {}", sc.rel))?;
    Ok((fm_title.or(r.title), r.html.trim().to_string()))
}

/// Filenames straight off a camera or phone (`IMG_2041`, `PXL_20240501_…`, `DSC00012`)
/// make poor titles, so those photos have none.
fn is_camera_name(stem: &str) -> bool {
    let s = stem.to_ascii_lowercase();
    let s = s.trim_start_matches('_');
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = &s[letters.len()..];
    let rest = rest.trim_start_matches(['_', '-', ' ']);
    let starts_digit = rest.chars().next().is_some_and(|c| c.is_ascii_digit());
    (letters.is_empty() && s.chars().all(|c| c.is_ascii_digit() || "_- ".contains(c)))
        || (starts_digit
            && matches!(
                letters.as_str(),
                "img" | "dsc" | "dscn" | "dscf" | "pxl" | "mvimg" | "photo" | "image" | "pic" | "gopr" | "dji" | "p" | "screenshot" | "signal" | "whatsapp" | "wp"
            ))
}

fn date_in_front(front: &Map<String, Json>, rel: &str) -> Result<Option<DateTime<FixedOffset>>> {
    match front.get("date") {
        None => Ok(None),
        Some(v) => match v.as_str().and_then(content::parse_date) {
            Some(d) => Ok(Some(d)),
            None => bail!("{rel}: unrecognised date {v} (use YYYY-MM-DD or RFC 3339)"),
        },
    }
}

fn local(n: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
    Local.from_local_datetime(&n).earliest().map(|d| d.fixed_offset())
}

/// Give each name a unique slug among `taken`, suffixing `-2`, `-3`…
fn unique(slug: String, taken: &mut HashSet<String>) -> String {
    let base = if slug.is_empty() { "photo".to_string() } else { slug };
    let mut s = base.clone();
    let mut n = 2;
    while !taken.insert(s.clone()) {
        s = format!("{base}-{n}");
        n += 1;
    }
    s
}

pub struct LoadArgs<'a> {
    pub content_root: &'a Path,
    pub cache_dir: &'a Path,
    pub route: &'a str,
    pub base_path: &'a str,
    pub drafts: bool,
    /// Remove cache entries this load did not use.
    pub prune_cache: bool,
}

pub fn load(a: LoadArgs, state: &mut State, warnings: &mut Vec<String>) -> Result<Gallery> {
    let root = a.content_root;
    let dir = root.join(GALLERY_DIR);
    let mut g = Gallery {
        title: "Gallery".into(),
        route: a.route.to_string(),
        intro_html: String::new(),
        front: Map::new(),
        photos: vec![],
        albums: vec![],
    };
    if !dir.is_dir() {
        return Ok(g);
    }
    if let Some(p) = ["index.md", "index.markdown"].iter().map(|n| dir.join(n)).find(|p| p.is_file()) {
        let sc = read_sidecar(root, &p)?;
        let (title, html) = render_body(&sc, a.base_path)?;
        if let Some(t) = title.filter(|t| !t.is_empty()) {
            g.title = t;
        }
        g.intro_html = html;
        g.front = sc.front;
    }

    // Albums: each folder directly inside gallery/. Deeper folders belong to their album.
    let mut taken_top: HashSet<String> = HashSet::from(["tags".to_string()]);
    let mut folders: Vec<(Option<PathBuf>, PathBuf)> = vec![(None, dir.clone())];
    for e in sorted_entries(&dir)? {
        let name = e.file_name().unwrap().to_string_lossy().to_string();
        if e.is_dir() && !content::is_debris(&name) {
            folders.push((Some(e.clone()), e));
        }
    }

    struct Found {
        abs: PathBuf,
        album: Option<usize>,
        sidecar: Option<Sidecar>,
    }
    let mut found: Vec<Found> = vec![];
    for (album_dir, walk_root) in folders {
        let album_idx = match &album_dir {
            None => None,
            Some(d) => {
                let name = d.file_name().unwrap().to_string_lossy().to_string();
                let index = ["index.md", "index.markdown"].iter().map(|n| d.join(n)).find(|p| p.is_file());
                let sc = index.map(|p| read_sidecar(root, &p)).transpose()?;
                let (title, html) = match &sc {
                    Some(sc) => render_body(sc, a.base_path)?,
                    None => (None, String::new()),
                };
                let front = sc.map(|s| s.front).unwrap_or_default();
                let source = d.strip_prefix(root).unwrap_or(d).to_string_lossy().replace('\\', "/");
                let date = date_in_front(&front, &source)?;
                let (_, rest) = content::split_date_prefix(&name);
                let slug_src = front.get("slug").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| rest.to_string());
                let slug = unique(slugify(&slug_src), &mut taken_top);
                g.albums.push(Album {
                    route: format!("{}{slug}/", a.route),
                    slug,
                    title: title.filter(|t| !t.is_empty()).unwrap_or_else(|| title_from_slug(rest)),
                    source: format!("{source}/"),
                    description_html: html,
                    date,
                    tags: content::tags_of(&front),
                    front,
                    photos: vec![],
                });
                Some(g.albums.len() - 1)
            }
        };
        let walker = walkdir::WalkDir::new(&walk_root)
            .sort_by_file_name()
            .max_depth(if album_dir.is_none() { 1 } else { usize::MAX })
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !content::is_debris(&e.file_name().to_string_lossy()));
        let mut files: Vec<PathBuf> = vec![];
        for e in walker {
            let e = e?;
            if e.file_type().is_file() || (e.file_type().is_symlink() && e.path().is_file()) {
                files.push(e.path().to_path_buf());
            }
        }
        let mut used_sidecars: HashSet<PathBuf> = HashSet::new();
        for f in &files {
            if !is_photo(f) {
                continue;
            }
            // `sunset.md` or `sunset.jpg.md` beside `sunset.jpg`.
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            let stem = f.file_stem().unwrap().to_string_lossy().to_string();
            let sc_path = [format!("{name}.md"), format!("{stem}.md")]
                .iter()
                .map(|n| f.with_file_name(n))
                .find(|p| p.is_file());
            let sidecar = match sc_path {
                Some(p) => {
                    used_sidecars.insert(p.clone());
                    Some(read_sidecar(root, &p)?)
                }
                None => None,
            };
            found.push(Found { abs: f.clone(), album: album_idx, sidecar });
        }
        for f in &files {
            let rel = f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/");
            let name = f.file_name().unwrap().to_string_lossy().to_lowercase();
            if is_photo(f) || used_sidecars.contains(f) || name == "index.md" || name == "index.markdown" {
                continue;
            }
            if content::is_markdown(f) {
                warnings.push(format!("{rel}: no photo with the same name beside it, so this caption is unused"));
            } else if matches!(ext_of(f).as_str(), "heic" | "heif" | "avif" | "tif" | "tiff" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "bmp") {
                warnings.push(format!("{rel}: this image format can't be shown on the web; export it as JPEG to add it to the gallery"));
            }
        }
    }

    // Image processing (cached, parallel on first sight).
    let infos = process_all(&found.iter().map(|f| f.abs.clone()).collect::<Vec<_>>(), a.cache_dir)?;
    if a.prune_cache {
        prune_cache(a.cache_dir, &found.iter().map(|f| cache_key(&f.abs)).collect());
    }

    // Photos in the gallery folder itself share URL space with albums; in albums, with each other.
    let mut taken_in: HashMap<Option<usize>, HashSet<String>> = HashMap::new();
    taken_in.insert(None, taken_top);
    for (f, info) in found.into_iter().zip(infos) {
        let info = match info {
            Ok(i) => i,
            Err(e) => {
                let rel = f.abs.strip_prefix(root).unwrap_or(&f.abs).display().to_string();
                bail!("{rel}: cannot read this image: {e:#}");
            }
        };
        let source = f.abs.strip_prefix(root).unwrap_or(&f.abs).to_string_lossy().replace('\\', "/");
        let stem = f.abs.file_stem().unwrap().to_string_lossy().to_string();
        let (name_date, rest) = content::split_date_prefix(&stem);
        let empty = Map::new();
        let front = f.sidecar.as_ref().map(|s| &s.front).unwrap_or(&empty);
        let (sc_title, caption_html) = match &f.sidecar {
            Some(sc) => render_body(sc, a.base_path)?,
            None => (None, String::new()),
        };
        let draft = front.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
        if draft && !a.drafts {
            continue;
        }
        let slug_src = front.get("slug").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| rest.to_string());
        let slug = unique(slugify(&slug_src), taken_in.entry(f.album).or_default());
        let title = sc_title.filter(|t| !t.is_empty()).unwrap_or_else(|| {
            if is_camera_name(rest) || name_date.is_some() && rest.len() == 10 { String::new() } else { title_from_slug(rest) }
        });
        let (date, date_source) = if let Some(d) = date_in_front(front, &source)? {
            (d, DateSource::FrontMatter)
        } else if let Some(d) = name_date.and_then(|d| local(d.and_hms_opt(0, 0, 0)?)) {
            (d, DateSource::Filename)
        } else if let Some(d) = info.taken.as_deref().and_then(|t| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S").ok()).and_then(local) {
            (d, DateSource::Exif)
        } else {
            (state.first_seen(&format!("gallery:{source}")), DateSource::FirstSeen)
        };
        let mut tags = content::tags_of(front);
        if let Some(i) = f.album {
            for t in &g.albums[i].tags {
                if !tags.iter().any(|x| slugify(x) == slugify(t)) {
                    tags.push(t.clone());
                }
            }
        }
        let alt = front
            .get("alt")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| (!title.is_empty()).then(|| title.clone()))
            .or_else(|| {
                let c = strip_tags(&caption_html);
                (!c.is_empty()).then_some(c)
            })
            .unwrap_or_default();
        let dir_route = match f.album {
            Some(i) => g.albums[i].route.clone(),
            None => a.route.to_string(),
        };
        let (image_file, thumb_file) = if info.passthrough {
            (f.abs.clone(), f.abs.clone())
        } else {
            let key = cache_key(&f.abs);
            (a.cache_dir.join(format!("{key}.large.{}", info.ext)), a.cache_dir.join(format!("{key}.thumb.{}", info.thumb_ext)))
        };
        let thumb_route = if info.passthrough {
            format!("{dir_route}{slug}.{}", info.ext)
        } else {
            format!("{dir_route}{slug}.thumb.{}", info.thumb_ext)
        };
        g.photos.push(Photo {
            source,
            sidecar: f.sidecar.as_ref().map(|s| s.rel.clone()),
            route: format!("{dir_route}{slug}/"),
            image_route: format!("{dir_route}{slug}.{}", info.ext),
            thumb_route,
            slug,
            album: f.album,
            title,
            alt,
            caption_html,
            date,
            date_source,
            tags,
            draft,
            front: front.clone(),
            info,
            image_file,
            thumb_file,
        });
    }

    // Newest first, then album membership in reading order.
    let mut order: Vec<usize> = (0..g.photos.len()).collect();
    order.sort_by(|&x, &y| g.photos[y].date.cmp(&g.photos[x].date).then(g.photos[x].source.cmp(&g.photos[y].source)));
    let mut photos: Vec<Option<Photo>> = std::mem::take(&mut g.photos).into_iter().map(Some).collect();
    g.photos = order.iter().map(|&i| photos[i].take().unwrap()).collect();
    for (i, p) in g.photos.iter().enumerate() {
        if let Some(a) = p.album {
            g.albums[a].photos.push(i);
        }
    }
    for al in &mut g.albums {
        let newest_first = al.front.get("order").and_then(|v| v.as_str()) == Some("newest");
        if !newest_first {
            al.photos.reverse();
        }
        if al.date.is_none() {
            al.date = al.photos.iter().map(|&i| g.photos[i].date).max();
        }
    }
    // Empty albums (or only drafts) are left out, so they never become empty pages.
    let keep: Vec<bool> = g.albums.iter().map(|a| !a.photos.is_empty()).collect();
    if keep.iter().any(|k| !k) {
        let mut remap = vec![None; g.albums.len()];
        let mut n = 0;
        for (i, k) in keep.iter().enumerate() {
            if *k {
                remap[i] = Some(n);
                n += 1;
            }
        }
        let mut i = 0;
        g.albums.retain(|_| {
            i += 1;
            keep[i - 1]
        });
        for p in &mut g.photos {
            p.album = p.album.and_then(|a| remap[a]);
        }
    }
    // Albums newest first; ties by title. Album order in g.photos indexes is untouched.
    let mut aorder: Vec<usize> = (0..g.albums.len()).collect();
    aorder.sort_by(|&x, &y| g.albums[y].date.cmp(&g.albums[x].date).then(g.albums[x].title.cmp(&g.albums[y].title)));
    let mut remap = vec![0; g.albums.len()];
    for (new, &old) in aorder.iter().enumerate() {
        remap[old] = new;
    }
    let mut albums: Vec<Option<Album>> = std::mem::take(&mut g.albums).into_iter().map(Some).collect();
    g.albums = aorder.iter().map(|&i| albums[i].take().unwrap()).collect();
    for p in &mut g.photos {
        p.album = p.album.map(|a| remap[a]);
    }
    Ok(g)
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    v.sort();
    Ok(v)
}

/// Cache key: the file's identity and version, so an edited photo is reprocessed.
fn cache_key(abs: &Path) -> String {
    let meta = std::fs::metadata(abs).ok();
    let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(format!("{CACHE_VERSION}|{}|{len}|{mtime}", abs.display()));
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

fn prune_cache(dir: &Path, keep: &HashSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let key = name.split('.').next().unwrap_or("");
        if !keep.contains(key) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Info for each image, reusing the cache and processing new images in parallel.
fn process_all(files: &[PathBuf], cache_dir: &Path) -> Result<Vec<Result<Info>>> {
    std::fs::create_dir_all(cache_dir).with_context(|| format!("creating {}", cache_dir.display()))?;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(files.len().max(1));
    let mut results: Vec<Option<Result<Info>>> = (0..files.len()).map(|_| None).collect();
    let done = std::sync::Mutex::new(&mut results);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= files.len() {
                        break;
                    }
                    let r = cached_info(&files[i], cache_dir);
                    done.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    Ok(results.into_iter().map(|r| r.unwrap()).collect())
}

fn cached_info(abs: &Path, cache_dir: &Path) -> Result<Info> {
    let key = cache_key(abs);
    let json = cache_dir.join(format!("{key}.json"));
    if let Ok(text) = std::fs::read_to_string(&json) {
        if let Ok(info) = serde_json::from_str::<Info>(&text) {
            let complete = info.passthrough
                || (cache_dir.join(format!("{key}.large.{}", info.ext)).is_file()
                    && cache_dir.join(format!("{key}.thumb.{}", info.thumb_ext)).is_file());
            if complete {
                return Ok(info);
            }
        }
    }
    let info = process(abs, cache_dir, &key)?;
    let tmp = json.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&info)?)?;
    std::fs::rename(&tmp, &json)?;
    Ok(info)
}

fn process(abs: &Path, cache_dir: &Path, key: &str) -> Result<Info> {
    use image::{ImageDecoder, ImageReader};
    let (taken, exif) = read_exif(abs);
    let ext = ext_of(abs);
    if ext == "gif" {
        let (w, h) = image::image_dimensions(abs)?;
        return Ok(Info {
            width: w,
            height: h,
            thumb_width: w,
            thumb_height: h,
            ext: ext.clone(),
            thumb_ext: ext,
            passthrough: true,
            taken,
            exif,
        });
    }
    let mut decoder = ImageReader::open(abs)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let alpha = img.color().has_alpha() && ext != "jpg" && ext != "jpeg";
    let out_ext = if alpha { "png" } else { "jpg" };

    let large = if img.width().max(img.height()) > LARGE_MAX {
        img.resize(LARGE_MAX, LARGE_MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let thumb = if large.width() > THUMB_MAX.0 || large.height() > THUMB_MAX.1 {
        large.resize(THUMB_MAX.0, THUMB_MAX.1, image::imageops::FilterType::CatmullRom)
    } else {
        large.clone()
    };
    let write = |img: &image::DynamicImage, name: &str, quality: u8| -> Result<()> {
        let path = cache_dir.join(name);
        let tmp = cache_dir.join(format!("{name}.tmp"));
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        if alpha {
            img.write_to(&mut f, image::ImageFormat::Png)?;
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality).encode_image(&img.to_rgb8())?;
        }
        drop(f);
        std::fs::rename(&tmp, &path)?;
        Ok(())
    };
    write(&large, &format!("{key}.large.{out_ext}"), 82)?;
    write(&thumb, &format!("{key}.thumb.{out_ext}"), 76)?;
    Ok(Info {
        width: large.width(),
        height: large.height(),
        thumb_width: thumb.width(),
        thumb_height: thumb.height(),
        ext: out_ext.into(),
        thumb_ext: out_ext.into(),
        passthrough: false,
        taken,
        exif,
    })
}

/// When the photo was taken (as `YYYY-MM-DDTHH:MM:SS`) and a few camera settings.
/// Location tags are never read.
fn read_exif(abs: &Path) -> (Option<String>, Option<Exif>) {
    use exif::{In, Tag, Value};
    let Ok(file) = std::fs::File::open(abs) else { return (None, None) };
    let Ok(data) = exif::Reader::new().read_from_container(&mut std::io::BufReader::new(file)) else {
        return (None, None);
    };
    let field = |t: Tag| data.get_field(t, In::PRIMARY);
    let text = |t: Tag| -> Option<String> {
        match &field(t)?.value {
            Value::Ascii(v) => {
                let s = String::from_utf8_lossy(v.first()?).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
                (!s.is_empty()).then_some(s)
            }
            _ => None,
        }
    };
    let rational = |t: Tag| -> Option<f64> {
        match &field(t)?.value {
            Value::Rational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
            _ => None,
        }
    };
    let taken = text(Tag::DateTimeOriginal)
        .or_else(|| text(Tag::DateTime))
        .and_then(|s| NaiveDateTime::parse_from_str(&s, "%Y:%m:%d %H:%M:%S").ok())
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string());
    let trim_float = |x: f64, digits: usize| {
        let s = format!("{x:.digits$}");
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    let camera = match (text(Tag::Make), text(Tag::Model)) {
        (Some(make), Some(model)) if !model.to_lowercase().starts_with(&make.to_lowercase().split_whitespace().next().unwrap_or("").to_string()) => {
            Some(format!("{make} {model}"))
        }
        (_, Some(model)) => Some(model),
        (Some(make), None) => Some(make),
        _ => None,
    };
    let shutter = rational(Tag::ExposureTime).filter(|x| *x > 0.0).map(|x| {
        if x < 1.0 { format!("1/{} s", (1.0 / x).round()) } else { format!("{} s", trim_float(x, 1)) }
    });
    let iso = field(Tag::PhotographicSensitivity).and_then(|f| f.value.get_uint(0)).map(|n| format!("ISO {n}"));
    let e = Exif {
        camera,
        lens: text(Tag::LensModel),
        aperture: rational(Tag::FNumber).filter(|x| *x > 0.0).map(|x| format!("ƒ/{}", trim_float(x, 1))),
        shutter,
        iso,
        focal_length: rational(Tag::FocalLength).filter(|x| *x > 0.0).map(|x| format!("{} mm", trim_float(x, 0))),
    };
    let any = e.camera.is_some() || e.aperture.is_some() || e.shutter.is_some() || e.iso.is_some() || e.focal_length.is_some();
    (taken, any.then_some(e))
}

/// Gallery tags: name and the photos (newest first) for each tag slug, sorted by name.
pub fn tag_index(g: &Gallery) -> Vec<(String, String, Vec<usize>)> {
    let mut tags: BTreeMap<String, (String, Vec<usize>)> = BTreeMap::new();
    for (i, p) in g.photos.iter().enumerate() {
        for t in &p.tags {
            tags.entry(slugify(t)).or_insert_with(|| (t.clone(), vec![])).1.push(i);
        }
    }
    let mut v: Vec<(String, String, Vec<usize>)> = tags.into_iter().filter(|(s, _)| !s.is_empty()).map(|(s, (n, p))| (s, n, p)).collect();
    v.sort_by_key(|t| t.1.to_lowercase());
    v
}

/// Turn the gallery on or off in blog.toml text, keeping everything else as written.
pub fn set_enabled(text: &str, on: bool) -> Result<String> {
    let value = if on { "true" } else { "false" };
    let lines: Vec<&str> = text.lines().collect();
    let header = lines.iter().position(|l| l.trim() == "[gallery]");
    let out = match header {
        Some(h) => {
            let end = lines[h + 1..].iter().position(|l| l.trim_start().starts_with('[')).map(|i| h + 1 + i).unwrap_or(lines.len());
            let mut v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            match (h + 1..end).find(|&i| {
                let t = lines[i].trim_start();
                t.starts_with("enabled") && t["enabled".len()..].trim_start().starts_with('=')
            }) {
                Some(i) => v[i] = format!("enabled = {value}"),
                None => v.insert(h + 1, format!("enabled = {value}")),
            }
            v.join("\n") + "\n"
        }
        None => {
            let mut s = text.trim_end().to_string();
            s.push_str(&format!("\n\n# Photo gallery: every picture in gallery/ (subfolders are albums).\n[gallery]\nenabled = {value}\n"));
            s
        }
    };
    let cfg = crate::config::Config::parse(&out).context("blog.toml would not parse after the change; edit [gallery] by hand")?;
    if cfg.gallery.enabled != on {
        bail!("could not set [gallery] enabled = {value} automatically; edit blog.toml by hand");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_names_have_no_title() {
        for s in ["IMG_2041", "PXL_20240501_101010", "DSC00012", "_DSC1234", "20240501_101010", "1234", "IMG-20240101-WA0001"] {
            assert!(is_camera_name(s), "{s}");
        }
        for s in ["sunset-over-the-bay", "eiffel", "img-of-the-year", "Picnic 2024"] {
            assert!(!is_camera_name(s), "{s}");
        }
    }

    #[test]
    fn toggling_keeps_the_rest_of_blog_toml() {
        let base = "title = \"t\"\nbase_url = \"https://x.com\"\n\n[params]\na = 1\n";
        let on = set_enabled(base, true).unwrap();
        assert!(on.starts_with(base.trim_end()));
        assert!(crate::config::Config::parse(&on).unwrap().gallery.enabled);
        let off = set_enabled(&on, false).unwrap();
        assert!(!crate::config::Config::parse(&off).unwrap().gallery.enabled);
        assert_eq!(off.matches("[gallery]").count(), 1);
        let custom = "title = \"t\"\nbase_url = \"https://x.com\"\n[gallery]\nurl = \"/photos/\"\n[params]\n";
        let on = set_enabled(custom, true).unwrap();
        let cfg = crate::config::Config::parse(&on).unwrap();
        assert!(cfg.gallery.enabled);
        assert_eq!(cfg.gallery.url, "/photos/");
    }
}
