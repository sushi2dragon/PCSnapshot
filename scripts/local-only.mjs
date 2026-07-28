// Preloaded via `node --import ./scripts/local-only.mjs` in front of Vite.
// The dev server may bind and talk to itself (HMR socket, the Tauri webview,
// module graph requests) but may not reach anything off this machine — no
// registry lookups, no CDN fetches, no telemetry. Egress to a non-loopback
// host throws ERR_NETWORK_ACCESS_DENIED naming the API and the destination.

import { installGuard } from "./network-guard.mjs";

installGuard({ allowLoopback: true });
