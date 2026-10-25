import CodyncKit
import SwiftUI

/// Extensions for the bots on one computer (the store in the environment): Agents,
/// Connectors and Skills, one at a time. Each list turns a page; a connector page past
/// the rows already loaded fetches the next registry page. Installing happens on that
/// computer; each bot turns connectors and skills on in its settings.
public struct MarketplaceView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var search = ""
    @State private var connectors = MarketplacePage<MarketConnector>()
    @State private var connectorRequest = UUID()
    @State private var connectorQuery = ""
    /// The registry page after the ones shown; nil at the end.
    @State private var connectorCursor: String?
    @State private var loadingMore = false
    @State private var skills: [MarketSkill] = []
    @State private var loadingConnectors = true
    @State private var loadingSkills = true
    @State private var error: String?
    @State private var installing: MarketConnector?
    @State private var busySkill: String?
    @State private var addingConnector = false
    @State private var writingSkill = false
    @State private var viewingSkill: SkillRef?
    @State private var agent: Backend?
    @State private var addingNode = false
    @State private var section = ExtensionSection.agents
    @State private var agentPage = 0
    /// How far the list is dragged, in points. Zero when it is sitting on a page.
    @State private var pageShift: CGFloat = 0
    @State private var listWidth: CGFloat = 0
    /// Bumps when a new drag starts, so a settle already in flight does not also turn the page.
    @State private var slideToken = 0
    @State private var connectorPage = 0
    @State private var skillPage = 0
    /// Nil until the menu is used, so a phone stays at 5 and a wide layout stays at 10.
    @State private var chosenPageSize: Int?
    @State private var showInstalled = false
    @State private var showCredentials = false
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.horizontalSizeClass) private var sizeClass

    /// Each computer has its own marketplace (the `BotStore` in the environment).
    public init() {}

    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    private var agents: [Backend] {
        let all = (model.hello?.backends ?? []).filter { $0.available || $0.installed == true || $0.curated == true }
        return query.isEmpty ? all : all.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    private var shownSkills: [MarketSkill] {
        query.isEmpty ? skills : skills.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.description.localizedCaseInsensitiveContains(query) }
    }

    private var installedCount: Int { model.installedConnectors.count + model.installedSkills.count }

    public var body: some View {
        ZStack {
            if showInstalled {
                InstalledView { withAnimation(Motion.layout) { showInstalled = false } }
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                page.transition(.move(edge: .leading).combined(with: .opacity))
            }
        }
        .background(Palette.background)
        .tracked(.screenViewed(.marketplace))
        .task {
            await model.refreshPlugins()
        }
        // Picks up CLIs installed or signed in outside Codync.
        .task { await model.refreshBackends() }
        .task { await loadConnectors() }
        .task {
            loadingSkills = true
            do { skills = try await model.marketSkills() } catch { report(error) }
            if !Task.isCancelled { loadingSkills = false }
        }
        .codyncSheet(item: $installing) { item in
            InstallConnectorSheet(item: item) { Task { await loadConnectors() } }
        }
        .codyncSheet(isPresented: $showCredentials) { CredentialsView() }
        .codyncSheet(isPresented: $addingConnector) { CustomConnectorSheet() }
        .codyncSheet(item: $viewingSkill) { SkillSheet(skill: $0) }
        .codyncSheet(isPresented: $writingSkill) { NewSkillSheet() }
        .codyncSheet(item: $agent) { b in AgentSheet(initial: b) }
        .codyncSheet(isPresented: $addingNode) { SetupTerminalView(backend: nil, step: .node) { addingNode = false } }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                header
                searchField
                if sizeClass == .compact {
                    VStack(alignment: .leading, spacing: 8) {
                        TabChoice(selection: $section, options: ExtensionSection.allCases.map { (id: $0, label: $0.title) })
                        sourceLine
                    }
                    tabBody
                } else {
                    HStack(alignment: .top, spacing: 20) {
                        extensionRail
                        VStack(alignment: .leading, spacing: 12) {
                            sourceLine
                            tabBody.frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
                if let error {
                    Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .padding(.horizontal, 44)
            .padding(.vertical, 20)
            .frame(maxWidth: 980)
            .frame(maxWidth: .infinity)
            .onPreferenceChange(PageWidthKey.self) { listWidth = $0 }
        }
        .background(Palette.background)
        .overlay(alignment: .topTrailing) {
            // dismissModal slides the sheet away; clearing the binding would cut it off.
            IconButton("Close", systemImage: "xmark") { dismiss() }
                .keyboardShortcut(.cancelAction)
                .padding(8)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Everything here lives on one computer; say which.
            HStack(spacing: 10) {
                ComputerBadge(model.computer, size: 32)
                VStack(alignment: .leading, spacing: 0) {
                    Text(L10n.tr("Extensions"))
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(Palette.text)
                    Text(model.hostName)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                }
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                Spacer(minLength: 8)
                // Beside the title, in the room left of Close, so the list can fill the screen.
                Button { withAnimation(Motion.layout) { showCredentials = true } } label: {
                    Label(L10n.tr("Credentials"), systemImage: "lock.shield")
                }
                .buttonStyle(SecondaryButtonStyle())
                .fixedSize()
                .layoutPriority(1)
            }
            .frame(maxWidth: .infinity)
            if installedCount > 0 {
                Button { withAnimation(Motion.layout) { showInstalled = true } } label: {
                    HStack(spacing: 8) {
                        HStack(spacing: -8) {
                            ForEach(model.installedConnectors.prefix(3)) { c in
                                ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 26)
                            }
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(L10n.tr("{count} installed").replacingOccurrences(of: "{count}", with: String(installedCount)))
                            Text(L10n.tr("Connectors & skills")).font(.caption)
                        }
                        .foregroundStyle(Palette.secondary)
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.top, 28)
    }

    /// One sentence for the open section, tucked under its tab.
    private var sourceLine: some View {
        Text(L10n.tr(section.source).replacingOccurrences(of: "{name}", with: model.hostName))
            .font(.footnote)
            .foregroundStyle(Palette.tertiary)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField(L10n.tr("Search agents, connectors and skills"), text: $search)
                .textFieldStyle(.plain)
                .plainTextInput()
                .onSubmit { Task { await loadConnectors() } }
                .onChange(of: search) { _, _ in agentPage = 0; skillPage = 0; connectorPage = 0 }
            if !search.isEmpty {
                Button { search = ""; Task { await loadConnectors() } } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.tertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(L10n.tr("Clear search"))
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
        .background(Palette.bubbleAgent, in: Capsule())
    }

    private func agentCard(_ b: Backend) -> some View {
        AgentCard(backend: b) { agent = b }
    }

    private func loadConnectors() async {
        let request = UUID()
        connectorRequest = request
        connectorQuery = query
        loadingConnectors = true
        loadingMore = false
        defer { if connectorRequest == request { loadingConnectors = false } }
        do {
            let page = try await model.marketConnectors(search: connectorQuery)
            guard !Task.isCancelled, connectorRequest == request else { return }
            connectors.replace(with: page.items)
            connectorCursor = page.next
            connectorPage = 0
            error = nil
        } catch {
            guard connectorRequest == request else { return }
            if error is CancellationError || Task.isCancelled { return }
            connectors.replace(with: [])
            connectorCursor = nil
            self.error = error.localizedDescription
        }
    }

    /// A load that SwiftUI cancelled is the next load starting, not a failure to show.
    private func report(_ error: Error) {
        if error is CancellationError || Task.isCancelled { return }
        self.error = error.localizedDescription
    }

    private var pageSize: Int {
        chosenPageSize ?? (sizeClass == .compact ? CatalogPaging.phonePageSize : CatalogPaging.pageSize)
    }

    private func canTurn(_ forward: Bool) -> Bool {
        switch section {
        case .agents:
            return inRange(agentPage, forward: forward, count: agents.count, hasMore: false)
        case .skills:
            return inRange(skillPage, forward: forward, count: shownSkills.count, hasMore: false)
        case .connectors:
            return !loadingMore && !loadingConnectors && inRange(connectorPage, forward: forward, count: connectors.items.count, hasMore: connectorCursor != nil)
        }
    }

    private func inRange(_ page: Int, forward: Bool, count: Int, hasMore: Bool) -> Bool {
        let next = page + (forward ? 1 : -1)
        return next >= 0 && next < CatalogPaging.pageCount(count, size: pageSize, hasMore: hasMore)
    }

    /// The arrow buttons slide the list over.
    private func slide(forward: Bool) {
        guard canTurn(forward) else { return }
        guard sizeClass == .compact, listWidth > 1, !reduceMotion else {
            turnPage(forward: forward)
            return
        }
        settle(to: forward ? -listWidth : listWidth, velocity: 0) { turnPage(forward: forward) }
    }

    private func settle(to target: CGFloat, velocity: CGFloat, commit: @escaping () -> Void) {
        slideToken += 1
        let token = slideToken
        let distance = target - pageShift
        let initial = abs(distance) > 30 ? Double(velocity / distance) : 0
        withAnimation(Motion.page(velocity: initial), completionCriteria: .logicallyComplete) {
            pageShift = target
        } completion: {
            guard token == slideToken else { return }
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) {
                commit()
                pageShift = 0
            }
        }
    }

    private func turnPage(forward: Bool) {
        switch section {
        case .agents:
            move(&agentPage, count: agents.count, hasMore: false, forward: forward)
        case .skills:
            move(&skillPage, count: shownSkills.count, hasMore: false, forward: forward)
        case .connectors:
            let next = connectorPage + (forward ? 1 : -1)
            connectorPage = next
            if CatalogPaging.needsFetch(count: connectors.items.count, page: next, size: pageSize) {
                Task { await showConnectorPage(next) }
            }
        }
    }

    private func move(_ page: inout Int, count: Int, hasMore: Bool, forward: Bool) {
        let next = page + (forward ? 1 : -1)
        guard next >= 0, next < CatalogPaging.pageCount(count, size: pageSize, hasMore: hasMore) else { return }
        page = next
    }

    private func changePageSize(_ size: Int) {
        chosenPageSize = size
        agentPage = 0
        connectorPage = 0
        skillPage = 0
    }

    private func showConnectorPage(_ next: Int) async {
        guard next >= 0, !loadingMore, !loadingConnectors else { return }
        if !CatalogPaging.needsFetch(count: connectors.items.count, page: next, size: pageSize) {
            connectorPage = CatalogPaging.clamp(page: next, count: connectors.items.count, size: pageSize, hasMore: connectorCursor != nil)
            return
        }
        guard connectorCursor != nil else { return }
        let request = connectorRequest
        loadingMore = true
        defer { if connectorRequest == request { loadingMore = false } }
        do {
            while let cursor = connectorCursor, CatalogPaging.needsFetch(count: connectors.items.count, page: next, size: pageSize) {
                let page = try await model.marketConnectors(search: connectorQuery, cursor: cursor)
                guard !Task.isCancelled, connectorRequest == request else { return }
                connectors.append(page.items)
                connectorCursor = page.next
            }
            connectorPage = CatalogPaging.clamp(page: next, count: connectors.items.count, size: pageSize, hasMore: connectorCursor != nil)
            error = nil
        } catch {
            guard connectorRequest == request else { return }
            report(error)
        }
    }

    private var extensionRail: some View {
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

    @ViewBuilder private var tabBody: some View {
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
        CatalogPager(page: agentPage, count: agents.count, pageSize: pageSize, go: { agentPage = $0 }, onSize: changePageSize)
    }

    @ViewBuilder private var connectorsTab: some View {
        MarketRow(title: "Custom connector", subtitle: "Any MCP server: a command or a URL", added: false, addLabel: "New") {
            TileIcon(systemName: "point.3.connected.trianglepath.dotted")
        } add: {
            addingConnector = true
        }
        if loadingConnectors || CatalogPaging.needsFetch(count: connectors.items.count, page: connectorPage, size: pageSize) {
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
            CatalogPager(page: skillPage, count: shownSkills.count, pageSize: pageSize, go: { skillPage = $0 }, onSize: changePageSize)
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

private enum ExtensionSection: String, CaseIterable, Identifiable {
    case agents, connectors, skills
    var id: String { rawValue }
    var title: String {
        switch self {
        case .agents: "Agents"
        case .connectors: "Connectors"
        case .skills: "Skills"
        }
    }
    var icon: String {
        switch self {
        case .agents: "terminal"
        case .connectors: "point.3.connected.trianglepath.dotted"
        case .skills: "book.pages"
        }
    }
    var source: String {
        switch self {
        case .agents: "Agents come from the ACP registry and install on {name}."
        case .connectors: "Connectors come from the official MCP Registry and install on {name}."
        case .skills: "Skills come from Anthropic and install on {name}."
        }
    }
}
