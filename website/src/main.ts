import "@pagefind/component-ui";
import "@pagefind/component-ui/css";
import "./styles/global.css";

// Dark mode toggle + system theme sync
// ----------------------------------------------------------------------------
function setTheme(isDark: boolean) {
  const html = document.documentElement;
  html.classList.toggle("dark", isDark);
  html.setAttribute("data-pf-theme", isDark ? "dark" : "light");
  const themeMeta = document.querySelectorAll('meta[name="theme-color"]');
  themeMeta.forEach((el) => {
    const media = el.getAttribute("media") || "";
    if (media.includes("dark")) el.setAttribute("content", "#111110");
    else el.setAttribute("content", isDark ? "#111110" : "#f3f1ea");
  });
}

function toggleDarkMode() {
  const isDark = document.documentElement.classList.toggle("dark");
  document.documentElement.setAttribute("data-pf-theme", isDark ? "dark" : "light");
  localStorage.setItem("darkMode", String(isDark));
}

function listenSystemTheme() {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", (e) => {
    // Only follow system when user never explicitly toggled
    if (localStorage.getItem("darkMode") !== null) return;
    setTheme(e.matches);
  });
}

declare global {
  interface Window {
    toggleDarkMode: () => void;
    copyCommand: (el: HTMLElement, text: string) => void;
  }
}
window.toggleDarkMode = toggleDarkMode;

// ----------------------------------------------------------------------------
// Intersection Observer — reveal on scroll
// ----------------------------------------------------------------------------
function initReveal() {
  const els = document.querySelectorAll(".reveal");
  if (!els.length) return;

  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add("visible");
          observer.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.1, rootMargin: "0px 0px -40px 0px" },
  );

  els.forEach((el) => observer.observe(el));
}

// ----------------------------------------------------------------------------
// Mobile menu
// ----------------------------------------------------------------------------
function initMobileMenu() {
  const btn = document.getElementById("mobile-menu-button");
  const menu = document.getElementById("mobile-menu");
  const overlay = document.getElementById("mobile-menu-overlay");
  const menuIcon = document.getElementById("menu-icon");
  const closeIcon = document.getElementById("close-icon");

  if (!btn || !menu) return;

  function open() {
    const header = document.querySelector("header");
    header?.classList.add("menu-open");
    menu!.classList.remove("hidden");
    overlay?.classList.remove("hidden");
    menuIcon?.classList.add("hidden");
    closeIcon?.classList.remove("hidden");
    btn!.setAttribute("aria-expanded", "true");
    document.body.style.overflow = "hidden";

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        menu!.classList.add("open");
      });
    });
  }

  function close() {
    const header = document.querySelector("header");
    header?.classList.remove("menu-open");
    menu!.classList.remove("open");
    setTimeout(() => {
      menu!.classList.add("hidden");
      overlay?.classList.add("hidden");
      menuIcon?.classList.remove("hidden");
      closeIcon?.classList.add("hidden");
      btn!.setAttribute("aria-expanded", "false");
      document.body.style.overflow = "";
    }, 250);
  }

  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    const expanded = btn.getAttribute("aria-expanded") === "true";
    if (expanded) close();
    else open();
  });
  overlay?.addEventListener("click", close);
  menu.querySelectorAll("a").forEach((a) => a.addEventListener("click", close));
  menu.querySelector("pagefind-modal-trigger")?.addEventListener("click", close);
}

// ----------------------------------------------------------------------------
// Copy command
// ----------------------------------------------------------------------------
async function copyCommand(el: HTMLElement, text: string) {
  try {
    await navigator.clipboard.writeText(text);
    const original = el.innerHTML;
    el.innerHTML =
      '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>';
    el.classList.add("text-green-500");
    setTimeout(() => {
      el.innerHTML = original;
      el.classList.remove("text-green-500");
    }, 1500);
  } catch {
    // Fallback
  }
}
window.copyCommand = copyCommand;

// ----------------------------------------------------------------------------
// Tabs — @mdit/plugin-tab switching
// ----------------------------------------------------------------------------
function initTabs() {
  const wrappers = document.querySelectorAll(".tabs-tabs-wrapper");
  if (!wrappers.length) return;

  wrappers.forEach((wrapper) => {
    const buttons = wrapper.querySelectorAll<HTMLButtonElement>(".tabs-tab-button");
    const panes = wrapper.querySelectorAll<HTMLDivElement>(".tabs-tab-content");

    if (!buttons.length || !panes.length) return;

    // Activate first tab if none is active by default
    const hasActive = [...buttons].some((b) => b.classList.contains("active"));
    if (!hasActive && buttons[0]) {
      buttons[0].classList.add("active");
      buttons[0].setAttribute("data-active", "");
      if (panes[0]) {
        panes[0].classList.add("active");
        panes[0].setAttribute("data-active", "");
      }
    }

    buttons.forEach((btn) => {
      btn.addEventListener("click", () => {
        const idx = btn.getAttribute("data-tab");
        if (idx === null) return;

        buttons.forEach((b) => {
          b.classList.remove("active");
          b.removeAttribute("data-active");
        });
        panes.forEach((p) => {
          p.classList.remove("active");
          p.removeAttribute("data-active");
        });

        btn.classList.add("active");
        btn.setAttribute("data-active", "");
        const pane = panes[Number(idx)];
        if (pane) {
          pane.classList.add("active");
          pane.setAttribute("data-active", "");
        }
      });
    });
  });
}

// ----------------------------------------------------------------------------
// Copy buttons — code blocks, overlay toast
// ----------------------------------------------------------------------------
function initCopyButtons() {
  document.querySelectorAll<HTMLButtonElement>(".copy-btn").forEach((btn) => {
    // Create toast inside button, overlay over content
    const toast = document.createElement("span");
    toast.className = "copy-toast";
    toast.textContent = "Copied!";
    btn.appendChild(toast);

    let timer: ReturnType<typeof setTimeout> | null = null;

    btn.addEventListener("click", async () => {
      const encoded = btn.getAttribute("data-code");
      if (!encoded) return;

      try {
        await navigator.clipboard.writeText(atob(encoded));
      } catch {
        const ta = document.createElement("textarea");
        ta.value = atob(encoded);
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
      }

      btn.classList.add("copying");
      toast.classList.add("visible");
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        btn.classList.remove("copying");
        toast.classList.remove("visible");
      }, 1200);
    });
  });
}

// ----------------------------------------------------------------------------
// Init
// ----------------------------------------------------------------------------
document.addEventListener("DOMContentLoaded", () => {
  initReveal();
  initMobileMenu();
  initTabs();
  initCopyButtons();
  listenSystemTheme();
});
