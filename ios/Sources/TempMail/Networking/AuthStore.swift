import Foundation
import Security

struct StoredCredentials {
    let url: URL
    let token: String
}

final class AuthStore: Sendable {
    static let shared = AuthStore()
    private init() {}

    private let service = "com.jalencode.tempmail"
    private let account = "default"

    func save(url: URL, token: String) {
        let value = "\(url.absoluteString)\n\(token)"
        guard let data = value.data(using: .utf8) else { return }

        SecItemDelete([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
        ] as CFDictionary)

        SecItemAdd([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecValueData: data,
            kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlock,
        ] as CFDictionary, nil)
    }

    func load() -> StoredCredentials? {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne,
        ]
        var item: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data,
              let str = String(data: data, encoding: .utf8) else { return nil }
        let parts = str.split(separator: "\n", maxSplits: 1)
        guard parts.count == 2, let url = URL(string: String(parts[0])) else { return nil }
        return StoredCredentials(url: url, token: String(parts[1]))
    }

    func clear() {
        SecItemDelete([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
        ] as CFDictionary)
    }
}
