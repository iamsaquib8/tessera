import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://iamsaquib8.github.io",
  base: "/tessera",
  integrations: [
    starlight({
      title: "Tessera",
      description: "Deterministic edit safety for AI coding agents.",
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
        { label: "Start", items: ["quickstart", "first-five-minutes"] },
        { label: "Use", items: ["integrations", "hooks", "configuration", "troubleshooting"] },
        { label: "Reference", items: ["mcp", "accuracy", "agents", "architecture", "when-not-to-use"] },
        { label: "Contribute", items: ["how-to-add-a-language", "how-to-add-an-agent"] },
        { label: "Roadmap", link: "https://github.com/iamsaquib8/tessera/blob/main/ROADMAP.md" },
      ],
    }),
  ],
});
