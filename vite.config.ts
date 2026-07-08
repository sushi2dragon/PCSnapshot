import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    host: '0.0.0.0',   // listen on all interfaces (IPv4 + IPv6) so WebView2 can reach it
    port: 5173,
    strictPort: true,  // fail fast if 5173 is taken rather than silently shifting ports
  },
})
