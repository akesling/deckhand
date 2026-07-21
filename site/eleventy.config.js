export default function (eleventyConfig) {
  // Decks are data the demo fetches, not pages to template.
  eleventyConfig.ignores.add("src/decks/**");
  eleventyConfig.addPassthroughCopy({ "src/css": "css" });
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
