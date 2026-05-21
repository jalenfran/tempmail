import SwiftUI

enum Theme {
    static let readingMaxWidth: CGFloat = 720
    static let cornerRadius: CGFloat = 18
    static let buttonCorner: CGFloat = 14

    static let dimText: Color = .secondary
}

extension View {
    func cardPadding() -> some View {
        self.padding(.horizontal, 16).padding(.vertical, 12)
    }
}
