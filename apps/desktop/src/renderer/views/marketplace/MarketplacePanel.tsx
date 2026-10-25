import type { ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { AgentTile, NodeNotice } from './AgentSheet'
import { clampPage, type ExtensionSection, type MarketConnector, type MarketSkill, type MarketplacePage } from './market-models'
import { ItemGrid, MarketRow, PageBar, ServiceLogo, SkeletonGrid, SkillGlyph, TileIcon } from './pieces'

/** One tab of Extensions: its source line, the page of cards, and the pager under them. */
export function MarketPanel({
  section, hostName, needsNode, query,
  agents, agentRows, agentPage,
  connectors, connectorRows, connectorPage, connectorCursor, connectorPending,
  shownSkills, skillRows, skillPage, installedSkillIds, busySkill,
  loadingConnectors, loadingMore, loadingSkills, pageSize, error,
  onAddNode, onAddConnector, onWriteSkill, onAgent, onInstall, onInstallSkill, onViewSkill,
  onAgentPage, onConnectorPage, onSkillPage, onPageSize,
}: {
  section: ExtensionSection
  hostName: string
  needsNode: boolean
  query: string
  agents: Backend[]
  agentRows: Backend[]
  agentPage: number
  connectors: MarketplacePage<MarketConnector>
  connectorRows: MarketConnector[]
  connectorPage: number
  connectorCursor: string | null
  connectorPending: boolean
  shownSkills: MarketSkill[]
  skillRows: MarketSkill[]
  skillPage: number
  installedSkillIds: string[]
  busySkill: string | null
  loadingConnectors: boolean
  loadingMore: boolean
  loadingSkills: boolean
  pageSize: number
  error: string | null
  onAddNode: () => void
  onAddConnector: () => void
  onWriteSkill: () => void
  onAgent: (backend: Backend) => void
  onInstall: (connector: MarketConnector) => void
  onInstallSkill: (skill: MarketSkill) => void
  onViewSkill: (skill: MarketSkill) => void
  onAgentPage: (page: number) => void
  onConnectorPage: (page: number) => void
  onSkillPage: (page: number) => void
  onPageSize: (size: number) => void
}) {
  const tr = useTr()
  const source = section === 'agents'
    ? 'Agents come from the ACP registry and install on {name}.'
    : section === 'connectors'
      ? 'Connectors come from the official MCP Registry and install on {name}.'
      : 'Skills come from Anthropic and install on {name}.'
  return (
    <div className="market-panel">
      <Text size="footnote" tone="tertiary">{tr(source).replaceAll('{name}', hostName)}</Text>
      {section === 'agents' && needsNode ? <NodeNotice onAdd={onAddNode} /> : null}
      {section === 'connectors' ? (
        <MarketRow
          title="Custom connector"
          subtitle="Any MCP server: a command or a URL"
          added={false}
          addLabel="New"
          icon={<TileIcon name="point.3.connected.trianglepath.dotted" />}
          onAdd={onAddConnector}
        />
      ) : null}
      {section === 'skills' ? (
        <MarketRow
          title="Your own skill"
          subtitle="Write instructions a bot can follow"
          added={false}
          addLabel="New"
          icon={<TileIcon name="square.and.pencil" />}
          onAdd={onWriteSkill}
        />
      ) : null}
      <div key={section} className="market-panel-scroll fade-in">
        {section === 'agents' ? (
          agents.length === 0 ? (
            <Text tone="secondary">{query ? tr('No agents match “{query}”.').replaceAll('{query}', query) : tr('No agents yet.')}</Text>
          ) : (
            <div className="agent-grid">
              {agentRows.map((b) => <AgentCard key={b.id} backend={b} onClick={() => onAgent(b)} />)}
              <PageSpacers count={pagePad(agents.length, agentRows.length, pageSize)} kind="agent" />
            </div>
          )
        ) : null}
        {section === 'agents' ? (
          <PageBar page={agentPage} count={agents.length} pageSize={pageSize} onPage={onAgentPage} onPageSize={onPageSize} />
        ) : null}
        {section === 'connectors' ? (
          loadingConnectors || (loadingMore && connectorPending) ? (
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
                  onAdd={() => onInstall(c)}
                />
              ))}
              <PageSpacers count={pagePad(connectors.items.length, connectorRows.length, pageSize)} kind="row" />
            </ItemGrid>
          )
        ) : null}
        {section === 'connectors' && !loadingConnectors ? (
          <PageBar page={connectorPage} count={connectors.items.length} pageSize={pageSize} hasMore={connectorCursor !== null} loading={loadingMore} onPage={onConnectorPage} onPageSize={onPageSize} />
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
                  added={s.installed || installedSkillIds.includes(s.source.toLowerCase())}
                  busy={busySkill === s.source}
                  icon={<SkillGlyph />}
                  onAdd={() => onInstallSkill(s)}
                  onOpen={() => onViewSkill(s)}
                />
              ))}
              <PageSpacers count={pagePad(shownSkills.length, skillRows.length, pageSize)} kind="row" />
            </ItemGrid>
          )
        ) : null}
        {section === 'skills' && !loadingSkills ? (
          <PageBar page={skillPage} count={shownSkills.length} pageSize={pageSize} onPage={(next) => onSkillPage(clampPage(next, shownSkills.length, pageSize))} onPageSize={onPageSize} />
        ) : null}
      </div>
      {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
    </div>
  )
}

function pagePad(count: number, shown: number, size: number) {
  return count > size ? Math.max(0, size - shown) : 0
}

function PageSpacers({ count, kind }: { count: number; kind: 'agent' | 'row' }) {
  if (count <= 0) return null
  const cells: ReactNode[] = []
  for (let i = 0; i < count; i++) {
    cells.push(kind === 'agent' ? (
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
    ))
  }
  return <>{cells}</>
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
