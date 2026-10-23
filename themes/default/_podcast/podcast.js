(function () {
  var player = document.querySelector(".episode-player");
  if (!player) return;
  document.querySelectorAll(".chapter-seek").forEach(function (b) {
    b.addEventListener("click", function () {
      player.currentTime = parseFloat(b.getAttribute("data-start")) || 0;
      player.play();
    });
  });
})();
