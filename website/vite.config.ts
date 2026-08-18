import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import vitto from "vitto";

import docsHook from "./src/hooks/docs.ts";
import postsHook from "./src/hooks/posts.ts";
import type { BlogPost } from "./types/blog.ts";

const isProduction = process.env.NODE_ENV === "production";

export default defineConfig({
  plugins: [
    vitto({
      minify: isProduction,
      enableSearchIndex: true,
      outputStrategy: "html",
      metadata: {
        siteName: "Elph",
        title: "Elph — opinionated AI coding agent harness",
        description:
          "Opinionated AI coding agent harness. Extensible TUI, mouse support, minimal overhead, with maximum extensibility and control.",
        keywords: ["elph", "coding agent", "cli", "tui", "rust", "ai agent", "acp"],
        author: "Aris Ripandi",
        language: "en",
        url: "https://elph.space",
        social: {
          github: "https://github.com/riipandi/elph",
          x: "https://x.com/intent/follow?screen_name=riipandi",
        },
      },
      hooks: {
        docs: docsHook,
        doc: docsHook,
        posts: postsHook,
        post: postsHook,
      },
      dynamicRoutes: [
        {
          template: "doc",
          dataSource: "docs",
          getParams: (doc: any) => ({ slug: doc.slug }),
          getPath: (doc: any) => `docs/${doc.slug}.html`,
        },
        {
          template: "post",
          dataSource: "posts",
          getParams: (post: BlogPost) => ({ slug: post.slug }),
          getPath: (post: BlogPost) => `news/${post.slug}.html`,
        },
        {
          template: "news",
          dataSource: "posts",
          pageSize: 5,
          getParams: (pageNum: number) => ({ _page: pageNum }),
          getPath: (pageNum: number) => (pageNum === 1 ? "news.html" : `news/${pageNum}.html`),
        },
      ],
    }),
    tailwindcss(),
  ],
  build: {
    minify: isProduction,
    chunkSizeWarningLimit: 1024 * 4,
    reportCompressedSize: false,
    emptyOutDir: true,
    manifest: true,
  },
});
