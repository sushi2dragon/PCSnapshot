import assert from "node:assert/strict";
import test from "node:test";
import dns from "node:dns";
import http from "node:http";
import net from "node:net";

// Proves the ./scripts/no-network.mjs preload is actually in effect for every
// test process. If the test runner ever stops forwarding --import to its child
// processes, this fails instead of the suite quietly regaining network access.

const denied = (fn) => {
  try {
    fn();
  } catch (err) {
    return err;
  }
  return null;
};

test("fetch is blocked", async () => {
  await assert.rejects(
    () => Promise.resolve().then(() => fetch("https://example.com")),
    (err) => err.code === "ERR_NETWORK_ACCESS_DENIED"
  );
});

test("raw sockets are blocked", () => {
  assert.equal(denied(() => net.connect(80, "example.com"))?.code, "ERR_NETWORK_ACCESS_DENIED");
  assert.equal(
    denied(() => new net.Socket().connect(80, "example.com"))?.code,
    "ERR_NETWORK_ACCESS_DENIED"
  );
});

test("http clients are blocked", () => {
  assert.equal(denied(() => http.get("http://example.com"))?.code, "ERR_NETWORK_ACCESS_DENIED");
});

test("dns resolution is blocked", () => {
  assert.equal(denied(() => dns.lookup("example.com", () => {}))?.code, "ERR_NETWORK_ACCESS_DENIED");
});
