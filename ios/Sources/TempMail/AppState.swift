import Foundation
import Observation
import SwiftUI

enum AppearanceMode: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var label: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

@MainActor
@Observable
final class AppState {
    struct Session: Equatable {
        var baseURL: URL
        var token: String
        var domain: String
    }

    var session: Session?
    var client: APIClient?
    var sse: SSEClient?
    var bootstrapping = true

    var addresses: [Address] = []
    var messagesByAddress: [String: [MessageSummary]] = [:]
    var messageDetails: [String: MessageDetail] = [:]

    var appearance: AppearanceMode {
        didSet {
            UserDefaults.standard.set(appearance.rawValue, forKey: "appearance")
        }
    }

    var sseTask: Task<Void, Never>?

    init() {
        let raw = UserDefaults.standard.string(forKey: "appearance") ?? AppearanceMode.system.rawValue
        self.appearance = AppearanceMode(rawValue: raw) ?? .system
    }

    func bootstrap() async {
        defer { bootstrapping = false }
        guard let stored = AuthStore.shared.load() else { return }
        let probe = APIClient(baseURL: stored.url, token: stored.token)
        do {
            let cfg = try await probe.publicConfig()
            establish(baseURL: stored.url, token: stored.token, domain: cfg.domain)
        } catch {
            AuthStore.shared.clear()
        }
    }

    func signIn(baseURL: URL, token: String) async throws {
        let probe = APIClient(baseURL: baseURL, token: token)
        let cfg = try await probe.publicConfig()
        let pinged = APIClient(baseURL: baseURL, token: token)
        _ = try await pinged.listAddresses()
        AuthStore.shared.save(url: baseURL, token: token)
        establish(baseURL: baseURL, token: token, domain: cfg.domain)
    }

    func signOut() {
        AuthStore.shared.clear()
        sseTask?.cancel()
        sseTask = nil
        session = nil
        client = nil
        sse = nil
        addresses = []
        messagesByAddress = [:]
        messageDetails = [:]
    }

    private func establish(baseURL: URL, token: String, domain: String) {
        let api = APIClient(baseURL: baseURL, token: token)
        let sseClient = SSEClient(baseURL: baseURL, token: token)
        session = Session(baseURL: baseURL, token: token, domain: domain)
        client = api
        sse = sseClient
        startSSE()
    }

    private func startSSE() {
        sseTask?.cancel()
        guard let sse else { return }
        sseTask = Task { [weak self] in
            for await event in sse.stream() {
                await self?.handle(event: event)
            }
        }
    }

    private func handle(event: SSEEvent) async {
        switch event {
        case .newMessage(_, let localPart):
            await refreshAddresses()
            await refreshMessages(for: localPart)
        case .messageDeleted, .addressAdded, .addressRemoved:
            await refreshAddresses()
        }
    }

    func refreshAddresses() async {
        guard let client else { return }
        do { addresses = try await client.listAddresses() } catch { }
    }

    func refreshMessages(for localPart: String?) async {
        guard let client else { return }
        do {
            let msgs = try await client.listMessages(address: localPart)
            if let localPart {
                messagesByAddress[localPart] = msgs
            } else {
                messagesByAddress["__all__"] = msgs
            }
        } catch { }
    }

    func loadDetail(id: String) async -> MessageDetail? {
        if let cached = messageDetails[id] { return cached }
        guard let client else { return nil }
        do {
            let detail = try await client.getMessage(id: id)
            messageDetails[id] = detail
            return detail
        } catch { return nil }
    }

    func markRead(id: String, localPart: String) async {
        guard let client else { return }
        try? await client.markRead(id: id)
        if var list = messagesByAddress[localPart],
           let idx = list.firstIndex(where: { $0.id == id }) {
            let m = list[idx]
            list[idx] = MessageSummary(
                id: m.id, localPart: m.localPart, fromAddr: m.fromAddr,
                fromName: m.fromName, subject: m.subject, preview: m.preview,
                receivedAt: m.receivedAt, sizeBytes: m.sizeBytes,
                hasHtml: m.hasHtml, hasText: m.hasText, read: true
            )
            messagesByAddress[localPart] = list
        }
        await refreshAddresses()
    }

    func deleteMessage(id: String, localPart: String) async {
        guard let client else { return }
        try? await client.deleteMessage(id: id)
        messagesByAddress[localPart]?.removeAll { $0.id == id }
        messageDetails[id] = nil
        await refreshAddresses()
    }

    func createAddress(localPart: String, label: String?, pinned: Bool) async throws {
        guard let client else { return }
        _ = try await client.createAddress(localPart: localPart, label: label, pinned: pinned)
        await refreshAddresses()
    }

    func setPinned(_ address: Address, pinned: Bool) async {
        guard let client else { return }
        do {
            _ = try await client.createAddress(
                localPart: address.localPart,
                label: address.label,
                pinned: pinned
            )
            await refreshAddresses()
        } catch { }
    }

    func deleteAddress(_ localPart: String) async {
        guard let client else { return }
        do {
            try await client.deleteAddress(localPart)
            messagesByAddress[localPart] = nil
            await refreshAddresses()
        } catch { }
    }
}
