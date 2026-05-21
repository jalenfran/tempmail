# Temp Mail (iOS + macOS)

SwiftUI client for the Temp Mail backend. Liquid Glass UI, iOS 26 / macOS 26.

## Setup

```bash
# One-time: install XcodeGen
brew install xcodegen

# Generate the Xcode project from project.yml
cd ios
xcodegen generate

# Open it in Xcode
open TempMail.xcodeproj
```

There are three targets / schemes:
- `TempMail-iOS` — iPhone + iPad app
- `TempMail-macOS` — macOS app
- `TempMailTests-macOS` — unit tests, attached to the macOS scheme so `xcodebuild test -scheme TempMail-macOS` runs them

All three share the same Swift sources in `Sources/TempMail/`. Tests live in `Sources/TempMailTests/`.

## Build from CLI

```bash
# macOS
xcodebuild -project TempMail.xcodeproj -scheme TempMail-macOS \
  -destination 'platform=macOS' -configuration Debug build

# iOS Simulator
xcodebuild -project TempMail.xcodeproj -scheme TempMail-iOS \
  -destination 'generic/platform=iOS Simulator' -configuration Debug build
```

## Tests

```bash
xcodebuild -project TempMail.xcodeproj -scheme TempMail-macOS \
  -destination 'platform=macOS' test
```

12 tests in two files:
- `ModelsTests` — decode every JSON shape returned by the backend (`PublicConfig`, `Address`, `MessageSummary`, `MessageDetail`, `AttachmentMeta`), plus display fallbacks and date formatting.
- `SSEParseTests` — every event payload (`new_message`, `message_deleted`, `address_added`, `address_removed`) plus SSE frame stripping.

## Pairing

The first screen asks for a **Server URL** and **API Token**. For local dev:

- URL: `http://127.0.0.1:8787`
- Token: whatever you set as `auth.api_token` in `backend/config.toml`

The iOS simulator can reach the host's `127.0.0.1` directly. `Info-iOS.plist` declares `NSAppTransportSecurity → NSAllowsLocalNetworking = true` so plain `http://` localhost URLs work in development; for production you'd be on `https://` anyway and the exception is moot.

The token is stored in the system Keychain. Use Settings → "Sign out" to clear it. Settings also has an **Appearance** picker (System / Light / Dark) that's persisted in UserDefaults and applied via `.preferredColorScheme(...)`.

## After editing `project.yml`

Re-run `xcodegen generate`. Source changes don't need regeneration — Xcode picks up new files in `Sources/TempMail/` automatically because the target's `sources:` is a directory.

## Layout choice (iPhone vs iPad/macOS)

`InboxView` dispatches on `horizontalSizeClass`:
- **Compact (iPhone portrait)** uses `StackedInboxView` — a `NavigationStack` with `NavigationLink(value:)` rows. This is iPhone's most reliable navigation idiom; the sidebar selection issue you hit during development was specifically `NavigationSplitView`'s collapse-to-stack behavior not pushing the content column.
- **Regular (iPad / macOS)** uses `SplitInboxView` — a three-column `NavigationSplitView` (addresses | messages | detail) with selection bindings, which works as expected when all three columns are visible side-by-side.

Both code paths share the same row content (`AddressRowContent`, `MessageRowContent`) and detail / compose / settings views.

## Project structure

```
ios/
  project.yml                    XcodeGen spec — three targets, deployment 26.0
  Sources/
    TempMail/
      TempMailApp.swift          @main, RootView branches pairing/inbox
      AppState.swift             @Observable global state, SSE subscription, appearance setting
      Models.swift               Address, MessageSummary, MessageDetail, AttachmentMeta
      Theme.swift                spacing/sizes
      Info-iOS.plist             iOS-only plist (ATS dev exception + standard keys)
      Assets.xcassets/           AccentColor + AppIcon catalog
      Networking/
        APIClient.swift          REST client, snake_case ↔ camelCase
        SSEClient.swift          URLSession.bytes-based event stream with reconnect
        AuthStore.swift          Keychain credentials
      Views/
        PairingView.swift        gradient + glass logo + glass card around fields + glassProminent Connect
        InboxView.swift          dispatcher + StackedInboxView (iPhone) + SplitInboxView (iPad/macOS) + shared row content
        AddressListView.swift    macOS/iPad sidebar list with selection binding + swipe pin/unpin/delete
        MessageListView.swift    macOS/iPad message list with selection binding + swipe read/delete
        MessageDetailView.swift  header in glass card, text/html toggle, dynamic-height WKWebView, glass attachment chips, dark-mode-aware HTML CSS
        NewAddressSheet.swift    readable / random / custom local-part with glass preview chip
        ComposeView.swift        send via Resend, recipient list parsed from text input
        SettingsView.swift       Appearance picker + server info + sign out
    TempMailTests/
      ModelsTests.swift          JSON decode coverage for every model
      SSEParseTests.swift        event payload parsing + SSE framing
```
