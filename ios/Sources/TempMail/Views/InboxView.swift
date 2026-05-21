import SwiftUI

struct InboxView: View {
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var hSize
    #endif

    var body: some View {
        #if os(iOS)
        if hSize == .compact {
            StackedInboxView()
        } else {
            SplitInboxView()
        }
        #else
        SplitInboxView()
        #endif
    }
}

// MARK: - Split layout (iPad regular / macOS)

private struct SplitInboxView: View {
    @Environment(AppState.self) private var appState
    @State private var selectedAddress: Address?
    @State private var selectedMessageID: String?
    @State private var showNewAddress = false
    @State private var showCompose = false
    @State private var showSettings = false

    var body: some View {
        NavigationSplitView {
            AddressListView(selection: $selectedAddress)
                .navigationTitle("Inboxes")
                .toolbar { sidebarToolbar(showNewAddress: $showNewAddress, showSettings: $showSettings) }
        } content: {
            MessageListView(
                address: selectedAddress,
                selection: $selectedMessageID
            )
            .navigationTitle(selectedAddress.map { addressTitle($0, domain: appState.session?.domain ?? "") } ?? "All Messages")
        } detail: {
            if let id = selectedMessageID {
                MessageDetailView(messageID: id, onReply: { showCompose = true })
                    .id(id)
            } else {
                ContentUnavailableView(
                    "No message selected",
                    systemImage: "envelope",
                    description: Text("Pick a message from the list.")
                )
            }
        }
        .overlay(alignment: .bottomTrailing) {
            composeFAB { showCompose = true }
                .padding(20)
        }
        .sheet(isPresented: $showNewAddress) { newAddressSheet() }
        .sheet(isPresented: $showCompose) {
            ComposeView(prefill: prefillFor(addresses: appState.addresses, selected: selectedAddress, domain: appState.session?.domain ?? ""))
                #if os(macOS)
                .frame(width: 620, height: 520)
                #endif
        }
        .sheet(isPresented: $showSettings) { settingsSheet() }
        .task {
            await appState.refreshAddresses()
            await appState.refreshMessages(for: nil)
        }
    }
}

// MARK: - Stacked layout (iPhone)

#if os(iOS)
private struct StackedInboxView: View {
    @Environment(AppState.self) private var appState
    @State private var path = NavigationPath()
    @State private var showNewAddress = false
    @State private var showCompose = false
    @State private var showSettings = false

    enum AddressNav: Hashable {
        case all
        case address(Address)

        var address: Address? {
            switch self {
            case .all: return nil
            case .address(let a): return a
            }
        }
    }

    private var pinned: [Address] { appState.addresses.filter { $0.pinned } }
    private var unpinned: [Address] { appState.addresses.filter { !$0.pinned } }
    private var totalUnread: Int { appState.addresses.reduce(0) { $0 + $1.unreadCount } }
    private var domain: String { appState.session?.domain ?? "" }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                Section {
                    NavigationLink(value: AddressNav.all) {
                        HStack {
                            Image(systemName: "tray.full").frame(width: 22)
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
                    }
                }

                if !pinned.isEmpty {
                    Section("Pinned") {
                        ForEach(pinned) { addr in
                            NavigationLink(value: AddressNav.address(addr)) {
                                AddressRowContent(addr: addr, domain: domain)
                            }
                            .swipeActions(edge: .leading) {
                                Button {
                                    Task { await appState.setPinned(addr, pinned: false) }
                                } label: {
                                    Label("Unpin", systemImage: "pin.slash")
                                }
                                .tint(.gray)
                            }
                            .swipeActions(edge: .trailing) {
                                Button(role: .destructive) {
                                    Task { await appState.deleteAddress(addr.localPart) }
                                } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                            }
                        }
                    }
                }

                Section(pinned.isEmpty ? "Addresses" : "All") {
                    ForEach(unpinned) { addr in
                        NavigationLink(value: AddressNav.address(addr)) {
                            AddressRowContent(addr: addr, domain: domain)
                        }
                        .swipeActions(edge: .leading) {
                            Button {
                                Task { await appState.setPinned(addr, pinned: true) }
                            } label: {
                                Label("Pin", systemImage: "pin")
                            }
                            .tint(.accentColor)
                        }
                        .swipeActions(edge: .trailing) {
                            Button(role: .destructive) {
                                Task { await appState.deleteAddress(addr.localPart) }
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationTitle("Inboxes")
            .toolbar { sidebarToolbar(showNewAddress: $showNewAddress, showSettings: $showSettings) }
            .refreshable { await appState.refreshAddresses() }
            .navigationDestination(for: AddressNav.self) { nav in
                StackedMessageList(address: nav.address) { messageID in
                    showCompose = true
                    _ = messageID
                }
            }
        }
        .safeAreaInset(edge: .bottom, alignment: .trailing, spacing: 0) {
            if path.count < 2 {
                composeFAB { showCompose = true }
                    .padding(20)
                    .transition(.scale(scale: 0.6).combined(with: .opacity))
            }
        }
        .animation(.spring(response: 0.35, dampingFraction: 0.85), value: path.count)
        .sheet(isPresented: $showNewAddress) { newAddressSheet() }
        .sheet(isPresented: $showCompose) {
            ComposeView(prefill: prefillFor(addresses: appState.addresses, selected: nil, domain: domain))
        }
        .sheet(isPresented: $showSettings) { settingsSheet() }
        .task { await appState.refreshAddresses() }
    }
}

private struct StackedMessageList: View {
    let address: Address?
    var onReplyRequested: (String) -> Void

    @Environment(AppState.self) private var appState

    var body: some View {
        List {
            ForEach(messages) { msg in
                NavigationLink(value: msg) {
                    MessageRowContent(message: msg)
                }
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
        .navigationTitle(address.map { addressTitle($0, domain: appState.session?.domain ?? "") } ?? "All Messages")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await appState.refreshMessages(for: address?.localPart) }
        .task(id: address?.localPart) {
            await appState.refreshMessages(for: address?.localPart)
        }
        .navigationDestination(for: MessageSummary.self) { msg in
            MessageDetailView(messageID: msg.id, onReply: { onReplyRequested(msg.id) })
        }
    }

    private var messages: [MessageSummary] {
        if let lp = address?.localPart {
            return appState.messagesByAddress[lp] ?? []
        }
        return appState.messagesByAddress["__all__"] ?? []
    }
}
#endif

// MARK: - Shared row content

struct AddressRowContent: View {
    let addr: Address
    let domain: String

    var body: some View {
        HStack(spacing: 12) {
            AddressAvatar(localPart: addr.localPart)
            VStack(alignment: .leading, spacing: 2) {
                Text(addr.displayName)
                    .font(.body)
                    .lineLimit(1)
                if !domain.isEmpty {
                    Text("\(addr.localPart)@\(domain)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            Spacer(minLength: 8)
            if addr.unreadCount > 0 {
                Text("\(addr.unreadCount)")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 8).padding(.vertical, 2)
                    .background(.tint, in: .capsule)
            }
        }
        .padding(.vertical, 2)
    }
}

struct AddressAvatar: View {
    let localPart: String

    var body: some View {
        ZStack {
            Circle()
                .fill(LinearGradient(
                    colors: [color.opacity(0.85), color.opacity(1.0)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ))
                .frame(width: 34, height: 34)
            Text(initial)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
        }
    }

    private var initial: String {
        let s = localPart.unicodeScalars.first(where: { $0.isASCII && $0.value > 0x20 })
        return s.map { String(Character($0)).uppercased() } ?? "?"
    }

    private var color: Color {
        let palette: [Color] = [
            .blue, .indigo, .purple, .pink, .red, .orange, .yellow,
            .green, .mint, .teal, .cyan
        ]
        var h: UInt64 = 5381
        for b in localPart.utf8 {
            h = h &* 33 &+ UInt64(b)
        }
        return palette[Int(h % UInt64(palette.count))]
    }
}

struct MessageRowContent: View {
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

// MARK: - Shared helpers

private func addressTitle(_ a: Address, domain: String) -> String {
    domain.isEmpty ? a.displayName : "\(a.localPart)@\(domain)"
}

private func composeFAB(action: @escaping () -> Void) -> some View {
    Button(action: action) {
        Label("Compose", systemImage: "square.and.pencil")
            .labelStyle(.iconOnly)
            .font(.title3.weight(.semibold))
            .frame(width: 56, height: 56)
    }
    .buttonStyle(.glassProminent)
    .clipShape(.circle)
    .shadow(color: .black.opacity(0.15), radius: 12, y: 6)
}

@ToolbarContentBuilder
private func sidebarToolbar(showNewAddress: Binding<Bool>, showSettings: Binding<Bool>) -> some ToolbarContent {
    ToolbarItem(placement: .primaryAction) {
        Button { showNewAddress.wrappedValue = true } label: {
            Label("New Address", systemImage: "plus")
        }
    }
    ToolbarItem(placement: .secondaryAction) {
        Button { showSettings.wrappedValue = true } label: {
            Label("Settings", systemImage: "gearshape")
        }
    }
}

@ViewBuilder
private func newAddressSheet() -> some View {
    NewAddressSheet()
        #if os(macOS)
        .frame(width: 460, height: 380)
        #else
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
        #endif
}

@ViewBuilder
private func settingsSheet() -> some View {
    SettingsView()
        #if os(macOS)
        .frame(width: 460, height: 360)
        #endif
}

private func prefillFor(addresses: [Address], selected: Address?, domain: String) -> ComposeView.Prefill? {
    let from = selected?.localPart ?? addresses.first?.localPart
    return ComposeView.Prefill(fromLocalPart: from, fromDomain: domain, to: [], subject: "", body: "", inReplyTo: nil)
}
