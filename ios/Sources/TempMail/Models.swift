import Foundation

struct PublicConfig: Decodable, Sendable {
    let domain: String
    let catchAll: Bool
    let maxMessageBytes: Int
}

struct Address: Decodable, Hashable, Identifiable, Sendable {
    let localPart: String
    let label: String?
    let pinned: Bool
    let createdAt: Int
    let messageCount: Int
    let lastReceivedAt: Int?
    let unreadCount: Int

    var id: String { localPart }
    var displayName: String { label ?? localPart }
}

struct MessageSummary: Decodable, Hashable, Identifiable, Sendable {
    let id: String
    let localPart: String
    let fromAddr: String
    let fromName: String?
    let subject: String?
    let preview: String?
    let receivedAt: Int
    let sizeBytes: Int
    let hasHtml: Bool
    let hasText: Bool
    let read: Bool

    var fromDisplay: String { fromName ?? fromAddr }
    var subjectDisplay: String { (subject?.isEmpty == false ? subject : nil) ?? "(no subject)" }
}

struct MessageDetail: Decodable, Sendable {
    let id: String
    let localPart: String
    let fromAddr: String
    let fromName: String?
    let subject: String?
    let preview: String?
    let receivedAt: Int
    let sizeBytes: Int
    let hasHtml: Bool
    let hasText: Bool
    let read: Bool
    let toAddrs: [String]
    let textBody: String?
    let htmlBody: String?
    let messageIdHeader: String?
    let inReplyTo: String?
    let attachments: [AttachmentMeta]
}

struct AttachmentMeta: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let filename: String?
    let mimeType: String?
    let sizeBytes: Int
}

extension Int {
    func formattedReceivedDate() -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(self))
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f.localizedString(for: date, relativeTo: Date())
    }
    func formattedReceivedDateLong() -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(self))
        return date.formatted(date: .abbreviated, time: .shortened)
    }
}
