import SwiftUI

struct NewAddressSheet: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    @State private var mode: Mode = .readable
    @State private var custom: String = ""
    @State private var label: String = ""
    @State private var pinned = true
    @State private var generated: String = ""
    @State private var isCreating = false
    @State private var errorMessage: String?

    enum Mode: String, CaseIterable, Identifiable {
        case readable, random, custom
        var id: String { rawValue }
        var label: String {
            switch self {
            case .readable: return "Readable"
            case .random: return "Random"
            case .custom: return "Custom"
            }
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Style") {
                    Picker("Style", selection: $mode) {
                        ForEach(Mode.allCases) { m in
                            Text(m.label).tag(m)
                        }
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: mode) { _, _ in regenerate() }
                }

                Section("Address") {
                    addressPreview
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))

                    if mode != .custom {
                        Button {
                            regenerate()
                        } label: {
                            Label("Regenerate", systemImage: "arrow.triangle.2.circlepath")
                        }
                    }
                }

                Section("Options") {
                    TextField("Label (optional)", text: $label)
                    Toggle("Pin to sidebar", isOn: $pinned)
                }

                if let err = errorMessage {
                    Section { Text(err).foregroundStyle(.red).font(.footnote) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New Address")
            #if !os(macOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await create() }
                    } label: {
                        if isCreating { ProgressView().controlSize(.small) } else { Text("Create") }
                    }
                    .disabled(isCreating || effectiveLocalPart.isEmpty)
                }
            }
            .onAppear { regenerate() }
        }
    }

    @ViewBuilder
    private var addressPreview: some View {
        VStack(spacing: 4) {
            if mode == .custom {
                HStack(spacing: 0) {
                    TextField("local-part", text: $custom)
                        #if !os(macOS)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled(true)
                        #endif
                        .font(.system(.title3, design: .monospaced).weight(.medium))
                        .multilineTextAlignment(.center)
                    Text("@\(appState.session?.domain ?? "")")
                        .font(.system(.title3, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
            } else {
                HStack(spacing: 0) {
                    Text(generated)
                        .font(.system(.title3, design: .monospaced).weight(.medium))
                    Text("@\(appState.session?.domain ?? "")")
                        .font(.system(.title3, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 22)
        .padding(.horizontal, 16)
        .glassEffect(.regular, in: .rect(cornerRadius: 18))
    }

    private var effectiveLocalPart: String {
        switch mode {
        case .custom: return custom.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        default: return generated
        }
    }

    private func regenerate() {
        switch mode {
        case .readable: generated = randomReadable()
        case .random: generated = randomString(length: 10)
        case .custom: break
        }
    }

    private func create() async {
        errorMessage = nil
        let lp = effectiveLocalPart
        guard !lp.isEmpty else { return }
        isCreating = true
        defer { isCreating = false }
        do {
            try await appState.createAddress(
                localPart: lp,
                label: label.isEmpty ? nil : label,
                pinned: pinned
            )
            dismiss()
        } catch {
            errorMessage = (error as? APIError)?.errorDescription ?? error.localizedDescription
        }
    }

    private func randomReadable() -> String {
        let adjectives = ["amber", "azure", "brisk", "calm", "clever", "dusky", "fern", "gentle",
                          "merry", "mossy", "nimble", "quiet", "river", "stone", "sunny", "velvet"]
        let nouns = ["otter", "ember", "harbor", "lantern", "linden", "meadow", "orchid", "puffin",
                     "raven", "shore", "spruce", "thistle", "willow", "yarrow", "robin", "fern"]
        let a = adjectives.randomElement()!
        let n = nouns.randomElement()!
        let num = Int.random(in: 10...99)
        return "\(a)-\(n)-\(num)"
    }

    private func randomString(length: Int) -> String {
        let chars = Array("abcdefghjkmnpqrstuvwxyz23456789")
        return String((0..<length).map { _ in chars.randomElement()! })
    }
}
