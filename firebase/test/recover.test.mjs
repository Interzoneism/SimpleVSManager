// Checks admin/recover-account.mjs against the Realtime Database emulator (`npm test`).
// The Vintage Story check is tested with a fake fetch; nothing here talks to a real server.

import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";
import { initializeApp } from "firebase-admin/app";
import { getDatabase } from "firebase-admin/database";
import { parseValid, planRecovery, sanitizeKey, validateSession } from "../admin/recover-account.mjs";

process.env.FIREBASE_DATABASE_EMULATOR_HOST ??= "127.0.0.1:9000";
let app, db;
before(() => {
  app = initializeApp({ projectId: "demo-svsm", databaseURL: "http://127.0.0.1:9000?ns=demo-svsm-recover" }, "recover-test");
  db = getDatabase(app);
});
after(async () => app?.delete());
beforeEach(async () => db.ref().set(null));

const list = (player, name) => ({ content: { name, uploaderId: player } });

async function seed() {
  await db.ref().set({
    owners: { PlayerZ: "x", PlayerA: "old", PlayerB: "other" },
    users: { PlayerA: { slot1: { registryId: "r1", content: { name: "Mine" } } } },
    registry: { r1: list("PlayerA", "Mine"), r2: list("PlayerB", "Bob's"), r3: list("PlayerX", "Odd one") },
    registrySummaries: { r1: { content: { name: "Mine" }, dateAdded: "x" } },
    registryOwners: { r1: "old", r2: "other", r3: "old", r4: "old" },
    modlistSubscriptions: { r2: { old: 5, someone: 6 }, r1: { someone: 7 } },
    modlistSubscriptionNotices: { r2: { old: 5 } },
    compatVotes: {
      carryon: {
        "1_0_0": { users: { old: { option: "fullyFunctional", vintageStoryVersion: "1.21", updatedUtc: "2026-01-02" }, fresh: { option: "notWorking", vintageStoryVersion: "1.21", updatedUtc: "2026-01-01", comment: "x" } } },
        "2_0_0": { users: { someone: { option: "fullyFunctional", vintageStoryVersion: "1.21", updatedUtc: "2026-01-01" } } },
      },
    },
  });
}

test("moves the player's owner record, lists, subscriptions and votes", async () => {
  await seed();
  const plan = await planRecovery(db, { player: "PlayerA", newAuth: "fresh" });
  assert.equal(plan.report.oldAuth, "old");
  assert.deepEqual(plan.report.lists.map((l) => l.id).sort(), ["r1", "r4"]);
  assert.deepEqual(plan.report.skippedLists.map((l) => l.id), ["r3"]);
  await db.ref().update(plan.updates);

  const v = (await db.ref().get()).val();
  assert.equal(v.owners.PlayerA, "fresh");
  assert.equal(v.owners.PlayerB, "other");
  assert.deepEqual(v.registryOwners, { r1: "fresh", r2: "other", r3: "old", r4: "fresh" });
  assert.deepEqual(v.modlistSubscriptions.r2, { fresh: 5, someone: 6 });
  assert.deepEqual(v.modlistSubscriptionNotices.r2, { fresh: 5 });
  assert.equal(v.compatVotes.carryon["1_0_0"].users.fresh.option, "fullyFunctional", "newer vote wins");
  assert.equal(v.compatVotes.carryon["1_0_0"].users.old, undefined);
  assert.ok(v.users.PlayerA.slot1, "slots stay keyed by the player");
});

test("a dry run writes nothing", async () => {
  await seed();
  const before = (await db.ref().get()).val();
  await planRecovery(db, { player: "PlayerA", newAuth: "fresh" });
  assert.deepEqual((await db.ref().get()).val(), before);
});

test("leaves installation data alone when the old installation had other players", async () => {
  await seed();
  await db.ref("owners/PlayerC").set("old");
  const plan = await planRecovery(db, { player: "PlayerA", newAuth: "fresh" });
  assert.equal(plan.report.installDataSkipped, true);
  assert.deepEqual(Object.keys(plan.updates).sort(), ["owners/PlayerA", "registryOwners/r1", "registryOwners/r4"]);
  const all = await planRecovery(db, { player: "PlayerA", newAuth: "fresh", includeInstallData: true });
  assert.equal(all.report.votes, 1);
});

test("unclaimed player only gets the owner record; already moved is a no-op", async () => {
  const plan = await planRecovery(db, { player: "New/Player", newAuth: "fresh" });
  assert.deepEqual(plan.updates, { "owners/New_Player": "fresh" });
  await db.ref().update(plan.updates);
  assert.equal((await planRecovery(db, { player: "New/Player", newAuth: "fresh" })).nothingToDo, true);
});

test("session check sends the form and reads the answer", async () => {
  assert.equal(sanitizeKey("a.b$c#d[e]f/g"), "a_b_c_d_e_f_g");
  for (const [valid, want] of [[1, true], [true, true], ["1", true], [0, false], [false, false], [undefined, false]]) assert.equal(parseValid({ valid }), want);
  let sent;
  const fetchImpl = async (url, init) => {
    sent = { url, init };
    return new Response(JSON.stringify({ valid: 1 }), { status: 200 });
  };
  assert.equal(await validateSession("uid+1", "se/cret", { fetchImpl }), true);
  assert.equal(sent.url, "https://auth3.vintagestory.at/clientvalidate");
  assert.equal(sent.init.body, "uid=uid%2B1&sessionkey=se%2Fcret");
  const failing = async () => new Response("nope", { status: 500 });
  await assert.rejects(validateSession("u", "se/cret", { fetchImpl: failing }), (e) => !e.message.includes("se/cret"));
});
