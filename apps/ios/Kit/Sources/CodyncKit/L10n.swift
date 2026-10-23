import Foundation
import os

/// The language of this app's own interface. Stored on the device; chats and bots are not translated.
public enum AppLanguage: String, CaseIterable, Sendable, Identifiable, Hashable {
    case system
    case zh
    case en

    public var id: String { rawValue }

    public static let storageKey = "appLanguage"

    /// The App Group, so the widget extension reads the same choice the app writes.
    /// A choice saved before this was shared is copied across the first time the app looks.
    public static var defaults: UserDefaults {
        let shared = UserDefaults(suiteName: SharedStore.appGroup) ?? .standard
        if shared.string(forKey: storageKey) == nil,
           let existing = UserDefaults.standard.string(forKey: storageKey) {
            shared.set(existing, forKey: storageKey)
        }
        return shared
    }

    public static func preference(in store: UserDefaults = defaults) -> AppLanguage {
        AppLanguage(rawValue: store.string(forKey: storageKey) ?? "") ?? .system
    }

    public static func systemIsChinese(languages: [String] = Locale.preferredLanguages) -> Bool {
        languages.first?.hasPrefix("zh") == true
    }

    public static func usesChinese(preference: AppLanguage, systemIsChinese: Bool) -> Bool {
        switch preference {
        case .zh: true
        case .en: false
        case .system: systemIsChinese
        }
    }

    /// The locale SwiftUI should use so dates and string lookups follow the choice.
    public static func locale(preference: AppLanguage, systemIsChinese: Bool) -> Locale {
        usesChinese(preference: preference, systemIsChinese: systemIsChinese)
            ? Locale(identifier: "zh-Hans")
            : Locale(identifier: "en")
    }

    /// What the language menu shows for this choice. Names of languages stay in that language.
    public static func menuTitle(_ language: AppLanguage, preference: AppLanguage, systemIsChinese: Bool) -> String {
        switch language {
        case .system: L10n.tr("Follow system", preference: preference, systemIsChinese: systemIsChinese)
        case .zh: "中文"
        case .en: "English"
        }
    }
}

/// English source strings, with Simplified Chinese when the app language asks for it.
public enum L10n {
    private static let log = Logger(subsystem: "com.pokai.Codync", category: "L10n")

    private static let chinese: [String: String] = {
        guard let url = Bundle.module.url(forResource: "zh-Hans", withExtension: "json") else {
            log.error("Chinese phrases are missing from the app bundle")
            return [:]
        }
        do {
            return try JSONDecoder().decode([String: String].self, from: Data(contentsOf: url))
        } catch {
            log.error("Chinese phrases failed to load: \(error, privacy: .public)")
            return [:]
        }
    }()

    public static func tr(
        _ key: String,
        preference: AppLanguage = AppLanguage.preference(),
        systemIsChinese: Bool = AppLanguage.systemIsChinese()
    ) -> String {
        guard AppLanguage.usesChinese(preference: preference, systemIsChinese: systemIsChinese) else { return key }
        if let hit = phrase(key) { return hit }
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed == key ? key : (phrase(trimmed) ?? key)
    }

    /// One piece of a phrase key such as "Runs the official installer on {name}."
    private struct Piece {
        var literal: String?
        var token: String?
    }

    /// Phrase keys that already contain `{token}` slots, longest fixed text first.
    private static let templates: [(key: String, pieces: [Piece])] = {
        chinese.compactMap { key, _ -> (Int, String, [Piece])? in
            guard let pieces = parseTemplate(key) else { return nil }
            let literal = pieces.reduce(0) { $0 + ($1.literal?.count ?? 0) }
            guard literal >= 6 else { return nil }
            return (literal, key, pieces)
        }
        .sorted { $0.0 == $1.0 ? $0.1.count > $1.1.count : $0.0 > $1.0 }
        .map { ($0.1, $0.2) }
    }()

    /// `{name}` and the same shape. A `{` that is not a slot makes the key unusable as a template.
    private static func parseTemplate(_ template: String) -> [Piece]? {
        var pieces: [Piece] = []
        var rest = template[...]
        while let open = rest.firstIndex(of: "{") {
            if open != rest.startIndex {
                pieces.append(Piece(literal: String(rest[..<open]), token: nil))
            }
            guard let close = rest[open...].firstIndex(of: "}") else { return nil }
            let token = String(rest[open...close])
            let name = token.dropFirst().dropLast()
            guard !name.isEmpty, name.allSatisfy({ $0.isLetter || $0.isNumber }), pieces.last?.token == nil else { return nil }
            pieces.append(Piece(literal: nil, token: token))
            rest = rest[rest.index(after: close)...]
        }
        if !rest.isEmpty { pieces.append(Piece(literal: String(rest), token: nil)) }
        return pieces.contains(where: { $0.token != nil }) ? pieces : nil
    }

    /// A sentence the host or a view already filled in, matched back to its `{token}` phrase.
    private static func phrase(_ key: String) -> String? {
        if let hit = chinese[key] { return hit }
        for template in templates {
            guard let phrase = chinese[template.key], let filled = fill(key, pieces: template.pieces, phrase: phrase, depth: 0) else { continue }
            return filled
        }
        return nil
    }

    private static func fill(_ text: String, pieces: [Piece], phrase: String, depth: Int) -> String? {
        var pos = text.startIndex
        var caps: [String: String] = [:]
        for (index, piece) in pieces.enumerated() {
            if let literal = piece.literal {
                guard text[pos...].hasPrefix(literal) else { return nil }
                pos = text.index(pos, offsetBy: literal.count)
                continue
            }
            guard let token = piece.token else { return nil }
            let end: String.Index
            if let next = index + 1 < pieces.count ? pieces[index + 1].literal : nil {
                guard let found = text[pos...].range(of: next)?.lowerBound, found > pos else { return nil }
                end = found
            } else {
                guard pos < text.endIndex else { return nil }
                end = text.endIndex
            }
            caps[token] = String(text[pos..<end])
            pos = end
        }
        guard pos == text.endIndex else { return nil }
        var result = phrase
        for (token, value) in caps {
            let inner = chinese[value] ?? (depth == 0 ? nested(value) : nil) ?? value
            result = result.replacingOccurrences(of: token, with: inner)
        }
        return result
    }

    /// One more template pass, for a setup hint inside an error. It does not look inside its own slots.
    private static func nested(_ text: String) -> String? {
        for template in templates {
            guard let phrase = chinese[template.key], let filled = fill(text, pieces: template.pieces, phrase: phrase, depth: 1) else { continue }
            return filled
        }
        return nil
    }

    /// "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置".
    public static func resetLine(_ raw: String) -> String {
        guard let time = resetRemainder(raw) else { return tr(raw) }
        let preference = AppLanguage.preference()
        guard AppLanguage.usesChinese(preference: preference, systemIsChinese: AppLanguage.systemIsChinese()) else { return raw }
        if let spoken = compactTime(time) { return spoken + tr("after reset") }
        return "\(tr("resets")) \(time)"
    }

    private static func resetRemainder(_ raw: String) -> String? {
        if raw.hasPrefix("resets in ") { return String(raw.dropFirst("resets in ".count)) }
        if raw.hasPrefix("resets ") { return String(raw.dropFirst("resets ".count)) }
        return nil
    }

    /// "3h 20m", "2d 6h", "45m". A clock time such as "Sep 26 at 12pm" stays nil.
    private static func compactTime(_ time: String) -> String? {
        var spoken = ""
        var matched = false
        for token in time.split(separator: " ") {
            guard token.count >= 2, let unit = token.last, let n = Int(token.dropLast()) else { return nil }
            let word: String
            switch unit {
            case "d": word = tr("day unit")
            case "h": word = tr("hour unit")
            case "m": word = tr("minute unit")
            default: return nil
            }
            matched = true
            if n == 0 { continue }
            spoken += "\(n)\(word)"
        }
        guard matched else { return nil }
        return spoken.isEmpty ? "0\(tr("minute unit"))" : spoken
    }
}
