import Testing
@testable import CodyncUI

private struct CatalogItem: Identifiable { let id: Int }

@Test func catalogPagesDeduplicate() {
    var page = MarketplacePage<CatalogItem>()
    page.replace(with: (0..<12).map { CatalogItem(id: $0) })
    page.append((10..<15).map { CatalogItem(id: $0) })
    #expect(page.items.map(\.id) == Array(0..<15))
    page.replace(with: [])
    #expect(page.isEmpty)
}

@Test func catalogPagingClampsAndFetchesOnlyPastLoadedRows() {
    let items = Array(0..<47)
    #expect(CatalogPaging.pageCount(47) == 5)
    #expect(CatalogPaging.pageCount(19) == 2)
    #expect(CatalogPaging.pageCount(0) == 1)
    #expect(CatalogPaging.pageCount(72, hasMore: true) == 9)
    #expect(Array(CatalogPaging.slice(items, page: 0)) == Array(0..<10))
    #expect(Array(CatalogPaging.slice(items, page: 4)) == Array(40..<47))
    #expect(Array(CatalogPaging.slice(items, page: 9)).isEmpty)
    #expect(CatalogPaging.clamp(page: 9, count: 47) == 4)
    #expect(CatalogPaging.clamp(page: 9, count: 72, hasMore: true) == 8)
    #expect(CatalogPaging.clamp(page: -1, count: 20) == 0)
    #expect(CatalogPaging.clamp(page: 4, count: 0) == 0)
    #expect(CatalogPaging.canForward(count: 10, page: 0, hasMore: true))
    #expect(CatalogPaging.needsFetch(count: 10, page: 1))
    #expect(CatalogPaging.canForward(count: 20, page: 0, hasMore: false))
    #expect(!CatalogPaging.needsFetch(count: 20, page: 1))
    #expect(!CatalogPaging.canForward(count: 8, page: 0, hasMore: false))
    #expect(!CatalogPaging.needsFetch(count: 60, page: 5))
    #expect(CatalogPaging.needsFetch(count: 60, page: 6))
}
