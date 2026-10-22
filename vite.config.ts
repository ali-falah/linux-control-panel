import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { compile, preprocess } from "svelte/compiler";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { relative, sep } from "node:path";
import pkg from "./package.json" with { type: "json" };

const host = process.env.TAURI_DEV_HOST;

/**
 * Same scope hash the Svelte Vite plugin uses for component styles in dev
 * mode (`s-<md5 of the root relative filename>`), see
 * `@sveltejs/vite-plugin-svelte/src/utils/compile.js`.
 */
function svelteCssHash(normalizedFilename: string) {
  const digest = createHash("md5").update(normalizedFilename).digest("base64");
  const safe = digest.replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "").slice(0, 12);
  return `s-${safe}`;
}

/**
 * Tailwind's Vite plugin transforms every module whose id contains
 * `&lang.css` — that includes the style module of every Svelte component.
 *
 * When such a module is requested before the component itself has been
 * compiled (cold dev server, or right after an HMR update) the Svelte plugin
 * has no CSS to hand out and falls through to returning the raw component
 * file. Tailwind then parses the component's `<script>` as CSS and fails with
 * "Invalid declaration: `Snippet`".
 *
 * This plugin makes sure a style request always resolves to valid CSS: it
 * compiles the component with the same options the Svelte plugin uses, so the
 * result is the very same (scoped) CSS the Svelte plugin would serve.
 */
function svelteStyleFallback() {
  let root = process.cwd();

  return {
    name: "svelte-style-fallback",
    enforce: "pre" as const,
    configResolved(config: { root: string }) {
      root = config.root;
    },
    async load(id: string) {
      if (!id.includes("type=style")) return null;
      const file = id.split("?")[0];
      if (!file.endsWith(".svelte")) return null;

      let source: string;
      try {
        source = readFileSync(file, "utf8");
      } catch {
        return null;
      }

      // no <style> block -> the component simply has no styles
      if (!/<style[\s>]/.test(source)) return "/* no component styles */";

      // the file path relative to the Vite root, as the Svelte plugin sees it
      const normalizedFilename = "/" + relative(root, file).split(sep).join("/");

      try {
        // in dev the Svelte plugin appends ` *{}` to the style block, so the
        // scope class is added to every node of the component
        const preprocessed = await preprocess(
          source,
          {
            name: "inject-scope-everything-rule",
            style({ content }: { content: string }) {
              return { code: `${content} *{}` };
            }
          },
          { filename: file }
        );

        const compiled = compile(preprocessed.code, {
          filename: file,
          generate: "client",
          css: "external",
          dev: true,
          hmr: true,
          cssHash: () => svelteCssHash(normalizedFilename)
        });

        return compiled.css?.code ?? "/* no component styles */";
      } catch {
        // let the Svelte plugin report the actual compile error
        return null;
      }
    }
  };
}

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  plugins: [
    svelteStyleFallback(),
    tailwindcss(),
    svelte(),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  // prevent vite from obscuring rust errors
  clearScreen: false,
  build: {
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/codemirror") || id.includes("node_modules/@codemirror")) {
            return "codemirror";
          }
          if (id.includes("node_modules/@lucide/svelte")) {
            return "lucide-icons";
          }
        },
      },
    },
  },
  // tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    cors: true,
    fs: {
      strict: false,
    },
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
          overlay: false,
        }
      : {
          overlay: false,
        },
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
