// Remember the language chosen with the EN / IT switch (read by the redirect at the top of the English page).
document.querySelectorAll('.lang a').forEach((a) => a.addEventListener('click', () => {
  try { localStorage.setItem('getpaper-lang', a.hreflang); } catch (e) {}
}));

// Fade sections in as they scroll into view.
if ('IntersectionObserver' in window) {
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
  }), { threshold: 0.12 });
  document.querySelectorAll('.reveal').forEach((el) => io.observe(el));
} else {
  document.querySelectorAll('.reveal').forEach((el) => el.classList.add('in'));
}

// Copy buttons for code blocks (shown only when JavaScript runs).
document.querySelectorAll('.copy').forEach((btn) => btn.addEventListener('click', async () => {
  const pre = document.getElementById(btn.dataset.copy);
  const label = btn.textContent;
  try {
    await navigator.clipboard.writeText(pre.textContent);
    btn.textContent = btn.dataset.done;
  } catch (e) {
    const r = document.createRange();
    r.selectNodeContents(pre);
    getSelection().removeAllRanges(); getSelection().addRange(r);
  }
  setTimeout(() => (btn.textContent = label), 1600);
}));
