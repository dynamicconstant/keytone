# Keytone for iPhone

A custom keyboard that plays a Keytone sound pack on every key press, in any app, plus a companion app with setup steps and pack previews. Uses the same `packs/` as the desktop app.

## Run it on your iPhone (free Apple ID)

1. Install **Xcode** from the Mac App Store, and XcodeGen: `brew install xcodegen`
2. Generate the project and open it:
   ```bash
   cd ios
   xcodegen
   open Keytone.xcodeproj
   ```
3. In Xcode, click the **Keytone** project → for **both** targets (*Keytone* and *KeytoneKeyboard*) → **Signing & Capabilities** → **Team**: pick your Apple ID (Add Account… if needed). If the bundle ID is taken, change `com.harshilnainani` to something unique.
4. Plug in your iPhone (or use Wi-Fi), pick it as the run destination, press **▶ Run**.
5. On the iPhone: **Settings → General → VPN & Device Management** → trust your developer certificate (first time only). Turn on **Developer Mode** if iOS asks.
6. Open Keytone and follow the steps: Settings → Keytone → Keyboards → enable **Keytone** and **Allow Full Access**.

With a free Apple ID the app stops launching after 7 days; press Run in Xcode again to refresh it. A paid Apple Developer account ($99/yr) removes that limit and allows TestFlight/App Store distribution.

## Notes

- **Full Access** is required because iOS doesn't let keyboards play their own audio without it. Keytone doesn't use network access or store anything you type.
- Pick a pack with the **♫** button at the top of the keyboard; mute with the speaker button.
- Sounds play even when the ringer switch is on silent (use the keyboard's mute button), and mix with music instead of pausing it.
