import CodyncKit
import SwiftUI

/// Extensions for the bots on one computer (the store in the environment): Agents,
/// Connectors and Skills, one at a time. Each list turns a page; a connector page past
/// the rows already loaded fetches the next registry page. Installing happens on that
/// computer; each bot turns connectors and skills on in its settings.
public struct MarketplaceView: View {
    @Environment(BotStore.self) var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State var search = ""
    @State var connectors = MarketplacePage<MarketConnector>()
    @State var connectorRequest = UUID()
    @State var connectorQuery = ""
    /// The registry page after the ones shown; nil at the end.
    @State var connectorCursor: String?
    @State var loadingMore = false
    @State var skills: [MarketSkill] = []
    @State var loadingConnectors = true
    @State var loadingSkills = true
    @State var error: String?
    @State var installing: MarketConnector?
    @State var busySkill: String?
    @State var addingConnector = false
    @State var writingSkill = false
    @State var viewingSkill: SkillRef?
    @State var agent: Backend?
    @State var addingNode = false
    @State var section = ExtensionSection.agents
    @State var agentPage = 0
    /// How far the list is dragged, in points. Zero when it is sitting on a page.
    @State var pageShift: CGFloat = 0
    @State var listWidth: CGFloat = 0
    /// Bumps when a new drag starts, so a settle already in flight does not also turn the page.
    @State var slideToken = 0
    @State var connectorPage = 0
    @State var skillPage = 0
    /// Nil until the menu is used, so a phone stays at 5 and a wide layout stays at 10.
    @State var chosenPageSize: Int?
    @State var showInstalled = false
    @State var showCredentials = false
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.horizontalSizeClass) var sizeClass

    /// Each computer has its own marketplace (the `BotStore` in the environment).
    public init() {}

    var query: String { search.trimmingCharacters(in: .whitespaces) }

    var agents: [Backend] {
        let all = (model.hello?.backends ?? []).filter { $0.available || $0.installed == true || $0.curated == true }
        return query.isEmpty ? all : all.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    var shownSkills: [MarketSkill] {
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

    func agentCard(_ b: Backend) -> some View {
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
    func report(_ error: Error) {
        if error is CancellationError || Task.isCancelled { return }
        self.error = error.localizedDescription
    }

    var pageSize: Int {
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
    func slide(forward: Bool) {
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
            Task { await showConnectorPage(next) }
        }
    }

    private func move(_ page: inout Int, count: Int, hasMore: Bool, forward: Bool) {
        let next = page + (forward ? 1 : -1)
        guard next >= 0, next < CatalogPaging.pageCount(count, size: pageSize, hasMore: hasMore) else { return }
        page = next
    }

    func changePageSize(_ size: Int) {
        chosenPageSize = size
        agentPage = 0
        connectorPage = 0
        skillPage = 0
    }

    func showConnectorPage(_ next: Int) async {
        guard next >= 0, !loadingMore, !loadingConnectors else { return }
        if !CatalogPaging.needsFetch(count: connectors.items.count, page: next, size: pageSize) {
            connectorPage = CatalogPaging.clamp(page: next, count: connectors.items.count, size: pageSize, hasMore: connectorCursor != nil)
            return
        }
        guard connectorCursor != nil else { return }
        let request = connectorRequest
        loadingMore = true
        defer { if connectorRequest == request { loadingMore = false } }
        var catalog = ConnectorCatalog(items: connectors, cursor: connectorCursor)
        do {
            try await catalog.fill(page: next, size: pageSize) { cursor in
                let page = try await model.marketConnectors(search: connectorQuery, cursor: cursor)
                guard !Task.isCancelled, connectorRequest == request else { throw CancellationError() }
                return (page.items, page.next)
            }
            guard !Task.isCancelled, connectorRequest == request else { return }
            connectors = catalog.items
            connectorCursor = catalog.cursor
            connectorPage = CatalogPaging.clamp(page: next, count: connectors.items.count, size: pageSize, hasMore: connectorCursor != nil)
            error = nil
        } catch is CatalogStall {
            guard connectorRequest == request else { return }
            error = "Couldn't load more connectors."
        } catch {
            guard connectorRequest == request else { return }
            report(error)
        }
    }

}

enum ExtensionSection: String, CaseIterable, Identifiable {
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
