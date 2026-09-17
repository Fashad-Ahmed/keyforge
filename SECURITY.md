# Security Policy

## Supported Versions

Security fixes are provided for the latest released version.

## Reporting a Vulnerability

Do not open a public issue for suspected vulnerabilities.

Until a private security contact is configured, use GitHub Private Vulnerability Reporting when available for this repository.

Include:

- affected version or commit;
- operating system;
- reproduction steps;
- expected and observed behavior;
- impact;
- proof of concept when safe to provide.

## Security Guarantees We Intend to Preserve

- no storage of typed key content;
- no transmission of typed key content;
- no telemetry;
- no executable sound-pack content;
- least-privilege native capabilities;
- release artifacts built through controlled CI.

## Current Product Boundary

Sound enabled, master volume, and selected pack ID are the only persisted product settings. Writes use a same-directory temporary file and platform-safe replacement; corrupt settings default safely without overwriting the source.

Pack import begins in a Rust-only native file picker. Paths, archive entries, audio bytes, decoded samples, and internal errors never cross IPC. Prepare-then-commit activation validates, decodes, and registers a full pack before changing the active selector. Sanitized failures preserve the currently active sound.

Close-to-tray behavior is owned by Rust. The tray exposes only Enable or Disable Sounds, Show KeyForge, and Quit; it does not broaden frontend permissions. The main capability allowlist remains empty.

There is no application networking. Autostart, networking, updates, and Windows/Linux input hooks remain excluded.
