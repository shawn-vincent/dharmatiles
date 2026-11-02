// Reference galleries: open a photo in a viewer, step with arrows, Esc closes.
(() => {
  const dlg = document.querySelector(".lightbox");
  const links = [...document.querySelectorAll(".ph > a")];
  if (!dlg || !links.length || !dlg.showModal) return;
  const img = dlg.querySelector("img"), cap = dlg.querySelector("figcaption"),
        n = dlg.querySelector(".lb-n"), open = dlg.querySelector(".lb-open");
  let i = 0;
  const show = k => {
    i = (k + links.length) % links.length;
    const a = links[i];
    img.src = a.getAttribute("href"); img.alt = a.dataset.cap || "";
    cap.textContent = a.dataset.cap || ""; open.href = a.getAttribute("href");
    n.textContent = `${i + 1} / ${links.length}`;
  };
  links.forEach((a, k) => a.addEventListener("click", e => { e.preventDefault(); show(k); dlg.showModal(); }));
  dlg.querySelectorAll("[data-step]").forEach(b => b.addEventListener("click", () => show(i + +b.dataset.step)));
  dlg.querySelector(".lb-close").addEventListener("click", () => dlg.close());
  dlg.addEventListener("click", e => { if (e.target === dlg) dlg.close(); });
  document.addEventListener("keydown", e => {
    if (!dlg.open) return;
    if (e.key === "ArrowRight") show(i + 1);
    if (e.key === "ArrowLeft") show(i - 1);
  });
})();
