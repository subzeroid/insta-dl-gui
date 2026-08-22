# Troubleshooting

## "Invalid token — get a new one at hikerapi.com"

The stored token was rejected (HTTP 401). Copy a fresh token from <https://hikerapi.com/tokens> and paste it again. Tokens can be revoked from the same page.

## "Quota exhausted — top up at hikerapi.com"

Your balance hit zero mid-run (HTTP 402). Top up at [hikerapi.com](https://hikerapi.com), then re-run the job — already-downloaded files are skipped, so you only pay for what's missing.

## "Not found on Instagram (private profile or deleted post)"

The target is private, or the post was deleted. Private profiles expose nothing but their avatar through any public API.

## Downloads fail with "CDN host … is not allowed"

A safety check: the app only downloads from official Instagram CDN hosts (`cdninstagram.com`, `fbcdn.net`). If you see this error on a normal post, the media URL likely expired between fetching and downloading — just retry; a fresh URL is minted per attempt.

## macOS: "Apple could not verify…" / «Файл не был открыт» (Gatekeeper)

The app is not code-signed yet, so macOS blocks the first launch. Two ways through:

**GUI:** dismiss the dialog with **Done/Готово** (don't press "Move to Trash"), then open **System Settings → Privacy & Security** and click **Open Anyway**.

**Terminal:**

```sh
xattr -cr /Applications/insta-dl-gui.app
```

Both only affect the first launch; after that the app opens normally.

## Windows SmartScreen blocked the installer

Click **More info → Run anyway**. Expected until code signing lands.

## Stories downloaded nothing

Stories only exist while they're live (24 h). If `@instagram` shows none, there simply are none right now.

## Nothing helps?

[Open an issue](https://github.com/subzeroid/insta-dl-gui/issues) with the exact error text and your OS.
