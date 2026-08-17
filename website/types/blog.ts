/** A parsed blog post with rendered content */
export interface BlogPost {
  id: number;
  slug: string;
  title: string;
  description: string;
  tags: string[];
  author: string;
  created: string;
  updated: string;
  dateDisplay: string;
  updatedDisplay: string;
  content: string;
  body: string;
  prev?: { slug: string; title: string };
  next?: { slug: string; title: string };
}

/** Raw frontmatter fields from a blog markdown file */
export interface BlogFrontmatter {
  title?: string;
  description?: string;
  tags?: string[];
  author?: string;
  created?: string;
  updated?: string;
  slug?: string;
  [key: string]: unknown;
}
