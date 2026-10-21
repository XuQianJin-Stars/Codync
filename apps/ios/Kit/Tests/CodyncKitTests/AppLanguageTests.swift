import Foundation
import Testing
@testable import CodyncKit

struct AppLanguageTests {
    @Test func chineseFollowsTheChoiceNotTheOtherLanguage() {
        #expect(AppLanguage.usesChinese(preference: .zh, systemIsChinese: false))
        #expect(!AppLanguage.usesChinese(preference: .en, systemIsChinese: true))
        #expect(AppLanguage.usesChinese(preference: .system, systemIsChinese: true))
        #expect(!AppLanguage.usesChinese(preference: .system, systemIsChinese: false))
    }

    @Test func systemChineseIsThePreferredLanguagePrefix() {
        #expect(AppLanguage.systemIsChinese(languages: ["zh-Hans-CN", "en"]))
        #expect(AppLanguage.systemIsChinese(languages: ["zh-Hant-TW"]))
        #expect(!AppLanguage.systemIsChinese(languages: ["en-US", "zh-Hans"]))
        #expect(!AppLanguage.systemIsChinese(languages: []))
    }

    @Test func phrasesUseSimplifiedChineseOnlyWhenThatLanguageIsActive() {
        #expect(L10n.tr("Pair a computer", preference: .zh, systemIsChinese: false) == "配对一台电脑")
        #expect(L10n.tr("Pair a computer", preference: .en, systemIsChinese: true) == "Pair a computer")
        #expect(L10n.tr("Pair a computer", preference: .system, systemIsChinese: true) == "配对一台电脑")
        #expect(L10n.tr("Not a phrase", preference: .zh, systemIsChinese: false) == "Not a phrase")
        #expect(AppLanguage.menuTitle(.zh, preference: .en, systemIsChinese: true) == "中文")
        #expect(AppLanguage.menuTitle(.system, preference: .zh, systemIsChinese: false) == "跟随系统")
    }
}
