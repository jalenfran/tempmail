import XCTest
@testable import TempMail

final class ModelsTests: XCTestCase {
    private func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    func testDecodePublicConfig() throws {
        let json = #"""
        { "domain": "jalencode.com", "catch_all": true, "max_message_bytes": 26214400 }
        """#.data(using: .utf8)!

        let cfg = try decoder().decode(PublicConfig.self, from: json)
        XCTAssertEqual(cfg.domain, "jalencode.com")
        XCTAssertTrue(cfg.catchAll)
        XCTAssertEqual(cfg.maxMessageBytes, 26_214_400)
    }

    func testDecodeAddress() throws {
        let json = #"""
        {
            "local_part": "hello",
            "label": "personal",
            "pinned": true,
            "created_at": 1779389071,
            "message_count": 2,
            "last_received_at": 1779389071,
            "unread_count": 1
        }
        """#.data(using: .utf8)!

        let a = try decoder().decode(Address.self, from: json)
        XCTAssertEqual(a.localPart, "hello")
        XCTAssertEqual(a.label, "personal")
        XCTAssertTrue(a.pinned)
        XCTAssertEqual(a.messageCount, 2)
        XCTAssertEqual(a.unreadCount, 1)
        XCTAssertEqual(a.id, "hello")
        XCTAssertEqual(a.displayName, "personal")
    }

    func testDecodeAddressWithNulls() throws {
        let json = #"""
        {
            "local_part": "no-label",
            "label": null,
            "pinned": false,
            "created_at": 1779389071,
            "message_count": 0,
            "last_received_at": null,
            "unread_count": 0
        }
        """#.data(using: .utf8)!

        let a = try decoder().decode(Address.self, from: json)
        XCTAssertNil(a.label)
        XCTAssertNil(a.lastReceivedAt)
        XCTAssertEqual(a.displayName, "no-label")
    }

    func testDecodeMessageSummary() throws {
        let json = #"""
        {
            "id": "01KS5Z6VJ7RVCWTKFQGH0B1Q1B",
            "local_part": "hello",
            "from_addr": "sender@example.com",
            "from_name": "A Sender",
            "subject": "Hi there",
            "preview": "first line",
            "received_at": 1779390705,
            "size_bytes": 479,
            "has_html": true,
            "has_text": true,
            "read": false
        }
        """#.data(using: .utf8)!

        let m = try decoder().decode(MessageSummary.self, from: json)
        XCTAssertEqual(m.id, "01KS5Z6VJ7RVCWTKFQGH0B1Q1B")
        XCTAssertEqual(m.fromAddr, "sender@example.com")
        XCTAssertEqual(m.fromDisplay, "A Sender")
        XCTAssertEqual(m.subjectDisplay, "Hi there")
        XCTAssertTrue(m.hasHtml)
        XCTAssertFalse(m.read)
    }

    func testMessageSummaryFallbacks() throws {
        let json = #"""
        {
            "id": "abc",
            "local_part": "x",
            "from_addr": "raw@example.com",
            "from_name": null,
            "subject": null,
            "preview": null,
            "received_at": 0,
            "size_bytes": 1,
            "has_html": false,
            "has_text": false,
            "read": true
        }
        """#.data(using: .utf8)!

        let m = try decoder().decode(MessageSummary.self, from: json)
        XCTAssertEqual(m.fromDisplay, "raw@example.com")
        XCTAssertEqual(m.subjectDisplay, "(no subject)")
    }

    func testDecodeMessageDetail() throws {
        let json = #"""
        {
            "id": "abc",
            "local_part": "hello",
            "from_addr": "s@example.com",
            "from_name": null,
            "subject": "Hi",
            "preview": "p",
            "received_at": 1,
            "size_bytes": 100,
            "has_html": true,
            "has_text": true,
            "read": false,
            "to_addrs": ["hello@jalencode.com", "cc@example.com"],
            "text_body": "plain",
            "html_body": "<p>html</p>",
            "message_id_header": "<m@x>",
            "in_reply_to": null,
            "attachments": [
                { "id": "att1", "filename": "a.txt", "mime_type": "text/plain", "size_bytes": 10 }
            ]
        }
        """#.data(using: .utf8)!

        let d = try decoder().decode(MessageDetail.self, from: json)
        XCTAssertEqual(d.toAddrs, ["hello@jalencode.com", "cc@example.com"])
        XCTAssertEqual(d.textBody, "plain")
        XCTAssertEqual(d.htmlBody, "<p>html</p>")
        XCTAssertEqual(d.messageIdHeader, "<m@x>")
        XCTAssertNil(d.inReplyTo)
        XCTAssertEqual(d.attachments.count, 1)
        XCTAssertEqual(d.attachments[0].filename, "a.txt")
        XCTAssertEqual(d.attachments[0].mimeType, "text/plain")
    }

    func testReceivedDateFormatting() {
        // 30 seconds ago
        let ts = Int(Date().timeIntervalSince1970) - 30
        let short = ts.formattedReceivedDate()
        XCTAssertFalse(short.isEmpty)
        // Just shouldn't crash and should produce a non-empty value
        let long = ts.formattedReceivedDateLong()
        XCTAssertFalse(long.isEmpty)
    }
}
