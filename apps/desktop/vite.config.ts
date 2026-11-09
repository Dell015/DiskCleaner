import { defineConfig, type Plugin } from "vite";

// The built page may only load its own files: no outside scripts, no inline code, no network.
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "worker-src 'self'",
  "style-src 'self'",
  "img-src 'self' data:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

function csp(): Plugin {
  return {
    name: "dc-csp",
    apply: "build",
    transformIndexHtml(html) {
      return html.replace("<head>", `<head>\n<meta http-equiv="Content-Security-Policy" content="${CSP}">`);
    },
  };
}

export default defineConfig({
  plugins: [csp()],
  worker: { format: "es" },
  build: { target: "es2022", sourcemap: false },
});
