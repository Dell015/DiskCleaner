# Code signing policy

Rhizome's Windows downloads will be signed so Windows and antivirus programs can check who made
them and that nobody changed them after they were built (Phase R1, D137).

**Status:** not signed yet. Applying to [SignPath Foundation](https://signpath.org), which signs
open-source projects for free, once the first release is out. Until then Windows may warn about
the download: see [If Windows warns you](../README.md#if-windows-warns-you).

When signing is in place, this page will say:

> Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
> [SignPath Foundation](https://signpath.org).

## What gets signed

Only files built by GitHub Actions from this repository's own source code, by the release
workflow (`.github/workflows/release.yml`). Nothing built on a person's PC is ever signed. Each
release also publishes SHA-256 checksums and a GitHub build attestation, so anyone can check that a
download came from that workflow and that commit:

```
gh attestation verify <downloaded file> --repo Dell015/DiskCleaner
```

## Team roles

| Role | Who | What they do |
| --- | --- | --- |
| Author (committer) | Randel Jhon L Serafica (GitHub: Dell015) | Writes and merges changes |
| Reviewer | Randel Jhon L Serafica | Reviews every change from anyone who is not a committer |
| Approver | Randel Jhon L Serafica | Approves every signing request by hand, after reading the release |

Everyone in these roles uses two-factor sign-in on GitHub and SignPath.

## Privacy

Rhizome sends nothing anywhere by itself. It has no telemetry or analytics. The one exception is
the optional AI advisor, which is off by default: when you turn it on, it shows the exact data it
would send first, and sends only file names, sizes and dates (never file contents, with your user
folder hidden) to the AI provider you chose. See [SAFETY.md](../SAFETY.md).

## Changes to your system

Rhizome changes nothing unless you pick it and confirm it:

- **Removing** moves what you picked to a hidden holding folder on the same drive; you can put it
  back for 30 days (changeable), then it is deleted for good.
- **Uninstalling** starts only that app's own uninstaller, with its own window, after you confirm.
- The installer adds a normal entry to Windows' Installed apps, so Rhizome uninstalls like any app.
