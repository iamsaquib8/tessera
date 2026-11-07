import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://iamsaquib8.github.io",
  base: "/tessera",
  // Pages that moved keep working.
  redirects: {
    "/first-five-minutes": "/tessera/quickstart/",
  },
  integrations: [
    starlight({
      title: "Tessera",
      description: "Deterministic edit safety for AI coding agents.",
      head: [
        {
          tag: "meta",
          attrs: {
            property: "og:image",
            content: "https://iamsaquib8.github.io/tessera/og-image.png",
          },
        },
        { tag: "meta", attrs: { property: "og:image:width", content: "1280" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "640" } },
        {
          tag: "meta",
          attrs: {
            property: "og:image:alt",
            content: "Tessera — deterministic edit safety for AI coding agents.",
          },
        },
        {
          tag: "meta",
          attrs: {
            name: "twitter:image",
            content: "https://iamsaquib8.github.io/tessera/og-image.png",
          },
        },
        {
          tag: "meta",
          attrs: { name: "twitter:card", content: "summary_large_image" },
        },
      ],
      logo: {
        light: "./src/assets/logo-light.svg",
        dark: "./src/assets/logo-dark.svg",
        replacesTitle: true,
      },
      favicon: "/favicon.svg",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/iamsaquib8/tessera" }],
      editLink: { baseUrl: "https://github.com/iamsaquib8/tessera/edit/main/docs/" },
      customCss: [
        "@fontsource-variable/archivo/wdth.css",
        "@fontsource/atkinson-hyperlegible/400.css",
        "@fontsource/atkinson-hyperlegible/700.css",
        "@fontsource-variable/jetbrains-mono",
        "./src/styles/tessera.css",
      ],
      sidebar: [
        { label: "Start", items: ["quickstart", "troubleshooting"] },
        {
          label: "Guides",
          items: [
            { slug: "guides", label: "All guides" },
            "guides/connect-your-agent",
            "guides/check-before-you-edit",
            "guides/catch-what-an-agent-broke",
            "guides/run-only-affected-tests",
            "guides/review-a-pull-request",
            "guides/adopt-on-a-legacy-repo",
            "guides/block-breaking-api-changes",
            "guides/monorepo-and-several-repos",
            "guides/understand-a-new-codebase",
          ],
        },
        {
          label: "Reference",
          items: ["integrations", "hooks", "ci", "contracts", "configuration", "mcp", "accuracy", "when-not-to-use", "architecture"],
        },
        { label: "Contribute", items: ["how-to-add-a-language", "how-to-add-an-agent", "agents"] },
        {
          label: "Project",
          items: [
            "stability",
            "roadmap",
            "landscape",
            { label: "Changelog", link: "https://github.com/iamsaquib8/tessera/blob/main/CHANGELOG.md" },
          ],
        },
      ],
    }),
  ],
});
