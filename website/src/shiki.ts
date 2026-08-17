import langAstro from "@shikijs/langs/astro";
import langBash from "@shikijs/langs/bash";
import langCss from "@shikijs/langs/css";
import langHtml from "@shikijs/langs/html";
import langJavascript from "@shikijs/langs/javascript";
import langJson from "@shikijs/langs/json";
import langMarkdown from "@shikijs/langs/markdown";
import langTsx from "@shikijs/langs/tsx";
import langTypescript from "@shikijs/langs/typescript";
import langYaml from "@shikijs/langs/yaml";
import { fromHighlighter } from "@shikijs/markdown-it/core";
import githubDark from "@shikijs/themes/github-dark";
import githubLight from "@shikijs/themes/github-light";
import type MarkdownIt from "markdown-it";
import { createHighlighterCoreSync } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

/** Map GitHub Dark’s cool blues onto Elph chrome (warm ink / bone / olive). */
const ELPH_DARK_COLORS: Record<string, string> = {
  "#24292e": "#1a1a17",
  "#2f363d": "#252320",
  "#6a737d": "#9a9890",
  "#79b8ff": "#b7c2a0",
  "#85e89d": "#c4c49a",
  "#9ecbff": "#d4cfc2",
  "#b392f0": "#c4b49a",
  "#d1d5da": "#c4c1b8",
  "#dbedff": "#ecebe6",
  "#e1e4e8": "#ecebe6",
  "#f97583": "#d4a090",
  "#fdaeb7": "#e0c4b8",
  "#ffab70": "#c9b089",
};

function remapTheme(theme: typeof githubDark, name: string, map: Record<string, string>) {
  const lookup = (hex: string) => map[hex.toLowerCase()] ?? hex;
  const next = structuredClone(theme) as typeof githubDark & { name: string };
  next.name = name;
  const colors = next.colors as Record<string, string> | undefined;
  if (colors) {
    for (const key of Object.keys(colors)) {
      const value = colors[key];
      if (typeof value === "string" && value.startsWith("#")) colors[key] = lookup(value);
    }
  }
  for (const rule of next.tokenColors ?? []) {
    const settings = rule.settings as { foreground?: string; background?: string };
    if (settings.foreground) settings.foreground = lookup(settings.foreground);
    if (settings.background) settings.background = lookup(settings.background);
  }
  return next;
}

const elphDark = remapTheme(githubDark, "elph-dark", ELPH_DARK_COLORS);

/** Pre-built Shiki highlighter instance */
export const highlighter = createHighlighterCoreSync({
  themes: [githubLight, elphDark],
  langs: [
    langJavascript,
    langTypescript,
    langBash,
    langJson,
    langHtml,
    langCss,
    langYaml,
    langMarkdown,
    langTsx,
    langAstro,
  ],
  engine: createJavaScriptRegexEngine(),
});

/** Register Shiki syntax highlighting on a markdown-it instance */
export function useShiki(md: MarkdownIt): void {
  md.use(
    fromHighlighter(highlighter as any, {
      themes: {
        light: "github-light",
        dark: "elph-dark",
      },
      defaultColor: "light",
    }),
  );
}
