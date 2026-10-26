# Releasing Inkroot

This guide is for the repo owner. It covers:

1. The GitHub secrets the release needs (one time).
2. How to export your Apple Developer ID certificate into those secrets (one time).
3. How to cut a release.
4. What to check on the very first release run.
5. The hardware test to do before the first public release.
6. Troubleshooting.

The workflow is [`.github/workflows/release.yml`](../.github/workflows/release.yml). Pushing a tag such as `v0.1.0` starts it. It builds Inkroot for macOS (Apple Silicon and Intel), Windows and Linux, signs and notarizes the Mac apps, and puts everything in a **draft** release. Nobody sees a draft until you publish it.

You need the [GitHub CLI](https://cli.github.com/) (`brew install gh`, then `gh auth login`) for the commands below.

---

## 1. One time: GitHub secrets

The release needs these six secrets. Section 2 shows how to get each one.

| Secret name | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Your Developer ID Application certificate, exported as a `.p12` file and then base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | The password you chose when you exported the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate's full name, e.g. `Developer ID Application: Your Name (ABCDE12345)` |
| `APPLE_ID` | The email address of your Apple Account |
| `APPLE_PASSWORD` | An **app-specific password**. This is not your normal Apple Account password. |
| `APPLE_TEAM_ID` | Your 10-character Team ID, e.g. `ABCDE12345` |

You can check which ones are set with `gh secret list -R n0dal/inkroot`. GitHub never shows a secret's value again, so you can't read one back. If you're unsure about one, set it again.

> **Heads-up: the signature shows the certificate holder's name.** Anyone who inspects the Mac app sees `Developer ID Application: <name on the certificate>`. That includes macOS's own "Open" dialogs and `codesign -dv`. For a personal Apple Developer account, that's your legal name. If you'd rather show an organization name, enroll as an organization and use that certificate instead. Nothing in the workflow changes; you only update the secrets.

## 2. One time: export the Developer ID certificate

You need a paid Apple Developer Program membership, and you must be its **Account Holder** to create a Developer ID certificate.

1. **Make a certificate request.** Open **Keychain Access** (in Applications → Utilities). From the menu bar choose *Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority…*. Enter your email, leave "CA Email Address" empty, choose **Saved to disk**, and save the `.certSigningRequest` file.
2. **Create the certificate.** Go to [developer.apple.com/account](https://developer.apple.com/account) → *Certificates, IDs & Profiles → Certificates* and click **+**. Choose **Developer ID Application** (it's under "Software"; if it's greyed out, you aren't the Account Holder). Upload the request file from step 1, download the `.cer` file and double-click it. Keychain Access installs it.
3. **Export it as `.p12`.** In Keychain Access, select the **login** keychain and the **My Certificates** tab. Right-click *Developer ID Application: …* and choose **Export…**. Pick the format **Personal Information Exchange (.p12)**, save it as `DeveloperID.p12`, and choose a strong password. macOS may then ask for your Mac login password; that's normal.
4. **Add the certificate secret.** In Terminal, in the folder where you saved the file, run:
   ```sh
   base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE -R n0dal/inkroot
   ```
5. **Add its password.** Run the command below and paste the `.p12` password when it asks:
   ```sh
   gh secret set APPLE_CERTIFICATE_PASSWORD -R n0dal/inkroot
   ```
6. **Add the signing identity.** Run:
   ```sh
   security find-identity -v -p codesigning
   ```
   It prints a line like `1) 3F2A… "Developer ID Application: Your Name (ABCDE12345)"`. Copy the text **inside the quotes**, then run this and paste it:
   ```sh
   gh secret set APPLE_SIGNING_IDENTITY -R n0dal/inkroot
   ```
7. **Make an app-specific password.** Go to [appleid.apple.com](https://appleid.apple.com) → *Sign-In and Security → App-Specific Passwords*. Generate one named `inkroot-notary` and copy it (it looks like `abcd-efgh-ijkl-mnop`). Then run this and paste it:
   ```sh
   gh secret set APPLE_PASSWORD -R n0dal/inkroot
   ```
8. **Add your Apple ID and Team ID.** Run each command and paste the value when it asks:
   ```sh
   gh secret set APPLE_ID -R n0dal/inkroot        # your Apple Account email
   gh secret set APPLE_TEAM_ID -R n0dal/inkroot   # 10 characters
   ```
   You can find the Team ID at [developer.apple.com/account](https://developer.apple.com/account) → *Membership details*. It's also the part in brackets at the end of the signing identity from step 6.
9. **Delete the local `.p12`** (`rm DeveloperID.p12`) and the request file. The certificate stays safely in your keychain.

The app-specific password stops working if you change your Apple Account password. If notarization suddenly fails with an authentication error, make a new one (step 7).

## 3. Cut a release

1. **Bump the version** in all three files. They must match, or the release stops at its first step with `tag vX ≠ app version Y`:
   - `app/src-tauri/tauri.conf.json` → `"version"`
   - `app/package.json` → `"version"`
   - `app/src-tauri/Cargo.toml` → `version` under `[package]`

   Then commit, e.g. `git commit -am "release: v0.2.0"`, and push.
2. **Tag and push the tag:**
   ```sh
   git tag v0.2.0 && git push origin v0.2.0
   ```
3. **Watch it:** `gh run watch`. It takes about 30–60 minutes. Most of that is Apple's notarization.
4. **Open the draft release** on GitHub (*Releases*). Check that it has exactly these 5 assets and that the notes look right:
   - `Inkroot-macOS-AppleSilicon.dmg`
   - `Inkroot-macOS-Intel.dmg`
   - `Inkroot-Windows-x64-setup.exe`
   - `Inkroot-Linux-x86_64.AppImage`
   - `SHA256SUMS.txt`
5. **Try the Mac DMG on a clean Mac**, meaning one that has never run a dev build. Download it with a browser so macOS treats it as downloaded. It should open and run with **no** Gatekeeper warning.
6. **Publish.** Click *Edit* on the draft, then *Publish release*. **Don't tick "Set as a pre-release".** The README and site download links use `releases/latest/download/…`, which skips pre-releases, so those links would break. The notes already say "beta — not yet hardware-verified on …" until the hardware test in section 5 passes.

**If a run fails partway:** fix the problem and re-run the failed jobs from the run page (*Re-run failed jobs*). Re-runs replace assets on the draft. If the fix needs a code change, delete the draft and the tag, then tag again:
```sh
gh release delete v0.2.0 -R n0dal/inkroot --yes
git push --delete origin v0.2.0 && git tag -d v0.2.0
```
You can also start the workflow by hand for an existing tag: *Actions → release → Run workflow*, and enter the tag. The workflow refuses to touch a release that's already published.

## 4. First release checklist

Some parts of the pipeline can only be tested on GitHub's machines. The first run is their first real test. Do a dry run with a throwaway version, e.g. set all three versions to `0.0.1-rc.1` and push the tag `v0.0.1-rc.1`. Read these log lines, then delete the draft and the tag (see "If a run fails partway" above).

| Where (job → step) | What to look for |
|---|---|
| `build-wdi` → `bash tools/build-wdi.sh` | `WDK redistributable OK sha256=…`. After that, no "WDK extraction did not produce …WdfCoInstaller01011.dll" error, which would mean the `msiexec /a` extraction under MSYS2 failed. |
| `build-wdi` → same step, last lines | `wdi-simple.exe OK sha256=…`. This means `wdi-simple.exe --help` ran and printed its usage. |
| `build-edl (windows-x86_64)` and `build-edl (linux-x86_64)` → `build_edl.py` | `libusb OK …` from the self-test, then `edl --help OK`. This shows the frozen edl finds its bundled libusb. |
| `bundle` (all four) → *Smoke-test the bundled edl* | `libusb OK …` again, this time from the copy inside the build. On macOS this also shows that the signed edl still starts under the hardened runtime. |
| `bundle` (all four) → *No placeholders were staged* | `ok: … is complete for <target>` |
| `bundle (macOS)` → *Import Developer ID certificate* | No `::error::`. If it says the identity is missing, `APPLE_SIGNING_IDENTITY` doesn't exactly match the certificate (section 2, step 6). |
| `bundle (macOS)` → tauri-action | `signed N Mach-O file(s)` from staging, then Tauri's signing and notarization messages. |
| `bundle (macOS)` → *Verify signatures & notarization* | `ok` (nested signatures), `valid on disk`, `satisfies its Designated Requirement`, `source=Notarized Developer ID`, `The validate action worked!`, `status: Accepted` for the DMG, and a final `accepted` for the DMG. |
| `checksums` | The 4 file names, then `SHA256SUMS.txt` with 4 lines. |
| The draft's notes | Three lines reading `beta — not yet hardware-verified on macOS` / `Windows` / `Linux`. |

If the Apple secrets aren't set yet, the macOS jobs stop at *Check Apple secrets are set*. That's expected: do sections 1–2 first.

### One-time GitHub settings (website and repo page)

1. *Settings → Pages → Build and deployment → Source:* **GitHub Actions**.
2. After merging to `main`, run *Actions → pages → Run workflow* and wait for it to go green. `curl -sI https://n0dal.github.io/inkroot/ | head -1` should print `HTTP/2 200`.
3. Set the repo's homepage:
   ```sh
   gh repo edit n0dal/inkroot --homepage https://n0dal.github.io/inkroot/
   ```
4. *Settings → General → Social preview → Upload* `assets/social/social-preview.png`. GitHub has no API for this, so it has to be done in the browser.

## 5. Hardware test checklist (before the first public release)

Do this on the real Note Max (spec §8). Your tablet is currently rooted, so **do the steps in this order**:

1. Detection and the safety checks show "Already rooted ✓".
2. **Import** `boot_b.stock.img`. Its hash matches the known-stock list.
3. **Return to stock** from macOS. Confirm that `magisk -c` now fails.
4. **Back up** from macOS. The new backup is marked `stock: true` and its `boot_b` SHA-256 is `2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc`.
5. **Root** end-to-end from macOS. Check that Magisk is 30.7 and that the boot-service log has the line `uid=0 … u:r:magisk:s0`.
6. **Rescue** with the deep-flash cable from a USB-A computer, using the Linux AppImage and/or Windows.
7. **Windows driver install** on a real Windows machine. WinUSB should install for `05c6:9008`. Also confirm the BOOX's ADB USB IDs.
8. Optional: **Reinstall boot service** (the path where the hook is already present).

Also find out which USB-debugging method (A or B) works on firmware 4.2. Put that one first in the app, `docs/01`, the README and the site, and drop the "not yet confirmed" note.

After each OS passes, put the date in [`.github/hardware-verification.json`](../.github/hardware-verification.json) and commit it, e.g. `{"macos": "2026-11-02", "windows": null, "linux": null}`. From the next release on, that OS's line in the notes changes from "beta — not yet hardware-verified on …" to "hardware-verified".

## 6. Troubleshooting

- **Notarization fails with "The binary is not signed with a valid Developer ID certificate"** (or "…does not have the hardened runtime enabled", or "…signature does not include a secure timestamp"): a file nested inside the app was missed. Look for `FAIL` and the list of problems from the `verifyAll` check, in the tauri-action step (staging) or in *Verify signatures & notarization*. Each line names the file.
- **Read Apple's notarization log.** The `notarytool` output includes an `id:`. Run this on your Mac:
  ```sh
  xcrun notarytool log <id> --apple-id "you@example.com" --team-id ABCDE12345 --password "<app-specific password>"
  ```
  For the DMG, the workflow prints this log automatically when notarization is rejected.
- **`tag vX ≠ app version Y`**: one of the three version fields wasn't bumped (section 3, step 1).
- **`release vX is already published`**: you're re-running a release that's already public. Bump the version instead.
- **`expected exactly these 4 assets`**: one of the `bundle` jobs failed or uploaded nothing. Re-run it, then re-run `checksums`.
