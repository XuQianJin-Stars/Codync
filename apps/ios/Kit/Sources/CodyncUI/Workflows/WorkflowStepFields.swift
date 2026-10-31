import CodyncKit
import SwiftUI

/// The step sheet's fields, split out so one view function does not hold every control.
struct WorkflowStepFields: View {
    @Binding var draft: WorkflowNode
    var webhook: WorkflowWebhook?
    var workflowId: String
    var bots: [(id: String, label: String)]
    var schedule: Binding<RoutineScheduleDraft>
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var revealed: [WorkflowWebhook]
    var showKey: Binding<Bool>
    var copied: Binding<String?>
    var rotating: Bool
    var offline: Bool
    var onPickZone: () -> Void
    var onReveal: () -> Void
    var onRotate: () -> Void
    var onTrigger: (String) -> Void
    var prepare: () async -> String? = { nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            kind
            WorkflowSoloRun(workflowId: workflowId, nodeId: draft.id, kind: draft.kind, prepare: prepare)
        }
    }

    @ViewBuilder private var kind: some View {
        switch draft.kind {
        case .trigger:
            WorkflowTriggerFields(draft: $draft, webhook: webhook, revealed: revealed, schedule: schedule, checked: checked,
                                  preview: preview, previewError: previewError, showKey: showKey, copied: copied,
                                  rotating: rotating, offline: offline, onPickZone: onPickZone, onReveal: onReveal,
                                  onRotate: onRotate, onTrigger: onTrigger)
        case .prompt:
            Field("Prompt") { WorkflowTextField(draft: $draft, key: "text", axis: .vertical) }
        case .agent:
            WorkflowAgentFields(draft: $draft, bots: bots)
        case .condition:
            WorkflowConditionFields(draft: $draft)
        case .approval:
            Field("Question") { WorkflowTextField(draft: $draft, key: "question", axis: .vertical) }
        case .output:
            WorkflowOutputFields(draft: $draft, bots: bots)
        default:
            EmptyView()
        }
    }
}

private struct WorkflowTriggerFields: View {
    @Binding var draft: WorkflowNode
    var webhook: WorkflowWebhook?
    var revealed: [WorkflowWebhook]
    var schedule: Binding<RoutineScheduleDraft>
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var showKey: Binding<Bool>
    var copied: Binding<String?>
    var rotating: Bool
    var offline: Bool
    var onPickZone: () -> Void
    var onReveal: () -> Void
    var onRotate: () -> Void
    var onTrigger: (String) -> Void

    var body: some View {
        Field("When to run") {
            ChoicePicker(selection: triggerKind, options: [
                (id: "manual", label: "Manual"), (id: "schedule", label: "Schedule"), (id: "webhook", label: "Webhook"),
            ], fitsAvailableWidth: true)
        }
        if draft.string("trigger") == "schedule" {
            WorkflowScheduleFields(schedule: schedule, checked: checked, preview: preview, previewError: previewError, onPickZone: onPickZone)
        }
        if draft.string("trigger") == "webhook" {
            WorkflowWebhookFields(hook: revealed.first { $0.nodeId == draft.id } ?? webhook, showKey: showKey, copied: copied,
                                  rotating: rotating, offline: offline, onReveal: onReveal, onRotate: onRotate)
        }
    }

    private var triggerKind: Binding<String> {
        Binding(get: { draft.string("trigger").isEmpty ? "manual" : draft.string("trigger") }, set: onTrigger)
    }
}

private struct WorkflowAgentFields: View {
    @Binding var draft: WorkflowNode
    var bots: [(id: String, label: String)]

    var body: some View {
        Field("Agent") { WorkflowBotPicker(draft: $draft, bots: bots) }
        Field("Task") { WorkflowTextField(draft: $draft, key: "task", axis: .vertical) }
    }
}

private struct WorkflowConditionFields: View {
    @Binding var draft: WorkflowNode

    var body: some View {
        Field("Condition") {
            ChoicePicker(selection: opKind, options: [
                (id: "contains", label: "Contains"), (id: "equals", label: "Equals"),
                (id: "regex", label: "Matches"), (id: "empty", label: "Empty"),
            ])
        }
        if draft.string("op") != "empty" {
            Field(valueLabel) { WorkflowTextField(draft: $draft, key: "value") }
        }
    }

    private var opKind: Binding<String> {
        Binding(get: { draft.string("op").isEmpty ? "contains" : draft.string("op") }, set: { set("op", $0) })
    }

    private var valueLabel: String {
        switch draft.string("op") {
        case "equals": "Equals"
        case "regex": "Matches"
        default: "Contains"
        }
    }

    private func set(_ key: String, _ value: String) {
        draft.setString(key, value, keepEmpty: true)
    }
}

private struct WorkflowOutputFields: View {
    @Binding var draft: WorkflowNode
    var bots: [(id: String, label: String)]

    var body: some View {
        Field("Notification") {
            ChoicePicker(selection: targetKind, options: [
                (id: "notify", label: "Notification"), (id: "chat", label: "Chat"),
            ])
        }
        if draft.string("target") == "chat" {
            Field("Chat") { WorkflowBotPicker(draft: $draft, bots: bots) }
        }
    }

    private var targetKind: Binding<String> {
        Binding(get: { draft.string("target").isEmpty ? "notify" : draft.string("target") }, set: { value in
            draft.setString("target", value, keepEmpty: true)
            if value != "chat" { draft.setString("botId", "") }
        })
    }
}

struct WorkflowTextField: View {
    @Binding var draft: WorkflowNode
    var key: String
    var axis: Axis = .horizontal

    var body: some View {
        TextField("", text: Binding(get: { draft.string(key) }, set: { draft.setString(key, $0, keepEmpty: true) }), axis: axis)
            .workflowInput()
    }
}

struct WorkflowBotPicker: View {
    @Binding var draft: WorkflowNode
    var bots: [(id: String, label: String)]

    var body: some View {
        ChoicePicker(selection: Binding(get: { draft.string("botId") }, set: { draft.setString("botId", $0, keepEmpty: true) }),
                     options: [(id: "", label: "Choose a bot")] + bots, fitsAvailableWidth: true)
    }
}

private struct WorkflowScheduleFields: View {
    @Binding var schedule: RoutineScheduleDraft
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var onPickZone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Field("Repeat") {
                ChoicePicker(selection: Binding(get: { schedule.calendarStyle }, set: { style in
                    let compiled = checked == schedule ? preview?.cronExpression : nil
                    schedule = schedule.repeating(style, compiled: compiled)
                }), options: RoutineScheduleDraft.repeatChoices)
            }
            repeatFields
            zoneField
            previewText
        }
    }

    @ViewBuilder private var repeatFields: some View {
        switch schedule.calendarStyle {
        case "hourly":
            Field("At minute") {
                ChoicePicker(selection: $schedule.minute, options: (0..<60).map { (id: $0, label: String(format: ":%02d", $0)) })
            }
        case "daily", "weekdays":
            timeField
        case "weekly":
            Field("On") {
                ChoicePicker(selection: $schedule.weekday,
                             options: [1, 2, 3, 4, 5, 6, 0].map { (id: $0, label: Calendar.current.standaloneWeekdaySymbols[$0]) })
            }
            timeField
        case "custom":
            Field("Cron") {
                TextField("Cron", text: $schedule.expression, prompt: Text(L10n.tr("0 9 * * 1-5")).foregroundStyle(Palette.secondary))
                    .font(.body.monospaced())
                    .autocorrectionDisabled()
                    .workflowInput()
            }
        default:
            EmptyView()
        }
    }

    private var timeField: some View {
        Field("At") {
            DatePicker("At", selection: $schedule.timeOfDay, displayedComponents: .hourAndMinute)
                .labelsHidden()
                .environment(\.timeZone, .gmt)
        }
    }

    private var zoneField: some View {
        Field("Time zone") {
            Button(action: onPickZone) {
                HStack(spacing: 6) {
                    Text(zoneName).lineLimit(1)
                    Text(TimeZones.offset(zoneName)).foregroundStyle(Palette.secondary)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
                .foregroundStyle(Palette.text)
                .outlinedPill()
            }
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder private var previewText: some View {
        if let previewError {
            Text(L10n.tr(previewError)).font(.caption).foregroundStyle(Palette.danger)
        } else if let preview, checked == schedule {
            Text(preview.summary).font(.caption.weight(.medium))
            if let next = preview.nextRunAt {
                Text(L10n.tr("Next \(Date(milliseconds: next).formatted(date: .abbreviated, time: .shortened))"))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private var zoneName: String {
        schedule.zone.isEmpty ? TimeZone.current.identifier : schedule.zone
    }
}

private struct WorkflowWebhookFields: View {
    var hook: WorkflowWebhook?
    @Binding var showKey: Bool
    @Binding var copied: String?
    var rotating: Bool
    var offline: Bool
    var onReveal: () -> Void
    var onRotate: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let hook {
                Field("URL") { valueRow(hook.url ?? hook.localUrl ?? "", copy: "url") }
                Field("Key") { keyRow(hook) }
            } else {
                Text(L10n.tr("Loading")).font(.caption).foregroundStyle(Palette.secondary)
            }
        }
    }

    private func keyRow(_ hook: WorkflowWebhook) -> some View {
        valueRow(showKey ? (hook.secret ?? "") : String(repeating: "•", count: 16), copy: "key") {
            IconButton(showKey ? "Hide key" : "Show key", systemImage: showKey ? "eye.slash" : "eye") {
                if hook.secret == nil {
                    onReveal()
                } else {
                    withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { showKey.toggle() }
                }
            }
            IconButton("Replace key", systemImage: "arrow.triangle.2.circlepath", action: onRotate)
                .disabled(rotating || offline)
        }
    }

    private func valueRow(_ text: String, copy: String, @ViewBuilder extra: () -> some View = { EmptyView() }) -> some View {
        HStack(spacing: 8) {
            Text(text.isEmpty ? " " : text)
                .font(.callout.monospaced())
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            extra()
            IconButton(copied == copy ? "Copied" : "Copy", systemImage: copied == copy ? "checkmark" : "doc.on.doc") {
                let value = copy == "key" ? hook?.secret : (hook?.url ?? hook?.localUrl)
                Pasteboard.copy(value)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = copy }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}

extension View {
    func workflowInput() -> some View {
        textFieldStyle(.plain)
            .padding(12)
            .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
