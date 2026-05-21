import SwiftUI

struct PairingView: View {
    @Environment(AppState.self) private var appState
    @State private var serverURL: String = "http://127.0.0.1:8787"
    @State private var token: String = ""
    @State private var isConnecting = false
    @State private var errorMessage: String?
    @FocusState private var focused: Field?

    enum Field { case url, token }

    var body: some View {
        ZStack {
            backgroundGradient

            VStack(spacing: 28) {
                header

                fields

                if let err = errorMessage {
                    Text(err)
                        .font(.footnote)
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 32)
                        .transition(.opacity)
                }

                connectButton
            }
            .frame(maxWidth: 460)
            .padding(.horizontal, 24)
        }
    }

    private var backgroundGradient: some View {
        LinearGradient(
            colors: [
                Color.accentColor.opacity(0.18),
                Color.accentColor.opacity(0.04),
                Color.clear,
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
        .ignoresSafeArea()
    }

    private var header: some View {
        VStack(spacing: 10) {
            Image(systemName: "envelope.open.badge.clock")
                .font(.system(size: 56, weight: .light))
                .foregroundStyle(.tint)
                .padding(20)
                .glassEffect(.regular, in: .circle)
            Text("Temp Mail")
                .font(.largeTitle.weight(.semibold))
            Text("Connect to your backend to start.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    private var fields: some View {
        VStack(spacing: 18) {
            LabeledField(
                title: "Server URL",
                placeholder: "https://temp-mail.example.com",
                text: $serverURL,
                isSecure: false
            )
            .focused($focused, equals: .url)
            #if !os(macOS)
            .keyboardType(.URL)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled(true)
            #endif

            Divider().opacity(0.3)

            LabeledField(
                title: "API Token",
                placeholder: "paste from server config",
                text: $token,
                isSecure: true
            )
            .focused($focused, equals: .token)
        }
        .padding(20)
        .glassEffect(.regular, in: .rect(cornerRadius: 22))
    }

    private var connectButton: some View {
        Button {
            Task { await connect() }
        } label: {
            HStack(spacing: 8) {
                if isConnecting {
                    ProgressView().controlSize(.small)
                }
                Text(isConnecting ? "Connecting…" : "Connect")
                    .font(.headline)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
        }
        .buttonStyle(.glassProminent)
        .controlSize(.large)
        .disabled(isConnecting || serverURL.isEmpty || token.isEmpty)
    }

    private func connect() async {
        focused = nil
        errorMessage = nil

        guard let url = normalizeURL(serverURL) else {
            errorMessage = "That URL doesn't look right."
            return
        }
        isConnecting = true
        defer { isConnecting = false }
        do {
            try await appState.signIn(baseURL: url, token: token)
        } catch APIError.server(401, _) {
            errorMessage = "Token rejected by the server."
        } catch let APIError.server(code, _) {
            errorMessage = "Server returned \(code)."
        } catch let APIError.transport(err) {
            errorMessage = "Couldn't reach server: \(err.localizedDescription)"
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func normalizeURL(_ s: String) -> URL? {
        var trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return nil }
        if !trimmed.contains("://") {
            trimmed = "https://" + trimmed
        }
        return URL(string: trimmed)
    }
}

private struct LabeledField: View {
    let title: String
    let placeholder: String
    @Binding var text: String
    let isSecure: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold).monospaced())
                .foregroundStyle(.secondary)
                .kerning(0.6)
            Group {
                if isSecure {
                    SecureField(placeholder, text: $text)
                } else {
                    TextField(placeholder, text: $text)
                }
            }
            .textFieldStyle(.plain)
            .font(.body)
        }
    }
}
