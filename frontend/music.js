(() => {
  const button = document.getElementById("music-button");
  const audio = new Audio("audio/menu-theme.wav");
  audio.loop = true;
  audio.preload = "none";
  audio.volume = .3;
  let enabled = true;
  let unlocked = false;
  let foreground = document.hasFocus();

  try {
    enabled = localStorage.getItem("bostris.music") !== "off";
  } catch {}

  function updateButton() {
    button.setAttribute("aria-pressed", String(enabled));
    button.setAttribute("aria-label", enabled ? "Mute music" : "Enable music");
    button.title = `${enabled ? "Mute" : "Enable"} music · Title Theme (8-Bit Style) by Kat · CC BY 4.0`;
  }

  function sync() {
    if (!enabled || !unlocked || !foreground || document.hidden) {
      audio.pause();
      return;
    }
    if (audio.paused) {
      audio.play().then(() => {
        if (!enabled || !foreground || document.hidden) audio.pause();
      }).catch(() => {});
    }
  }

  button.addEventListener("click", () => {
    enabled = !enabled;
    unlocked = true;
    foreground = document.hasFocus();
    try {
      localStorage.setItem("bostris.music", enabled ? "on" : "off");
    } catch {}
    updateButton();
    sync();
  });
  document.addEventListener("pointerdown", (event) => {
    if (event.target.closest("#music-button")) return;
    unlocked = true;
    foreground = document.hasFocus();
    sync();
  });
  document.addEventListener("keydown", (event) => {
    if (event.metaKey || event.altKey || event.target.closest("#music-button")) return;
    unlocked = true;
    foreground = document.hasFocus();
    sync();
  });
  window.addEventListener("blur", () => { foreground = false; sync(); });
  window.addEventListener("focus", () => { foreground = true; sync(); });
  document.addEventListener("visibilitychange", sync);
  window.addEventListener("pagehide", () => audio.pause());
  updateButton();
})();
