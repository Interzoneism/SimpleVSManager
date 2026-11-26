// Checks database.rules.json against the writes Simple VS Manager (v3 and 2.x) makes.
// Run from this folder: `npm install`, then `npm test` (starts the Realtime Database emulator;
// needs Java 11+). Nothing here talks to the real database.

import { readFileSync } from "node:fs";
import { after, before, beforeEach, test } from "node:test";
import { assertFails, assertSucceeds, initializeTestEnvironment } from "@firebase/rules-unit-testing";

let env;
const TS = { ".sv": "timestamp" };

before(async () => {
  env = await initializeTestEnvironment({
    projectId: "demo-svsm",
    database: { rules: readFileSync(process.env.RULES ?? new URL("../database.rules.json", import.meta.url), "utf8"), host: "127.0.0.1", port: 9000 },
  });
});
after(async () => env?.cleanup());
beforeEach(async () => env.clearDatabase());

const db = (uid) => env.authenticatedContext(uid).database();
const admin = (fn) => env.withSecurityRulesDisabled((ctx) => fn(ctx.database()));

function content(player, extra = {}) {
  return { name: "My list", description: "Nice", uploader: "Alice", uploaderName: "Alice", uploaderId: player, gameVersion: "1.21.5", mods: [{ modId: "carryon", version: "1.8.0" }], ...extra };
}

function summary(c, extra = {}) {
  const { uploaderId, configurations, ...rest } = c;
  return { content: { ...rest, vsVersion: c.gameVersion }, dateAdded: "2026-11-26T10:00:00.000000Z", revision: 1, createdAt: "2026-11-26T10:00:00.000000Z", updatedAt: "2026-11-26T10:00:00.000000Z", ...extra };
}

/** The multi-location PATCH `CloudStore::publish` sends. */
function publishPatch(uid, player, slot, id, c = content(player)) {
  return {
    [`/users/${player}/${slot}`]: { registryId: id, content: c, dateAdded: "2026-11-26T10:00:00Z", revision: 1, createdAt: "x", updatedAt: "x" },
    [`/registryOwners/${id}`]: uid,
    [`/registry/${id}`]: { content: c, dateAdded: "2026-11-26T10:00:00Z", revision: 1, createdAt: "x", updatedAt: "x" },
    [`/registrySummaries/${id}`]: summary(c),
  };
}

async function alicePublishes(id = "rid1") {
  await assertSucceeds(db("alice").ref("owners/PlayerA").set("alice"));
  await assertSucceeds(db("alice").ref().update(publishPatch("alice", "PlayerA", "slot1", id)));
}

test("owner claims and publishes; re-publishing works", async () => {
  await alicePublishes();
  await assertSucceeds(db("alice").ref().update(publishPatch("alice", "PlayerA", "slot1", "rid1", content("PlayerA", { description: "Changed" }))));
});

test("2.x-shaped publish (no icon, revision or timestamps in the summary) still works", async () => {
  await assertSucceeds(db("alice").ref("owners/PlayerA").set("alice"));
  const c = content("PlayerA", { configurations: [{ path: "carryon.json", content: "{}" }], version: "1.0.0" });
  const patch = publishPatch("alice", "PlayerA", "slot2", "rid2", c);
  patch["/registrySummaries/rid2"] = { content: { name: c.name, description: c.description, version: "1.0.0", uploader: "Alice", uploaderName: "Alice", gameVersion: c.gameVersion, vsVersion: c.gameVersion, mods: c.mods }, dateAdded: "2026-11-26T10:00:00Z" };
  await assertSucceeds(db("alice").ref().update(patch));
});

test("someone else cannot overwrite or take over a published list", async () => {
  await alicePublishes();
  await assertSucceeds(db("bob").ref("owners/PlayerB").set("bob"));
  await assertFails(db("bob").ref().update(publishPatch("bob", "PlayerB", "slot1", "rid1", content("PlayerB"))));
  await assertFails(db("bob").ref("registry/rid1").set({ content: content("PlayerB") }));
  await assertFails(db("bob").ref("registryOwners/rid1").set("bob"));
});

test("a list cannot be published under another player's id", async () => {
  await assertSucceeds(db("alice").ref("owners/PlayerA").set("alice"));
  await assertSucceeds(db("bob").ref("owners/PlayerB").set("bob"));
  const patch = publishPatch("bob", "PlayerB", "slot1", "rid9", content("PlayerA"));
  await assertFails(db("bob").ref().update(patch));
});

test("an owner cannot switch the uploader to another player later (was allowed before)", async () => {
  await alicePublishes();
  await assertSucceeds(db("bob").ref("owners/PlayerB").set("bob"));
  await assertFails(db("alice").ref("registry/rid1/content/uploaderId").set("PlayerB"));
  await assertFails(db("alice").ref("registry/rid1").update({ content: content("PlayerB") }));
});

test("a registry entry can't be created without owning it in the same write (was first-come before)", async () => {
  await assertSucceeds(db("bob").ref("owners/PlayerB").set("bob"));
  await assertFails(db("bob").ref("registry/fresh").set({ content: content("PlayerB") }));
  // A leftover owner record blocks reuse of the id by anyone else.
  await admin((d) => d.ref("registryOwners/stale").set("alice"));
  await assertFails(db("bob").ref().update(publishPatch("bob", "PlayerB", "slot1", "stale", content("PlayerB"))));
});

test("summaries reject oversized text and unknown fields", async () => {
  await assertSucceeds(db("alice").ref("owners/PlayerA").set("alice"));
  const big = publishPatch("alice", "PlayerA", "slot1", "rid1");
  big["/registrySummaries/rid1"].content.description = "x".repeat(1001);
  await assertFails(db("alice").ref().update(big));
  const extra = publishPatch("alice", "PlayerA", "slot1", "rid1");
  extra["/registrySummaries/rid1"].content.payload = "x".repeat(500000);
  await assertFails(db("alice").ref().update(extra));
  const modExtra = publishPatch("alice", "PlayerA", "slot1", "rid1");
  modExtra["/registrySummaries/rid1"].content.mods[0].junk = "x";
  await assertFails(db("alice").ref().update(modExtra));
  const longName = publishPatch("alice", "PlayerA", "slot1", "rid1", content("PlayerA", { name: "n".repeat(101) }));
  await assertFails(db("alice").ref().update(longName));
});

test("subscribing, owner notices and acknowledging them", async () => {
  await alicePublishes();
  const sub = { "/modlistSubscriptions/rid1/bob": TS, "/modlistSubscriptionNotices/rid1/bob": TS };
  await assertSucceeds(db("bob").ref().update(sub));
  await assertFails(db("alice").ref().update({ "/modlistSubscriptions/rid1/alice": TS }), "not to your own list");
  await assertFails(db("carol").ref("modlistSubscriptionNotices/rid1").once("value"));
  await assertSucceeds(db("alice").ref("modlistSubscriptionNotices/rid1").once("value"));
  // CloudStore::acknowledge_notices clears the whole list node (denied by the old rules).
  await assertFails(db("bob").ref().update({ "/modlistSubscriptionNotices/rid1": null }));
  await assertSucceeds(db("alice").ref().update({ "/modlistSubscriptionNotices/rid1": null }));
  await assertSucceeds(db("bob").ref("modlistSubscriptions/rid1/bob").remove());
});

test("deleting a slot removes the list, its owner record and its subscriptions", async () => {
  await alicePublishes();
  await assertSucceeds(db("bob").ref().update({ "/modlistSubscriptions/rid1/bob": TS, "/modlistSubscriptionNotices/rid1/bob": TS }));
  const del = { "/users/PlayerA/slot1": null };
  for (const p of ["registry", "registryOwners", "registrySummaries", "modlistSubscriptions", "modlistSubscriptionNotices"]) del[`/${p}/rid1`] = null;
  await assertFails(db("bob").ref().update(del));
  await assertSucceeds(db("alice").ref().update(del));
  // 2.x deletes without the subscription paths.
  await alicePublishes("rid2");
  const del2 = { "/users/PlayerA/slot1": null, "/registry/rid2": null, "/registryOwners/rid2": null, "/registrySummaries/rid2": null };
  await assertSucceeds(db("alice").ref().update(del2));
});

test("compatibility votes", async () => {
  const ok = { option: "notWorking", vintageStoryVersion: "1.21.5", updatedUtc: "2026-11-26T10:00:00Z", comment: "Crashes on load" };
  await assertSucceeds(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/alice").set(ok));
  await assertFails(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/bob").set(ok));
  await assertFails(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/alice").set({ ...ok, comment: " " }));
  await assertFails(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/alice").set({ ...ok, comment: "x".repeat(1001) }));
  await assertFails(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/alice").set({ ...ok, extra: "x" }));
  await assertFails(db("alice").ref(`compatVotes/${"k".repeat(201)}/MS44LjA/users/alice`).set(ok));
  await assertSucceeds(db("alice").ref("compatVotes/Y2Fycnlvbg/MS44LjA/users/alice").remove());
});

test("player slots: owner only, five slots", async () => {
  await assertSucceeds(db("alice").ref("owners/PlayerA").set("alice"));
  await assertFails(db("bob").ref("owners/PlayerA").set("bob"));
  await assertFails(db("bob").ref("users/PlayerA").once("value"));
  await assertFails(db("alice").ref("users/PlayerA/slot6").set({ content: { name: "x" } }));
  await assertSucceeds(db("alice").ref("users/PlayerA/slot5").set({ content: { name: "x" } }));
  await assertSucceeds(db("alice").ref("users/PlayerA/slot5").remove());
  await assertSucceeds(db("alice").ref("owners/PlayerA").remove());
});
