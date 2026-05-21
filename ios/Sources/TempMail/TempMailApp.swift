import SwiftUI

@main
struct TempMailApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(appState)
                .task { await appState.bootstrap() }
                .tint(.accentColor)
                .preferredColorScheme(appState.appearance.colorScheme)
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 720)
        .windowResizability(.contentMinSize)
        #endif
    }
}

struct RootView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Group {
            if appState.bootstrapping {
                LaunchView()
            } else if appState.session != nil {
                InboxView()
            } else {
                PairingView()
            }
        }
        .animation(.snappy, value: appState.session)
        .animation(.snappy, value: appState.bootstrapping)
    }
}

private struct LaunchView: View {
    var body: some View {
        ProgressView()
            .controlSize(.large)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
