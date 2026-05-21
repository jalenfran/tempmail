import SwiftUI

struct ComposeView: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    struct Prefill {
        var fromLocalPart: String?
        var fromDomain: String
        var to: [String]
        var subject: String
        var body: String
        var inReplyTo: String?
    }

    let prefill: Prefill?

    @State private var fromLocalPart: String = ""
    @State private var toText: String = ""
    @State private var subject: String = ""
    @State private var bodyText: String = ""
    @State private var isSending = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("From") {
                    Picker("Address", selection: $fromLocalPart) {
                        ForEach(appState.addresses) { addr in
                            Text("\(addr.localPart)@\(domain)").tag(addr.localPart)
                        }
                    }
                    .pickerStyle(.menu)
                }

                Section("To") {
                    TextField("recipient@example.com, …", text: $toText)
                        #if !os(macOS)
                        .keyboardType(.emailAddress)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled(true)
                        #endif
                }

                Section("Subject") {
                    TextField("Subject", text: $subject)
                }

                Section("Message") {
                    TextEditor(text: $bodyText)
                        .frame(minHeight: 180)
                }

                if let err = errorMessage {
                    Section { Text(err).foregroundStyle(.red).font(.footnote) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New Message")
            #if !os(macOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await send() }
                    } label: {
                        HStack {
                            if isSending { ProgressView().controlSize(.small) }
                            Text(isSending ? "Sending…" : "Send")
                        }
                    }
                    .disabled(isSending || !canSend)
                }
            }
            .onAppear {
                if let p = prefill {
                    fromLocalPart = p.fromLocalPart ?? appState.addresses.first?.localPart ?? ""
                    toText = p.to.joined(separator: ", ")
                    subject = p.subject
                    bodyText = p.body
                } else {
                    fromLocalPart = appState.addresses.first?.localPart ?? ""
                }
            }
        }
    }

    private var domain: String { appState.session?.domain ?? "" }

    private var recipients: [String] {
        toText
            .split { $0 == "," || $0 == ";" || $0.isWhitespace }
            .map { String($0) }
            .filter { !$0.isEmpty }
    }

    private var canSend: Bool {
        !fromLocalPart.isEmpty && !recipients.isEmpty && !subject.isEmpty && !bodyText.isEmpty
    }

    private func send() async {
        errorMessage = nil
        guard let client = appState.client else { return }
        isSending = true
        defer { isSending = false }
        do {
            try await client.send(
                fromLocalPart: fromLocalPart,
                to: recipients,
                subject: subject,
                text: bodyText,
                html: nil,
                inReplyTo: prefill?.inReplyTo
            )
            dismiss()
        } catch {
            errorMessage = (error as? APIError)?.errorDescription ?? error.localizedDescription
        }
    }
}
