// folderblog.ninepointlabs.com: the "try it" toy, tabs, and copy buttons.
// The toy applies folderblog's real content rules: title from the first heading
// (removed from the body), address from the file name, date = first seen.
(() => {
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];

  // ---------- tabs (themes and install) ----------
  for (const attr of ["theme", "os"]) {
    $$(`.tab[data-${attr}]`).forEach(tab => tab.addEventListener("click", () => {
      const v = tab.dataset[attr];
      $$(`.tab[data-${attr}]`).forEach(t => t.classList.toggle("on", t === tab));
      $$(`[data-${attr}]:not(.tab)`).forEach(p => p.classList.toggle("on", p.dataset[attr] === v));
    }));
  }

  // ---------- copy buttons ----------
  $$(".copy").forEach(b => b.addEventListener("click", async () => {
    try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = "Copied"; b.classList.add("done"); }
    catch { b.textContent = "Select & copy"; }
    setTimeout(() => { b.textContent = "Copy"; b.classList.remove("done"); }, 1600);
  }));

  // ---------- the toy ----------
  const toy = $("#toy");
  if (!toy) return;
  const text = $("#toy-text"), file = $("#toy-file"), log = $("#toy-log"), page = $("#toy-page"), url = $("#toy-url"),
    timer = $("#toy-timer"), site = $("#toy-site"), hint = $("#toy-hint");
  const esc = s => s.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
  const inline = s => esc(s).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
    .replace(/\*([^*]+)\*/g, "<i>$1</i>").replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="#toy">$1</a>');
  const slugify = s => s.toLowerCase().normalize("NFKD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  const firstSeen = {};
  let busy = false;

  function render() {
    const name = (file.value.trim() || "untitled.md").replace(/\.(md|markdown)$/i, "");
    const m = name.match(/^(\d{4})-(\d{2})-(\d{2})-(.+)$/);
    const slug = slugify(m ? m[4] : name) || "untitled";
    // Date: a filename prefix, else the first time this slug was saved (then it never moves).
    let date = m ? new Date(+m[1], +m[2] - 1, +m[3]) : (firstSeen[slug] ||= new Date());
    const blocks = text.value.replace(/\r/g, "").split(/\n\s*\n/).map(b => b.trim()).filter(Boolean);
    let title = null;
    if (blocks[0] && /^#{1,6}\s+/.test(blocks[0])) title = blocks.shift().replace(/^#{1,6}\s+/, "");
    if (!title) title = (m ? m[4] : name).replace(/[-_]+/g, " ").replace(/^./, c => c.toUpperCase());
    const body = blocks.map(b => /^#{1,6}\s/.test(b) ? `<p><b>${inline(b.replace(/^#+\s+/, ""))}</b></p>` : `<p>${inline(b).replace(/\n/g, " ")}</p>`).join("");
    const y = date.getFullYear(), mo = String(date.getMonth() + 1).padStart(2, "0");
    const path = `/${y}/${mo}/${slug}/`;
    return { title, body, path, when: date.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" }), words: text.value.split(/\s+/).filter(Boolean).length };
  }

  const line = s => { log.innerHTML += "\n" + s; log.scrollTop = log.scrollHeight; };
  const wait = ms => new Promise(r => setTimeout(r, ms));

  async function save() {
    if (busy) return;
    busy = true; hint.textContent = "saved: watch the log";
    const r = render(), t0 = performance.now();
    log.innerHTML = '<span class="d">[folderblog]</span> watching ~/Blogs';
    const tick = setInterval(() => { timer.textContent = ((performance.now() - t0) / 1000).toFixed(1) + "s"; }, 50);
    await wait(350); line(`<span class="d">[folderblog]</span> change: posts/${esc(file.value || "untitled.md")}`);
    await wait(650); line(`<span class="d">[folderblog]</span> myblog: built ${9 + Math.min(3, r.words % 4)} files in ${4 + (r.words % 5)} ms`);
    await wait(700); line(`<span class="d">[folderblog]</span> myblog: <span class="g">deployed (pushed ${Math.random().toString(16).slice(2, 9)})</span>`);
    await wait(300);
    clearInterval(tick);
    url.textContent = "yourname.github.io/myblog" + r.path;
    page.innerHTML = `<div class="toy-sitename">My Blog</div><h4>${inline(r.title)}</h4><div class="meta">${r.when} · 1 min read</div>${r.body || '<p class="toy-empty">(an empty post)</p>'}`;
    page.classList.remove("enter"); void page.offsetWidth; page.classList.add("enter");
    site.classList.remove("flash"); void site.offsetWidth; site.classList.add("flash");
    timer.textContent = "live ✓";
    hint.innerHTML = "Live. <b>Title</b> from the heading, <b>address</b> from the file name.";
    busy = false;
  }

  $("#toy-save").addEventListener("click", save);
  toy.addEventListener("keydown", e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") { e.preventDefault(); save(); } });
  const dirty = () => { if (!busy) { hint.textContent = "unsaved changes"; } };
  text.addEventListener("input", dirty); file.addEventListener("input", dirty);
})();
