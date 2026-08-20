# Threat Model

## Protected Assets

- user keystroke privacy
- integrity of local sound packs
- integrity of settings
- integrity of application binaries
- integrity of the release pipeline

## Threats

1. accidental key logging
2. malicious code disguised as a sound pack
3. archive path traversal
4. malformed audio causing crashes or resource exhaustion
5. compromised Rust or npm dependency
6. excessive Tauri permissions
7. compromised CI workflow
8. tampered release artifact
9. malicious contributor change
10. future updater or registry compromise

## Initial Mitigations

- raw keyboard events remain in Rust
- frontend receives no raw key data
- V1 has no application networking
- packs will be data-only
- explicit Tauri capabilities
- locked dependencies
- protected review workflow
- automated tests and static analysis
- bounded decoded-PCM registry memory
- bounded playback commands and 32 mixer voices
- allocation-free audio callback work
- private Rust-side output-device recovery without IPC exposure

Later milestones must add concrete pack parser limits, dependency review, SBOM, provenance, artifact scanning, and signing.
