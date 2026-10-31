//! Replay a frozen script. Each replay is a new realm. A finished call is not sent again.

#![allow(clippy::too_many_lines, clippy::map_unwrap_or, clippy::needless_pass_by_value, clippy::similar_names)]

use super::graph::BotState;
use super::limits::{MAX_ERROR_CHARS, MAX_INPUT, MAX_PARALLEL, MAX_SCRIPT_CALLS, MAX_STAGES, MAX_TEXT, SCRIPT_FUEL};
use super::model::{OutputTarget, new_id};
use super::run::{self, Effects, Push};
use super::store;
use crate::hub::Hub;
use crate::remote::push::AlertKind;
use crate::store::{EntryKind, Lane, now_ms};
use anyhow::{Result, bail};
use boa_engine::object::builtins::JsPromise;
use boa_engine::property::Attribute;
use boa_engine::{Context, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Source, js_string};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;

struct Call {
    status: String,
    output: Option<String>,
    error: Option<String>,
    schema: Option<String>,
}

struct Note {
    kind: String,
    text: String,
}

struct Fresh {
    index: i64,
    kind: String,
    bot: String,
    prompt: String,
    label: String,
    phase: String,
    schema: Option<String>,
    target: String,
    text: String,
    fail: Option<String>,
}

struct Host {
    index: i64,
    calls: HashMap<i64, Call>,
    fresh: Vec<Fresh>,
    notes: Vec<Note>,
    fatal: Option<String>,
    phase: Option<String>,
    output: Option<String>,
    /// While a parallel or pipeline stage is starting, a cached failure must not stop the other calls.
    defer_fatal: bool,
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host {
        index: 0,
        calls: HashMap::new(),
        fresh: Vec::new(),
        notes: Vec::new(),
        fatal: None,
        phase: None,
        output: None,
        defer_fatal: false,
    });
}

pub fn replay(hub: &Hub, run_id: &str) -> Result<()> {
    let Some(row) = load_run(&hub.store, run_id)? else { return Ok(()) };
    if row.status == "stopping" || row.status == "stopped" || row.status == "failed" || row.status == "done" {
        return Ok(());
    }
    let calls = load_calls(&hub.store, run_id)?;
    let script = rewrite(&row.script);
    let payload = row.payload.clone();
    let host = match interpret(script, payload, calls) {
        Ok(host) => host,
        Err(error) => {
            finish(hub, &row, "failed", Some(error.to_string()), None)?;
            hub.emit_workflow(
                json!({"type": "workflow", "change": "run", "workflowId": row.workflow, "runId": run_id}),
            );
            return Ok(());
        }
    };
    apply(hub, &row, host)
}

struct RunInfo {
    id: String,
    workflow: String,
    title: String,
    script: String,
    payload: String,
    status: String,
}

fn load_run(store: &crate::store::Store, id: &str) -> Result<Option<RunInfo>> {
    store.with_immediate(|tx| {
        let row = tx
            .query_row(
                "SELECT r.workflow_id, w.title, r.script_text, COALESCE(r.payload, ''), r.status, COALESCE(r.exec, 'graph')
                 FROM workflow_runs r JOIN workflows w ON w.id = r.workflow_id WHERE r.id = ?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)),
            )
            .optional()?;
        let Some((workflow, title, script, payload, status, exec)) = row else { return Ok(None) };
        if exec != "script" {
            return Ok(None);
        }
        Ok(Some(RunInfo { id: id.to_owned(), workflow, title, script, payload, status }))
    })
}

fn load_calls(store: &crate::store::Store, run_id: &str) -> Result<HashMap<i64, Call>> {
    store.with_immediate(|tx| {
        let mut st = tx.prepare(
            "SELECT node_id, status, output, error, schema_json, attempt FROM workflow_steps WHERE run_id = ?1 ORDER BY attempt",
        )?;
        let rows = st.query_map([run_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        let mut calls = HashMap::new();
        for row in rows {
            let (node, status, output, error, schema, _) = row?;
            let Some(index) = node.strip_prefix("call-").and_then(|n| n.parse().ok()) else { continue };
            calls.insert(index, Call { status, output, error, schema });
        }
        Ok(calls)
    })
}

fn interpret(script: String, payload: String, calls: HashMap<i64, Call>) -> Result<Host> {
    let run = || {
        HOST.with(|host| {
            *host.borrow_mut() = Host {
                index: 0,
                calls,
                fresh: Vec::new(),
                notes: Vec::new(),
                fatal: None,
                phase: None,
                output: None,
                defer_fatal: false,
            };
        });
        let mut context =
            Context::builder().instructions_remaining(SCRIPT_FUEL).build().map_err(|e| anyhow::anyhow!("{e}"))?;
        seal(&mut context)?;
        register(&mut context)?;
        let args = boa_engine::object::JsObject::with_null_proto();
        args.set(js_string!("input"), js_string!(payload), false, &mut context).map_err(|e| anyhow::anyhow!("{e}"))?;
        context
            .register_global_property(js_string!("args"), args, Attribute::all())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let value = context.eval(Source::from_bytes(script.as_bytes())).map_err(|e| message_of(&e))?;
        let () = context.run_jobs();
        if let Some(object) = value.as_object()
            && let Ok(promise) = JsPromise::from_object(object.clone())
        {
            match promise.state() {
                boa_engine::builtins::promise::PromiseState::Rejected(error) => {
                    note_fatal(&error.to_string(&mut context).map(|s| s.to_std_string_lossy()).unwrap_or_default());
                }
                boa_engine::builtins::promise::PromiseState::Fulfilled(value) => {
                    let text = returned(&value, &mut context);
                    HOST.with(|host| host.borrow_mut().output = Some(text));
                }
                boa_engine::builtins::promise::PromiseState::Pending => {}
            }
        }
        Ok::<_, anyhow::Error>(HOST.with(|host| host.borrow().snapshot()))
    };
    if tokio::runtime::Handle::try_current()
        .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
    {
        tokio::task::block_in_place(run)
    } else {
        run()
    }
}

impl Host {
    fn snapshot(&self) -> Host {
        Host {
            index: self.index,
            calls: HashMap::new(),
            fresh: self.fresh.iter().map(Fresh::copy).collect(),
            notes: self.notes.iter().map(|n| Note { kind: n.kind.clone(), text: n.text.clone() }).collect(),
            fatal: self.fatal.clone(),
            phase: self.phase.clone(),
            output: self.output.clone(),
            defer_fatal: false,
        }
    }
}

impl Fresh {
    fn copy(&self) -> Fresh {
        Fresh {
            index: self.index,
            kind: self.kind.clone(),
            bot: self.bot.clone(),
            prompt: self.prompt.clone(),
            label: self.label.clone(),
            phase: self.phase.clone(),
            schema: self.schema.clone(),
            target: self.target.clone(),
            text: self.text.clone(),
            fail: self.fail.clone(),
        }
    }
}

fn note_fatal(message: &str) {
    let message = clip(message, MAX_ERROR_CHARS);
    let message = if message.is_empty() { "This workflow's script didn't finish".into() } else { message };
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        if host.fatal.is_none() {
            host.fatal = Some(message);
        }
    });
}

fn seal(context: &mut Context) -> Result<()> {
    let global = context.global_object().clone();
    for name in ["Date", "eval", "Function", "setTimeout", "setInterval", "queueMicrotask", "fetch", "WebSocket"] {
        let _ = global.delete_property_or_throw(js_string!(name), context);
    }
    let math = global.get(js_string!("Math"), context).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(math) = math.as_object() {
        let _ = math.delete_property_or_throw(js_string!("random"), context);
    }
    Ok(())
}

fn register(context: &mut Context) -> Result<()> {
    for (name, length, function) in [
        ("agent", 2, NativeFunction::from_fn_ptr(js_agent)),
        ("approval", 1, NativeFunction::from_fn_ptr(js_approval)),
        ("output", 1, NativeFunction::from_fn_ptr(js_output)),
        ("phase", 1, NativeFunction::from_fn_ptr(js_phase)),
        ("log", 1, NativeFunction::from_fn_ptr(js_log)),
        ("parallel", 1, NativeFunction::from_fn_ptr(js_parallel)),
        ("pipeline", 2, NativeFunction::from_fn_ptr(js_pipeline)),
        ("match", 2, NativeFunction::from_fn_ptr(js_match)),
    ] {
        context.register_global_callable(js_string!(name), length, function).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}

fn js_agent(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let bot = arg_string(args, 0);
    let prompt = arg_string(args, 1);
    let label = object_string(args.get(2), "label", context);
    let schema = object_json(args.get(2), "schema", context);
    let phase = HOST.with(|host| host.borrow().phase.clone().unwrap_or_default());
    host_call(
        context,
        HostRequest {
            kind: "agent".into(),
            bot,
            prompt,
            label,
            phase,
            schema,
            target: String::new(),
            text: String::new(),
        },
    )
}

fn js_approval(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let question = arg_string(args, 0);
    let phase = HOST.with(|host| host.borrow().phase.clone().unwrap_or_default());
    host_call(
        context,
        HostRequest {
            kind: "approval".into(),
            bot: String::new(),
            prompt: question,
            label: String::new(),
            phase,
            schema: None,
            target: String::new(),
            text: String::new(),
        },
    )
}

fn js_output(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = object_string(args.first(), "target", context);
    let text = object_string(args.first(), "text", context);
    let bot = object_string(args.first(), "botId", context);
    let phase = HOST.with(|host| host.borrow().phase.clone().unwrap_or_default());
    host_call(
        context,
        HostRequest {
            kind: "output".into(),
            bot,
            prompt: String::new(),
            label: String::new(),
            phase,
            schema: None,
            target,
            text,
        },
    )
}

#[allow(clippy::unnecessary_wraps)]
fn js_phase(_this: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let title = clip(&arg_string(args, 0), 80);
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.phase = Some(title.clone());
        host.notes.push(Note { kind: "phase".into(), text: title });
    });
    Ok(JsValue::undefined())
}

#[allow(clippy::unnecessary_wraps)]
fn js_log(_this: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let text = clip(&arg_string(args, 0), MAX_TEXT);
    HOST.with(|host| host.borrow_mut().notes.push(Note { kind: "log".into(), text }));
    Ok(JsValue::undefined())
}

#[allow(clippy::unnecessary_wraps)]
fn js_match(_this: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let text = arg_string(args, 0);
    let pattern = arg_string(args, 1);
    let matched = regex::Regex::new(&pattern).is_ok_and(|re| re.is_match(&text));
    Ok(JsValue::from(matched))
}

fn js_parallel(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let Some(object) = args.first().and_then(JsValue::as_object) else {
        note_fatal("This parallel didn't get a list of steps");
        return Err(JsNativeError::typ().with_message("This parallel didn't get a list of steps").into());
    };
    let length = js_len(object.get(js_string!("length"), context)?.as_number().unwrap_or(0.0));
    if length > MAX_PARALLEL {
        note_fatal("Too many steps at once");
        return Err(JsNativeError::typ().with_message("Too many steps at once").into());
    }
    if HOST.with(|host| host.borrow().fatal.is_some()) {
        return Ok(pending_promise(context));
    }
    let mut called = Vec::new();
    let defer = DeferFatal::start();
    for index in 0..length {
        let item = object.get(index, context)?;
        let Some(func) = item.as_object() else {
            note_fatal("This parallel didn't get a list of steps");
            break;
        };
        called.push(func.call(&JsValue::undefined(), &[], context)?);
        let () = context.run_jobs();
    }
    drop(defer);
    let mut values = Vec::new();
    let mut pending = false;
    for value in called {
        let Some(value) = await_settled(value, context) else {
            pending = true;
            continue;
        };
        values.push(value);
    }
    if pending {
        return Ok(pending_promise(context));
    }
    Ok(boa_engine::object::builtins::JsArray::from_iter(values, context).into())
}

fn js_pipeline(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let Some(items) = args.first().and_then(JsValue::as_object) else {
        note_fatal("The pipeline didn't get a list");
        return Err(JsNativeError::typ().with_message("The pipeline didn't get a list").into());
    };
    let stages: Vec<JsValue> = args.iter().skip(1).cloned().collect();
    if stages.is_empty() || stages.len() > MAX_STAGES {
        note_fatal("Too many stages");
        return Err(JsNativeError::typ().with_message("Too many stages").into());
    }
    let count = js_len(items.get(js_string!("length"), context)?.as_number().unwrap_or(0.0));
    if count > MAX_PARALLEL {
        note_fatal("Too many steps at once");
        return Err(JsNativeError::typ().with_message("Too many steps at once").into());
    }
    let mut current = Vec::new();
    for index in 0..count {
        current.push(items.get(index, context)?);
    }
    for stage in &stages {
        let Some(func) = stage.as_object() else {
            note_fatal("Too many stages");
            break;
        };
        if HOST.with(|host| host.borrow().fatal.is_some()) {
            break;
        }
        let mut called = Vec::new();
        let defer = DeferFatal::start();
        for (index, value) in current.iter().enumerate() {
            called.push(func.call(&JsValue::undefined(), &[value.clone(), JsValue::from(index)], context)?);
            let () = context.run_jobs();
        }
        drop(defer);
        let mut next = Vec::new();
        let mut pending = false;
        for value in called {
            let Some(settled) = await_settled(value, context) else {
                pending = true;
                continue;
            };
            next.push(settled);
        }
        if pending {
            return Ok(pending_promise(context));
        }
        current = next;
    }
    Ok(boa_engine::object::builtins::JsArray::from_iter(current, context).into())
}

struct HostRequest {
    kind: String,
    bot: String,
    prompt: String,
    label: String,
    phase: String,
    schema: Option<String>,
    target: String,
    text: String,
}

fn host_call(context: &mut Context, request: HostRequest) -> JsResult<JsValue> {
    let (promise, resolvers) = JsPromise::new_pending(context);
    let index = HOST.with(|host| {
        let mut host = host.borrow_mut();
        if host.fatal.is_some() && !host.defer_fatal {
            return Err("stop");
        }
        host.index += 1;
        if host.index > MAX_SCRIPT_CALLS {
            host.fatal = Some("This run called for too many steps".into());
            return Err("cap");
        }
        let index = host.index;
        if let Some(call) = host.calls.get(&index) {
            return Ok((
                index,
                Some((call.status.clone(), call.output.clone(), call.error.clone(), call.schema.clone())),
            ));
        }
        Ok((index, None))
    });
    let Ok((index, cached)) = index else {
        resolvers.reject.call(
            &JsValue::undefined(),
            &[js_string!("This workflow's script didn't finish").into()],
            context,
        )?;
        return Ok(promise.into());
    };
    if let Some((status, output, error, schema)) = cached {
        if status == "done" {
            let value = typed_value(context, &output.unwrap_or_default(), schema.as_deref());
            resolvers.resolve.call(&JsValue::undefined(), &[value], context)?;
        } else if status == "failed" || status == "stopped" {
            let message = error.unwrap_or_else(|| "This workflow's script didn't finish".into());
            let defer = HOST.with(|host| host.borrow().defer_fatal);
            if !defer {
                note_fatal(&message);
            }
            resolvers.reject.call(&JsValue::undefined(), &[js_string!(message).into()], context)?;
        }
        return Ok(promise.into());
    }
    let mut fail = None;
    if request.kind == "agent" && request.prompt.trim().is_empty() {
        fail = Some("The step didn't include a prompt".into());
        note_fatal("The step didn't include a prompt");
    }
    let text = request.text.clone();
    let kind = request.kind.clone();
    HOST.with(|host| {
        host.borrow_mut().fresh.push(Fresh {
            index,
            kind: request.kind,
            bot: request.bot,
            prompt: request.prompt,
            label: request.label,
            phase: request.phase,
            schema: request.schema,
            target: request.target,
            text: text.clone(),
            fail,
        });
    });
    if kind == "output" {
        resolvers.resolve.call(&JsValue::undefined(), &[js_string!(text).into()], context)?;
    }
    Ok(promise.into())
}

fn typed_value(context: &mut Context, output: &str, schema: Option<&str>) -> JsValue {
    if schema.is_some()
        && let Ok(parsed) = serde_json::from_str::<Value>(output)
        && let Ok(value) = JsValue::from_json(&parsed, context)
    {
        return value;
    }
    js_string!(output).into()
}

/// Keeps a cached failure from stopping the other calls in the same parallel or pipeline stage.
struct DeferFatal;

impl DeferFatal {
    fn start() -> Self {
        HOST.with(|host| host.borrow_mut().defer_fatal = true);
        Self
    }
}

impl Drop for DeferFatal {
    fn drop(&mut self) {
        HOST.with(|host| host.borrow_mut().defer_fatal = false);
    }
}

fn await_settled(value: JsValue, context: &mut Context) -> Option<JsValue> {
    let Some(object) = value.as_object() else { return Some(value) };
    let Ok(promise) = JsPromise::from_object(object.clone()) else { return Some(value) };
    match promise.state() {
        boa_engine::builtins::promise::PromiseState::Fulfilled(value) => Some(value),
        boa_engine::builtins::promise::PromiseState::Rejected(error) => {
            let message = error.to_string(context).map(|text| text.to_std_string_lossy()).unwrap_or_default();
            note_fatal(&message);
            None
        }
        boa_engine::builtins::promise::PromiseState::Pending => None,
    }
}

fn pending_promise(context: &mut Context) -> JsValue {
    JsPromise::new_pending(context).0.into()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn js_len(value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f64::from(u32::MAX) {
        return u32::MAX as usize;
    }
    value as usize
}

fn apply(hub: &Hub, run: &RunInfo, host: Host) -> Result<()> {
    let mut effects = Effects { agents: Vec::new(), pushes: Vec::new() };
    let now = now_ms();
    let mut output_error: Option<String> = None;
    for fresh in &host.fresh {
        let step = new_id();
        let node = format!("call-{}", fresh.index);
        let missing_chat = fresh.kind == "output"
            && fresh.fail.is_none()
            && OutputTarget::parse(Some(&fresh.target)).is_chat()
            && !matches!(store::bot_state(&hub.store, fresh.bot.trim()), BotState::Ok);
        let (status, output, error) = if let Some(fail) = &fresh.fail {
            ("failed", None, Some(fail.clone()))
        } else if missing_chat {
            output_error = Some("Choose a bot for this step".into());
            ("failed", None, Some("Choose a bot for this step".into()))
        } else if fresh.kind == "output" {
            ("done", Some(clip(&fresh.text, MAX_TEXT)), None)
        } else if fresh.kind == "approval" {
            ("approval", None, None)
        } else {
            ("running", None, None)
        };
        let prompt = clip(&if fresh.kind == "output" { fresh.text.clone() } else { fresh.prompt.clone() }, MAX_INPUT);
        let truncated = i64::from(prompt.chars().count() >= MAX_INPUT);
        let request = format!("script:{}:{}", run.id, fresh.index);
        hub.store.with_immediate(|tx| {
            tx.execute(
                "INSERT INTO workflow_steps(id, workflow_id, run_id, node_id, kind, status, attempt, request_id, input, output, error, bot_id, passes, truncated, label, phase, schema_json, created_at, updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8,?9,?10,?11,1,?12,?13,?14,?15,?16,?16)",
                params![
                    step, run.workflow, run.id, node, fresh.kind, status, request, prompt, output, error,
                    fresh.bot.trim(), truncated, clip(&fresh.label, 80), clip(&fresh.phase, 80), fresh.schema, now
                ],
            )?;
            Ok(())
        })?;
        if fresh.kind == "agent" && status == "running" {
            effects.agents.push((fresh.bot.clone(), step.clone()));
        }
        if fresh.kind == "approval"
            && let Some(bot) = anchor(hub, &run.workflow)
        {
            effects.pushes.push(Push {
                bot,
                title: run.title.clone(),
                body: clip(&fresh.prompt, 180),
                kind: AlertKind::NeedsInput,
                workflow: run.workflow.clone(),
                run: Some(run.id.clone()),
                step: step.clone(),
            });
        }
        if fresh.kind == "output" && status == "done" {
            deliver_output(hub, run, &step, &fresh.bot, &fresh.target, &prompt, &mut effects)?;
        }
    }
    replace_notes(hub, &run.id, &host.notes, now)?;
    if let Some(phase) = &host.phase {
        hub.emit_workflow(
            json!({"type": "workflow", "change": "phase", "workflowId": run.workflow, "runId": run.id, "phase": phase}),
        );
    }
    let failed = host.fatal.clone().or(output_error);
    let busy =
        host.fresh.iter().any(|fresh| fresh.fail.is_none() && (fresh.kind == "agent" || fresh.kind == "approval"))
            || waiting(&hub.store, &run.id)?;
    if let Some(message) = failed {
        if busy {
            set_status(hub, &run.id, "running")?;
        } else {
            finish(hub, run, "failed", Some(message), None)?;
        }
    } else if host.fresh.iter().any(|f| f.kind == "approval" && f.fail.is_none()) {
        set_status(hub, &run.id, "approval")?;
    } else if host.fresh.iter().any(|f| f.kind == "agent" && f.fail.is_none()) || waiting(&hub.store, &run.id)? {
        set_status(hub, &run.id, "running")?;
    } else {
        finish(hub, run, "done", None, host.output)?;
    }
    run::dispatch_agents(hub, &effects);
    run::dispatch_pushes(hub, &effects);
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": run.workflow, "runId": run.id}));
    Ok(())
}

fn waiting(store: &crate::store::Store, run_id: &str) -> Result<bool> {
    store.with_immediate(|tx| {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM workflow_steps WHERE run_id = ?1 AND status IN ('running','approval')",
            [run_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    })
}

fn deliver_output(
    hub: &Hub,
    run: &RunInfo,
    step: &str,
    bot: &str,
    target: &str,
    text: &str,
    effects: &mut Effects,
) -> Result<()> {
    if OutputTarget::parse(Some(target)).is_chat() {
        if !matches!(store::bot_state(&hub.store, bot), BotState::Ok) {
            bail!("Choose a bot for this step");
        }
        let turn = hub.store.max_turn(bot) + 1;
        let data = json!({"text": text, "workflowId": run.workflow, "runId": run.id, "stepId": step, "notice": "workflow-output"});
        hub.add_entry_once(&format!("workflow-output-{step}"), &Lane::main(bot), EntryKind::Notice, turn, &data)?;
        return Ok(());
    }
    if let Some(anchor) =
        anchor(hub, &run.workflow).or_else(|| bot.trim().is_empty().then(String::new).filter(|id| !id.is_empty()))
    {
        let _ = anchor;
    }
    if let Some(bot) = anchor(hub, &run.workflow) {
        effects.pushes.push(Push {
            bot,
            title: run.title.clone(),
            body: clip(text, 180),
            kind: AlertKind::Done,
            workflow: run.workflow.clone(),
            run: Some(run.id.clone()),
            step: step.to_owned(),
        });
    }
    Ok(())
}

fn anchor(hub: &Hub, workflow: &str) -> Option<String> {
    let graph = store::load(&hub.store, workflow).ok().flatten().map(|(_, graph)| graph)?;
    graph
        .nodes
        .iter()
        .find_map(|n| {
            if n.kind != super::model::NodeKind::Agent {
                return None;
            }
            let bot = super::model::field_str(&n.fields, "botId").unwrap_or("").trim().to_owned();
            matches!(store::bot_state(&hub.store, &bot), BotState::Ok).then_some(bot)
        })
        .or_else(|| lead(hub))
}

fn lead(hub: &Hub) -> Option<String> {
    hub.store.bots().ok()?.into_iter().find(|bot| bot.config.lead && !bot.deleted).map(|bot| bot.config.id)
}

fn replace_notes(hub: &Hub, run_id: &str, notes: &[Note], now: i64) -> Result<()> {
    hub.store.with_immediate(|tx| {
        tx.execute("DELETE FROM workflow_run_notes WHERE run_id = ?1", [run_id])?;
        for (position, note) in notes.iter().enumerate() {
            tx.execute(
                "INSERT INTO workflow_run_notes(run_id, position, kind, text, at) VALUES(?1,?2,?3,?4,?5)",
                params![run_id, i64::try_from(position).unwrap_or(i64::MAX), note.kind, note.text, now],
            )?;
        }
        Ok(())
    })
}

fn set_status(hub: &Hub, run_id: &str, status: &str) -> Result<()> {
    hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_runs SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![run_id, status, now_ms()],
        )?;
        Ok(())
    })
}

fn finish(hub: &Hub, run: &RunInfo, status: &str, error: Option<String>, output: Option<String>) -> Result<()> {
    hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_runs SET status = ?2, error = ?3, output = ?4, ended_at = ?5, updated_at = ?5 WHERE id = ?1",
            params![run.id, status, error, output, now_ms()],
        )?;
        Ok(())
    })
}

fn returned(value: &JsValue, context: &mut Context) -> String {
    if value.is_undefined() || value.is_null() {
        return String::new();
    }
    if let Some(text) = value.as_string() {
        return text.to_std_string_lossy();
    }
    value.to_json(context).map(|v| v.to_string()).unwrap_or_default()
}

fn rewrite(src: &str) -> String {
    let body = strip_export(src, "export const meta", "const meta");
    let body = strip_export(&body, "export default async function", "const __codync_main = async function");
    format!("{body}\n__codync_main({{ agent, parallel, pipeline, phase, log, args, approval, output }})\n")
}

fn strip_export(src: &str, from: &str, to: &str) -> String {
    let mut i = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while i < src.len() {
        let Some(ch) = src[i..].chars().next() else { break };
        let width = ch.len_utf8();
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            i += width;
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            i += width;
            continue;
        }
        if src[i..].starts_with(from) {
            return format!("{}{}{}", &src[..i], to, &src[i + from.len()..]);
        }
        i += width;
    }
    src.to_owned()
}

fn arg_string(args: &[JsValue], index: usize) -> String {
    args.get(index).and_then(JsValue::as_string).map(boa_engine::JsString::to_std_string_lossy).unwrap_or_default()
}

fn object_string(value: Option<&JsValue>, key: &str, context: &mut Context) -> String {
    let Some(object) = value.and_then(JsValue::as_object) else { return String::new() };
    object
        .get(js_string!(key), context)
        .ok()
        .and_then(|v| v.as_string().map(boa_engine::JsString::to_std_string_lossy))
        .unwrap_or_default()
}

fn object_json(value: Option<&JsValue>, key: &str, context: &mut Context) -> Option<String> {
    let object = value.and_then(JsValue::as_object)?;
    let value = object.get(js_string!(key), context).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }
    Some(value.to_json(context).ok()?.to_string())
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn message_of(error: &JsError) -> anyhow::Error {
    let _ = error;
    anyhow::anyhow!("This workflow's script didn't finish")
}

pub fn is_script_run(store: &crate::store::Store, run_id: &str) -> bool {
    store
        .with_immediate(|tx| {
            Ok(tx
                .query_row("SELECT exec FROM workflow_runs WHERE id = ?1", [run_id], |r| r.get::<_, String>(0))
                .optional()?)
        })
        .ok()
        .flatten()
        .is_some_and(|exec| exec == "script")
}

pub fn schema_ok(schema: &str, output: &str) -> bool {
    let Ok(schema) = serde_json::from_str::<Value>(schema) else { return false };
    let Ok(value) = serde_json::from_str::<Value>(output) else { return false };
    matches_schema(&schema, &value)
}

fn matches_schema(schema: &Value, value: &Value) -> bool {
    let Some(obj) = schema.as_object() else { return true };
    if let Some(kind) = obj.get("type").and_then(Value::as_str) {
        let ok = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    if let Some(choices) = obj.get("enum").and_then(Value::as_array)
        && !choices.iter().any(|choice| choice == value)
    {
        return false;
    }
    if let Some(items) = obj.get("items") {
        let Some(array) = value.as_array() else { return false };
        if array.iter().any(|item| !matches_schema(items, item)) {
            return false;
        }
    }
    if let Some(required) = obj.get("required").and_then(Value::as_array) {
        let Some(object) = value.as_object() else { return false };
        if required.iter().any(|key| key.as_str().is_some_and(|key| !object.contains_key(key))) {
            return false;
        }
    }
    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        let Some(object) = value.as_object() else { return false };
        if object.keys().any(|key| !props.contains_key(key)) {
            return false;
        }
        for (key, prop) in props {
            if let Some(item) = object.get(key)
                && !matches_schema(prop, item)
            {
                return false;
            }
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub fn start_script(
    hub: &Hub,
    id: &str,
    summary: &Value,
    snapshot: &super::model::Graph,
    _request: &str,
    from: Option<&str>,
    source: super::model::RunSource,
    payload: &str,
    scheduled_for: Option<i64>,
    key: &str,
) -> Result<Value> {
    if let Some(node) = from
        && !snapshot.nodes.iter().any(|n| n.id == node && n.kind == super::model::NodeKind::Trigger)
    {
        bail!("That step is not in this workflow");
    }
    let run_id = new_id();
    let now = now_ms();
    hub.store.with_immediate(|tx| {
        let number: i64 = tx.query_row("SELECT next_run_number FROM workflows WHERE id = ?1", [id], |r| r.get(0))?;
        tx.execute("UPDATE workflows SET next_run_number = ?2 WHERE id = ?1", params![id, number + 1])?;
        tx.execute(
            "INSERT INTO workflow_runs(id, workflow_id, number, trigger_node_id, from_node_id, source, scheduled_for, idempotency_key, revision, graph_json, payload, status, started_at, updated_at, exec, script_text)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'running',?12,?12,'script',?13)",
            params![
                run_id, id, number, from, from, source.as_str(), scheduled_for, key,
                summary["revision"].as_i64().unwrap_or(0), serde_json::to_string(snapshot)?, payload, now,
                summary["script"].as_str().unwrap_or("")
            ],
        )?;
        Ok(())
    })?;
    replay(hub, &run_id)?;
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": id, "runId": run_id}));
    run::run_detail(hub, &run_id)
}
