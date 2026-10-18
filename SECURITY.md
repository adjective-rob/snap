# Security Policy

## Reporting a vulnerability

Please report security problems privately, not in a public issue or pull request.

Use GitHub's private reporting: go to the [Security tab](https://github.com/adjective-rob/snap/security) and click **Report a vulnerability**. Only the maintainers can see the report.

Include what you can:

- What the problem is and what an attacker could do with it
- Steps to reproduce, or a proof of concept
- Your OS and display server (Wayland, X11, macOS, Windows) and the Snap version or commit

You should get a reply within 7 days. Once a fix is ready, we publish it in a release and then disclose the issue in a GitHub security advisory, credited to you unless you would rather not be named.

## Supported versions

Only the latest release and the `main` branch get security fixes. Update by re-running the installer.

## How Snap handles your data

Snap is a local tool. Knowing what it touches helps you judge what counts as a vulnerability.

- **Screenshots stay on your machine.** Annotations are saved as PNG and JSON pairs in `~/.snap/inbox/`. Snap makes no network calls and has no telemetry.
- **Screenshots can contain secrets.** Whatever was on screen when you pressed the hotkey is in the image: tokens in a terminal, messages, documents. Any program running as your user can read `~/.snap/inbox/`, and so can any AI agent you connect to the MCP server. Clear the inbox (`clear_inbox`) when you are done, and avoid capturing screens with secrets on them.
- **The MCP server is local and limited.** It talks to your agent over stdio, opens no network port, and only reads and deletes files in the inbox. It never writes annotations.
- **Logs hold no image data.** `~/.snap/snap.log` records timestamps, event names, and annotation filenames. It rotates at 1MB.

## Installing safely

The one-line installers (`install.sh`, `install.ps1`) download and run code from this repository and its GitHub releases. Running them means trusting this repository and GitHub. If you would rather check first, download the script, read it, then run it. Both are short.

The installers fetch only from this repository, its releases, and the Python Package Index (for the MCP server's dependencies). They do not ask for administrator rights; the Windows app installer may ask on its own.

## In scope

- Anything that lets annotations, screenshots, or other files leave your machine or be read by something that should not have them
- The MCP server reading, writing, or deleting files outside `~/.snap/`
- The installers fetching code from anywhere other than the sources listed above, or changing files they should not touch
- Build or release workflow problems that could put tampered files in a release

## Out of scope

- Programs already running as your user reading `~/.snap/inbox/`. That is how local files work; see above.
- An AI agent you connected doing what you or its instructions told it to with your annotations
- Problems that need an attacker who already has administrator or root access
