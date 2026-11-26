#!/usr/bin/env node
// Moves a player's cloud modlists to a new installation (a new firebase-auth.json) after the
// player proved they own the Vintage Story account. Run it on your own PC; see ../README.md.
//
//   node admin/recover-account.mjs --player <playeruid> --new-auth <firebase uid> [--dry-run]
//
// The player's session key is read from a hidden prompt (or the VS_SESSION_KEY environment
// variable). It is only sent to the Vintage Story auth server and is never printed or saved.

import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const VALIDATE_URL = "https://auth3.vintagestory.at/clientvalidate";
export const DEFAULT_DB_URL = "https://simplevsmanager-default-rtdb.europe-west1.firebasedatabase.app";
const DEFAULT_KEY_FILE = new URL("./service-account.json", import.meta.url);

/** Same as `sanitize_key` in the app: Firebase keys can't hold `. $ # [ ] /` or control chars. */
export function sanitizeKey(s) {
  return [...s].map((c) => (".$#[]/".includes(c) || c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127 ? "_" : c)).join("");
}

/** The auth server answers `{"valid": 1}` (or true / "1") for a live session. */
export function parseValid(body) {
  const v = body?.valid;
  return v === true || v === 1 || v === "1" || v === "true";
}

/**
 * Asks the Vintage Story auth server whether `sessionKey` is a live session of `playerUid`.
 * Errors never include the key.
 */
export async function validateSession(playerUid, sessionKey, { fetchImpl = fetch, url = VALIDATE_URL } = {}) {
  let res;
  try {
    res = await fetchImpl(url, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ uid: playerUid, sessionkey: sessionKey }).toString(),
      redirect: "manual",
      signal: AbortSignal.timeout(20_000),
    });
  } catch (e) {
    throw new Error(`Couldn't reach the Vintage Story auth server (${e.name}).`);
  }
  if (!res.ok) throw new Error(`The Vintage Story auth server answered HTTP ${res.status}.`);
  let body;
  try {
    body = await res.json();
  } catch {
    throw new Error("The Vintage Story auth server sent something that isn't JSON.");
  }
  return parseValid(body);
}

const entries = (snap) => Object.entries(snap.val() ?? {});

/**
 * Works out what moves from the old installation to `newAuth`. Reads only.
 * Returns `{ oldAuth, updates, report }`; `updates` is one multi-path update for `ref().update`.
 */
export async function planRecovery(db, { player, newAuth, includeInstallData = false }) {
  const key = sanitizeKey(player);
  const report = { player: key, newAuth, oldAuth: null, lists: [], skippedLists: [], otherPlayers: [], subscriptions: 0, notices: 0, votes: 0, installDataSkipped: false };
  const updates = {};

  const oldAuth = (await db.ref(`owners/${key}`).get()).val();
  report.oldAuth = typeof oldAuth === "string" ? oldAuth : null;
  if (report.oldAuth === newAuth) return { oldAuth, updates, report, nothingToDo: true };
  updates[`owners/${key}`] = newAuth;
  if (!report.oldAuth) return { oldAuth: null, updates, report };

  // Public lists: registryOwners/{id} = old auth uid, and the entry was uploaded by this player.
  for (const [id, owner] of entries(await db.ref("registryOwners").get())) {
    if (owner !== oldAuth) continue;
    const uploaderId = (await db.ref(`registry/${id}/content/uploaderId`).get()).val();
    const name = (await db.ref(`registrySummaries/${id}/content/name`).get()).val() ?? (await db.ref(`registry/${id}/content/name`).get()).val();
    if (uploaderId === key || uploaderId == null) {
      updates[`registryOwners/${id}`] = newAuth;
      report.lists.push({ id, name: name ?? "(no name)", orphan: uploaderId == null });
    } else {
      report.skippedLists.push({ id, name: name ?? "(no name)", uploaderId });
    }
  }

  // Subscriptions, notices and votes are keyed by the installation, not the player. Only move
  // them when the old installation belonged to this player alone.
  report.otherPlayers = entries(await db.ref("owners").get()).filter(([p, a]) => a === oldAuth && p !== key).map(([p]) => p);
  if (report.otherPlayers.length && !includeInstallData) {
    report.installDataSkipped = true;
    return { oldAuth, updates, report };
  }

  for (const [node, counter] of [["modlistSubscriptions", "subscriptions"], ["modlistSubscriptionNotices", "notices"]]) {
    for (const [id, subs] of entries(await db.ref(node).get())) {
      if (!subs || typeof subs !== "object" || !(oldAuth in subs)) continue;
      // The owner can't subscribe to their own list, and a moved list may now be theirs.
      const ownsList = updates[`registryOwners/${id}`] === newAuth;
      if (!(newAuth in subs) && !ownsList) updates[`${node}/${id}/${newAuth}`] = subs[oldAuth];
      updates[`${node}/${id}/${oldAuth}`] = null;
      report[counter]++;
    }
  }

  for (const [modKey, versions] of entries(await db.ref("compatVotes").get())) {
    for (const [versionKey, node] of Object.entries(versions ?? {})) {
      const vote = node?.users?.[oldAuth];
      if (!vote) continue;
      const existing = node.users[newAuth];
      if (!existing || String(existing.updatedUtc ?? "") < String(vote.updatedUtc ?? "")) {
        updates[`compatVotes/${modKey}/${versionKey}/users/${newAuth}`] = vote;
      }
      updates[`compatVotes/${modKey}/${versionKey}/users/${oldAuth}`] = null;
      report.votes++;
    }
  }
  return { oldAuth, updates, report };
}

function printReport({ report, nothingToDo }, dryRun) {
  const log = (s = "") => console.log(s);
  log(`Player:            ${report.player}`);
  log(`Old installation:  ${report.oldAuth ?? "(none, the player's slots were never claimed)"}`);
  log(`New installation:  ${report.newAuth}`);
  if (nothingToDo) {
    log("\nThe new installation already owns this player. Nothing to do.");
    return;
  }
  log(`\nPublic modlists moving: ${report.lists.length}`);
  for (const l of report.lists) log(`  ${l.id}  ${l.name}${l.orphan ? "  (entry is gone, only the owner record)" : ""}`);
  if (report.skippedLists.length) {
    log(`Lists of the old installation uploaded by another player (left as they are): ${report.skippedLists.length}`);
    for (const l of report.skippedLists) log(`  ${l.id}  ${l.name}  (uploader ${l.uploaderId})`);
  }
  if (report.installDataSkipped) {
    log(`\nThe old installation also owns other players (${report.otherPlayers.join(", ")}).`);
    log("Its subscriptions, notices and votes were left in place. Add --include-install-data to move them too.");
  } else if (report.oldAuth) {
    log(`Subscriptions moving: ${report.subscriptions}, subscriber notices: ${report.notices}, compatibility votes: ${report.votes}`);
  }
  log(dryRun ? "\nDry run: nothing was written." : "\nDone.");
}

function parseArgs(argv) {
  const opts = { dryRun: false, includeInstallData: false, keyFile: process.env.GOOGLE_APPLICATION_CREDENTIALS ?? null, dbUrl: DEFAULT_DB_URL };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => {
      const v = argv[++i];
      if (v === undefined) throw new Error(`${a} needs a value.`);
      return v;
    };
    if (a === "--player") opts.player = next().trim();
    else if (a === "--new-auth") opts.newAuth = next().trim();
    else if (a === "--key") opts.keyFile = next();
    else if (a === "--db") opts.dbUrl = next();
    else if (a === "--dry-run") opts.dryRun = true;
    else if (a === "--include-install-data") opts.includeInstallData = true;
    else if (a === "--session-key" || a.startsWith("--session-key=")) throw new Error("Don't pass the session key on the command line (it ends up in your shell history). The script asks for it.");
    else if (a === "-h" || a === "--help") opts.help = true;
    else throw new Error(`Unknown option ${a}.`);
  }
  return opts;
}

const USAGE = `Usage: node admin/recover-account.mjs --player <playeruid> --new-auth <firebase uid> [options]

  --dry-run               Show what would move, write nothing
  --key <file>            Service-account key (default admin/service-account.json,
                          or GOOGLE_APPLICATION_CREDENTIALS)
  --db <url>              Realtime Database URL (default ${DEFAULT_DB_URL})
  --include-install-data  Also move subscriptions and votes when the old installation
                          owned other players too

The session key is asked for (hidden), or read from VS_SESSION_KEY.`;

/** Reads a line without echoing it. */
async function askHidden(prompt) {
  const { stdin, stdout } = process;
  if (!stdin.isTTY) throw new Error("No terminal to ask for the session key. Set VS_SESSION_KEY instead.");
  stdout.write(prompt);
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding("utf8");
  return new Promise((resolve, reject) => {
    let value = "";
    const done = (err) => {
      stdin.setRawMode(false);
      stdin.pause();
      stdin.off("data", onData);
      stdout.write("\n");
      err ? reject(err) : resolve(value);
    };
    const onData = (chunk) => {
      for (const ch of chunk) {
        if (ch === "\r" || ch === "\n") return done();
        if (ch === "\u0003") return done(new Error("Cancelled."));
        if (ch === "\u007f" || ch === "\b") value = value.slice(0, -1);
        else value += ch;
      }
    };
    stdin.on("data", onData);
  });
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help || !opts.player || !opts.newAuth) {
    console.log(USAGE);
    process.exitCode = opts.help ? 0 : 2;
    return;
  }

  let sessionKey = process.env.VS_SESSION_KEY?.trim() || (await askHidden("Player's session key (hidden): ")).trim();
  if (!sessionKey) throw new Error("No session key given.");
  const valid = await validateSession(opts.player, sessionKey);
  sessionKey = null;
  if (!valid) {
    console.error("The Vintage Story auth server says this session key is not valid for that player uid. Nothing was changed.");
    console.error("Ask the player to start the game (so the key is current), then copy it from clientsettings.json again.");
    process.exitCode = 1;
    return;
  }
  console.log("Session verified with the Vintage Story auth server.\n");

  const { initializeApp, cert } = await import("firebase-admin/app");
  const { getDatabase } = await import("firebase-admin/database");
  const { getAuth } = await import("firebase-admin/auth");
  const keyFile = opts.keyFile ?? DEFAULT_KEY_FILE;
  let credentials;
  try {
    credentials = JSON.parse(readFileSync(keyFile, "utf8"));
  } catch (e) {
    throw new Error(`Couldn't read the service-account key ${keyFile} (${e.code ?? e.message}). See firebase/README.md.`);
  }
  const app = initializeApp({ credential: cert(credentials), databaseURL: opts.dbUrl });
  const db = getDatabase(app);

  try {
    await getAuth(app).getUser(opts.newAuth);
  } catch (e) {
    throw new Error(`There is no Firebase account ${opts.newAuth} (${e.code ?? e.message}). Check the id the player sent.`);
  }

  const plan = await planRecovery(db, opts);
  if (!plan.nothingToDo && !opts.dryRun) await db.ref().update(plan.updates);
  printReport(plan, opts.dryRun);
  if (!plan.nothingToDo && !opts.dryRun) {
    console.log("\nTell the player to restart Simple VS Manager, and to log out of the game and back in so");
    console.log("the session key they sent you stops working.");
  }
  await app.delete();
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((e) => {
    console.error(e.message);
    process.exitCode = 1;
  });
}
