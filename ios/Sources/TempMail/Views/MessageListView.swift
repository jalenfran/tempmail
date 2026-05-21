import SwiftUI

struct MessageListView: View {
    @Environment(AppState.self) private var appState
    let address: Address?
    @Binding var selection: String?

    var body: some View {
        List(selection: $selection) {
            ForEach(messages) { msg in
                MessageRow(message: msg)
                    .tag(msg.id as String?)
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            Task { await appState.deleteMessage(id: msg.id, localPart: msg.localPart) }
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
                    .swipeActions(edge: .leading) {
                        if !msg.read {
                            Button {
                                Task { await appState.markRead(id: msg.id, localPart: msg.localPart) }
                            } label: {
                                Label("Read", systemImage: "envelope.open")
                            }
                            .tint(.accentColor)
                        }
                    }
            }
        }
        .listStyle(.plain)
        .overlay {
            if messages.isEmpty {
                ContentUnavailableView(
                    "No mail yet",
                    systemImage: "tray",
                    description: Text(address == nil
                        ? "When mail arrives at any address, it shows up here."
                        : "Mail to \(address!.localPart) will appear here.")
                )
            }
        }
        .refreshable {
            await appState.refreshMessages(for: address?.localPart)
        }
        .task(id: address?.localPart) {
            await appState.refreshMessages(for: address?.localPart)
        }
    }

    private var messages: [MessageSummary] {
        if let localPart = address?.localPart {
            return appState.messagesByAddress[localPart] ?? []
        }
        return appState.messagesByAddress["__all__"] ?? []
    }
}

private struct MessageRow: View {
    let message: MessageSummary

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Circle()
                .fill(message.read ? .clear : Color.accentColor)
                .frame(width: 8, height: 8)
                .padding(.top, 8)

            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text(message.fromDisplay)
                        .font(.subheadline.weight(message.read ? .regular : .semibold))
                        .lineLimit(1)
                    Spacer()
                    Text(message.receivedAt.formattedReceivedDate())
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Text(message.subjectDisplay)
                    .font(.subheadline)
                    .lineLimit(1)
                    .foregroundStyle(.primary)
                if let preview = message.preview, !preview.isEmpty {
                    Text(preview)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
        }
        .padding(.vertical, 4)
    }
}
