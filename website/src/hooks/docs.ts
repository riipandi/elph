import fs from "node:fs";
import path from "node:path";

import { defineHooks } from "vitto";

import { createMarkdownRenderer } from "../markdown.ts";

interface DocItem {
  slug: string;
  title: string;
  description: string;
  content: string;
  order: number;
  section: string;
  sectionLabel: string;
}

interface DocSection {
  id: string;
  label: string;
  items: { slug: string; title: string; order: number }[];
}

const CONTENT_DIR = path.resolve(import.meta.dirname, "../../content/docs");

// Section definitions — folder name = section id, order defines display order
export const SECTIONS: { id: string; label: string; order: number }[] = [
  { id: "start", label: "Start here", order: 1 },
  { id: "customize", label: "Customize", order: 2 },
  { id: "integrate", label: "Integrate", order: 3 },
  { id: "reference", label: "Reference", order: 4 },
];

const md = createMarkdownRenderer();

/**
 * Scan content directory recursively and return all doc items.
 * Folder structure: content/{category}/{slug}.md
 * Slug = "category/slug" (e.g. "getting-started/introduction")
 */
function getAllDocs(): DocItem[] {
  if (!fs.existsSync(CONTENT_DIR)) return [];

  const docs: DocItem[] = [];

  for (const section of SECTIONS) {
    const sectionDir = path.join(CONTENT_DIR, section.id);
    if (!fs.existsSync(sectionDir)) continue;

    const files = fs.readdirSync(sectionDir).filter((f) => f.endsWith(".md"));

    for (const file of files) {
      const filePath = path.join(sectionDir, file);
      const doc = parseDocFile(filePath, section.id, section.label);
      if (doc) docs.push(doc);
    }
  }

  return docs;
}

function parseDocFile(filePath: string, sectionId: string, sectionLabel: string): DocItem | null {
  try {
    const raw = fs.readFileSync(filePath, "utf-8");
    const fileName = path.basename(filePath, ".md");
    const slug = `${sectionId}/${fileName}`;

    // Extract first heading as title
    const titleMatch = raw.match(/^#\s+(.+)$/m);
    const title = titleMatch?.[1]?.trim() || fileName;

    // Extract description (first paragraph after title)
    const descMatch = raw.replace(/^#.+$/m, "").match(/\n\n(.+?)(?:\n\n|$)/);
    const description = descMatch?.[1]?.trim() || "";

    // Convert markdown to HTML with Shiki highlighting (synchronous)
    const content = md.render(raw);

    // Extract order number from filename (optional prefix like "01-")
    const orderMatch = fileName.match(/^(\d+)/);
    const order = orderMatch ? parseInt(orderMatch[1], 10) : 99;

    return {
      slug,
      title,
      description,
      content,
      order,
      section: sectionId,
      sectionLabel,
    };
  } catch {
    return null;
  }
}

function getDocBySlug(slug: string): DocItem | null {
  const filePath = path.join(CONTENT_DIR, `${slug}.md`);
  if (!fs.existsSync(filePath)) return null;

  // Determine section from slug path
  const parts = slug.split("/");
  const sectionId = parts[0] || "";
  const section = SECTIONS.find((s) => s.id === sectionId);
  return parseDocFile(filePath, sectionId, section?.label || "");
}

const SECTION_ITEMS_ORDER: Record<string, string[]> = {
  start: [
    "overview",
    "installation",
    "usage",
    "providers",
    "settings",
    "keybindings",
    "sessions",
    "plan-mode",
    "memory",
    "permissions",
  ],
  customize: ["skills", "mcp", "models", "subagents"],
  integrate: ["acp", "run"],
  reference: ["commands"],
};

function getSections(): DocSection[] {
  const allDocs = getAllDocs();
  return SECTIONS.map((s) => ({
    id: s.id,
    label: s.label,
    items: allDocs
      .filter((d: DocItem) => d.section === s.id)
      .toSorted((a: DocItem, b: DocItem) => {
        const order = SECTION_ITEMS_ORDER[s.id] ?? [];
        return order.indexOf(a.slug.split("/")[1]) - order.indexOf(b.slug.split("/")[1]);
      })
      .map((d: DocItem) => ({ slug: d.slug, title: d.title, order: d.order })),
  }));
}

// Hook for dynamic routes — returns array of docs, or single doc with nav
export default defineHooks("docs", async (params?: { slug?: string }) => {
  if (params?.slug) {
    const doc = getDocBySlug(params.slug);
    if (!doc) return null;

    const sections = getSections();
    // Flatten all items into navigation order
    const flatNav = sections.flatMap((s) =>
      s.items.map((item) => ({ ...item, sectionLabel: s.label })),
    );
    const idx = flatNav.findIndex((item) => item.slug === doc.slug);

    return {
      ...doc,
      sections,
      prev: idx > 0 ? flatNav[idx - 1] : null,
      next: idx >= 0 && idx < flatNav.length - 1 ? flatNav[idx + 1] : null,
    };
  }
  return getAllDocs();
});

// Hook for docs index page — returns sections with docs
export const docsIndex = defineHooks("docsIndex", async () => {
  return { sections: getSections() };
});
