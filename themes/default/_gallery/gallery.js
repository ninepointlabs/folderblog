/* Gallery lightbox and keyboard navigation. Each grid is its own slideshow. */
(function () {
  if (window.__galleryReady) return;
  window.__galleryReady = true;
  var icon = function (d) { return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="' + d + '"/></svg>'; };
  var typing = function (e) { var t = e.target; return t && (t.isContentEditable || /^(input|textarea|select)$/i.test(t.tagName)); };

  // On a photo page, ← and → go to the neighbouring photos.
  var nav = document.querySelector("[data-photo-nav]");
  if (nav) {
    document.addEventListener("keydown", function (e) {
      if (e.altKey || e.ctrlKey || e.metaKey || typing(e)) return;
      var a = nav.querySelector(e.key === "ArrowLeft" ? "a[rel=prev]" : e.key === "ArrowRight" ? "a[rel=next]" : null);
      if (a) location.href = a.href;
    });
  }

  if (!window.HTMLDialogElement) return;
  var box, img, items = [], at = 0, opener = null, token = 0;

  function build(host) {
    box = document.createElement("dialog");
    box.className = "gallery-lightbox";
    box.setAttribute("aria-label", "Photo viewer");
    box.innerHTML =
      '<div class="lb-stage"><img alt=""></div>' +
      '<button type="button" class="lb-btn lb-close" aria-label="Close">' + icon("M6 6l12 12M18 6L6 18") + "</button>" +
      '<button type="button" class="lb-btn lb-prev" aria-label="Previous photo">' + icon("M15 5l-7 7 7 7") + "</button>" +
      '<button type="button" class="lb-btn lb-next" aria-label="Next photo">' + icon("M9 5l7 7-7 7") + "</button>" +
      '<div class="lb-info"><div class="lb-text"><span class="lb-title"></span><span class="lb-caption"></span></div>' +
      '<div class="lb-side"><span class="lb-count"></span><a class="lb-open" href="">Photo page</a></div></div>';
    host.appendChild(box);
    img = box.querySelector(".lb-stage img");
    box.querySelector(".lb-close").onclick = function () { box.close(); };
    box.querySelector(".lb-prev").onclick = function () { show(at - 1); };
    box.querySelector(".lb-next").onclick = function () { show(at + 1); };
    box.addEventListener("click", function (e) { if (e.target.classList.contains("lb-stage")) box.close(); });
    box.addEventListener("close", function () { img.removeAttribute("src"); if (opener) opener.focus({ preventScroll: true }); });
    box.addEventListener("keydown", function (e) {
      if (e.key === "ArrowLeft") { e.preventDefault(); show(at - 1); }
      if (e.key === "ArrowRight") { e.preventDefault(); show(at + 1); }
    });
    var x0 = null, y0 = 0;
    var stage = box.querySelector(".lb-stage");
    stage.addEventListener("pointerdown", function (e) { x0 = e.clientX; y0 = e.clientY; });
    stage.addEventListener("pointerup", function (e) {
      if (x0 === null) return;
      var dx = e.clientX - x0, dy = e.clientY - y0;
      x0 = null;
      if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) show(at + (dx < 0 ? 1 : -1));
    });
  }

  function show(n) {
    var len = items.length;
    at = ((n % len) + len) % len;
    var a = items[at], thumb = a.querySelector("img"), mine = ++token;
    // The thumbnail is already loaded: show it at once, then swap in the full image.
    img.src = thumb.currentSrc || thumb.src;
    img.alt = thumb.alt;
    var full = new Image();
    full.onload = function () { if (mine === token) img.src = full.src; };
    full.src = a.dataset.image;
    box.querySelector(".lb-title").textContent = a.dataset.title || "";
    box.querySelector(".lb-caption").textContent = a.dataset.caption || "";
    box.querySelector(".lb-count").textContent = len > 1 ? (at + 1) + " / " + len : "";
    box.querySelector(".lb-open").href = a.href;
    box.querySelector(".lb-prev").hidden = box.querySelector(".lb-next").hidden = len < 2;
    [1, -1].forEach(function (d) { var b = items[(at + d + len) % len]; if (b) new Image().src = b.dataset.image; });
  }

  document.addEventListener("click", function (e) {
    var a = e.target.closest && e.target.closest("[data-gallery] .gallery-tile");
    if (!a || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    if (!box) build(a.closest(".gallery") || document.body);
    items = Array.prototype.slice.call(a.closest("[data-gallery]").querySelectorAll(".gallery-tile"));
    opener = a;
    show(items.indexOf(a));
    if (!box.open) box.showModal();
  });
})();
