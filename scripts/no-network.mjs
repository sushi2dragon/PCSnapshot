// Preloaded via `node --test --import ./scripts/no-network.mjs`.
// Tests here are pure unit tests over the companion-extension logic; nothing
// they cover is allowed to touch the network, loopback included. Any attempt
// fails loudly instead of silently succeeding on a machine that is online.

import { installGuard } from "./network-guard.mjs";

installGuard({ allowLoopback: false });
