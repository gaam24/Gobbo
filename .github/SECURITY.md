# Security policy

## Supported versions

Only the latest release receives security fixes.

## Reporting a vulnerability

Please do not report security problems in public issues or pull requests.

Report them privately instead: open the repository's **Security** tab and choose **Report a vulnerability**, or use
[this link](https://github.com/gaam24/Gobbo/security/advisories/new).

Include:

- the Gobbo version and your operating system (Windows or Linux),
- the steps to reproduce the problem,
- what an attacker could do with it, and
- a proof of concept, if you have one.

## Scope and known limits

These areas are in scope:

- **Mod archives.** Gobbo reads archives from third parties. An archive that crashes Gobbo, writes outside the mod
  folders or the game folder, or runs code during import is a vulnerability.
- **Deployment and launch.** The overlay method loads a hook DLL into the game process on Windows, and uses overlayfs in
  user namespaces on Linux. Problems in how these layers isolate or restore files are in scope.
- **Settings and keys.** Gobbo stores the Nexus Mods API key in the operating system's credential store (Credential
  Manager on Windows or Secret Service on Linux), never in `settings.json`. The key is used only in the backend and is
  not sent to the webview. A way to read the key through the interface, a log or an error is a vulnerability.
