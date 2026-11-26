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
- **Free anonymous accounts.** Votes and subscriptions can be stuffed with throwaway accounts.
  Firebase App Check (and a per-account write rate in a Cloud Function) is the usual answer.
- **Uploader names** come from clientsettings.json and are not verified.
