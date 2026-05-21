import Foundation

enum APIError: Error, LocalizedError {
    case invalidURL
    case transport(Error)
    case server(Int, String)
    case decoding(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL: return "Invalid URL."
        case .transport(let e): return "Network error: \(e.localizedDescription)"
        case .server(let code, let msg):
            switch code {
            case 401: return "Server rejected token (401)."
            case 404: return "Not found (404)."
            default: return "Server error \(code)\(msg.isEmpty ? "" : ": \(msg)")"
            }
        case .decoding(let e): return "Could not decode response: \(e)"
        }
    }
}

final class APIClient: @unchecked Sendable {
    let baseURL: URL
    let token: String
    private let session: URLSession
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder

    init(baseURL: URL, token: String) {
        self.baseURL = baseURL
        self.token = token
        let cfg = URLSessionConfiguration.default
        cfg.waitsForConnectivity = true
        cfg.timeoutIntervalForRequest = 30
        self.session = URLSession(configuration: cfg)
        self.decoder = JSONDecoder()
        self.decoder.keyDecodingStrategy = .convertFromSnakeCase
        self.encoder = JSONEncoder()
        self.encoder.keyEncodingStrategy = .convertToSnakeCase
    }

    // MARK: - Endpoints

    func publicConfig() async throws -> PublicConfig {
        try await get("/v1/config", authenticated: false)
    }

    func listAddresses() async throws -> [Address] {
        try await get("/v1/addresses")
    }

    func createAddress(localPart: String, label: String?, pinned: Bool) async throws -> Address {
        struct Req: Encodable { let localPart: String; let label: String?; let pinned: Bool }
        return try await post("/v1/addresses", body: Req(localPart: localPart, label: label, pinned: pinned))
    }

    func deleteAddress(_ localPart: String) async throws {
        try await delete("/v1/addresses/\(localPart)")
    }

    func listMessages(address: String?, cursor: String? = nil, limit: Int = 50) async throws -> [MessageSummary] {
        var items: [URLQueryItem] = [URLQueryItem(name: "limit", value: String(limit))]
        if let address { items.append(URLQueryItem(name: "address", value: address)) }
        if let cursor { items.append(URLQueryItem(name: "cursor", value: cursor)) }
        return try await get("/v1/messages", query: items)
    }

    func getMessage(id: String) async throws -> MessageDetail {
        try await get("/v1/messages/\(id)")
    }

    func markRead(id: String) async throws {
        struct Empty: Encodable {}
        let _: EmptyResponse = try await post("/v1/messages/\(id)/read", body: Empty())
    }

    func deleteMessage(id: String) async throws {
        try await delete("/v1/messages/\(id)")
    }

    func send(fromLocalPart: String, to: [String], subject: String, text: String?, html: String?, inReplyTo: String?) async throws {
        struct Req: Encodable {
            let fromLocalPart: String
            let to: [String]
            let subject: String
            let text: String?
            let html: String?
            let inReplyTo: String?
        }
        let body = Req(fromLocalPart: fromLocalPart, to: to, subject: subject, text: text, html: html, inReplyTo: inReplyTo)
        let _: EmptyResponse = try await post("/v1/send", body: body)
    }

    func attachmentURL(messageId: String, attachmentId: String) -> URL {
        baseURL.appendingPathComponent("/v1/messages/\(messageId)/attachments/\(attachmentId)")
    }

    func rawEmlURL(messageId: String) -> URL {
        baseURL.appendingPathComponent("/v1/messages/\(messageId)/raw")
    }

    // MARK: - Plumbing

    private struct EmptyResponse: Decodable {}

    private func makeURL(_ path: String, query: [URLQueryItem] = []) throws -> URL {
        guard var comps = URLComponents(url: baseURL.appendingPathComponent(path), resolvingAgainstBaseURL: false) else {
            throw APIError.invalidURL
        }
        if !query.isEmpty { comps.queryItems = query }
        guard let url = comps.url else { throw APIError.invalidURL }
        return url
    }

    private func get<T: Decodable>(_ path: String, query: [URLQueryItem] = [], authenticated: Bool = true) async throws -> T {
        let url = try makeURL(path, query: query)
        var req = URLRequest(url: url)
        req.httpMethod = "GET"
        if authenticated {
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return try await execute(req)
    }

    private func post<B: Encodable, T: Decodable>(_ path: String, body: B) async throws -> T {
        let url = try makeURL(path)
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try encoder.encode(body)
        return try await execute(req)
    }

    private func delete(_ path: String) async throws {
        let url = try makeURL(path)
        var req = URLRequest(url: url)
        req.httpMethod = "DELETE"
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let _: EmptyResponse = try await execute(req)
    }

    private func execute<T: Decodable>(_ req: URLRequest) async throws -> T {
        let data: Data, resp: URLResponse
        do {
            (data, resp) = try await session.data(for: req)
        } catch {
            throw APIError.transport(error)
        }
        guard let http = resp as? HTTPURLResponse else {
            throw APIError.server(-1, "no http response")
        }
        guard (200..<300).contains(http.statusCode) else {
            let msg = String(data: data, encoding: .utf8) ?? ""
            throw APIError.server(http.statusCode, msg)
        }
        if T.self == EmptyResponse.self {
            return EmptyResponse() as! T
        }
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw APIError.decoding(error)
        }
    }
}
