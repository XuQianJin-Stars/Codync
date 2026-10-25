import CodyncKit
import SwiftUI

extension MarketplaceView {
    var extensionRail: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(ExtensionSection.allCases) { item in
                let on = section == item
                Button {
                    withAnimation(Motion.morph) { section = item }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: item.icon)
                        Text(L10n.tr(item.title))
                    }
                    .font(.subheadline.weight(on ? .semibold : .regular))
                    .foregroundStyle(on ? Palette.text : Palette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(on ? Palette.bubbleAgent : Color.clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(PressScale())
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .frame(width: 168)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(L10n.tr("Extensions sections"))
    }

    @ViewBuilder var tabBody: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch section {
            case .agents: agentsTab
            case .connectors: connectorsTab
            case .skills: skillsTab
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .id(section)
        .transition(.opacity)
    }

    @ViewBuilder private var agentsTab: some View {
        if model.hello?.backends.contains(where: \.needsNode) == true {
            NodeNotice { addingNode = true }
        }
        if agents.isEmpty {
            Text(query.isEmpty ? L10n.tr("No agents yet.") : L10n.tr("No agents match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                .foregroundStyle(Palette.secondary)
        } else {
            flanked(count: agents.count, page: agentPage) { index in
                let shown = Array(CatalogPaging.slice(agents, page: index, size: pageSize))
                LazyVGrid(columns: agentColumns, spacing: 10) {
                    ForEach(shown) { b in agentCard(b) }
                    ForEach(0..<pagePad(count: agents.count, shown: shown.count), id: \.self) { _ in AgentCardSpacer() }
                }
            }
        }
        CatalogPager(page: agentPage, count: agents.count, pageSize: pageSize, go: { agentPage = CatalogPaging.clamp(page: $0, count: agents.count, size: pageSize) }, onSize: changePageSize)
    }

    @ViewBuilder private var connectorsTab: some View {
        MarketRow(title: "Custom connector", subtitle: "Any MCP server: a command or a URL", added: false, addLabel: "New") {
            TileIcon(systemName: "point.3.connected.trianglepath.dotted")
        } add: {
            addingConnector = true
        }
        if loadingConnectors || (loadingMore && CatalogPaging.needsFetch(count: connectors.items.count, page: connectorPage, size: pageSize)) {
            SkeletonGrid()
        } else if connectors.isEmpty {
            Text(query.isEmpty ? L10n.tr("Couldn't load connectors.") : L10n.tr("No connectors match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                .foregroundStyle(Palette.secondary)
        } else {
            flanked(count: connectors.items.count, page: connectorPage, hasMore: connectorCursor != nil, loading: loadingMore) { index in
                let shown = Array(CatalogPaging.slice(connectors.items, page: index, size: pageSize))
                ItemGrid {
                    ForEach(shown) { c in
                        MarketRow(title: c.title, subtitle: c.description ?? c.name, added: c.installed) {
                            ServiceLogo(website: c.website, name: c.title, registryName: c.name)
                        } add: {
                            installing = c
                        }
                    }
                    ForEach(0..<pagePad(count: connectors.items.count, shown: shown.count), id: \.self) { _ in MarketRowSpacer() }
                }
            }
        }
        if !loadingConnectors {
            CatalogPager(page: connectorPage, count: connectors.items.count, pageSize: pageSize, hasMore: connectorCursor != nil, loading: loadingMore, go: { page in
                Task { await showConnectorPage(page) }
            }, onSize: changePageSize)
        }
    }

    @ViewBuilder private var skillsTab: some View {
        MarketRow(title: "Your own skill", subtitle: "Write instructions a bot can follow", added: false, addLabel: "New") {
            TileIcon(systemName: "square.and.pencil")
        } add: {
            writingSkill = true
        }
        if loadingSkills {
            SkeletonGrid()
        } else if shownSkills.isEmpty {
            if !query.isEmpty {
                Text(L10n.tr("No skills match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                    .foregroundStyle(Palette.secondary)
            }
        } else {
            flanked(count: shownSkills.count, page: skillPage) { index in
                let shown = Array(CatalogPaging.slice(shownSkills, page: index, size: pageSize))
                ItemGrid {
                    ForEach(shown) { s in
                        let added = s.installed || model.installedSkills.contains { $0.id == s.source.lowercased() }
                        MarketRow(title: s.name, subtitle: s.description, added: added, busy: busySkill == s.source,
                                  open: { viewingSkill = SkillRef(s, installed: added) }) {
                            SkillGlyph()
                        } add: {
                            busySkill = s.source
                            Task {
                                do { try await model.installSkill(source: s.source) } catch { report(error) }
                                busySkill = nil
                            }
                        }
                    }
                    ForEach(0..<pagePad(count: shownSkills.count, shown: shown.count), id: \.self) { _ in MarketRowSpacer() }
                }
            }
        }
        if !loadingSkills {
            CatalogPager(page: skillPage, count: shownSkills.count, pageSize: pageSize, go: { skillPage = CatalogPaging.clamp(page: $0, count: shownSkills.count, size: pageSize) }, onSize: changePageSize)
        }
    }

    /// How many invisible rows a short last page needs so the bar under the list does not jump.
    private func pagePad(count: Int, shown: Int) -> Int {
        count > pageSize ? max(0, pageSize - shown) : 0
    }

    /// Previous and next on either side of the list.
    @ViewBuilder private func flanked<Content: View>(count: Int, page: Int, hasMore: Bool = false, loading: Bool = false, @ViewBuilder content: @escaping (Int) -> Content) -> some View {
        let pages = CatalogPaging.pageCount(count, size: pageSize, hasMore: hasMore)
        let current = CatalogPaging.clamp(page: page, count: count, size: pageSize, hasMore: hasMore)
        let canNext = CatalogPaging.canForward(count: count, page: current, hasMore: hasMore, size: pageSize) && !loading
        let edges = sizeClass == .compact && pages > 1
        Group {
            if sizeClass == .compact {
                PageDeck(shift: pageShift, page: current, pageCount: pages, content: content)
            } else {
                content(current)
            }
        }
        .overlay {
            if edges {
                HStack(spacing: 0) {
                    PageEdgeButton(title: "Previous page", systemImage: "chevron.left", enabled: current > 0 && !loading) { slide(forward: false) }
                    Color.clear.frame(maxWidth: .infinity).allowsHitTesting(false)
                    PageEdgeButton(title: loading ? "Loading…" : "Next page", systemImage: "chevron.right", enabled: canNext) { slide(forward: true) }
                }
                .padding(.horizontal, -40)
            }
        }
    }

    /// Two tiles across on a phone. One flexible column stretches each icon into a full-width card.
    private var agentColumns: [GridItem] {
        sizeClass == .compact
            ? [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)]
            : [GridItem(.adaptive(minimum: 160), spacing: 10)]
    }

}
