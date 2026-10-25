import { useEffect, useRef, useState } from 'react'
import type { Backend } from '@shared/models'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { ComputerBadge } from '../ComputerBadge'
import { AgentSheet, AgentTile, NodeNotice } from './AgentSheet'
import { CredentialsView } from './CredentialsView'
import { CustomConnectorSheet, InstallConnectorSheet, NewSkillSheet } from './ConnectorSheets'
import { SetupTerminalView } from './SetupTerminal'
import { SkillSheet, type SkillRef } from './SkillSheet'
import {
  appendPage, clampPage, emptyPage, errorText, market, MARKET_PAGE_SIZE, needsFetch, needsSignIn, pageItems, replacePage,
  type ExtensionSection, type MarketConnector, type MarketSkill,
} from './market-models'
import { AppLogo, ExtensionRail, FormSheet, ItemGrid, ItemRow, MarketRow, MarketSection, PageBar, ScreenHeader, ServiceLogo, SkeletonGrid, SkillGlyph, SlideStack, TileIcon } from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'

const connectorId = (c: MarketConnector) => c.name

/**
 * Extensions for the bots on one computer (the store in context): Agents, Connectors
 * and Skills, one at a time. Each list turns a page; a connector page past the rows already
 * loaded fetches the next registry page. Installing happens on that computer; each bot
 * turns connectors and skills on in its settings. Each computer has its own, opened
 * from its heading in the sidebar.
 */
export function MarketplaceView() {
  const store = useStore()
  const tr = useTr()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  useScreenViewed('marketplace')
  const [search, setSearch] = useState('')
  const [connectors, setConnectors] = useState(emptyPage<MarketConnector>)
  /** The registry page after the ones shown; null at the end. */
  const [connectorCursor, setConnectorCursor] = useState<string | null>(null)
  const [loadingMore, setLoadingMore] = useState(false)
  const [skills, setSkills] = useState<MarketSkill[]>([])
  const [loadingConnectors, setLoadingConnectors] = useState(true)
  const [loadingSkills, setLoadingSkills] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [installing, setInstalling] = useState<MarketConnector | null>(null)
  const [busySkill, setBusySkill] = useState<string | null>(null)
  const [addingConnector, setAddingConnector] = useState(false)
  const [writingSkill, setWritingSkill] = useState(false)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const [agent, setAgent] = useState<Backend | null>(null)
  const [addingNode, setAddingNode] = useState(false)
  const [section, setSection] = useState<ExtensionSection>('agents')
  const [agentPage, setAgentPage] = useState(0)
  const [connectorPage, setConnectorPage] = useState(0)
  const [skillPage, setSkillPage] = useState(0)
  const [pageSize, setPageSize] = useState(MARKET_PAGE_SIZE)
  const [showInstalled, setShowInstalled] = useState(false)
  const [showCredentials, setShowCredentials] = useState(false)
  const request = useRef(0)
  const connectorQuery = useRef('')

  const query = search.trim()
  const matches = (text: string | null | undefined) => (text ?? '').toLowerCase().includes(query.toLowerCase())
  const allAgents = (store.hello?.backends ?? []).filter((b) => b.available || b.installed === true || b.curated === true)
  const agents = query ? allAgents.filter((b) => matches(b.name)) : allAgents
  const shownSkills = query ? skills.filter((s) => matches(s.name) || matches(s.description)) : skills
  const installedCount = plugins.installedConnectors.length + plugins.installedSkills.length

  const loadConnectors = async (q = query) => {
    const id = ++request.current
    connectorQuery.current = q
    setLoadingConnectors(true)
    setLoadingMore(false)
    try {
      const page = await market.marketConnectors(await store.ready(), q)
      if (request.current !== id) return
      setConnectors(replacePage(page.items, connectorId))
      setConnectorCursor(page.next)
      setConnectorPage(0)
      setError(null)
    } catch (e) {
      if (request.current !== id) return
      setConnectors(emptyPage())
      setConnectorCursor(null)
      setError(errorText(e))
    } finally {
      if (request.current === id) setLoadingConnectors(false)
    }
  }

  const changePageSize = (size: number) => {
    setPageSize(size)
    setAgentPage(0)
    setConnectorPage(0)
    setSkillPage(0)
  }

  const showConnectorPage = async (next: number) => {
    if (next < 0 || loadingMore || loadingConnectors) return
    const id = request.current
    let items = connectors
    let cursor = connectorCursor
    if (!needsFetch(items.items.length, next, pageSize)) {
      setConnectorPage(clampPage(next, items.items.length, pageSize, cursor !== null))
      return
    }
    if (!cursor) {
      setConnectorPage(clampPage(next, items.items.length, pageSize, false))
      return
    }
    setLoadingMore(true)
    try {
      while (cursor && needsFetch(items.items.length, next, pageSize)) {
        const page = await market.marketConnectors(await store.ready(), connectorQuery.current, cursor)
        if (request.current !== id) return
        items = appendPage(items, page.items, connectorId)
        cursor = page.next
      }
      if (request.current !== id) return
      setConnectors(items)
      setConnectorCursor(cursor)
      setConnectorPage(clampPage(next, items.items.length, pageSize, cursor !== null))
      setError(null)
    } catch (e) {
      if (request.current === id) setError(errorText(e))
    } finally {
      if (request.current === id) setLoadingMore(false)
    }
  }

  useEffect(() => {
    void plugins.refresh()
    // Picks up CLIs installed or signed in outside Codync.
    void store.refreshBackends()
    void loadConnectors('')
    void (async () => {
      try {
        setSkills(await market.marketSkills(await store.ready()))
      } catch (e) {
        setError(errorText(e))
      }
      setLoadingSkills(false)
    })()
    return () => void request.current++
    // Mount only, like the view's `.task`s.
  }, [])

  const submit = (q: string) => {
    void loadConnectors(q)
  }

  const installSkill = async (s: MarketSkill) => {
    setBusySkill(s.source)
    try {
      await plugins.installSkill(s.source)
    } catch (e) {
      setError(errorText(e))
    }
    setBusySkill(null)
  }

  const header = (
    <div className="market-heading">
      {/* Everything here lives on one computer; say which. */}
      <HStack gap={10} shrink>
        <ComputerBadge computer={store.computer} size={32} />
        <VStack shrink className="market-heading-titles">
          <Text size="title" weight="semibold" tone="text">{tr(`Extensions`)}</Text>
          <Text size="subheadline" tone="secondary" truncate>{store.hostName}</Text>
        </VStack>
      </HStack>
      <Spacer />
      <Button kind="secondary" onClick={() => setShowCredentials(true)}><Icon name="lock.shield" size={12} />{tr('Credentials')}</Button>
      {installedCount > 0 ? (
        <button className="press market-installed" onClick={() => setShowInstalled(true)}>
          <HStack as="span" align="stretch" className="market-installed-logos">
            {plugins.installedConnectors.slice(0, 3).map((c) => (
              <HStack as="span" align="stretch" key={c.id}>
                <ServiceLogo name={c.name} registryName={c.registryName} size={26} />
              </HStack>
            ))}
          </HStack>
          <VStack as="span" align="flex-start" className="market-installed-text">
            <Text tone="secondary">{tr('{count} installed').replaceAll('{count}', String(installedCount))}</Text>
            <Text size="caption" tone="secondary">{tr(`Connectors & skills`)}</Text>
          </VStack>
          <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
        </button>
      ) : null}
    </div>
  )

  const searchField = (
    <div className="market-search">
      <Icon name="magnifyingglass" size={13} color="var(--secondary)" />
      <input
        value={search}
        placeholder={tr('Search agents, connectors and skills')}
        aria-label={tr('Search agents, connectors and skills')}
        spellCheck={false}
        onChange={(e) => {
          setSearch(e.target.value)
          setAgentPage(0)
          setSkillPage(0)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) submit(search.trim())
        }}
      />
      {search ? (
        <button
          aria-label={tr('Clear search')}
          title={tr('Clear search')}
          className="market-search-clear"
          onClick={() => {
            setSearch('')
            setAgentPage(0)
            setSkillPage(0)
            submit('')
          }}
        >
          <Icon name="xmark.circle.fill" size={13} />
        </button>
      ) : null}
    </div>
  )

  const skillRows = pageItems(shownSkills, skillPage, pageSize)
  const agentRows = pageItems(agents, agentPage, pageSize)
  const connectorRows = pageItems(connectors.items, connectorPage, pageSize)
  const connectorPending = needsFetch(connectors.items.length, connectorPage, pageSize)

  const page = (
    <div className="market-page">
      <div className="market-frame">
        {header}
        {searchField}
        <div className="market-split">
          <ExtensionRail selection={section} onChange={setSection} />
          <div className="market-panel">
            <Text size="footnote" tone="tertiary">
              {tr(section === 'agents'
                ? 'Agents come from the ACP registry and install on {name}.'
                : section === 'connectors'
                  ? 'Connectors come from the official MCP Registry and install on {name}.'
                  : 'Skills come from Anthropic and install on {name}.').replaceAll('{name}', store.hostName)}
            </Text>
            {section === 'agents' && allAgents.some((b) => b.node === 'missing') ? <NodeNotice onAdd={() => setAddingNode(true)} /> : null}
            {section === 'connectors' ? (
              <MarketRow
                title="Custom connector"
                subtitle="Any MCP server: a command or a URL"
                added={false}
                addLabel="New"
                icon={<TileIcon name="point.3.connected.trianglepath.dotted" />}
                onAdd={() => setAddingConnector(true)}
              />
            ) : null}
            {section === 'skills' ? (
              <MarketRow
                title="Your own skill"
                subtitle="Write instructions a bot can follow"
                added={false}
                addLabel="New"
                icon={<TileIcon name="square.and.pencil" />}
                onAdd={() => setWritingSkill(true)}
              />
            ) : null}
            <div key={section} className="market-panel-scroll fade-in">
              {section === 'agents' ? (
                agents.length === 0 ? (
                  <Text tone="secondary">{query ? tr('No agents match “{query}”.').replaceAll('{query}', query) : tr('No agents yet.')}</Text>
                ) : (
                  <div className="agent-grid">
                    {agentRows.map((b) => <AgentCard key={b.id} backend={b} onClick={() => setAgent(b)} />)}
                    <PageSpacers count={pagePad(agents.length, agentRows.length, pageSize)} kind="agent" />
                  </div>
                )
              ) : null}
              {section === 'agents' ? (
                <PageBar page={agentPage} count={agents.length} pageSize={pageSize} onPage={(next) => setAgentPage(clampPage(next, agents.length, pageSize))} onPageSize={changePageSize} />
              ) : null}
              {section === 'connectors' ? (
                loadingConnectors || connectorPending ? (
                  <SkeletonGrid />
                ) : connectors.items.length === 0 ? (
                  <Text tone="secondary">{query ? tr('No connectors match “{query}”.').replaceAll('{query}', query) : tr(`Couldn't load connectors.`)}</Text>
                ) : (
                  <ItemGrid>
                    {connectorRows.map((c) => (
                      <MarketRow
                        key={c.name}
                        title={c.title}
                        subtitle={c.description ?? c.name}
                        added={c.installed}
                        icon={<ServiceLogo website={c.website} name={c.title} registryName={c.name} />}
                        onAdd={() => setInstalling(c)}
                      />
                    ))}
                    <PageSpacers count={pagePad(connectors.items.length, connectorRows.length, pageSize)} kind="row" />
                  </ItemGrid>
                )
              ) : null}
              {section === 'connectors' && !loadingConnectors ? (
                <PageBar page={connectorPage} count={connectors.items.length} pageSize={pageSize} hasMore={connectorCursor !== null} loading={loadingMore} onPage={(next) => void showConnectorPage(next)} onPageSize={changePageSize} />
              ) : null}
              {section === 'skills' ? (
                loadingSkills ? (
                  <SkeletonGrid />
                ) : shownSkills.length === 0 ? (
                  query ? <Text tone="secondary">{tr('No skills match “{query}”.').replaceAll('{query}', query)}</Text> : null
                ) : (
                  <ItemGrid>
                    {skillRows.map((s) => (
                      <MarketRow
                        key={s.source}
                        title={s.name}
                        subtitle={s.description}
                        added={s.installed || plugins.installedSkills.some((i) => i.id === s.source.toLowerCase())}
                        busy={busySkill === s.source}
                        icon={<SkillGlyph />}
                        onAdd={() => void installSkill(s)}
                        onOpen={() => setViewingSkill(s)}
                      />
                    ))}
                    <PageSpacers count={pagePad(shownSkills.length, skillRows.length, pageSize)} kind="row" />
                  </ItemGrid>
                )
              ) : null}
              {section === 'skills' && !loadingSkills ? (
                <PageBar page={skillPage} count={shownSkills.length} pageSize={pageSize} onPage={(next) => setSkillPage(clampPage(next, shownSkills.length, pageSize))} onPageSize={changePageSize} />
              ) : null}
            </div>
            {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
          </div>
        </div>
      </div>
      {/* Dismissing slides the sheet away; clearing the caller's state would cut it off. */}
      <div className="market-close">
        <IconButton title="Close" icon="xmark" onClick={dismiss} />
      </div>
    </div>
  )

  return (
    <>
      <SlideStack base={page} top={showCredentials ? <CredentialsView onBack={() => setShowCredentials(false)} /> : showInstalled ? <InstalledView back={() => setShowInstalled(false)} /> : null} />
      <FormSheet open={installing !== null} onClose={() => setInstalling(null)}>
        {installing ? <InstallConnectorSheet key={installing.name} item={installing} done={() => void loadConnectors(connectorQuery.current)} /> : null}
      </FormSheet>
      <FormSheet open={addingConnector} onClose={() => setAddingConnector(false)}>
        <CustomConnectorSheet />
      </FormSheet>
      <FormSheet open={writingSkill} onClose={() => setWritingSkill(false)}>
        <NewSkillSheet />
      </FormSheet>
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
      <FormSheet open={addingNode} onClose={() => setAddingNode(false)} width={560} height={560} fixed>
        {addingNode ? <SetupTerminalView backend={null} step="node" back={() => setAddingNode(false)} /> : null}
      </FormSheet>
      <FormSheet open={agent !== null} onClose={() => setAgent(null)} width={560} height={560} fixed>
        {agent ? <AgentSheet key={agent.id} initial={agent} /> : null}
      </FormSheet>
    </>
  )
}

function pagePad(count: number, shown: number, size: number) {
  return count > size ? Math.max(0, size - shown) : 0
}

function PageSpacers({ count, kind }: { count: number; kind: 'agent' | 'row' }) {
  if (count <= 0) return null
  return (
    <>
      {Array.from({ length: count }, (_, i) => kind === 'agent' ? (
        <div key={i} className="agent-card page-spacer" aria-hidden="true">
          <AgentTile registry={null} icon={24} tile={40} radius={11} />
          <VStack as="span" align="flex-start" gap={3}>
            <Text size="subheadline" weight="semibold" className="agent-card-line">&nbsp;</Text>
            <Text size="caption" className="agent-card-line">&nbsp;</Text>
          </VStack>
        </div>
      ) : (
        <div key={i} className="market-row page-spacer" aria-hidden="true">
          <div className="market-row-icon" />
          <VStack gap={2} grow>
            <Text size="body" weight="medium">&nbsp;</Text>
            <Text size="subheadline">&nbsp;</Text>
          </VStack>
        </div>
      ))}
    </>
  )
}

function AgentCard({ backend, onClick }: { backend: Backend; onClick: () => void }) {
  const status = backend.installed !== true && backend.curated === true ? 'Not installed'
    : backend.node === 'missing' ? 'Needs Node.js'
    : backend.installed !== true ? 'Sets up on first use'
    : backend.signedIn === false ? 'Not signed in'
    : 'Installed'
  return (
    <button className="agent-card" title={backend.name} onClick={onClick}>
      <AgentTile registry={backend.registry ?? backend.id} icon={24} tile={40} radius={11} />
      <VStack as="span" align="flex-start" gap={3} shrink>
        <Text size="subheadline" weight="semibold" tone="text" truncate className="agent-card-line">{backend.name}</Text>
        <Text size="caption" tone="secondary" truncate className="agent-card-line">{status}</Text>
      </VStack>
    </button>
  )
}

// MARK: - Installed

function InstalledView({ back }: { back: () => void }) {
  const store = useStore()
  const tr = useTr()
  const plugins = usePlugins(store)
  const [removing, setRemoving] = useState<{ id: string; name: string; isSkill: boolean } | null>(null)
  const [signingIn, setSigningIn] = useState<string | null>(null)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const empty = plugins.installedConnectors.length === 0 && plugins.installedSkills.length === 0

  const signIn = async (id: string) => {
    setSigningIn(id)
    try {
      await plugins.signIn(id)
    } catch (e) {
      store.setError(errorText(e))
    }
    setSigningIn(null)
  }

  const remove = async (r: { id: string; isSkill: boolean }) => {
    try {
      if (r.isSkill) await plugins.removeSkill(r.id)
      else await plugins.removeConnector(r.id)
    } catch (e) {
      store.setError(errorText(e))
    }
  }

  return (
    <SheetPage>
      <ScreenHeader title="Connectors & skills" onBack={back} />
      <div className="market-installed-body">
        <div className="market-scroll">
          <div className="market-page-content">
            {empty ? <EmptyState icon="shippingbox" title="Nothing installed" detail="Connectors and skills you add show up here." /> : null}
            {plugins.installedConnectors.length ? (
              <MarketSection title="Connectors">
                <ItemGrid>
                  {plugins.installedConnectors.map((c) => (
                    <ItemRow
                      key={c.id}
                      title={c.name}
                      subtitle={needsSignIn(c) ? 'Sign in so bots can use it' : (c.command ?? c.url ?? c.description)}
                      icon={c.kind === 'composio' ? <AppLogo url={c.logo} name={c.name} /> : <ServiceLogo name={c.name} registryName={c.registryName} />}
                      trailing={(
                        <>
                          {signingIn === c.id ? <Spinner /> : needsSignIn(c) ? <Button onClick={() => void signIn(c.id)}>Sign in</Button> : null}
                          <RemoveButton title={c.name} onClick={() => setRemoving({ id: c.id, name: c.name, isSkill: false })} />
                        </>
                      )}
                    />
                  ))}
                </ItemGrid>
              </MarketSection>
            ) : null}
            {plugins.installedSkills.length ? (
              <MarketSection title="Skills">
                <ItemGrid>
                  {plugins.installedSkills.map((s) => (
                    <ItemRow
                      key={s.id}
                      title={s.name}
                      subtitle={s.description}
                      icon={<SkillGlyph />}
                      onOpen={() => setViewingSkill({ id: s.id, name: s.name, description: s.description })}
                      trailing={<RemoveButton title={s.name} onClick={() => setRemoving({ id: s.id, name: s.name, isSkill: true })} />}
                    />
                  ))}
                </ItemGrid>
              </MarketSection>
            ) : null}
          </div>
        </div>
      </div>
      <Dialog
        open={removing !== null}
        title={tr('Remove {name}?').replaceAll('{name}', removing?.name ?? '')}
        message={removing ? tr(removing.isSkill ? 'Bots stop using this skill.' : 'Bots lose this connector, and the keys saved for it are deleted.') : null}
        actions={removing ? [{ title: 'Remove', destructive: true, action: () => void remove(removing) }] : []}
        onClose={() => setRemoving(null)}
      />
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
    </SheetPage>
  )
}

/** A quiet "nothing here" message (replaces `ContentUnavailableView`). */
function EmptyState({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  const tr = useTr()
  return (
    <VStack align="center" gap={8} className="market-empty">
      <Icon name={icon} size={28} color="var(--tertiary)" />
      <Text size="compactBody" weight="semibold" tone="text">{tr(title)}</Text>
      <Text size="compactSecondary" tone="secondary">{tr(detail)}</Text>
    </VStack>
  )
}

function RemoveButton({ title, onClick }: { title: string; onClick: () => void }) {
  return (
    <button
      title="Remove"
      aria-label={`Remove ${title}`}
      className="market-remove"
      onClick={(e) => {
        e.stopPropagation()
        onClick()
      }}
    >
      <Icon name="trash" size={13} />
    </button>
  )
}
