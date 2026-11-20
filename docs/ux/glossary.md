# Gobbo glossary

One concept, one label. The interface uses the left column. The right column is internal or advanced and may appear only
behind **Advanced**, a `HelpHint`, or a technical log.

| In the interface            | Internal / advanced | Meaning                                                                               |
| --------------------------- | ------------------- | ------------------------------------------------------------------------------------- |
| **Apply**                   | deploy              | Put the enabled mods into the game.                                                   |
| **Applied**                 | deployed            | The enabled list matches what is in the game.                                         |
| **Changes waiting**         | draft changes       | The list changed after the last apply.                                                |
| **Not applied yet**         | not deployed        | The active profile was never applied.                                                 |
| **Apply and play**          | apply and launch    | Apply the changes, then start the game.                                               |
| **Apply again**             | re-apply            | Write the same list again, to repair a deployment.                                    |
| **Undo apply**              | undo deployment     | Remove what the last apply wrote.                                                     |
| **Play without mods**       | vanilla launch      | Start the game as it is in its folder.                                                |
| **Stop game**               | force close         | Stop every process of the game.                                                       |
| **Profile**                 | profile             | A saved set of mods. Switching changes which mods are on.                             |
| **Your mods**               | library             | Every mod installed in Gobbo.                                                         |
| **Installer options**       | FOMOD               | The choices an archive asks for before it is installed.                               |
| **Archive report**          | archive comparison  | The archive copy checked against the installed mod folder.                            |
| **Needs attention**         | unmet requirements  | A mod needs another mod, a loader or a file the profile does not provide.             |
| **Overrides / Overridden**  | wins / loses        | Which mod's copy of a shared file reaches the game.                                   |
| **Group**                   | mod group           | A named set of mods in a profile that stays together in the list and switches as one. |
| **Translation group**       | translation group   | A group whose first mod is the base and whose other mods are localized copies of it.  |
| **Mod page**                | source / provider   | The page a mod comes from, such as Nexus Mods or Thunderstore.                        |
| **How mods reach the game** | overlay             | A layer that leaves the real game folder untouched.                                   |
| **Loader**                  | loader preset       | A shared layout for a mod loader such as BepInEx.                                     |
| **Game folder**             | game root           | The folder that holds the game executable.                                            |

## Rules

- A user-facing string never says **deploy**. The word stays in code, logs and the advanced definition.
- **Apply** is a verb; **Applied** is a state. Do not use "deployed" in the interface.
- A failure says what happened and what to do next. A recovery action is a button, not a sentence.
- A technical name is allowed only in **Advanced**, a `HelpHint`, the console, an error's **Details**, or a log.
- Every label exists in `src/locales/en.ts` and `src/locales/pl.ts`. Product names (Steam, Nexus Mods) are not translated.
