# GitHub distribution

## Repository separation

The public application is independent of team configuration. `team-config/` is an ignored, separately initialized Git repository containing one `workspace.json` with repositories, maintainers, and VIPs. It is deliberately not a submodule: public contributors and CI do not need private repository access. Imports are snapshots, not a remote subscription. Pull/download the private repository and use Settings → Workspace → Import workspace, review all three lists, and Save workspace.

The local app history predates this separation and includes team rosters. Do not push that history to the public repository. Create the public repository from a reviewed clean export instead:

```sh
python3 desktop-rune/scripts/export-public.py
```

The script uses an allowlist and refuses nonempty default rosters or an existing nonempty destination. Review `public-export/`, initialize a new Git repo there, commit as the project author, and push only that repository. Keep the original local history private. Do not add the original repo as a public remote or push all branches/tags. Existing application data on your Mac is unaffected.

## CI builds and releases

`Tests` runs on main/init pushes and pull requests. To release:

1. Merge reviewed changes into `main`. The app's version in `desktop-rune/Cargo.toml` must be the version you intend to release (update Cargo.lock with a version bump).
2. Open **Actions → macOS release → Run workflow**, select `main`, and enter the version, such as `0.1.0` (without `v`).
3. CI validates the version, runs tests and builds on Apple Silicon and Intel.
4. Only after both builds pass, CI tags the exact tested commit and creates a draft release containing both ZIPs and SHA-256 checksums.
5. Download and test the app, then publish the draft in GitHub Releases.

No local build or tag command is needed. Existing version tags cannot be moved to different commits. Reruns of the same commit can repair a draft's assets, but cannot replace published releases. Manually pushed `vMAJOR.MINOR.PATCH` tags remain supported. A tag created by this workflow uses GITHUB_TOKEN, so it does not trigger a second tag-push workflow run.

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
