import Foundation

/// The language of this app's own interface. Stored on the device; chats and bots are not translated.
public enum AppLanguage: String, CaseIterable, Sendable, Identifiable, Hashable {
    case system
    case zh
    case en

    public var id: String { rawValue }

    public static let storageKey = "appLanguage"

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
    private static let chinese: [String: String] = {
        guard let url = Bundle.module.url(forResource: "zh-Hans", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let table = try? JSONDecoder().decode([String: String].self, from: data) else { return [:] }
        return table
    }()

    public static func tr(
        _ key: String,
        preference: AppLanguage = AppLanguage(rawValue: UserDefaults.standard.string(forKey: AppLanguage.storageKey) ?? "") ?? .system,
        systemIsChinese: Bool = AppLanguage.systemIsChinese()
    ) -> String {
        guard AppLanguage.usesChinese(preference: preference, systemIsChinese: systemIsChinese) else { return key }
        return chinese[key] ?? key
    }

    /// "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置".
    public static func resetLine(_ raw: String) -> String {
        guard let time = resetRemainder(raw) else { return tr(raw) }
        let preference = AppLanguage(rawValue: UserDefaults.standard.string(forKey: AppLanguage.storageKey) ?? "") ?? .system
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
