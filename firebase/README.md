# Realtime Database rules

`database.rules.json` is the proposed rule set for the cloud modlists, compatibility votes and
subscriptions. It is **not deployed from here**: paste it into the Firebase console
(Realtime Database > Rules) or run `npx firebase deploy --only database --project <project>`.

`test/rules.test.mjs` runs the writes Simple VS Manager 3 and 2.x make against the local
emulator (needs Java 11+; never touches the real database):

```
npm install
npm test
```

The same run also tests `admin/recover-account.mjs` (below) against the emulator.

`RULES=<file> npm test` runs the same tests against another rule file, for example the live
rules copied from the console.

## What changed from the rules live on 2026-11-26

Against the live rules, 6 of the 11 tests fail:

- **Deleting an online modlist from v3 and clearing "new subscriber" notices were refused.**
  v3 removes `modlistSubscriptions/{id}` and `modlistSubscriptionNotices/{id}` as a whole, but
  there was only a rule per subscriber. The owner may now remove those nodes.
- **An owner could later change `uploaderId` to another player.** The uploader check only ran
  when an entry was created. It now runs on every write.
- **Anyone could create a registry entry at any free id, including one whose owner record was
  left behind.** An entry can now only be written by the account that owns it in the same
  write (`registryOwners` is set in the same PATCH by both v3 and 2.x).
- **Owner records could be handed to anyone.** An owner may keep or delete theirs, not give it
  to another account.
- **No size limits.** Everyone downloads all of `registrySummaries` (3 MB for 586 lists today),
  so one oversized entry slowed every client. Summaries now only take the known fields, with
  limits (name 100, description 1000, uploader 64, versions 32, mod id 100, mod version 50,
  at most 9,999 mods). The full `registry` entry gets the same text limits but may hold other
  fields (configs). Votes take only their four fields, comments up to 1000 characters, and the
  mod and version keys are capped.
- `\S` in the comment check is not a supported regular expression in the emulator; it is now
  `[^ ]` (the app trims comments).

The current data fits all limits (longest name 91, description 200, uploader 16 characters).

## What the rules can't fix

- **Squatting a player's slots.** `owners/{player uid}` is first come. Anyone who knows a
  player's uid (it is visible on multiplayer servers) can claim it before that player
  publishes. Rules can't check a Vintage Story login; that needs a Cloud Function that
  verifies the game's session with the Vintage Story auth server before writing `owners`.
  Until then, a player whose slots were taken can be helped with the recovery script below.
- **Free anonymous accounts.** Votes and subscriptions can be stuffed with throwaway accounts.
  Firebase App Check (and a per-account write rate in a Cloud Function) is the usual answer.
- **Uploader names** come from clientsettings.json and are not verified.

## Recovering a player's modlists (lost firebase-auth.json)

Everything a player publishes belongs to the anonymous Firebase account in their
`firebase-auth.json`. If that file is lost (new PC, reinstall without the `SVSM Backup` copy),
the new installation signs up a new account and the app says the player's online modlists
"are owned by another installation". The message shows the two ids you need.

`admin/recover-account.mjs` moves the player to the new account. It runs on your own PC with
the Admin SDK, which works on the free Spark plan (no Cloud Functions).

**What the player sends you**

- Their player id (`playeruid` in the game's `clientsettings.json`, also shown in the error).
- The new installation's id (shown in the error).
- Their session key (`sessionkey` in `clientsettings.json`). This is the proof: only someone
  logged in to that Vintage Story account has a live key. The script checks it with the Vintage
  Story auth server (`auth3.vintagestory.at/clientvalidate`) and stops if it isn't valid.
  The player should start the game before copying it, so the key is current.

The username or a modlist name is not proof, since anyone can see those.

**One-time setup**

1. Firebase console > Project settings > Service accounts > *Generate new private key*.
2. Save the file as `firebase/admin/service-account.json` (git ignores it; keep it private,
   it can read and write everything). Or point `--key` / `GOOGLE_APPLICATION_CREDENTIALS` at it.
3. `npm install` in this folder.

**Running it** (from this folder)

```
node admin/recover-account.mjs --player <playeruid> --new-auth <new id> --dry-run
node admin/recover-account.mjs --player <playeruid> --new-auth <new id>
```

It asks for the session key without showing it (or reads `VS_SESSION_KEY`). It refuses a key
on the command line, and never prints or saves it. Then it:

1. checks the session with the Vintage Story auth server,
2. checks that the new id is a real Firebase account,
3. sets `owners/{player}` to the new id,
4. moves `registryOwners` of the old account's public lists that this player uploaded,
5. moves the old account's subscriptions, "new subscriber" notices and compatibility votes
   (keyed by the account, not the player). If the old account also owned other players, these
   stay where they are unless you add `--include-install-data`.

All changes are one multi-path update, so it either all happens or none of it. `--dry-run`
does the checks and prints what would move without writing. Slots (`users/{player}`) are keyed
by the player and need no move.

**Afterwards** tell the player to restart Simple VS Manager, and to log out of the game and back
in so the session key they sent you stops working.
