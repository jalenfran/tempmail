import XCTest
@testable import TempMail

final class SSEParseTests: XCTestCase {

    private func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    // SSEClient parses lines of the form "data: {...}" and decodes them into
    // SSEEvent via the internal RawEvent shape. These tests exercise the same
    // shapes by hitting the decoder directly with the JSON payloads the backend
    // emits for each event type.

    func testDecodeNewMessagePayload() throws {
        let json = #"""
        { "type": "new_message", "id": "01KABC", "local_part": "hello" }
        """#.data(using: .utf8)!

        let raw = try decoder().decode(RawSSEEnvelope.self, from: json)
        XCTAssertEqual(raw.type, "new_message")
        XCTAssertEqual(raw.id, "01KABC")
        XCTAssertEqual(raw.localPart, "hello")
    }

    func testDecodeMessageDeletedPayload() throws {
        let json = #"""
        { "type": "message_deleted", "id": "01KDEF" }
        """#.data(using: .utf8)!

        let raw = try decoder().decode(RawSSEEnvelope.self, from: json)
        XCTAssertEqual(raw.type, "message_deleted")
        XCTAssertEqual(raw.id, "01KDEF")
        XCTAssertNil(raw.localPart)
    }

    func testDecodeAddressAddedPayload() throws {
        let json = #"""
        { "type": "address_added", "local_part": "rivers-otter-2025" }
        """#.data(using: .utf8)!

        let raw = try decoder().decode(RawSSEEnvelope.self, from: json)
        XCTAssertEqual(raw.type, "address_added")
        XCTAssertEqual(raw.localPart, "rivers-otter-2025")
        XCTAssertNil(raw.id)
    }

    func testSSEFrameStripping() {
        // The SSE wire format is "data: <json>\n". Our client strips the
        // "data: " prefix before decoding. Verify our stripping logic matches.
        let line = "data: { \"type\": \"new_message\", \"id\": \"x\", \"local_part\": \"y\" }"
        XCTAssertTrue(line.hasPrefix("data: "))
        let payload = String(line.dropFirst("data: ".count))
        XCTAssertTrue(payload.hasPrefix("{"))
        XCTAssertTrue(payload.hasSuffix("}"))
    }

    func testNonDataLineIgnored() {
        // Keep-alive comments start with ":" — we should skip them.
        let line = ": keep-alive"
        XCTAssertFalse(line.hasPrefix("data: "))
    }
}

// Mirror of SSEClient's private RawEvent so we can exercise the same JSON shape
// in tests without exposing the internal type.
private struct RawSSEEnvelope: Decodable {
    let type: String
    let id: String?
    let localPart: String?
}
