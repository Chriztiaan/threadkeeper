# GitHub distribution

## Repository separation

The public application is independent of team configuration. `team-config/` is an ignored, separately initialized Git repository containing `repositories.json`, `maintainers.json`, and `vips.json`. It is deliberately not a submodule: public contributors and CI do not need private repository access. Imports are snapshots, not a remote subscription. Pull/download the private repository and import each JSON under its matching Settings tab, then save.

The local app history predates this separation and includes team rosters. Do not push that history to the public repository. Create the public repository from a reviewed clean export instead:

```sh
python3 desktop-rune/scripts/export-public.py
```

The script uses an allowlist and refuses nonempty default rosters or an existing nonempty destination. Review `public-export/`, initialize a new Git repo there, commit as the project author, and push only that repository. Keep the original local history private. Do not add the original repo as a public remote or push all branches/tags. Existing application data on your Mac is unaffected.

## CI builds and releases

`Tests` runs on main/init pushes and pull requests. `macOS release` builds and tests on Apple Silicon and Intel runners. Run it manually for downloadable Actions artifacts, or push a `vMAJOR.MINOR.PATCH` tag for a draft GitHub Release with both ZIPs and SHA-256 checksums. Publish the draft after checking the downloaded build. Re-running a tag can update a draft, but refuses to replace an already-published release.

Artifacts use stable names:

- `Threadkeeper-macOS-arm64.zip` — Apple Silicon
- `Threadkeeper-macOS-x86_64.zip` — Intel

Once a release is published, its release page is `https://github.com/OWNER/REPO/releases/latest`. The Apple Silicon direct download is `https://github.com/OWNER/REPO/releases/latest/download/Threadkeeper-macOS-arm64.zip` (substitute the Intel asset name for Intel). Public repository releases can be downloaded without access to the private team-config repository.

No release has been uploaded by this setup alone. CI still needs its first hosted run to establish both architectures build successfully. The pinned Rust version is read from `rust-toolchain.toml`.

## macOS trust

The initial workflow uses ad-hoc signing only. It does **not** claim to be Developer ID signed or notarized. macOS may block downloaded preview builds; after verifying the source, users can explicitly allow that app under System Settings → Privacy & Security. Do not disable Gatekeeper globally.

For normal team distribution, replace that step with Developer ID signing using hardened runtime, submit the signed ZIP through `xcrun notarytool`, staple the accepted ticket to the app, verify with `spctl`, and recreate the final ZIP and checksums. Keep the signing certificate/password and Apple notarization credentials in GitHub Actions secrets, never in either repository. This requires your Apple Developer identity and has not been provisioned here.

## Public-source readiness

The default `tracker.config.json` and `maintainers.json` are intentionally empty. Demo fixtures are fictional and work without private configuration. Real VIP notes and other rosters belong only in the private configuration repo. A project source license still needs to be selected before describing Threadkeeper as open source; public visibility alone is not an open-source license. Preserve applicable third-party license notices when distributing builds.

References: [GitHub release links](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases), [hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [Apple macOS distribution](https://developer.apple.com/macos/distribution/).
