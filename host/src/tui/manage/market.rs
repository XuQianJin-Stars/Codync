//! The marketplace: agents, connectors and skills.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay, fuzzy};
use super::super::l10n::tr;
use super::{Fields, Input, Market, Reply, Row, Submit, TABS, Tab, ctrl, s};

impl App {
    pub fn open_market(&mut self) {
        self.call("refreshBackends", json!({}), After::Backends);
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
        self.sheet("marketSkills", json!({}), Reply::Skills);
        self.sheet("marketConnectors", json!({"search": ""}), Reply::Registry { more: false });
        self.overlays.push(Overlay::Market(Box::new(Market {
            tab: Tab::Agents,
            query: Editor::default(),
            searched: None,
            registry: vec![],
            next: None,
            skills: None,
            loading: true,
            cursor: 0,
            error: None,
            pages: [0; 3],
            page_size: PAGE_SIZE,
            jumping: false,
            jump: String::new(),
        })));
    }

    /// The current page of the marketplace's rows.
    pub fn market_rows<'a>(&'a self, m: &'a Market) -> Vec<Row<'a>> {
        let all = self.market_catalog(m);
        let more = m.tab == Tab::Connectors && m.next.is_some();
        let page = clamp_page(m.pages[m.tab.index()], all.len(), m.page_size, more);
        let start = page.saturating_mul(m.page_size.max(1));
        all.into_iter().skip(start).take(m.page_size.max(1)).collect()
    }

    /// Count of the current tab, and whether the connector registry has another cursor page.
    fn market_span(&self, m: &Market) -> (usize, bool) {
        let count = self.market_catalog(m).len();
        (count, m.tab == Tab::Connectors && m.next.is_some())
    }

    /// The pager, in the same order as the other clients: count, size, previous, current / total, next.
    pub fn market_pager_line(&self, m: &Market) -> String {
        let (count, more) = self.market_span(m);
        let pages = page_count(count, m.page_size, more);
        let page = clamp_page(m.pages[m.tab.index()], count, m.page_size, more) + 1;
        let size = match m.page_size {
            6 => "6 / page",
            20 => "20 / page",
            50 => "50 / page",
            _ => "10 / page",
        };
        let mut line = format!(
            "{}   {} {}   {}   {page} / {pages}   {}",
            tr("{count} total").replace("{count}", &count.to_string()),
            tr("Per page"),
            tr(size),
            tr("Previous page"),
            tr("Next page"),
        );
        if m.jumping {
            use std::fmt::Write as _;
            let _ = write!(line, "   {} {}", tr("Jump to"), m.jump);
        }
        line
    }

    fn load_more_connectors(&mut self, m: &mut Market) {
        if m.loading || m.next.is_none() {
            return;
        }
        m.loading = true;
        let search = m.searched.clone().unwrap_or_default();
        self.sheet("marketConnectors", json!({"search": search, "cursor": m.next}), Reply::Registry { more: true });
    }

    /// After a registry page arrives, keep fetching until the open page is filled or the cursor ends.
    pub(super) fn continue_market_page(&mut self, m: &mut Market) {
        let (count, more) = self.market_span(m);
        let i = m.tab.index();
        if needs_fetch(count, m.pages[i], m.page_size) && m.next.is_some() {
            self.load_more_connectors(m);
            return;
        }
        m.pages[i] = clamp_page(m.pages[i], count, m.page_size, more);
    }

    fn turn_market_page(&mut self, m: &mut Market, forward: bool) {
        let (count, more) = self.market_span(m);
        let pages = page_count(count, m.page_size, more);
        let i = m.tab.index();
        if forward {
            if m.pages[i] + 1 >= pages {
                return;
            }
            m.pages[i] += 1;
            m.cursor = 0;
            if needs_fetch(count, m.pages[i], m.page_size) {
                self.load_more_connectors(m);
            }
        } else if m.pages[i] > 0 {
            m.pages[i] -= 1;
            m.cursor = 0;
        }
    }

    fn jump_market(&mut self, m: &mut Market) {
        let Some(n) = m.jump.parse::<usize>().ok().filter(|n| *n >= 1) else {
            m.jumping = false;
            m.jump.clear();
            return;
        };
        m.jumping = false;
        m.jump.clear();
        let (count, more) = self.market_span(m);
        let i = m.tab.index();
        m.pages[i] = clamp_page(n - 1, count, m.page_size, more);
        m.cursor = 0;
        if needs_fetch(count, m.pages[i], m.page_size) {
            self.load_more_connectors(m);
        }
    }

    /// Every row on the current tab, before the page window.
    fn market_catalog<'a>(&'a self, m: &'a Market) -> Vec<Row<'a>> {
        let q = m.query.text.trim();
        let hit = |v: &Value, keys: &[&str]| {
            let fields: Vec<&str> = keys.iter().map(|k| v[*k].as_str().unwrap_or_default()).collect();
            fuzzy(q, &fields)
        };
        match m.tab {
            Tab::Agents => {
                let agents = self
                    .backends
                    .iter()
                    .filter(|b| b["available"] == true || b["installed"] == true || b["curated"] == true);
                let node = agents.clone().any(|b| b["node"] == "missing").then_some(Row::Node);
                node.into_iter().chain(agents.filter(|b| hit(b, &["name", "id"])).map(Row::Agent)).collect()
            }
            Tab::Connectors => {
                let installed: Vec<&str> = self.plugins.0.iter().filter_map(|c| c["registryName"].as_str()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.0.iter().filter(|c| hit(c, &["name", "description"])).map(Row::Installed).collect();
                rows.push(Row::Custom);
                rows.extend(
                    m.registry
                        .iter()
                        .filter(|c| !installed.contains(&c["name"].as_str().unwrap_or_default()))
                        .map(Row::Registry),
                );
                rows
            }
            Tab::Skills => {
                let installed: Vec<String> = self.plugins.1.iter().map(|v| s(v, "id").to_lowercase()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.1.iter().filter(|v| hit(v, &["name", "description"])).map(Row::OwnSkill).collect();
                rows.extend(
                    m.skills
                        .iter()
                        .flatten()
                        .filter(|v| !installed.contains(&s(v, "source").to_lowercase()))
                        .filter(|v| hit(v, &["name", "description"]))
                        .map(Row::Skill),
                );
                rows
            }
        }
    }

    /// The query no longer matches what the registry list shows: ↵ searches first.
    pub fn market_stale(m: &Market) -> bool {
        m.tab == Tab::Connectors && m.searched.as_deref().is_some_and(|s| s != m.query.text.trim())
    }

    pub(in crate::tui) fn market_key(&mut self, mut m: Box<Market>, k: KeyEvent) -> Option<Overlay> {
        if m.jumping {
            match k.code {
                KeyCode::Esc => {
                    m.jumping = false;
                    m.jump.clear();
                }
                KeyCode::Enter => self.jump_market(&mut m),
                KeyCode::Backspace => {
                    m.jump.pop();
                }
                KeyCode::Char(c) if c.is_ascii_digit() && m.jump.len() < 4 => m.jump.push(c),
                _ => {}
            }
            return Some(Overlay::Market(m));
        }
        let n = self.market_rows(&m).len();
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let ctrl_key = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Tab | KeyCode::BackTab => {
                let i = TABS.iter().position(|(t, _)| *t == m.tab).unwrap_or(0);
                let d = if k.code == KeyCode::Tab { 1 } else { TABS.len() - 1 };
                m.tab = TABS[(i + d) % TABS.len()].0;
                m.cursor = 0;
            }
            KeyCode::Down => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::PageDown => self.turn_market_page(&mut m, true),
            KeyCode::PageUp => self.turn_market_page(&mut m, false),
            KeyCode::Char('g') if ctrl_key => {
                m.jumping = true;
                m.jump.clear();
            }
            KeyCode::Char('s') if alt => {
                let i = PAGE_SIZES.iter().position(|s| *s == m.page_size).unwrap_or(0);
                m.page_size = PAGE_SIZES[(i + 1) % PAGE_SIZES.len()];
                m.pages = [0; 3];
                m.cursor = 0;
            }
            KeyCode::Char('n' | 'j') if ctrl(k) => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Char('p') if ctrl(k) => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('r') if ctrl(k) => {
                self.call("connectors", json!({}), After::Connectors);
                self.call("skills", json!({}), After::Skills);
                self.call("refreshBackends", json!({}), After::Backends);
            }
            KeyCode::Enter if Self::market_stale(&m) => {
                let search = m.query.text.trim().to_owned();
                m.loading = true;
                m.cursor = 0;
                m.registry.clear();
                m.next = None;
                m.pages[Tab::Connectors.index()] = 0;
                self.sheet("marketConnectors", json!({"search": search}), Reply::Registry { more: false });
                m.searched = Some(search);
            }
            KeyCode::Enter => return self.market_enter(m),
            KeyCode::Char('x') if ctrl(k) => {
                let (id, name, skill) = match self.market_rows(&m).get(m.cursor) {
                    Some(Row::Installed(c)) => (s(c, "id"), s(c, "name"), false),
                    Some(Row::OwnSkill(v)) => (s(v, "id"), s(v, "name"), true),
                    _ => return Some(Overlay::Market(m)),
                };
                let c = Confirm {
                    title: tr("Remove {name}?").replace("{name}", &name),
                    detail: tr(if skill {
                        "Bots stop using this skill."
                    } else {
                        "Bots lose this connector, and the keys saved for it are deleted."
                    })
                    .into_owned(),
                    note: String::new(),
                    button: "Remove",
                    act: if skill { ConfirmAct::RemoveSkill(id) } else { ConfirmAct::RemoveConnector(id) },
                };
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {
                if m.query.key(k) {
                    m.cursor = 0;
                    m.pages[m.tab.index()] = 0;
                }
            }
        }
        Some(Overlay::Market(m))
    }

    fn market_enter(&mut self, m: Box<Market>) -> Option<Overlay> {
        enum Act {
            Node,
            Agent(String),
            SignIn(String),
            Custom,
            Install(Value),
            Skill(Value, bool),
            None,
        }
        let act = match self.market_rows(&m).get(m.cursor) {
            Some(Row::Node) => Act::Node,
            Some(Row::Agent(b)) => Act::Agent(s(b, "id")),
            Some(Row::Installed(c)) if c["auth"] == "signedOut" => Act::SignIn(s(c, "id")),
            Some(Row::Custom) => Act::Custom,
            Some(Row::Registry(c)) => Act::Install((*c).clone()),
            Some(Row::Skill(v)) => Act::Skill((*v).clone(), true),
            Some(Row::OwnSkill(v)) => Act::Skill((*v).clone(), false),
            _ => Act::None,
        };
        match act {
            Act::Node => {
                let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                self.start_setup("", "node", None, &json!({"cols": cols, "rows": rows}));
            }
            Act::Agent(id) => {
                self.overlays.push(Overlay::Market(m));
                self.open_agent(&id, false);
                return None;
            }
            Act::SignIn(id) => {
                self.flash("Starting the sign-in…");
                self.sheet("connectorSignIn", json!({"id": id}), Reply::SignIn);
            }
            Act::Custom => {
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Fields(Box::new(Fields {
                    title: "Custom connector".into(),
                    note: "Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added.".into(),
                    choices: vec![],
                    choice: 0,
                    inputs: vec![Input {
                        name: "config".into(),
                        label: "Config".into(),
                        secret: false,
                        required: true,
                        multiline: true,
                        placeholder: r#"{"mcpServers": {"name": {"command": "npx", "args": ["-y", "…"]}}}"#.into(),
                        ed: Editor::default(),
                    }],
                    cursor: 0,
                    error: None,
                    saving: false,
                    submit: Submit::Import,
                })));
                return None;
            }
            Act::Install(item) => {
                let options = item["options"].as_array().cloned().unwrap_or_default();
                if options.len() == 1 && options[0]["inputs"].as_array().is_none_or(Vec::is_empty) {
                    self.flash(&tr("Adding {name}…").replace("{name}", &s(&item, "title")));
                    self.sheet(
                        "installConnector",
                        json!({"registryName": item["name"], "option": options[0]["id"], "inputs": {}}),
                        Reply::FieldsSaved,
                    );
                } else {
                    let host = self.host.clone();
                    let choices = options
                        .iter()
                        .map(|o| {
                            if o["kind"] == "remote" {
                                tr("Hosted by {name}").replace("{name}", &s(&item, "title"))
                            } else {
                                tr("On {name} ({kind})").replace("{name}", &host).replace("{kind}", &s(o, "kind"))
                            }
                        })
                        .collect();
                    let mut f = Fields {
                        title: tr("Add connector").into_owned(),
                        note: tr("Keys are saved on {name} only.").replace("{name}", &host),
                        choices,
                        choice: 0,
                        inputs: vec![],
                        cursor: 0,
                        error: None,
                        saving: false,
                        submit: Submit::Connector(item),
                    };
                    connector_inputs(&mut f);
                    super::super::connections::option_fields(&mut f);
                    self.overlays.push(Overlay::Market(m));
                    self.overlays.push(Overlay::Fields(Box::new(f)));
                    return None;
                }
            }
            Act::Skill(v, catalog) => {
                self.overlays.push(Overlay::Market(m));
                self.open_skill(&v, catalog);
                return None;
            }
            Act::None => {}
        }
        Some(Overlay::Market(m))
    }
}

const PAGE_SIZE: usize = 10;
const PAGE_SIZES: [usize; 4] = [6, 10, 20, 50];

/// Loaded rows split into pages. While the registry still has a cursor, one more page is offered.
fn page_count(count: usize, size: usize, has_more: bool) -> usize {
    let size = size.max(1);
    let loaded = if count == 0 { 1 } else { count.div_ceil(size) };
    if has_more { loaded + 1 } else { loaded }
}

fn clamp_page(page: usize, count: usize, size: usize, has_more: bool) -> usize {
    page.min(page_count(count, size, has_more).saturating_sub(1))
}

fn needs_fetch(count: usize, page: usize, size: usize) -> bool {
    page.saturating_mul(size.max(1)) >= count
}

/// The inputs of a connector's chosen way to run, defaults filled in.
pub(super) fn connector_inputs(f: &mut Fields) {
    let Submit::Connector(item) = &f.submit else { return };
    f.inputs = item["options"][f.choice]["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let name = s(i, "name");
            let description = s(i, "description");
            Input {
                label: name.clone(),
                name,
                secret: i["secret"] == true,
                required: i["required"] == true,
                multiline: false,
                placeholder: if description.is_empty() { s(i, "placeholder") } else { description },
                ed: Editor::with(i["default"].as_str().unwrap_or_default()),
            }
        })
        .collect();
    f.cursor = f.cursor.min(f.offset() + f.inputs.len().saturating_sub(1));
}

#[cfg(test)]
mod tests {
    use super::{clamp_page, needs_fetch, page_count};

    #[test]
    fn forty_seven_agents_are_five_pages() {
        assert_eq!(page_count(47, 10, false), 5);
        assert_eq!(page_count(19, 10, false), 2);
        assert_eq!(page_count(0, 10, false), 1);
        assert_eq!(page_count(72, 10, true), 9);
        assert_eq!(clamp_page(9, 47, 10, false), 4);
        assert_eq!(clamp_page(9, 72, 10, true), 8);
        assert!(!needs_fetch(60, 5, 10));
        assert!(needs_fetch(60, 6, 10));
    }
}
