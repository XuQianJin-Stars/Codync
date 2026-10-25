/// Keep fetched catalog metadata in memory. The Extensions tabs page through it.
struct MarketplacePage<Item: Identifiable> {
    private(set) var items: [Item] = []

    var isEmpty: Bool { items.isEmpty }

    mutating func replace(with items: [Item]) {
        self.items = []
        append(items)
    }

    mutating func append(_ page: [Item]) {
        var known = Set(items.map(\.id))
        items += page.filter { known.insert($0.id).inserted }
    }
}

/// How many catalog rows one Extensions page shows, and which page a click lands on.
enum CatalogPaging {
    static let pageSize = 10
    /// Two agent tiles across, three rows: a page has no leftover card.
    static let phonePageSize = 6
    static let pageSizes = [6, 10, 20, 50]

    /// Loaded rows split into pages. While the registry still has a cursor, one more page is offered so the bar is never 1 / 1 for a list that continues.
    static func pageCount(_ count: Int, size: Int = pageSize, hasMore: Bool = false) -> Int {
        let loaded = count <= 0 ? 1 : (count + size - 1) / size
        return hasMore ? loaded + 1 : loaded
    }

    static func clamp(page: Int, count: Int, size: Int = pageSize, hasMore: Bool = false) -> Int {
        min(max(0, page), pageCount(count, size: size, hasMore: hasMore) - 1)
    }

    static func slice<T>(_ items: [T], page: Int, size: Int = pageSize) -> ArraySlice<T> {
        let start = max(0, page) * size
        return items.dropFirst(start).prefix(size)
    }

    /// Next stays available through the last shown page, including the page that still has to be fetched.
    static func canForward(count: Int, page: Int, hasMore: Bool, size: Int = pageSize) -> Bool {
        page < pageCount(count, size: size, hasMore: hasMore) - 1
    }

    /// The page starts past the rows already fetched, so the registry has to be asked.
    static func needsFetch(count: Int, page: Int, size: Int = pageSize) -> Bool {
        page * size >= count
    }
}
