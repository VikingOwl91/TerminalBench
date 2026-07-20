(() => {
  "use strict";

  document.documentElement.classList.add("js");

  const copyText = async (text) => {
    if (!navigator.clipboard?.writeText) {
      throw new Error("Clipboard API unavailable");
    }
    await navigator.clipboard.writeText(text);
  };

  document.querySelectorAll("[data-copy-target]").forEach((button) => {
    button.addEventListener("click", async () => {
      const code = document.getElementById(button.dataset.copyTarget ?? "");
      const status = document.getElementById(button.dataset.statusTarget ?? "");
      if (!code || !status) return;

      try {
        await copyText(code.textContent ?? "");
        status.textContent = button.dataset.copySuccess ?? "Command copied";
      } catch {
        status.textContent =
          button.dataset.copyFailure ??
          "Couldn’t copy the command. Select it and copy it manually.";
      }
    });
  });

  document.querySelectorAll('a[href="#quickstart"]').forEach((link) => {
    link.addEventListener("click", () => {
      window.setTimeout(() => {
        document.getElementById("quickstart-heading")?.focus({ preventScroll: true });
      }, 0);
    });
  });
})();
