import Foundation

enum SSEEvent: Sendable {
    case newMessage(id: String, localPart: String)
    case messageDeleted(id: String)
    case addressAdded(localPart: String)
    case addressRemoved(localPart: String)
}

private struct RawEvent: Decodable {
    let type: String
    let id: String?
    let localPart: String?
}

final class SSEClient: @unchecked Sendable {
    let baseURL: URL
    let token: String

    init(baseURL: URL, token: String) {
        self.baseURL = baseURL
        self.token = token
    }

    func stream() -> AsyncStream<SSEEvent> {
        AsyncStream { continuation in
            let task = Task { [baseURL, token] in
                let url = baseURL.appendingPathComponent("/v1/events")
                var req = URLRequest(url: url)
                req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
                req.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                req.timeoutInterval = TimeInterval.greatestFiniteMagnitude

                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase

                // Reconnect loop with backoff.
                var backoff: UInt64 = 1_000_000_000
                while !Task.isCancelled {
                    do {
                        let (bytes, response) = try await URLSession.shared.bytes(for: req)
                        if let http = response as? HTTPURLResponse,
                           !(200..<300).contains(http.statusCode) {
                            try await Task.sleep(nanoseconds: backoff)
                            backoff = min(backoff * 2, 30_000_000_000)
                            continue
                        }
                        backoff = 1_000_000_000
                        for try await line in bytes.lines {
                            if Task.isCancelled { break }
                            guard line.hasPrefix("data: ") else { continue }
                            let payload = String(line.dropFirst("data: ".count))
                            guard let data = payload.data(using: .utf8),
                                  let raw = try? decoder.decode(RawEvent.self, from: data) else {
                                continue
                            }
                            if let event = parse(raw) {
                                continuation.yield(event)
                            }
                        }
                    } catch {
                        if Task.isCancelled { break }
                        try? await Task.sleep(nanoseconds: backoff)
                        backoff = min(backoff * 2, 30_000_000_000)
                    }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    private func parse(_ raw: RawEvent) -> SSEEvent? {
        switch raw.type {
        case "new_message":
            guard let id = raw.id, let lp = raw.localPart else { return nil }
            return .newMessage(id: id, localPart: lp)
        case "message_deleted":
            guard let id = raw.id else { return nil }
            return .messageDeleted(id: id)
        case "address_added":
            guard let lp = raw.localPart else { return nil }
            return .addressAdded(localPart: lp)
        case "address_removed":
            guard let lp = raw.localPart else { return nil }
            return .addressRemoved(localPart: lp)
        default:
            return nil
        }
    }
}
