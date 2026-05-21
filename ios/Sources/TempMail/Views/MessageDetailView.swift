import SwiftUI
#if canImport(WebKit)
import WebKit
#endif

struct MessageDetailView: View {
    @Environment(AppState.self) private var appState
    let messageID: String
    var onReply: () -> Void

    @State private var detail: MessageDetail?
    @State private var mode: Mode = .auto

    enum Mode: String, CaseIterable, Identifiable {
        case auto, text, html
        var id: String { rawValue }
        var label: String {
            switch self {
            case .auto: return "Auto"
            case .text: return "Text"
            case .html: return "HTML"
            }
        }
    }

    var body: some View {
        Group {
            if let detail {
                content(detail)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: messageID) { await load() }
    }

    private func load() async {
        detail = nil
        detail = await appState.loadDetail(id: messageID)
        if let d = detail, !d.read {
            await appState.markRead(id: d.id, localPart: d.localPart)
        }
    }

    @ViewBuilder
    private func content(_ d: MessageDetail) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header(d)
                Divider().padding(.horizontal)
                body(d)
                if !d.attachments.isEmpty {
                    attachmentsSection(d)
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
            .frame(maxWidth: Theme.readingMaxWidth, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    onReply()
                } label: {
                    Label("Reply", systemImage: "arrowshape.turn.up.left")
                }
            }
            ToolbarItem(placement: .secondaryAction) {
                Menu {
                    ForEach(Mode.allCases) { m in
                        Button {
                            mode = m
                        } label: {
                            HStack {
                                Text(m.label)
                                if mode == m { Image(systemName: "checkmark") }
                            }
                        }
                    }
                } label: {
                    Label("View Mode", systemImage: "doc.plaintext")
                }
            }
            ToolbarItem(placement: .destructiveAction) {
                Button(role: .destructive) {
                    Task { await appState.deleteMessage(id: d.id, localPart: d.localPart) }
                } label: {
                    Label("Delete", systemImage: "trash")
                }
            }
        }
    }

    private func header(_ d: MessageDetail) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(d.subject ?? "(no subject)")
                .font(.title2.weight(.semibold))
                .textSelection(.enabled)

            HStack(spacing: 10) {
                AddressAvatar(localPart: d.fromAddr)
                VStack(alignment: .leading, spacing: 1) {
                    Text(d.fromName ?? d.fromAddr)
                        .font(.subheadline.weight(.medium))
                        .textSelection(.enabled)
                    Text(d.fromAddr)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                Text(d.receivedAt.formattedReceivedDateLong())
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 6) {
                Image(systemName: "arrow.right")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                ForEach(d.toAddrs, id: \.self) { addr in
                    Text(addr)
                        .font(.caption.monospaced())
                        .padding(.horizontal, 10).padding(.vertical, 4)
                        .glassEffect(.regular, in: .capsule)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(18)
        .glassEffect(.regular, in: .rect(cornerRadius: 20))
    }

    @ViewBuilder
    private func body(_ d: MessageDetail) -> some View {
        let effective: Mode = {
            switch mode {
            case .auto:
                return d.hasHtml ? .html : .text
            case .text, .html:
                return mode
            }
        }()

        switch effective {
        case .html:
            if let html = d.htmlBody {
                HTMLBody(html: html)
            } else {
                Text("(no HTML body)").foregroundStyle(.secondary)
            }
        default:
            if let text = d.textBody {
                Text(text)
                    .font(.body)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("(no text body)").foregroundStyle(.secondary)
            }
        }
    }

    private func attachmentsSection(_ d: MessageDetail) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Attachments")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            FlowLayout(spacing: 8) {
                ForEach(d.attachments) { att in
                    AttachmentChip(messageID: d.id, attachment: att)
                }
            }
        }
        .padding(.top, 12)
    }
}

private struct AttachmentChip: View {
    @Environment(AppState.self) private var appState
    let messageID: String
    let attachment: AttachmentMeta

    var body: some View {
        Button {
            #if os(macOS)
            if let url = appState.client?.attachmentURL(messageId: messageID, attachmentId: attachment.id) {
                NSWorkspace.shared.open(url)
            }
            #else
            if let url = appState.client?.attachmentURL(messageId: messageID, attachmentId: attachment.id) {
                UIApplication.shared.open(url)
            }
            #endif
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "paperclip")
                VStack(alignment: .leading, spacing: 1) {
                    Text(attachment.filename ?? "attachment")
                        .font(.caption.weight(.medium))
                        .lineLimit(1)
                    Text(byteCount(attachment.sizeBytes))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
        }
        .buttonStyle(.glass)
        .clipShape(.rect(cornerRadius: 12))
    }

    private func byteCount(_ n: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(n), countStyle: .file)
    }
}

private struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        for v in subviews {
            let size = v.sizeThatFits(.unspecified)
            if x + size.width > maxWidth {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: maxWidth.isFinite ? maxWidth : x, height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x: CGFloat = bounds.minX
        var y: CGFloat = bounds.minY
        var rowHeight: CGFloat = 0
        for v in subviews {
            let size = v.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX {
                x = bounds.minX
                y += rowHeight + spacing
                rowHeight = 0
            }
            v.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}

#if canImport(WebKit)

struct HTMLBody: View {
    let html: String
    @State private var measuredHeight: CGFloat = 80

    var body: some View {
        WebViewBridge(html: html, height: $measuredHeight)
            .frame(height: measuredHeight)
    }
}

#if os(macOS)
import AppKit

private struct WebViewBridge: NSViewRepresentable {
    let html: String
    @Binding var height: CGFloat

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> WKWebView {
        let cfg = WKWebViewConfiguration()
        cfg.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: cfg)
        view.setValue(false, forKey: "drawsBackground")
        view.navigationDelegate = context.coordinator
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        if context.coordinator.lastHTML != html {
            context.coordinator.lastHTML = html
            view.loadHTMLString(htmlWrap(html), baseURL: nil)
        }
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        let parent: WebViewBridge
        var lastHTML: String?
        init(_ parent: WebViewBridge) { self.parent = parent }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            measureHeight(webView)
        }

        private func measureHeight(_ webView: WKWebView) {
            webView.evaluateJavaScript("document.documentElement.scrollHeight") { [weak self] result, _ in
                guard let self else { return }
                let h: CGFloat = {
                    if let n = result as? CGFloat { return n }
                    if let n = result as? Double { return CGFloat(n) }
                    if let n = result as? Int { return CGFloat(n) }
                    return 80
                }()
                DispatchQueue.main.async {
                    self.parent.height = max(h, 80)
                }
            }
        }
    }
}

#else
import UIKit

private struct WebViewBridge: UIViewRepresentable {
    let html: String
    @Binding var height: CGFloat

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> WKWebView {
        let cfg = WKWebViewConfiguration()
        cfg.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: cfg)
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.backgroundColor = .clear
        view.scrollView.isScrollEnabled = false
        view.scrollView.bounces = false
        view.navigationDelegate = context.coordinator
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        if context.coordinator.lastHTML != html {
            context.coordinator.lastHTML = html
            view.loadHTMLString(htmlWrap(html), baseURL: nil)
        }
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        let parent: WebViewBridge
        var lastHTML: String?
        init(_ parent: WebViewBridge) { self.parent = parent }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            // Some content (images, late layout) settles after didFinish.
            // Measure now, then once more after a short delay.
            measure(webView)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self, weak webView] in
                guard let self, let webView else { return }
                self.measure(webView)
            }
        }

        private func measure(_ webView: WKWebView) {
            webView.evaluateJavaScript("document.documentElement.scrollHeight") { [weak self] result, _ in
                guard let self else { return }
                let h: CGFloat = {
                    if let n = result as? CGFloat { return n }
                    if let n = result as? Double { return CGFloat(n) }
                    if let n = result as? Int { return CGFloat(n) }
                    return 80
                }()
                DispatchQueue.main.async {
                    self.parent.height = max(h, 80)
                }
            }
        }
    }
}
#endif

private func htmlWrap(_ body: String) -> String {
    """
    <!doctype html>
    <html>
    <head>
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <meta name="color-scheme" content="light dark">
    <style>
    :root { color-scheme: light dark; }
    html, body {
        margin: 0;
        padding: 4px 2px 16px;
        font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif;
        font-size: 15px;
        line-height: 1.55;
        background: transparent;
        color: #1a1a1a;
        -webkit-text-size-adjust: 100%;
    }
    a { color: #2563eb; text-decoration: none; }
    a:hover { text-decoration: underline; }
    img, video { max-width: 100%; height: auto; border-radius: 6px; }
    blockquote {
        margin: 12px 0;
        padding: 4px 14px;
        border-left: 3px solid currentColor;
        opacity: 0.65;
    }
    pre, code {
        font-family: ui-monospace, "SF Mono", Menlo, monospace;
        font-size: 13px;
    }
    pre {
        background: rgba(127,127,127,0.12);
        padding: 12px;
        border-radius: 10px;
        overflow-x: auto;
    }
    table { border-collapse: collapse; }
    hr { border: 0; border-top: 1px solid rgba(127,127,127,0.3); }
    @media (prefers-color-scheme: dark) {
        body { color: #f5f5f7; }
        a { color: #6ab0ff; }
    }
    </style>
    </head>
    <body>\(body)</body>
    </html>
    """
}
#else
struct HTMLBody: View {
    let html: String
    var body: some View {
        Text(html).foregroundStyle(.secondary)
    }
}
#endif
