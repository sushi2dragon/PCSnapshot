// Shared outbound-network guard, preloaded via `node --import`.
//
// Two policies:
//   installGuard({ allowLoopback: false })  — used by `npm test`. Nothing at all.
//   installGuard({ allowLoopback: true })   — used by `npm run dev`. The dev
//     server must still talk to itself (HMR, the Tauri webview, proxying), so
//     127.0.0.1/::1/localhost egress is permitted and everything else is not.
//
// Only *outbound* connections are policed. `server.listen` is untouched — a
// dev server that can't bind is a dev server that can't run.

import dns from "node:dns";
import dnsPromises from "node:dns/promises";
import http from "node:http";
import https from "node:https";
import net from "node:net";
import tls from "node:tls";
import dgram from "node:dgram";

export class NetworkAccessDeniedError extends Error {
  constructor(api, target, allowLoopback) {
    super(
      `Network access is disabled: ${api}${target ? ` → ${target}` : ""}. ` +
        (allowLoopback
          ? "Only loopback (localhost, 127.0.0.1, ::1) is permitted here."
          : "Stub the boundary instead of making a real request.")
    );
    this.name = "NetworkAccessDeniedError";
    this.code = "ERR_NETWORK_ACCESS_DENIED";
  }
}

const LOOPBACK_NAMES = new Set(["localhost", "127.0.0.1", "::1", "[::1]", "0.0.0.0", "::"]);

function isLoopback(host) {
  if (!host) return true; // no host == this machine (net defaults to localhost)
  const h = String(host).toLowerCase().replace(/^\[|\]$/g, "");
  return LOOPBACK_NAMES.has(h) || h.startsWith("127.") || h.endsWith(".localhost");
}

// Pull the destination host out of the many shapes these APIs accept.
// Returns null for non-network targets (unix sockets, Windows named pipes),
// which are always allowed — they are IPC, not network.
function targetHost(args) {
  const [first, second] = args;
  if (typeof first === "number") return typeof second === "string" ? second : "";
  if (typeof first === "string") {
    if (/^https?:\/\//i.test(first)) {
      try {
        return new URL(first).hostname;
      } catch {
        return first;
      }
    }
    return null; // socket path / named pipe
  }
  if (first && typeof first === "object") {
    if (first instanceof URL) return first.hostname;
    if (typeof first.path === "string" && !("port" in first)) return null;
    return first.hostname ?? first.host ?? "";
  }
  return "";
}

export function installGuard({ allowLoopback = false } = {}) {
  const gate = (api, extract) => (original) =>
    function guarded(...args) {
      const host = extract ? extract(args) : targetHost(args);
      if (host === null) return original.apply(this, args); // IPC, not network
      if (allowLoopback && isLoopback(host)) return original.apply(this, args);
      throw new NetworkAccessDeniedError(api, host, allowLoopback);
    };

  const wrap = (obj, key, api, extract) => {
    const original = obj[key];
    obj[key] = gate(api, extract)(original);
  };

  // Sockets — the floor every higher-level client lands on.
  wrap(net.Socket.prototype, "connect", "net.Socket#connect");
  wrap(net, "connect", "net.connect");
  wrap(net, "createConnection", "net.createConnection");
  wrap(tls, "connect", "tls.connect");
  wrap(tls, "createConnection", "tls.createConnection");

  // Higher-level clients — wrapped too so a denial names the API in use.
  wrap(http, "request", "http.request");
  wrap(http, "get", "http.get");
  wrap(https, "request", "https.request");
  wrap(https, "get", "https.get");

  // Name resolution. Resolving a loopback name is part of binding locally, so
  // it follows the same policy rather than being denied outright.
  const nameOf = (args) => (typeof args[0] === "string" ? args[0] : "");
  wrap(dns, "lookup", "dns.lookup", nameOf);
  wrap(dns, "resolve", "dns.resolve", nameOf);
  wrap(dnsPromises, "lookup", "dns.promises.lookup", nameOf);
  wrap(dnsPromises, "resolve", "dns.promises.resolve", nameOf);

  // UDP — patch per-socket, since the constructor itself is harmless.
  const createSocket = dgram.createSocket.bind(dgram);
  dgram.createSocket = (...args) => {
    const socket = createSocket(...args);
    wrap(socket, "connect", "dgram.Socket#connect", (a) => (typeof a[1] === "string" ? a[1] : ""));
    wrap(socket, "send", "dgram.Socket#send", (a) => a.find((x) => typeof x === "string") ?? "");
    return socket;
  };

  // Globals. fetch/WebSocket go through undici's own connector rather than the
  // public `net` surface, so they are wrapped directly.
  const originalFetch = globalThis.fetch;
  globalThis.fetch = function guardedFetch(input, init) {
    const host = targetHost([input]);
    if (host !== null && !(allowLoopback && isLoopback(host))) {
      throw new NetworkAccessDeniedError("fetch", host, allowLoopback);
    }
    return originalFetch.call(this, input, init);
  };
  for (const name of ["WebSocket", "EventSource", "XMLHttpRequest"]) {
    const Original = globalThis[name];
    if (!Original) continue;
    globalThis[name] = class Guarded extends Original {
      constructor(url, ...rest) {
        const host = targetHost([url]);
        if (host !== null && !(allowLoopback && isLoopback(host))) {
          throw new NetworkAccessDeniedError(name, host, allowLoopback);
        }
        super(url, ...rest);
      }
    };
  }
}
