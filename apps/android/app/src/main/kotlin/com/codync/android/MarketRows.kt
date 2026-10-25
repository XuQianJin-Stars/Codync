package com.codync.android

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import coil3.compose.AsyncImage
import com.codync.android.core.AgentBackend
import com.codync.android.core.CatalogPaging
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

// The Extensions page's building blocks (iOS `MarketRows`): sections, rows, agent cards and logos.

@Composable internal fun MarketSection(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(tr(title), Modifier.padding(start = 4.dp).semantics { heading() }, style = Type.headline, color = Palette.text)
        content()
    }
}

/** One catalog item: its icon, name and line, then Add (or a check once added). */
@Composable internal fun MarketRow(title: String, subtitle: String, added: Boolean, busy: Boolean = false, addLabel: String = "Add",
    open: (() -> Unit)? = null, icon: @Composable () -> Unit, add: () -> Unit) {
    // The whole row opens the item's details; without it the row does what its button does.
    val rowAction = open ?: if (added || busy) null else add
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).then(if (rowAction != null) Modifier.plainButton(action = rowAction) else Modifier)
        .padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(46.dp), contentAlignment = Alignment.Center) { icon() }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(title), style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(subtitle), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        when {
            busy -> Box(Modifier.width(60.dp), contentAlignment = Alignment.Center) { ThinkingOrb(size = 18.dp, color = Palette.secondary) }
            added -> Box(Modifier.width(60.dp), contentAlignment = Alignment.Center) {
                Symbol("checkmark", size = 16.dp, tint = Palette.secondary, contentDescription = tr("Added"))
            }
            else -> Text(tr(addLabel), Modifier.clip(Capsule()).background(Palette.bubbleAgent).plainButton(action = add)
                .padding(horizontal = 16.dp, vertical = 7.dp), style = Type.subheadline.semibold, color = Palette.text)
        }
    }
}

/** The size of a catalog row, with nothing to tap. A short last page keeps a full page of rows. */
@Composable internal fun MarketRowSpacer() {
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp).clearAndSetSemantics {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(46.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(" ", style = Type.body.medium, maxLines = 1)
            Text(" ", style = Type.subheadline, maxLines = 1)
        }
        Text(" ", Modifier.padding(horizontal = 16.dp, vertical = 7.dp), style = Type.subheadline.semibold, maxLines = 1)
    }
}

/** An agent in the Agents row: its logo on a tile, its name and where it stands. */
@Composable internal fun AgentCard(backend: AgentBackend, modifier: Modifier = Modifier.width(128.dp), action: () -> Unit) {
    val status = when {
        !backend.installed && backend.curated -> "Not installed"
        backend.needsNode -> "Needs Node.js"
        !backend.installed -> "Sets up on first use"
        backend.signedIn == false -> "Not signed in"
        else -> "Installed"
    }
    Column(modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent.copy(alpha = 0.45f)).plainButton(action = action)
        .semantics(mergeDescendants = true) {}.padding(horizontal = 10.dp, vertical = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(64.dp).clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
            AgentIcon(backend.registry ?: backend.id, 34.dp)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(backend.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(status), style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** The size of an agent tile, with nothing to tap. A short last page keeps a full page of rows, so the bar under it stays put. */
@Composable internal fun AgentCardSpacer(modifier: Modifier = Modifier) {
    Column(modifier.padding(horizontal = 10.dp, vertical = 18.dp).clearAndSetSemantics {},
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(64.dp))
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(" ", style = Type.subheadline.semibold, maxLines = 1)
            Text(" ", style = Type.caption, maxLines = 1)
        }
    }
}

/** Previous or next, beside the list on a phone. */
@Composable internal fun PageEdgeButton(title: String, symbol: String, enabled: Boolean, modifier: Modifier = Modifier, action: () -> Unit) {
    val tint = if (enabled) Palette.text else Palette.tertiary
    Box(modifier.size(32.dp).graphicsLayer { alpha = if (enabled) 1f else 0.45f }.clip(CircleShape).background(Palette.bubbleAgent)
        .plainButton(enabled = enabled, action = action), contentAlignment = Alignment.Center) {
        Symbol(symbol, size = 13.dp, tint = tint, contentDescription = tr(title))
    }
}

/** A coding agent's logo by ACP registry id, in its brand color when it has one; a terminal glyph otherwise. */
@Composable internal fun AgentIcon(registry: String?, size: Dp) {
    val tint = AGENT_TINTS[registry]?.let { Color(it) } ?: Palette.text
    val res = agentIconRes(registry)
    if (res != null) Image(painterResource(res), null, Modifier.size(size).clearAndSetSemantics {}, colorFilter = ColorFilter.tint(tint))
    else Symbol("terminal", Modifier.padding(size * 0.1f), size = size * 0.8f, tint = tint)
}

// Registry logos are one color; these carry their brand's (a gradient's middle for Gemini and omp).
private val AGENT_TINTS = mapOf("claude-acp" to 0xFFD97857, "gemini" to 0xFF9178C7, "antigravity-acp" to 0xFF4285F5,
    "mistral-vibe" to 0xFFFA520F, "qwen-code" to 0xFF615CED, "amp-acp" to 0xFFF24F40, "kiro" to 0xFF8F45FF,
    "cortex-code" to 0xFF29B5E8, "omp" to 0xFF9C4DFF)

/** Placeholder rows while a list loads, shaped like the real ones. */
@Composable internal fun SkeletonRows() {
    val dim by rememberInfiniteTransition(label = "Skeleton").animateFloat(1f, 0.5f, infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "Dim")
    val loading = tr("Loading…")
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(loading, style = Type.footnote, color = Palette.secondary)
        Column(Modifier.graphicsLayer { alpha = dim }.clearAndSetSemantics { contentDescription = loading }) {
            repeat(4) {
                Row(Modifier.padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.size(46.dp).clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent))
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(Modifier.size(120.dp, 12.dp).clip(Capsule()).background(Palette.bubbleAgent))
                        Box(Modifier.size(190.dp, 10.dp).clip(Capsule()).background(Palette.bubbleAgent))
                    }
                }
            }
        }
    }
}

/** A service's logo from its website favicon, on a white tile; its initial otherwise. */
@Composable internal fun ServiceLogo(website: String?, name: String, registryName: String? = null, size: Dp = 46.dp, site: String? = null) {
    val domain = site ?: serviceDomain(website, name, registryName)
    var loaded by remember(domain) { mutableStateOf(false) }
    val shape = RoundedRectangle(size * 0.26f)
    Box(Modifier.size(size).clip(shape).background(if (loaded) Color.White else Palette.bubbleAgent).clearAndSetSemantics {},
        contentAlignment = Alignment.Center) {
        if (!loaded) Text(name.firstOrNull()?.uppercase() ?: "?", style = Type.system(size.value * 0.4f, FontWeight.SemiBold), color = Palette.text)
        if (domain != null) AsyncImage("https://www.google.com/s2/favicons?domain=$domain&sz=128", null, imageLoader = LocalNetworkImages.current,
            modifier = Modifier.fillMaxSize().padding(size * 0.2f), contentScale = ContentScale.Fit, onSuccess = { loaded = true })
    }
}

/** MCP Registry names are reverse-DNS ("com.notion/mcp"): the owner's domain; a website's host otherwise. */
internal fun serviceDomain(website: String?, name: String, registryName: String?): String? {
    website?.let { it.toUri().host }?.takeIf { it != "github.com" }?.let { return it }
    val labels = registryName?.substringBefore('/')?.split('.') ?: return null
    // io.github.<user> is a GitHub account, not the service's own site.
    if (labels.size < 2 || labels[0] == "io" && labels[1] == "github") return if (name.contains("github", ignoreCase = true)) "github.com" else null
    return "${labels[1]}.${labels[0]}"
}

/** An app's logo from Composio, a letter tile until it loads. */
@Composable internal fun AppLogo(url: String?, name: String, size: Dp = 46.dp) {
    Box(Modifier.size(size).clip(RoundedRectangle(size * 0.28f)).background(Palette.bubbleAgent).clearAndSetSemantics {},
        contentAlignment = Alignment.Center) {
        Text(name.take(1).uppercase(), style = Type.system(size.value * 0.4f, FontWeight.SemiBold), color = Palette.secondary)
        if (url != null) AsyncImage(url, null, imageLoader = LocalNetworkImages.current,
            modifier = Modifier.fillMaxSize().background(Palette.bubbleAgent).padding(size * 0.18f), contentScale = ContentScale.Fit)
    }
}

@Composable internal fun TileIcon(systemImage: String) {
    Box(Modifier.fillMaxSize().clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
        Symbol(systemImage, size = 20.dp, tint = Palette.text)
    }
}

@Composable internal fun SkillGlyph() = TileIcon("book.pages")

/** Text that opens a web page (iOS `WebLink`). */
@Composable internal fun WebLink(title: String, url: String, style: androidx.compose.ui.text.TextStyle = Type.body) {
    val uri = LocalUriHandler.current
    var failed by remember { mutableStateOf(false) }
    val label = tr(title)
    Text(if (failed) "$label (couldn't open the browser)" else label,
        Modifier.plainButton { failed = runCatching { uri.openUri(url) }.isFailure }, style = style.copy(textDecoration = TextDecoration.Underline),
        color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

/** A quiet "nothing here" message. */
@Composable internal fun MarketEmpty(icon: String, title: String, detail: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(icon, size = 30.dp, tint = Palette.tertiary)
        Text(tr(title), style = Type.body.semibold, color = Palette.text)
        Text(tr(detail), style = Type.subheadline, color = Palette.secondary)
    }
}

internal enum class ExtensionSection(val title: String, val icon: String) {
    Agents("Agents", "terminal"),
    Connectors("Connectors", "point.3.connected.trianglepath.dotted"),
    Skills("Skills", "book.pages"),
}

/** Agents, Connectors, Skills: the chosen row is filled, like a side tab. */
@Composable internal fun ExtensionRail(selection: ExtensionSection, select: (ExtensionSection) -> Unit) {
    Column(Modifier.width(168.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (section in ExtensionSection.entries) {
            val on = section == selection
            Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp))
                .background(if (on) Palette.bubbleAgent else Color.Transparent)
                .selectable(selected = on, role = Role.Tab) { select(section) }
                .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Symbol(section.icon, size = 16.dp, tint = if (on) Palette.text else Palette.secondary)
                Text(tr(section.title), style = if (on) Type.subheadline.semibold else Type.subheadline,
                    color = if (on) Palette.text else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** Count, page size, previous, current / total, next, and a jump box. */
@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun CatalogPager(page: Int, count: Int, pageSize: Int, hasMore: Boolean = false, loading: Boolean = false,
    go: (Int) -> Unit, onSize: (Int) -> Unit) {
    val pages = CatalogPaging.pageCount(count, pageSize, hasMore)
    val current = CatalogPaging.clamp(page, count, pageSize, hasMore)
    var jump by remember { mutableStateOf("") }
    val commit = {
        jump.toIntOrNull()?.takeIf { it >= 1 }?.let { go(it - 1) }
        jump = ""
    }
    val total = tr("{count} total").replace("{count}", "$count")
    val canPrev = current > 0 && !loading
    val canNext = CatalogPaging.canForward(count, current, hasMore, pageSize) && !loading
    @Composable fun SizeLabel() {
        DropdownMenu(items = {
            CatalogPaging.pageSizes.map { size -> MenuItem("$size / page", selected = size == pageSize) { onSize(size) } }
        }) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(tr("$pageSize / page"), style = Type.footnote, color = Palette.secondary)
                Symbol("chevron.down", size = 10.dp, tint = Palette.secondary)
            }
        }
    }
    @Composable fun PhoneJump() {
        Text(tr("Jump to"), style = Type.footnote, color = Palette.secondary)
        PlainTextField(jump, { jump = it.filter(Char::isDigit).take(4) },
            Modifier.width(36.dp).height(28.dp).clip(RoundedRectangle(8.dp)).background(Palette.background),
            singleLine = true, style = Type.footnote.copy(textAlign = TextAlign.Center),
            keyboard = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { commit() }))
        Text(tr("Go"), Modifier.clip(Capsule()).background(Palette.background).plainButton(enabled = !loading, action = commit)
            .padding(horizontal = 12.dp).height(28.dp), style = Type.footnote.semibold, color = if (loading) Palette.tertiary else Palette.text)
    }
    @Composable fun SizeMenu() {
        Text(tr("Per page"), style = Type.footnote, color = Palette.secondary)
        DropdownMenu(items = {
            CatalogPaging.pageSizes.map { size -> MenuItem("$size / page", selected = size == pageSize) { onSize(size) } }
        }) {
            Row(Modifier.clip(RoundedRectangle(8.dp)).background(Palette.bubbleAgent).padding(horizontal = 10.dp).height(28.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(tr("$pageSize / page"), style = Type.footnote, color = Palette.text)
                Symbol("chevron.down", size = 10.dp, tint = Palette.secondary)
            }
        }
    }
    @Composable fun Jump() {
        Text(tr("Jump to"), style = Type.footnote, color = Palette.secondary)
        PlainTextField(jump, { jump = it.filter(Char::isDigit).take(4) },
            Modifier.width(44.dp).height(28.dp).clip(RoundedRectangle(8.dp)).background(Palette.bubbleAgent),
            singleLine = true, style = Type.footnote.copy(textAlign = TextAlign.Center),
            keyboard = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { commit() }))
        PagerStep("Go", !loading, action = commit)
    }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        if (maxWidth < 560.dp) {
            // Previous and next sit beside the list. Jump hides when it no longer fits beside the page size and the page number.
            val showJump = maxWidth >= 300.dp
            Row(Modifier.fillMaxWidth().clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
                SizeLabel()
                Text("${current + 1} / $pages", style = Type.subheadline.semibold, color = Palette.text)
                if (showJump) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) { PhoneJump() }
            }
        } else FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
            itemVerticalAlignment = Alignment.CenterVertically) {
            Text(total, style = Type.footnote, color = Palette.secondary)
            SizeMenu()
            PagerStep("Previous page", canPrev) { go(current - 1) }
            Text("${current + 1} / $pages", style = Type.footnote, color = Palette.text)
            PagerStep(if (loading) "Loading…" else "Next page", canNext) { go(current + 1) }
            Jump()
        }
    }
}

@Composable private fun PagerStep(title: String, enabled: Boolean, symbol: String? = null, action: () -> Unit) {
    val label = tr(title)
    val tint = if (enabled) Palette.secondary else Palette.tertiary
    Box(Modifier.clip(RoundedRectangle(8.dp)).plainButton(enabled = enabled, action = action)
        .padding(horizontal = if (symbol == null) 10.dp else 6.dp, vertical = 6.dp), contentAlignment = Alignment.Center) {
        if (symbol == null) Text(label, style = Type.subheadline, color = tint, maxLines = 1)
        else Symbol(symbol, size = 14.dp, tint = tint, contentDescription = label)
    }
}
