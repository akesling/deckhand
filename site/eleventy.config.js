export default function (eleventyConfig) {
  // The release version, for the footer picker (scripts/build-site.sh
  // sets it from Cargo.toml; `bun run dev` builds show "dev").
  eleventyConfig.addGlobalData("version", process.env.DECKHAND_VERSION ?? "dev");
  // Decks are data the demo fetches, not pages to template.
  eleventyConfig.ignores.add("src/decks/**");
  // The markdown-sourced content pages, in reading order — mirrored as
  // raw markdown for agents (src/mirrors.njk) and concatenated into
  // /llms-full.txt (src/llms-full.njk). NB: those mirrors ship the
  // page's rawInput, so keep these files plain markdown (no njk tags).
  const readingOrder = ["getting-started", "docs", "why"];
  eleventyConfig.addCollection("pages", (api) =>
    api
      .getFilteredByGlob("src/*.md")
      .sort((a, b) => readingOrder.indexOf(a.page.fileSlug) - readingOrder.indexOf(b.page.fileSlug)),
  );
  eleventyConfig.addPassthroughCopy({ "src/css": "css" });
  // Cloudflare Pages header rules (CORS for versions.json).
  eleventyConfig.addPassthroughCopy({ "src/_headers": "_headers" });
  eleventyConfig.addPassthroughCopy({ "src/decks": "decks" });
  eleventyConfig.addPassthroughCopy({ "dist/js": "js" });
  eleventyConfig.addPassthroughCopy({ "wasm/deckhand_bg.wasm": "wasm/deckhand_bg.wasm" });
  eleventyConfig.addPassthroughCopy({
    "node_modules/@xterm/xterm/css/xterm.css": "css/xterm.css",
  });

  return {
    dir: {
      input: "src",
      output: "_site",
      includes: "_includes",
    },
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk",
  };
}
