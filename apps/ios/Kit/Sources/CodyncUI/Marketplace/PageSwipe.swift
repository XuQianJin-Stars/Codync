import SwiftUI

/// The current page, plus the one sliding in beside it when an arrow is tapped. Sized by the current page; the slide is a transform, clipped to that page.
struct PageDeck<Content: View>: View {
    var shift: CGFloat
    var page: Int
    var pageCount: Int
    @ViewBuilder var content: (Int) -> Content

    var body: some View {
        content(page)
            .offset(x: shift)
            .background {
                GeometryReader { geo in
                    let width = geo.size.width
                    ZStack(alignment: .topLeading) {
                        if shift > 0, page > 0 {
                            content(page - 1).frame(width: width, alignment: .top).offset(x: shift - width)
                        }
                        if shift < 0, page + 1 < pageCount {
                            content(page + 1).frame(width: width, alignment: .top).offset(x: shift + width)
                        }
                    }
                    .preference(key: PageWidthKey.self, value: width)
                }
            }
            .clipped()
    }
}

struct PageWidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
