import path from "node:path";
import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
import { redact } from "@tanstack/redact/vite";
import react from "@vitejs/plugin-react";
import { visualizer } from "rollup-plugin-visualizer";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;
const virtualizerHookFile = /use-virtual-window\.ts$/;

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
	plugins: [
		react({
			compiler: { logDiagnostics: mode === "development" },
			// The only module that calls TanStack Virtual. It must stay uncompiled: the compiler would cache a view on the
			// virtualizer's stable instance and freeze the scroll window. Its callers receive plain values instead, so
			// they stay compiled. Excluding just this hook keeps their diagnostics meaningful and does not cost HMR.
			exclude: [virtualizerHookFile],
		}),
		redact({
			preset: "full",
			features: {
				// Client-only app: no hydration, Activity or ViewTransition. Class components stay on because the app's
				// ErrorBoundary is one: the stub renderer drops getDerivedStateFromError/componentDidCatch, which would
				// leave a render failure with no fallback.
				viewTransitions: false,
				hydration: false,
				classComponents: true,
				activity: false,
				fragmentRefs: false,
			},
		}),
		tailwindcss(),
		mode === "analyze" && visualizer({ open: true, filename: "dist/stats.html" }),
	].filter(Boolean),

	// Tauri webviews (WebKitGTK, WebView2) are evergreen; ES2023 output skips downleveling.
	// No manualChunks: the screens are lazy, so Rolldown already keeps heavy libraries (the drag-and-drop kit, the
	// console, the inspector) in the chunk that needs them. A manual vendor chunk would pull them into the first paint.
	build: {
		target: "es2023",
		minify: "oxc",
		cssMinify: "lightningcss",
		reportCompressedSize: false,
		chunkSizeWarningLimit: 600,
	},

	optimizeDeps: {
		// react and react-dom are aliased to @tanstack/redact, so the plugin removes them from this list.
		include: [
			"@dnd-kit/core",
			"@dnd-kit/sortable",
			"@dnd-kit/utilities",
			"@tauri-apps/api/core",
			"@tauri-apps/api/event",
			"@tauri-apps/api/webview",
			"@tauri-apps/api/window",
			"@tauri-apps/plugin-dialog",
			"class-variance-authority",
			"cn",
			"lucide-react",
			"radix-ui",
			"react-resizable-panels",
		],
	},

	resolve: {
		alias: {
			"@": path.resolve(import.meta.dirname, "./src"),
		},
	},

	// Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
	//
	// 1. prevent Vite from obscuring rust errors
	clearScreen: false,
	// 2. tauri expects a fixed port, fail if that port is not available
	server: {
		port: 1420,
		strictPort: true,
		// An empty TAURI_DEV_HOST must fall back to `false`, so `??` would not do; `||` is the point here.
		// oxlint-disable-next-line typescript/prefer-nullish-coalescing
		host: host || false,
		hmr: host
			? {
					protocol: "ws",
					host,
					port: 1421,
				}
			: undefined,
		warmup: {
			// Pre-transform the first screen so the dev server does not waterfall on startup.
			clientFiles: [
				"./src/main.tsx",
				"./src/app/index.tsx",
				"./src/app/app-providers.tsx",
				"./src/app/shell/app-shell.tsx",
				"./src/app/pages/mods-page.tsx",
				"./src/index.css",
				"./src/views/mods/mods-view.tsx",
				"./src/views/mod-table/mod-table.tsx",
				"./src/views/mod-table/mod-row.tsx",
				"./src/components/game-rail.tsx",
				"./src/components/ui/*.tsx",
			],
		},
		watch: {
			// 3. tell Vite to ignore watching `src-tauri` and build artefacts
			ignored: ["**/src-tauri/**", "**/target/**", "**/dist-release/**", "**/design/**"],
		},
	},
}));
