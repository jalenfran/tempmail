import SwiftUI

struct AddressListView: View {
    @Environment(AppState.self) private var appState
    @Binding var selection: Address?

    var body: some View {
        List(selection: $selection) {
            Section {
                HStack {
                    Image(systemName: "tray.full")
                        .frame(width: 22)
                    Text("All Messages")
                    Spacer()
                    if totalUnread > 0 {
                        Text("\(totalUnread)")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 6).padding(.vertical, 2)
                            .background(.fill.tertiary, in: .capsule)
                    }
                }
                .tag(nil as Address?)
            }

            if !pinned.isEmpty {
                Section("Pinned") {
                    ForEach(pinned) { addr in
                        addressRow(addr)
                            .tag(addr as Address?)
                            .swipeActions(edge: .leading) { pinActions(for: addr) }
                            .swipeActions(edge: .trailing) { deleteAction(for: addr) }
                            .contextMenu { pinActions(for: addr); deleteAction(for: addr) }
                    }
                }
            }

            Section(pinned.isEmpty ? "Addresses" : "All") {
                ForEach(unpinned) { addr in
                    addressRow(addr)
                        .tag(addr as Address?)
                        .swipeActions(edge: .leading) { pinActions(for: addr) }
                        .swipeActions(edge: .trailing) { deleteAction(for: addr) }
                        .contextMenu { pinActions(for: addr); deleteAction(for: addr) }
                }
            }
        }
        .listStyle(.sidebar)
        .refreshable {
            await appState.refreshAddresses()
        }
    }

    private var pinned: [Address] { appState.addresses.filter { $0.pinned } }
    private var unpinned: [Address] { appState.addresses.filter { !$0.pinned } }
    private var totalUnread: Int { appState.addresses.reduce(0) { $0 + $1.unreadCount } }

    @ViewBuilder
    private func pinActions(for addr: Address) -> some View {
        if addr.pinned {
            Button {
                Task { await appState.setPinned(addr, pinned: false) }
            } label: {
                Label("Unpin", systemImage: "pin.slash")
            }
            .tint(.gray)
        } else {
            Button {
                Task { await appState.setPinned(addr, pinned: true) }
            } label: {
                Label("Pin", systemImage: "pin")
            }
            .tint(.accentColor)
        }
    }

    @ViewBuilder
    private func deleteAction(for addr: Address) -> some View {
        Button(role: .destructive) {
            Task { await appState.deleteAddress(addr.localPart) }
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }

    private func addressRow(_ addr: Address) -> some View {
        AddressRowContent(addr: addr, domain: appState.session?.domain ?? "")
    }
}
